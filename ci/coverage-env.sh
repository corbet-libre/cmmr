#!/usr/bin/env bash
# Source only inside a coverage subprocess, preserving the stable build compiler.
[ "${CI:-}" = crow ]
coverage_setup=/nix/store/a7wahr975zknixs667f1vl68r4jpm5kx-cmty-coverage-tools/share/cmty-coverage-tools/setup.sh
printf '%s  %s\n' 212a02eaa479a023b9ef94d8907269de2c6f1c8dce6337f4f6904b8f97ba6f96 "$coverage_setup" | sha256sum --check --strict
. "$coverage_setup"
rustc -V | grep -- '-nightly'
test -x "$LLVM_COV"
test -x "$LLVM_PROFDATA"
