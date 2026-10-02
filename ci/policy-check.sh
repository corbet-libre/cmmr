#!/usr/bin/env bash
set -euo pipefail
: "${CI:?CI only}"
python3 .github/check-first-party.py --archive
cargo deny --version
cargo deny --locked check advisories
