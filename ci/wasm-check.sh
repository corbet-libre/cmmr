#!/usr/bin/env bash
set -euo pipefail
[ "${CI:-}" = crow ]
: "${CI_ACCEPTANCE_ARTIFACTS:?}"
python3 ci/wasm-runner-version.py "$CI_ACCEPTANCE_ARTIFACTS/resolved-dependency-metadata.json"
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
export WASM_BINDGEN_USE_BROWSER=1 WASM_BINDGEN_TEST_ONLY_WEB=1
export WASM_BINDGEN_TEST_TIMEOUT=120
cargo check --locked --target wasm32-unknown-unknown
cargo test --locked --target wasm32-unknown-unknown --features conformance --test contract --test validation
