#!/usr/bin/env bash
set -euo pipefail
[ "${CI:-}" = crow ]
[ -z "${CHECKS:-}" ]
: "${CI_ACCEPTANCE_ARTIFACTS:?}"
export PYTHONDONTWRITEBYTECODE=1
python3 ci/acceptance.py
