#!/usr/bin/env python3
"""Refuse a runner that does not match the single actual resolved Wasm schema."""
import json
import subprocess
import sys
from pathlib import Path

packages = json.loads(Path(sys.argv[1]).read_text())['packages']
versions = {p['version'] for p in packages if p['name'] == 'wasm-bindgen'}
if len(versions) != 1:
    raise SystemExit('Exactly one resolved wasm-bindgen version is required')
expected = versions.pop()
for program in ('wasm-bindgen', 'wasm-bindgen-test-runner'):
    reported = subprocess.check_output([program, '--version'], text=True).strip()
    print(reported)
    if reported.split()[-1] != expected:
        raise SystemExit('Wasm runner differs from the candidate lock')
