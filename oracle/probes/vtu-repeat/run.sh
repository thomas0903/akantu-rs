#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Runs INSIDE the oracle image (host wrapper: oracle/scripts/run-probe.sh
# vtu-repeat <config>). Full-precision VTU comparison of 3 isolated repeats of
# the two 2-rank examples, with AKA_SEED=1 and with AKA_SEED unset.
set -euo pipefail
config="${1:-strict}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
seeded="$(python3 -I "${here}/vtu_repeat.py" --config "${config}" --runs 3 --aka-seed 1 \
  phase_field_parallel-test parallel_2d-test)"
unseeded="$(python3 -I "${here}/vtu_repeat.py" --config "${config}" --runs 3 \
  phase_field_parallel-test parallel_2d-test)"
printf '{"probe": "vtu-repeat", "aka_seed_1": %s, "aka_seed_unset": %s}\n' "${seeded}" "${unseeded}"
