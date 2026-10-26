#!/usr/bin/env python3
"""Prepare version PRs and resolve an immutable release identity. Publishing is separate."""

import argparse
import copy
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
TAURI = Path('apps/shell/src-tauri/tauri.conf.json')
MANIFESTS = Path('apps/desktop/manifests')
VERSION = re.compile(
    r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)'
    r'(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?'
    r'(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?'
)
REQUESTED_VERSION = re.compile(
    r'(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)'
    r'(?:-(?:alpha|beta|rc)\.(?:0|[1-9][0-9]*))?'
)


def version_key(text, *, requested=False):
    if not isinstance(text, str) or len(text) > 128:
        raise ValueError('version must be a string of at most 128 characters')
    match = VERSION.fullmatch(text)
    if not match or (requested and not REQUESTED_VERSION.fullmatch(text)):
        raise ValueError(f'unsupported version: {text}; use X.Y.Z or X.Y.Z-alpha.N/beta.N/rc.N')
    prerelease = match[4]
    identifiers = []
    for part in prerelease.split('.') if prerelease else []:
        if part.isdigit():
            if len(part) > 1 and part.startswith('0'):
                raise ValueError('numeric prerelease identifiers cannot have leading zeros')
            identifiers.append((0, int(part)))
        else:
            identifiers.append((1, part))
    return (*map(int, match.group(1, 2, 3)), 0 if prerelease else 1, tuple(identifiers))


def run(*args, check=True):
    result = subprocess.run(args, cwd=ROOT, text=True, capture_output=True, timeout=120)
    if check and result.returncode:
        raise RuntimeError(f'{args[0]} {args[1]} failed: {result.stderr.strip()}')
    return result


def command(*args):
    return run(*args).stdout.strip()


def read(path):
    return (ROOT / path).read_text(encoding='utf-8')


def parse_toml(path):
    return tomllib.loads(read(path))


def workspace_packages():
    workspace = parse_toml('Cargo.toml')['workspace']
    manifests = set()
    excluded = set()
    for pattern in workspace.get('exclude', []):
        excluded.update(path.resolve() for path in ROOT.glob(pattern))
    for pattern in workspace['members']:
        for directory in ROOT.glob(pattern):
            if directory.resolve() in excluded:
                continue
            directory.resolve().relative_to(ROOT)
            manifest = directory / 'Cargo.toml'
            if not manifest.is_file():
                raise ValueError(f'workspace member lacks Cargo.toml: {directory.relative_to(ROOT)}')
            manifests.add(manifest)
    inherited = set()
    for path in manifests:
        package = tomllib.loads(path.read_text(encoding='utf-8'))['package']
        if package.get('version') == {'workspace': True}:
            if package['name'] in inherited:
                raise ValueError(f'duplicate workspace package: {package["name"]}')
            inherited.add(package['name'])
    if 'anvil' not in inherited:
        raise ValueError('anvil must inherit the Rust workspace version')
    return inherited


def cli_version():
    current = parse_toml('Cargo.toml')['workspace']['package']['version']
    version_key(current, requested=True)
    names = workspace_packages()
    packages = parse_toml('Cargo.lock')['package']
    for name in names:
        matches = [p for p in packages if p['name'] == name and 'source' not in p]
        if len(matches) != 1 or matches[0]['version'] != current:
            raise ValueError(f'Cargo.lock workspace package version mismatch: {name}')
    return current, names


def desktop_files():
    manifests = sorted((ROOT / MANIFESTS).glob('*.json'))
    if not manifests:
        raise ValueError('no platform App manifests found')
    files = [TAURI, *(path.relative_to(ROOT) for path in manifests)]
    values = {path: json.loads(read(path)) for path in files}
    current = values[TAURI]['version']
    version_key(current, requested=True)
    for path, value in values.items():
        if value.get('version') != current:
            raise ValueError(f'Desktop version mismatch: {path}')
        if path != TAURI and (value.get('app_id') != 'anvil.desktop' or value.get('target') != path.stem):
            raise ValueError(f'App manifest identity mismatch: {path}')
    return current, values


def product_version(product):
    return desktop_files()[0] if product == 'desktop' else cli_version()[0]


def tag_versions(product):
    prefix = f'{product}/v'
    tags = command('git', 'tag', '--list', prefix + '*').splitlines()
    return [(tag, version_key(tag.removeprefix(prefix))) for tag in tags]


def new_version(product, requested):
    key = version_key(requested, requested=True)
    current = product_version(product)
    if key <= version_key(current):
        raise ValueError(f'{product} version must exceed {current}')
    for tag, previous in tag_versions(product):
        if key <= previous:
            raise ValueError(f'new version must exceed existing product tag {tag}')
    return current


def replace_version_line(text, current, requested):
    pattern = re.compile(r'(?m)^(\s*version\s*=\s*)"' + re.escape(current) + r'"(?=\s*(?:#.*)?$)')
    changed, count = pattern.subn(lambda m: m[1] + json.dumps(requested), text)
    if count != 1:
        raise ValueError('cannot uniquely locate the expected TOML version assignment')
    return changed


def version_changes(product, requested):
    current = new_version(product, requested)
    if product == 'desktop':
        _, values = desktop_files()
        changes = {}
        pattern = re.compile(r'("version"\s*:\s*)"' + re.escape(current) + r'"')
        for path, value in values.items():
            changed, count = pattern.subn(lambda m: m[1] + json.dumps(requested), read(path))
            expected = {**value, 'version': requested}
            if count != 1 or json.loads(changed) != expected:
                raise ValueError(f'cannot precisely update Desktop version: {path}')
            changes[path] = changed
        return current, changes

    _, names = cli_version()
    manifest = read('Cargo.toml')
    section = re.compile(r'(?ms)^\[workspace\.package\][^\n]*\n.*?(?=^\[|\Z)')
    manifest, count = section.subn(lambda m: replace_version_line(m[0], current, requested), manifest)
    expected_manifest = copy.deepcopy(parse_toml('Cargo.toml'))
    expected_manifest['workspace']['package']['version'] = requested
    if count != 1 or tomllib.loads(manifest) != expected_manifest:
        raise ValueError('workspace manifest update changed unexpected fields')

    old_lock = parse_toml('Cargo.lock')
    changed_names = set()

    def update_package(match):
        block = match[0]
        package = tomllib.loads(block)['package'][0]
        if package['name'] in names and 'source' not in package:
            changed_names.add(package['name'])
            return replace_version_line(block, current, requested)
        return block

    lock = re.sub(r'(?ms)^\[\[package\]\]\n.*?(?=^\[\[package\]\]|\Z)', update_package, read('Cargo.lock'))
    # Unqualified references remain intact. Only exact name+version references to local
    # workspace packages may change; qualified registry/git identities stay untouched.
    for name in sorted(names):
        pattern = re.compile(r'(?m)^(\s*)"' + re.escape(f'{name} {current}') + r'"(?=,?\s*$)')
        lock = pattern.sub(lambda m: m[1] + json.dumps(f'{name} {requested}'), lock)
    expected_lock = copy.deepcopy(old_lock)
    references = {f'{name} {current}': f'{name} {requested}' for name in names}
    for package in expected_lock['package']:
        if package['name'] in names and 'source' not in package:
            package['version'] = requested
        if 'dependencies' in package:
            package['dependencies'] = [references.get(dep, dep) for dep in package['dependencies']]
    if changed_names != names or tomllib.loads(lock) != expected_lock:
        raise ValueError('lockfile update changed dependencies or unexpected fields')
    return current, {Path('Cargo.toml'): manifest, Path('Cargo.lock'): lock}


def github_context():
    if os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('GITHUB_EVENT_NAME') != 'workflow_dispatch':
        raise ValueError('this action must run through a GitHub Actions manual dispatch')
    if os.environ.get('GITHUB_REF') != 'refs/heads/main':
        raise ValueError('this action must run on main')
    repository = os.environ.get('GITHUB_REPOSITORY', '')
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
        raise ValueError('invalid GITHUB_REPOSITORY')
    sha = command('git', 'rev-parse', 'HEAD')
    if sha != os.environ.get('GITHUB_SHA'):
        raise ValueError('checkout does not match the workflow commit SHA')
    if command('git', 'status', '--porcelain'):
        raise ValueError('release checkout must be clean')
    run('git', 'fetch', 'origin', 'main', '--tags')
    run('git', 'merge-base', '--is-ancestor', sha, 'origin/main')
    return repository, sha


def summary(text):
    print(text)
    if path := os.environ.get('GITHUB_STEP_SUMMARY'):
        with open(path, 'a', encoding='utf-8') as stream:
            stream.write(text + '\n')


def write_outputs(**values):
    if path := os.environ.get('GITHUB_OUTPUT'):
        with open(path, 'a', encoding='utf-8') as stream:
            for key, value in values.items():
                if '\n' in str(value) or '\r' in str(value):
                    raise ValueError('invalid multiline workflow output')
                stream.write(f'{key}={value}\n')


def prepare(product, requested):
    repository, base = github_context()
    if base != command('git', 'rev-parse', 'origin/main'):
        raise ValueError('main advanced before preparation; rerun on the latest main')
    current, changes = version_changes(product, requested)
    branch = f'release/{product}-v{requested}'
    location = f'https://github.com/{repository}/tree/{branch}'
    local = run('git', 'show-ref', '--verify', '--quiet', f'refs/heads/{branch}', check=False)
    if local.returncode not in (0, 1):
        raise RuntimeError('cannot inspect local release branch')
    remote = run('git', 'ls-remote', '--exit-code', '--heads', 'origin', f'refs/heads/{branch}', check=False)
    if remote.returncode not in (0, 2):
        raise RuntimeError('cannot inspect remote release branch')
    if local.returncode == 0 or remote.returncode == 0:
        raise ValueError(f'release branch already exists; inspect it instead of overwriting: {location}')

    run('git', 'switch', '-c', branch)
    for path, text in changes.items():
        (ROOT / path).write_text(text, encoding='utf-8')
    if product_version(product) != requested:
        raise ValueError('written product version does not match the request')
    run('git', 'diff', '--check')
    changed_files = set(command('git', 'diff', '--name-only').splitlines())
    if changed_files != set(map(str, changes)):
        raise ValueError('version preparation modified an unexpected file set')
    run('git', 'config', 'user.name', 'github-actions[bot]')
    run('git', 'config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com')
    run('git', 'add', '--', *map(str, changes))
    run('git', 'commit', '-m', f'chore({product}): prepare {requested}')
    head = command('git', 'rev-parse', 'HEAD')
    summary(f'## Version preparation\nProduct: **{product}**\nVersion: **{current} → {requested}**\n'
            f'Base: `{base}`\nCommit: `{head}`\nBranch: {location}\n')
    write_outputs(branch=branch, sha=head, version=requested, product=product)
    run('git', 'push', 'origin', f'HEAD:refs/heads/{branch}')

    files = '\n'.join(f'- `{path}`' for path in changes)
    related = (parse_toml('Cargo.toml')['workspace']['package']['version'] if product == 'desktop'
               else json.loads(read(TAURI))['version'])
    related_name = 'Bundled Runtime' if product == 'desktop' else 'Desktop (independent version)'
    artifacts = ('Shell installers, App update packages and SHA256SUMS' if product == 'desktop'
                 else 'CLI archives, Runtime binaries, SHA256SUMS and update indexes')
    body = (f'## Version preparation\n\nPrepare **{product} {requested}** from `{base}`.\n\n'
            f'Version: `{current}` → `{requested}`.\n{related_name}: `{related}`.\n\n'
            f'Expected artifacts: {artifacts}.\n'
            'Targets: Windows x64, macOS Apple Silicon / Intel, Linux x64 (current release matrix).\n\n'
            f'## Files\n\n{files}\n\n'
            '## Verification\n\nVersion file consistency and `git diff --check` checked by preparation.\n'
            'Rust CI is explicitly dispatched on this branch; review and merge after it succeeds.\n'
            'Publishing is a separate approved action on main.\n'
            'This preparation does not create tags, publish installers, or update feeds.\n\n'
            '## Release notes\n\nSummarize the user-visible changes before merging.\n')
    failures = []
    with tempfile.TemporaryDirectory(prefix='anvil-release-pr-') as temporary:
        body_path = Path(temporary) / 'body.md'
        body_path.write_text(body, encoding='utf-8')
        try:
            url = command('gh', 'pr', 'create', '--repo', repository, '--base', 'main', '--head', branch,
                          '--title', f'chore({product}): prepare {requested}', '--body-file', str(body_path))
            summary(f'Version PR: {url}\n')
        except (RuntimeError, subprocess.TimeoutExpired, OSError) as error:
            failures.append(str(error))
    # Token-created PRs do not trigger pull_request. Dispatch even if PR creation failed,
    # so the pushed version proposal can still be reviewed and recovered manually.
    try:
        run('gh', 'workflow', 'run', 'rust.yml', '--repo', repository, '--ref', branch)
        summary('Rust CI dispatched to the version branch.\n')
    except (RuntimeError, subprocess.TimeoutExpired, OSError) as error:
        failures.append(str(error))
    if failures:
        raise RuntimeError(f'Version branch preserved at {location}; create the PR or dispatch Rust CI manually. '
                           + '; '.join(failures))


def approval_policy(repository, environment):
    if not re.fullmatch(r'[A-Za-z0-9_-]+', environment):
        raise ValueError('invalid approval environment name')
    path = f'repos/{repository}/environments/{environment}'
    try:
        policy = json.loads(command('gh', 'api', path))
        branches = json.loads(command('gh', 'api', path + '/deployment-branch-policies'))
    except RuntimeError as error:
        raise ValueError(f'Cannot read protected Environment {environment}; configure it and grant read access before releasing') from error
    owner = repository.split('/')[0].lower()
    reviewers = [rule for rule in policy.get('protection_rules', [])
                 if rule.get('type') == 'required_reviewers']
    valid_reviewers = any(rule.get('prevent_self_review') is False
                         and any(entry.get('type') == 'User'
                                 and entry.get('reviewer', {}).get('login', '').lower() == owner
                                 for entry in rule.get('reviewers', []))
                         for rule in reviewers)
    branch_policy = policy.get('deployment_branch_policy') or {}
    rules = branches.get('branch_policies', [])
    main_only = (branch_policy.get('protected_branches') is False
                 and branch_policy.get('custom_branch_policies') is True
                 and len(rules) == 1 and rules[0].get('name') == 'main'
                 and rules[0].get('type') == 'branch')
    if not valid_reviewers or policy.get('can_admins_bypass') is not False or not main_only:
        raise ValueError(f'Environment {environment} must require owner approval, allow self-review, '
                         'disable admin bypass and allow only the main branch')


def resolve(product, environment=None):
    repository, sha = github_context()
    current = product_version(product)
    key = version_key(current, requested=True)
    tag = f'{product}/v{current}'
    for existing, previous in tag_versions(product):
        if previous > key or (previous == key and existing != tag):
            raise ValueError(f'release is superseded by product tag {existing}')
    exists = run('git', 'show-ref', '--verify', '--quiet', f'refs/tags/{tag}', check=False)
    if exists.returncode == 0 and command('git', 'rev-parse', f'{tag}^{{commit}}') != sha:
        raise ValueError('existing release tag points to another commit; tags must never move')
    if exists.returncode not in (0, 1):
        raise RuntimeError('cannot inspect release tag')
    path = f'repos/{repository}/actions/workflows/rust.yml/runs?head_sha={sha}&per_page=100'
    runs = json.loads(command('gh', 'api', path))['workflow_runs']
    trusted = [entry for entry in runs if entry['head_sha'] == sha and entry['head_branch'] == 'main'
               and entry['event'] in ('push', 'workflow_dispatch')]
    latest = max(trusted, key=lambda entry: entry['id']) if trusted else None
    if not latest or latest['status'] != 'completed' or latest['conclusion'] != 'success':
        raise ValueError('latest trusted Rust CI on this exact main commit must succeed; run or finish Rust CI first')
    if environment:
        approval_policy(repository, environment)
    write_outputs(product=product, version=current, tag=tag, sha=sha, ci_url=latest['html_url'])
    summary(f'## Release identity\nProduct: **{product}**\nVersion: **{current}**\n'
            f'Tag: `{tag}`\nCommit: `{sha}`\nCI: {latest["html_url"]}\n'
            f'Approval environment: `{environment or "not checked"}`\n'
            'Targets: Linux x64, macOS Intel / Apple Silicon, Windows x64.\n'
            + ('Feeds: `desktop-updates-stable`, `shell-updates-stable`.\n' if product == 'desktop'
               else 'Feed: `runtime-updates`.\n')
            + 'Approval, tagging, signing and publishing follow identity validation.\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('check', 'plan', 'prepare', 'resolve'))
    parser.add_argument('--product', required=True, choices=('desktop', 'cli'))
    parser.add_argument('--version', help='required for plan and prepare')
    parser.add_argument('--environment', help='protected Environment to check during resolve')
    args = parser.parse_args()
    if args.environment and args.action != 'resolve':
        parser.error('--environment is only accepted for resolve')
    if args.action in ('plan', 'prepare'):
        if not args.version:
            parser.error('--version is required for plan and prepare')
    elif args.version:
        parser.error('--version is only accepted for plan and prepare')
    if args.action == 'check':
        print(json.dumps({'product': args.product, 'version': product_version(args.product)}, indent=2))
    elif args.action == 'plan':
        current, changes = version_changes(args.product, args.version)
        print(json.dumps({'product': args.product, 'current': current, 'version': args.version,
                          'files': [str(path) for path in changes]}, indent=2))
    elif args.action == 'prepare':
        prepare(args.product, args.version)
    else:
        resolve(args.product, args.environment)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, RuntimeError, subprocess.TimeoutExpired, OSError) as error:
        raise SystemExit(str(error)) from error
