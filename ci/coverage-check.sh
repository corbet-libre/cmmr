#!/usr/bin/env bash
set -euo pipefail
: "${CI:?CI only}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export RUST_TEST_THREADS="${RUST_TEST_THREADS:-2}"
. ci/coverage-env.sh
if [ -z "${CI_ACCEPTANCE_ARTIFACTS:-}" ]; then
  : "${CARGO_HOME:?}"
  mkdir -p "$CARGO_HOME/ccid-artifacts"
  CI_ACCEPTANCE_ARTIFACTS=$(mktemp -d "$CARGO_HOME/ccid-artifacts/cmmr-coverage.XXXXXXXX")
  export CI_ACCEPTANCE_ARTIFACTS
fi
printf 'Coverage artifacts: %s\n' "$CI_ACCEPTANCE_ARTIFACTS"
cargo llvm-cov --version
rustc -Vv
sha256sum Cargo.lock
cargo llvm-cov --locked --workspace --all-features --branch --no-cfg-coverage --no-cfg-coverage-nightly --ignore-filename-regex '/tests/' --json --output-path "$CI_ACCEPTANCE_ARTIFACTS/coverage.json"
cargo llvm-cov report --locked --branch --ignore-filename-regex '/tests/' --lcov --output-path "$CI_ACCEPTANCE_ARTIFACTS/coverage.lcov"
cargo llvm-cov report --locked --branch --ignore-filename-regex '/tests/' --text > "$CI_ACCEPTANCE_ARTIFACTS/coverage.txt"
python3 .github/check-coverage.py "$CI_ACCEPTANCE_ARTIFACTS/coverage.lcov" "$CI_ACCEPTANCE_ARTIFACTS/coverage.json" "$CI_ACCEPTANCE_ARTIFACTS/coverage.txt"
sha256sum Cargo.lock
