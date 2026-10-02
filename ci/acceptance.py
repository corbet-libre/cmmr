#!/usr/bin/env python3
"""Run the fixed complete gate union against the resolver's one candidate lock."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import shutil
import tempfile

if os.environ.get('CI') != 'crow' or os.environ.get('CHECKS'):
    raise SystemExit('Fixed Crow acceptance only')
root = Path(os.environ['CI_ACCEPTANCE_ARTIFACTS']).resolve(strict=True)
source = Path.cwd().resolve()
if root == source or source in root.parents:
    raise SystemExit('Acceptance artifacts must be outside the immutable source')
phases_path = root / 'phases.json'
if phases_path.exists():
    raise SystemExit('Acceptance phases already exist')

def lock_digest():
    return hashlib.sha256(Path('Cargo.lock').read_bytes()).hexdigest()

expected = lock_digest()
with (root / 'resolved-dependency-metadata.json').open('x') as out:
    subprocess.run(['cargo', 'metadata', '--locked', '--all-features', '--format-version', '1'],
                   stdout=out, check=True)
subprocess.run(['python3', '.github/check-first-party.py', '--archive', '--metadata',
                str(root / 'resolved-dependency-metadata.json')], check=True)
subprocess.run(['python3', 'ci/wasm-runner-acquire.py'], check=True)
runner = json.loads((root / 'wasm-runner.json').read_text())
os.environ['CMEET_WASM_BINDGEN_BIN_DIR'] = str(root / 'wasm-runner/bin')
os.environ['CMEET_WASM_BINDGEN_CLI_SHA256'] = runner['binaries']['wasm-bindgen']
os.environ['CMEET_WASM_BINDGEN_RUNNER_SHA256'] = runner['binaries']['wasm-bindgen-test-runner']
os.environ['CMEET_WASM_TEST_LOCK_SHA256'] = expected
os.environ['WASM_PACK_CACHE'] = tempfile.mkdtemp(prefix='wasm-pack.', dir=os.environ['TMPDIR'])
os.environ['PATH'] = str(root / 'wasm-runner/bin') + os.pathsep + os.environ['PATH']
with (root / 'tool-versions.txt').open('a') as out:
    subprocess.run(['python3', 'ci/wasm-runner-version.py',
                    str(root / 'resolved-dependency-metadata.json')], stdout=out, check=True)
# Maintained wasm-bindgen runner accepts an external W3C capability file.
# The browser binary and driver came from the separately verified shared setup.
capabilities = {'goog:chromeOptions': {
    'binary': os.environ['CHROME_BIN'],
    'args': ['--no-sandbox', '--disable-dev-shm-usage']}}
(root / 'webdriver.json').write_text(json.dumps(capabilities, indent=2) + '\n')
os.environ['WASM_BINDGEN_TEST_WEBDRIVER_JSON'] = str(root / 'webdriver.json')
phases = []
phases_path.write_text('[]\n')
for name, script in [('native', 'native-check'), ('wasm', 'wasm-check'),
                     ('browser', 'browser-check'), ('coverage', 'coverage-check'),
                     ('dependency-policy', 'policy-check')]:
    if lock_digest() != expected:
        raise SystemExit('Candidate lock changed before ' + name)
    log = root / (name + '.log')
    with log.open('x') as out:
        result = subprocess.run(['bash', 'ci/' + script + '.sh'], stdout=out,
                                stderr=subprocess.STDOUT)
    print(log.read_text(), flush=True)
    if name == 'browser':
        inputs = Path(os.environ['WASM_PACK_CACHE']) / 'wasm-pack-inputs.json'
        if inputs.is_file():
            shutil.copyfile(inputs, root / 'wasm-pack-inputs.json')
    after = lock_digest()
    if result.returncode or after != expected:
        (root / 'failure.json').write_text(json.dumps({
            'phase': name, 'exit_code': result.returncode,
            'lock_before': expected, 'lock_after': after}) + '\n')
        raise SystemExit(result.returncode or 1)
    phases.append({'name': name, 'status': 'passed', 'lock_before': expected,
                   'lock_after': after, 'log': log.name})
    phases_path.write_text(json.dumps(phases, indent=2) + '\n')
