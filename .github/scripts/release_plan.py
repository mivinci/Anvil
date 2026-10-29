#!/usr/bin/env python3
"""Plan changed product versions, then approve/tag the same immutable plan."""
import argparse
import json
import os
import tomllib

import release


def changed_products():
    """Only the exact commit's version changes participate, never old pending tags."""
    parent = release.command('git', 'rev-parse', 'HEAD^')
    before = {
        'desktop': json.loads(release.command('git', 'show', f'{parent}:{release.TAURI}'))['version'],
        'runtime': tomllib.loads(release.command('git', 'show', f'{parent}:Cargo.toml'))['workspace']['package']['version'],
    }
    selected = {}
    for product, old in before.items():
        current = release.product_version(product)
        if current == old:
            continue
        if release.version_key(current, requested=True) <= release.version_key(old):
            raise ValueError(f'{product} version must increase from parent commit')
        selected[product] = current
    return selected


def public_keys(products):
    names = {'RUNTIME_PUBLIC_KEY'}
    if 'desktop' in products:
        names.add('DESKTOP_PUBLIC_KEY')
    for name in names:
        value = os.environ.get(name, '')
        if len(value) != 64 or any(c not in '0123456789abcdefABCDEF' for c in value):
            raise ValueError(f'{name} must be a 32-byte hex public key')


def plan():
    _, sha = release.github_context(releasing=True)
    products = changed_products()
    if not products:
        release.write_outputs(selected='false')
        release.summary('No product version changed in this commit; no release requested.')
        return
    public_keys(products)
    for product, version in products.items():
        release.resolve(product, 'anvil-release')
        if product == 'desktop':
            release.run('python3', '.github/scripts/validate_desktop_release.py', f'desktop/v{version}')
    # resolve writes generic outputs too; these stable outputs describe the whole plan.
    release.write_outputs(selected='true', sha=sha, products=json.dumps(products, separators=(',', ':')),
                          desktop_tag=f'desktop/v{products["desktop"]}' if 'desktop' in products else '',
                          runtime_tag=f'runtime/v{products["runtime"]}' if 'runtime' in products else '')


def approve(products):
    from nacl.signing import SigningKey
    if not products or set(products) - {'desktop', 'runtime'}:
        raise ValueError('invalid approved product selection')
    _, sha = release.github_context(releasing=True)
    actual = changed_products()
    if actual != products:
        raise ValueError('approved products differ from exact commit version changes')
    public_keys(products)
    # Recheck all identities and keys before creating any tag.
    for product in products:
        release.resolve(product, 'anvil-release')
        prefix = 'DESKTOP' if product == 'desktop' else 'RUNTIME'
        try:
            key = SigningKey(bytes.fromhex(os.environ[f'{prefix}_SIGNING_KEY']))
            valid = key.verify_key.encode().hex() == os.environ[f'{prefix}_PUBLIC_KEY'].lower()
        except (KeyError, ValueError, TypeError):
            valid = False
        if not valid:
            raise ValueError(f'{product} signing identity is invalid')
    release.run('git', 'config', 'user.name', 'github-actions[bot]')
    release.run('git', 'config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com')
    tags = []
    for product, version in products.items():
        tag = f'{product}/v{version}'
        result = release.run('git', 'show-ref', '--verify', '--quiet', f'refs/tags/{tag}', check=False)
        if result.returncode == 1:
            release.run('git', 'tag', '-a', tag, sha, '-m', f'Anvil {product} {version}')
        elif result.returncode != 0 or release.command('git', 'rev-parse', f'{tag}^{{commit}}') != sha:
            raise ValueError('existing tag must point to approved SHA')
        tags.append(f'refs/tags/{tag}')
    release.run('git', 'push', '--atomic', 'origin', *tags)
    release.summary(f'Approved release plan: `{json.dumps(products)}` at `{sha}`. Tags verified and pushed.')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('plan', 'approve'))
    args = parser.parse_args()
    try:
        plan() if args.action == 'plan' else approve(json.loads(os.environ['RELEASE_PRODUCTS']))
    except (ValueError, KeyError, RuntimeError, OSError) as error:
        raise SystemExit(str(error)) from error
