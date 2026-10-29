#!/usr/bin/env python3
"""Reuse GitHub artifacts only from trusted release runs and an exact product/tag/SHA."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import zipfile

from publish_release import expected
from release import command, summary, version_key, write_outputs

TARGETS = {
    'x86_64-unknown-linux-gnu': 'ubuntu-22.04',
    'x86_64-apple-darwin': 'macos-15-intel',
    'aarch64-apple-darwin': 'macos-15',
    'x86_64-pc-windows-msvc': 'windows-2022',
}
MAX_BYTES = 2 * 1024**3


def identity(product, tag, sha, target):
    version = tag.removeprefix(f'{product}/v')
    version_key(version, requested=True)
    if product not in ('desktop', 'runtime') or tag != f'{product}/v{version}':
        raise ValueError('artifact product/tag mismatch')
    if len(sha) != 40 or any(c not in '0123456789abcdef' for c in sha) or target not in TARGETS:
        raise ValueError('invalid artifact identity')
    return f'release-{product}-{version}-{sha}-{target}'


def api(path):
    return json.loads(command('gh', 'api', path))


def pages(path, field):
    for page in range(1, 21):
        data = api(f'{path}{"&" if "?" in path else "?"}per_page=100&page={page}')[field]
        yield from data
        if len(data) < 100:
            return
    raise ValueError('artifact history exceeds bounded lookup; remove expired artifacts or narrow lookup')


def trusted(run, repository):
    return (run.get('path') == '.github/workflows/release.yml'
            and run.get('head_branch') == 'main'
            and run.get('event') in ('workflow_dispatch', 'workflow_run')
            and run.get('head_repository', {}).get('full_name') == repository)


def find(product, tag, sha):
    repository = os.environ['GITHUB_REPOSITORY']
    found, runs = {}, {}
    for target in TARGETS:
        name = identity(product, tag, sha, target)
        for artifact in pages(f'repos/{repository}/actions/artifacts?name={name}', 'artifacts'):
            if artifact.get('expired') or artifact.get('name') != name:
                continue
            digest = artifact.get('digest', '')
            if not digest.startswith('sha256:') or len(digest) != 71:
                continue
            run_id = artifact['workflow_run']['id']
            if run_id not in runs:
                runs[run_id] = api(f'repos/{repository}/actions/runs/{run_id}')
            if trusted(runs[run_id], repository):
                found[target] = artifact
                break
    return found


def target_files(product, version, target):
    if product == 'runtime':
        return {name for name in expected(product, version) if target in name}
    installer = {
        'x86_64-unknown-linux-gnu': {f'Anvil_{version}_amd64.deb'},
        'x86_64-apple-darwin': {f'Anvil_{version}_x64.dmg'},
        'aarch64-apple-darwin': {f'Anvil_{version}_aarch64.dmg'},
        'x86_64-pc-windows-msvc': {f'Anvil_{version}_x64-setup.exe'},
    }[target]
    if '-' not in version:
        if target == 'x86_64-unknown-linux-gnu':
            installer.add(f'Anvil_{version}_amd64.AppImage')
        if target == 'x86_64-pc-windows-msvc':
            installer.add(f'Anvil_{version}_x64_en-US.msi')
    return installer | {f'anvil-desktop-{version}-{target}.tar.gz'}


def extract(archive, destination, names):
    with zipfile.ZipFile(archive) as source:
        entries = source.infolist()
        if len(entries) != len(names) or {e.filename for e in entries} != names:
            raise ValueError('artifact files differ from expected target payload')
        if sum(e.file_size for e in entries) > MAX_BYTES:
            raise ValueError('artifact exceeds extraction budget')
        for entry in entries:
            mode = entry.external_attr >> 16
            if (Path(entry.filename).name != entry.filename or '\\' in entry.filename
                    or entry.is_dir() or stat.S_ISLNK(mode) or entry.file_size == 0):
                raise ValueError('invalid artifact entry')
            path = destination / entry.filename
            with source.open(entry) as reader, path.open('xb') as writer:
                shutil.copyfileobj(reader, writer, 1024 * 1024)


def collect(product, tag, sha, destination):
    artifacts = find(product, tag, sha)
    if set(artifacts) != set(TARGETS):
        raise ValueError('missing or expired platform artifacts; rerun the release to rebuild missing targets')
    version = tag.split('/v', 1)[1]
    destination.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='anvil-artifacts-') as temporary:
        for target, artifact in artifacts.items():
            archive = Path(temporary) / f'{target}.zip'
            url = f'repos/{os.environ["GITHUB_REPOSITORY"]}/actions/artifacts/{artifact["id"]}/zip'
            with archive.open('wb') as stream:
                subprocess.run(['gh', 'api', url], stdout=stream, check=True, timeout=300)
            if archive.stat().st_size > MAX_BYTES:
                raise ValueError('artifact exceeds download budget')
            with archive.open('rb') as stream:
                digest = 'sha256:' + hashlib.file_digest(stream, 'sha256').hexdigest()
            if digest != artifact['digest']:
                raise ValueError('GitHub artifact digest mismatch')
            extract(archive, destination, target_files(product, version, target))
            summary(f'- {product}/{target}: verified artifact `{artifact["id"]}` from run `{artifact["workflow_run"]["id"]}`')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('plan', 'collect'))
    parser.add_argument('--product', choices=('desktop', 'runtime'), required=True)
    args = parser.parse_args()
    tag, sha = os.environ['RELEASE_TAG'], os.environ['RELEASE_SHA']
    if args.action == 'collect':
        collect(args.product, tag, sha, Path('dist'))
        return
    artifacts = find(args.product, tag, sha)
    missing = []
    for target, runner in TARGETS.items():
        if target in artifacts:
            summary(f'- {args.product}/{target}: reuse artifact `{artifacts[target]["id"]}`')
        else:
            missing.append({'os': runner, 'target': target})
            summary(f'- {args.product}/{target}: build required (no retained trusted artifact)')
    write_outputs(version=tag.split('/v', 1)[1], build_required=str(bool(missing)).lower(),
                  matrix=json.dumps({'include': missing}, separators=(',', ':')))


if __name__ == '__main__':
    main()
