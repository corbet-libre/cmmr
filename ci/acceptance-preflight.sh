#!/usr/bin/env bash
set -euo pipefail
[ "${CI:-}" = crow ]
: "${CI_ACCEPTANCE_ARTIFACTS:?}"
export PYTHONDONTWRITEBYTECODE=1
test ! -e "$CI_ACCEPTANCE_ARTIFACTS/tool-versions.txt"
{
  rustc -Vv
  cargo --version
  cargo deny --version
  wasm-pack --version
  curl --version
  tar --version
  chromedriver --version
  chromium --version
  command -v wasm-ld
  stable_wasm_lib=$(rustc --print target-libdir --target wasm32-unknown-unknown)
  test -d "$stable_wasm_lib"
  compgen -G "$stable_wasm_lib/libstd-*.rlib" > /dev/null
  printf 'Stable Wasm stdlib: %s\n' "$stable_wasm_lib"
  ( . ci/coverage-env.sh; rustc -Vv; cargo llvm-cov --version; "$LLVM_COV" --version; "$LLVM_PROFDATA" --version )
} > "$CI_ACCEPTANCE_ARTIFACTS/tool-versions.txt"
cat "$CI_ACCEPTANCE_ARTIFACTS/tool-versions.txt"
python3 .github/check-first-party.py --archive
cargo metadata --locked --all-features --format-version 1 > "$CI_ACCEPTANCE_ARTIFACTS/baseline-dependency-metadata.json"
python3 .github/check-first-party.py --archive --metadata "$CI_ACCEPTANCE_ARTIFACTS/baseline-dependency-metadata.json"
