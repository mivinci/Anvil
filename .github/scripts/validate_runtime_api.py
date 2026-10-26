#!/usr/bin/env python3
"""Compare a built Runtime's API version with the checked-out source contract."""

from pathlib import Path
import re
import subprocess
import sys


def main():
    if len(sys.argv) != 2:
        raise SystemExit('usage: validate_runtime_api.py BINARY')
    root = Path(__file__).resolve().parents[2]
    source = (root / 'crates/anvil/src/serve.rs').read_text(encoding='utf-8')
    versions = re.findall(r'^pub\(crate\) const HTTP_API_VERSION: &str = "([^"]+)";', source, re.MULTILINE)
    if len(versions) != 1 or not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', versions[0]):
        raise SystemExit('Cannot identify a canonical Runtime API version in source')
    result = subprocess.run([str(Path(sys.argv[1]).resolve()), '--serve-api-version'],
                            capture_output=True, text=True, check=True, timeout=30)
    if result.stdout.strip() != versions[0]:
        raise SystemExit('Built Runtime API version does not match the checked-out source')
    print(versions[0])


if __name__ == '__main__':
    try:
        main()
    except (OSError, subprocess.SubprocessError) as error:
        raise SystemExit(f'Cannot validate Runtime API: {error}') from error
