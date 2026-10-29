#!/usr/bin/env python3
"""Publish complete immutable product assets, then reconcile signed mutable feeds."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from urllib.parse import quote

from release import version_key, validate_release_checkout
from sign_runtime_index import TARGETS

ROOT = Path(__file__).resolve().parents[2]


def gh(*args):
    result = subprocess.run(['gh', *args], capture_output=True, text=True, timeout=300)
    if result.returncode:
        raise RuntimeError(f'GitHub operation failed: {result.stderr.strip()}')
    return result.stdout


def release(tag):
    path = f'repos/{os.environ["GITHUB_REPOSITORY"]}/releases/tags/{quote(tag, safe="")}'
    result = subprocess.run(['gh', 'api', path], capture_output=True, text=True, timeout=60)
    if result.returncode == 0:
        return json.loads(result.stdout)
    if '(HTTP 404)' not in result.stderr:
        raise RuntimeError(f'Cannot inspect release: {result.stderr.strip()}')
    # The tag endpoint can return 404 for a draft. The authenticated list
    # includes drafts; paginate so an older pending release is not missed.
    repository = os.environ['GITHUB_REPOSITORY']
    page = 1
    while True:
        entries = json.loads(gh('api', f'repos/{repository}/releases?per_page=100&page={page}'))
        matches = [entry for entry in entries if entry['tag_name'] == tag]
        if len(matches) > 1:
            raise ValueError(f'Ambiguous release identity: {tag}')
        if matches:
            return matches[0]
        if len(entries) < 100:
            return None
        page += 1


def create_draft(tag, sha, title):
    # Use the mutation response identity; newly created drafts can be absent from
    # both tag lookup and the release list for a short time.
    info = json.loads(gh('api', f'repos/{os.environ["GITHUB_REPOSITORY"]}/releases',
                         '--method', 'POST', '-f', f'tag_name={tag}',
                         '-f', f'target_commitish={sha}', '-f', f'name={title}',
                         '-F', 'draft=true', '-F', 'generate_release_notes=true'))
    if info.get('tag_name') != tag or info.get('draft') is not True or not isinstance(info.get('id'), int):
        raise ValueError('Created draft response identity mismatch')
    return info


def release_by_id(info):
    result = json.loads(gh('api', f'repos/{os.environ["GITHUB_REPOSITORY"]}/releases/{info["id"]}'))
    if result.get('id') != info['id'] or result.get('tag_name') != info['tag_name']:
        raise ValueError('Release ID readback identity mismatch')
    return result


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def report(message):
    print(message)
    if os.environ.get('GITHUB_STEP_SUMMARY'):
        with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as stream:
            stream.write(message + '\n')


def expected(product, version):
    if product == 'runtime':
        binaries = {f'anvil-{version}-{target}' + ('.exe' if 'windows' in target else '') for target in TARGETS}
        archives = {f'anvil-{version}-{target}' + ('.zip' if 'windows' in target else '.tar.gz') for target in TARGETS}
        return binaries | archives
    installers = {f'Anvil_{version}_aarch64.dmg', f'Anvil_{version}_x64.dmg',
                  f'Anvil_{version}_amd64.deb', f'Anvil_{version}_x64-setup.exe'}
    if '-' not in version:
        installers |= {f'Anvil_{version}_amd64.AppImage', f'Anvil_{version}_x64_en-US.msi'}
    return installers | {f'anvil-desktop-{version}-{target}.tar.gz' for target in TARGETS}


def validate_files(folder, names):
    actual = {p.name for p in folder.iterdir() if p.is_file()}
    if actual != names:
        raise ValueError(f'Artifact set mismatch: missing={sorted(names - actual)}, unexpected={sorted(actual - names)}')
    if any((folder / name).stat().st_size == 0 for name in names):
        raise ValueError('Empty release artifact')


def signed(path):
    from nacl.signing import VerifyKey
    envelope = json.loads(path.read_text())
    payload = bytes.fromhex(envelope['payload'])
    VerifyKey(bytes.fromhex(os.environ['UPDATE_PUBLIC_KEY'])).verify(payload, bytes.fromhex(envelope['signature']))
    return json.loads(payload)


def script(name, folder, **env):
    subprocess.run(['python3', str(ROOT / '.github/scripts' / name), str(folder)],
                   env={**os.environ, **env}, check=True, timeout=120)


def runtime_index(folder, version):
    binary = folder / f'anvil-{version}-x86_64-unknown-linux-gnu'
    binary.chmod(0o755)
    api = subprocess.check_output(['python3', str(ROOT / '.github/scripts/validate_runtime_api.py'),
                                   str(binary)], text=True, timeout=60).strip()
    actual = subprocess.check_output([str(binary.resolve()), '--version'], text=True, timeout=30).strip()
    if actual != f'anvil {version}':
        raise ValueError('Runtime product version mismatch')
    script('sign_runtime_index.py', folder, RELEASE_VERSION=version, API_VERSION=api)
    targets = {}
    for target in TARGETS:
        name = f'anvil-{version}-{target}' + ('.zip' if 'windows' in target else '.tar.gz')
        targets[target] = {'archive': name, 'archive_format': 'zip' if name.endswith('.zip') else 'tar.gz',
                           'sha256': digest(folder / name), 'url': asset_url(name)}
    (folder / 'latest.json').write_text(json.dumps({'version': version, 'tag': os.environ['RELEASE_TAG'],
                                                 'targets': targets}, sort_keys=True) + '\n')


def asset_url(name):
    return (f'https://github.com/{os.environ["GITHUB_REPOSITORY"]}/releases/download/'
            f'{quote(os.environ["RELEASE_TAG"], safe="")}/{name}')


def verify_identity(info, sha):
    # Annotated and lightweight tags both resolve through git to their commit.
    tag = os.environ['RELEASE_TAG']
    subprocess.run(['git', 'fetch', 'origin', f'refs/tags/{tag}:refs/tags/{tag}'], check=True, capture_output=True)
    actual = subprocess.check_output(['git', 'rev-parse', f'{tag}^{{commit}}'], text=True).strip()
    if actual != sha or info['tag_name'] != tag:
        raise ValueError('Release tag identity mismatch')


def download(info, folder, names):
    if {asset['name'] for asset in info['assets']} != names:
        raise ValueError('Remote release attachment set is incomplete or unexpected')
    for asset in info['assets']:
        endpoint = f'repos/{os.environ["GITHUB_REPOSITORY"]}/releases/assets/{asset["id"]}'
        with (folder / asset['name']).open('xb') as stream:
            subprocess.run(['gh', 'api', endpoint, '-H', 'Accept: application/octet-stream'],
                           stdout=stream, check=True, timeout=300)
    validate_files(folder, names)


def validate_checksums(folder, names):
    lines = (folder / 'SHA256SUMS').read_text().splitlines()
    recorded = {}
    for line in lines:
        checksum, name = line.split('  ', 1)
        if name not in names - {'SHA256SUMS'} or name in recorded:
            raise ValueError('Invalid checksum filename or duplicate')
        recorded[name] = checksum
    if set(recorded) != names - {'SHA256SUMS'}:
        raise ValueError('Checksum file is incomplete')
    if any(digest(folder / name) != checksum for name, checksum in recorded.items()):
        raise ValueError('Downloaded release checksum mismatch')


def validate_runtime_index(folder, version):
    data = signed(folder / 'runtime-index.json')
    if data.get('product') != 'anvil' or data.get('schema_version') != 1 or data.get('version') != version or set(data['targets']) != set(TARGETS):
        raise ValueError('Runtime index identity mismatch')
    # Do not execute published binaries when recovering a feed.
    source = (ROOT / 'crates/anvil/src/serve.rs').read_text()
    import re
    api = re.findall(r'^pub\(crate\) const HTTP_API_VERSION: &str = "([^"]+)";', source, re.MULTILINE)
    if api != [data.get('http_api_version')]:
        raise ValueError('Runtime index API mismatch')
    for target, entry in data['targets'].items():
        name = f'anvil-{version}-{target}' + ('.exe' if 'windows' in target else '')
        if (entry.get('binary') != name or entry.get('url') != asset_url(name)
                or entry.get('size') != (folder / name).stat().st_size or entry.get('sha256') != digest(folder / name)):
            raise ValueError(f'Runtime index artifact mismatch: {target}')
    latest = json.loads((folder / 'latest.json').read_text())
    if latest.get('version') != version or latest.get('tag') != os.environ['RELEASE_TAG'] or set(latest['targets']) != set(TARGETS):
        raise ValueError('CLI archive index identity mismatch')
    for target, entry in latest['targets'].items():
        suffix = '.zip' if 'windows' in target else '.tar.gz'
        name = f'anvil-{version}-{target}{suffix}'
        if entry != {'archive': name, 'archive_format': suffix[1:], 'sha256': digest(folder / name), 'url': asset_url(name)}:
            raise ValueError(f'CLI archive index mismatch: {target}')


def update_feed(tag, files, version, sha):
    info = release(tag)
    with tempfile.TemporaryDirectory(prefix='anvil-feed-') as temporary:
        old = Path(temporary) / 'old'
        old.mkdir()
        if info and info['assets']:
            gh('release', 'download', tag, '--dir', str(old))
        pending = []
        for path in files:
            candidate = signed(path)
            if candidate['version'] != version:
                raise ValueError('Candidate feed version mismatch')
            existing = old / path.name
            if existing.exists():
                previous = signed(existing)
                # Verify the identity before comparing versions; never trust an unsigned number.
                fields = ('schema_version', 'channel', 'app_id', 'product_id', 'product', 'target')
                if any(previous.get(field) != candidate.get(field) for field in fields):
                    raise ValueError(f'Feed identity mismatch: {tag}/{path.name}')
                before, after = version_key(previous['version']), version_key(version)
                if before > after:
                    report(f'- {tag}/{path.name}: retained newer {previous["version"]}')
                    continue
                if before == after:
                    if previous != candidate:
                        raise ValueError(f'Conflicting content for same feed version: {tag}/{path.name}')
                    report(f'- {tag}/{path.name}: already synchronized')
                    continue
            pending.append(path)
        if not info:
            gh('release', 'create', tag, '--draft', '--prerelease', '--target', sha,
               '--title', f'Anvil {tag} feed', '--notes', 'Mutable signed update feed.')
        for path in pending:
            gh('release', 'upload', tag, str(path), '--clobber')
            report(f'- {tag}/{path.name}: uploaded; readback pending')
        gh('release', 'edit', tag, '--draft=false', '--prerelease')
        readback = Path(temporary) / 'readback'
        readback.mkdir()
        gh('release', 'download', tag, '--dir', str(readback))
        for path in files:
            actual = signed(readback / path.name)
            desired = signed(path)
            if version_key(actual['version']) < version_key(version):
                raise ValueError('Feed readback is older than the release')
            if actual['version'] == version and actual != desired:
                raise ValueError('Feed readback content mismatch')
            if actual != (signed(old / path.name) if (old / path.name).exists() and
                          version_key(signed(old / path.name)['version']) > version_key(version) else desired):
                raise ValueError('Feed changed unexpectedly during publication')
            report(f'- {tag}/{path.name}: verified {actual["version"]}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('inspect', 'publish'))
    parser.add_argument('--product', choices=('desktop', 'runtime'), required=True)
    args = parser.parse_args()
    tag, sha = os.environ['RELEASE_TAG'], os.environ['RELEASE_SHA']
    version = tag.removeprefix(f'{args.product}/v')
    version_key(version, requested=True)
    if tag != f'{args.product}/v{version}':
        raise ValueError('Product tag mismatch')
    actual_sha = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    validate_release_checkout(actual_sha)
    if actual_sha != sha:
        raise ValueError('publication checkout differs from release SHA')
    info = release(tag)
    if info:
        verify_identity(info, sha)
    published = bool(info and not info['draft'])
    if args.action == 'inspect':
        with open(os.environ['GITHUB_OUTPUT'], 'a') as stream:
            stream.write(f'published={str(published).lower()}\n')
        report('Published release found; recover feeds from public assets without rebuilding.' if published
               else 'No published version; build all targets before publication.')
        return
    names = expected(args.product, version)
    if args.product == 'runtime':
        names |= {'runtime-index.json', 'latest.json'}
    names.add('SHA256SUMS')
    with tempfile.TemporaryDirectory(prefix='anvil-publish-') as temporary:
        folder = Path(temporary) / 'assets'
        folder.mkdir()
        if not published:
            source = ROOT / 'dist'
            validate_files(source, expected(args.product, version))
            for name in expected(args.product, version):
                shutil.copy2(source / name, folder / name)
            if args.product == 'runtime':
                runtime_index(folder, version)
                validate_runtime_index(folder, version)
            else:
                # Signing validates package manifests and installer coverage before public release.
                script('sign_desktop_updates.py', folder, RELEASE_VERSION=version)
                script('sign_shell_updates.py', folder, RELEASE_VERSION=version)
            (folder / 'SHA256SUMS').write_text(''.join(f'{digest(folder / name)}  {name}\n' for name in sorted(names - {'SHA256SUMS'})))
            if not info:
                info = create_draft(tag, sha, f'Anvil {args.product} v{version}')
            # Drafts may be repaired; unexpected old draft assets are removed explicitly.
            verify_identity(info, sha)
            if not info['draft']:
                raise ValueError('Release became public; refusing to replace assets')
            # Asset operations use the response ID too, avoiding tag/list visibility races.
            repository = os.environ['GITHUB_REPOSITORY']
            for asset in info['assets']:
                gh('api', f'repos/{repository}/releases/assets/{asset["id"]}', '--method', 'DELETE')
            for name in sorted(names):
                endpoint = (f'https://uploads.github.com/repos/{repository}/releases/{info["id"]}/assets'
                            f'?name={quote(name, safe="")}')
                gh('api', endpoint, '--method', 'POST', '-H', 'Content-Type: application/octet-stream',
                   '--input', str(folder / name))
            staged = Path(temporary) / 'draft-readback'
            staged.mkdir()
            download(release_by_id(info), staged, names)
            if any(digest(folder / name) != digest(staged / name) for name in names):
                raise ValueError('Draft readback differs from local assets')
            validate_checksums(staged, names)
            gh('api', f'repos/{os.environ["GITHUB_REPOSITORY"]}/releases/{info["id"]}',
               '--method', 'PATCH', '-F', 'draft=false', '-F', f'prerelease={str("-" in version).lower()}',
               '-f', f'make_latest={str(args.product == "runtime" and "-" not in version).lower()}')
            report(f'## Version release published\n{tag}: all attachments verified before publication.')
        public = Path(temporary) / 'public'
        public.mkdir()
        info = release_by_id(info)
        if info['draft']:
            raise ValueError('Version release is not public; refusing feed update')
        download(info, public, names)
        validate_checksums(public, names)
        report(f'## Public assets verified\n{tag}: using published bytes for feed reconciliation.')
        if args.product == 'desktop':
            script('sign_desktop_updates.py', public, RELEASE_VERSION=version)
            script('sign_shell_updates.py', public, RELEASE_VERSION=version)
            update_feed('desktop-updates-stable', sorted((public / 'update-manifests').glob('*.json')), version, sha)
            update_feed('shell-updates-stable', sorted((public / 'shell-update-manifests').glob('*.json')), version, sha)
        else:
            validate_runtime_index(public, version)
            update_feed('runtime-updates', [public / 'runtime-index.json'], version, sha)
        report('## Feed reconciliation complete\nAll target manifests read back and verified.')


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        report(f'## Publication incomplete\n{error}\nRetry the same release to reconcile remaining stages; public assets are never replaced.')
        raise SystemExit(str(error)) from error
