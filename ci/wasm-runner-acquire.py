#!/usr/bin/env python3
"""Acquire the candidate-lock runner from the maintained upstream release."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile

if os.environ.get('CI') != 'crow':
    raise SystemExit('Runner acquisition is CI only')
root = Path(os.environ['CI_ACCEPTANCE_ARTIFACTS']).resolve(strict=True)
packages = json.loads((root / 'resolved-dependency-metadata.json').read_text())['packages']
versions = {p['version'] for p in packages if p['name'] == 'wasm-bindgen'}
if len(versions) != 1:
    raise SystemExit('Exactly one candidate wasm-bindgen schema is required')
version = versions.pop()
if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-[a-zA-Z0-9.-]+)?', version):
    raise SystemExit('Invalid runner version')
stem = 'wasm-bindgen-' + version + '-x86_64-unknown-linux-musl'
name = stem + '.tar.gz'
url = 'https://github.com/wasm-bindgen/wasm-bindgen/releases/download/' + version + '/'
destination = root / 'wasm-runner'
destination.mkdir()
for filename in (name, name + '.sha256sum'):
    subprocess.run(['curl', '--user-agent', 'ci-artifact-fetch/1.0', '--fail', '--location',
                    '--proto', '=https', '--proto-redir', '=https', '--retry', '3',
                    '--output', str(destination / filename), url + filename], check=True)
archive = destination / name
expected = (destination / (name + '.sha256sum')).read_text().split()[0]
with archive.open('rb') as stream:
    actual = hashlib.file_digest(stream, 'sha256').hexdigest()
if not re.fullmatch('[0-9a-f]{64}', expected) or actual != expected:
    raise SystemExit('Upstream runner checksum mismatch')
binary_dir = destination / 'bin'
binary_dir.mkdir()
binaries = {}
with tarfile.open(archive, 'r:gz') as package:
    for binary in ('wasm-bindgen', 'wasm-bindgen-test-runner'):
        members = [m for m in package.getmembers() if m.name == stem + '/' + binary]
        if len(members) != 1 or not members[0].isfile() or not 0 < members[0].size <= 128 * 1024 * 1024:
            raise SystemExit('Missing or invalid runner binary')
        contents = package.extractfile(members[0]).read()
        path = binary_dir / binary
        path.write_bytes(contents)
        path.chmod(0o755)
        binaries[binary] = hashlib.sha256(contents).hexdigest()
receipt = {'version': version, 'url': url + name, 'archive_sha256': actual,
           'binaries': binaries, 'metadata': 'resolved-dependency-metadata.json'}
(root / 'wasm-runner.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))
