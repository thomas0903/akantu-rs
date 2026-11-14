#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness: repeat the runs of a campaign that
# collect.py reports as invalid or missing. The typical cause is Akantu's
# MUMPS memory-retry path (error -9 during the static factorisation), which
# returns a wrong static solution (UD-216, docs/BENCHMARKS.md); a nonzero exit
# status, a malformed Chrono row or inconsistent rank output also make a run
# invalid. The invalid attempt is kept as rep<k>.invalid-<n> (collect.py
# reports the attempts and their cost); at most --attempts rounds.
#
# Usage: scripts/rerun-invalid.sh <campaign> [--attempts 3] [--memory 8g]
#
# Every field read back from the collected records is validated again before
# use; nothing is evaluated as shell code.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
[ $# -ge 1 ] || { echo "usage: $0 <campaign> [--attempts N] [--memory 8g]" >&2; exit 2; }
campaign="$1"; shift
perf_check_campaign "${campaign}" || exit 2
attempts=3
while [ $# -gt 0 ]; do
  case "$1" in
    --attempts) [ $# -ge 2 ] || exit 2; attempts="$2"; shift 2 ;;
    --memory) [ $# -ge 2 ] || exit 2; PERF_MEMORY="$2"; shift 2 ;;
    *) echo "ERROR: unknown argument $1" >&2; exit 2 ;;
  esac
done
perf_check_posint attempts "${attempts}" || exit 2
perf_set_run_opts
set -f  # the extra parameters are split on spaces below: no globbing

for attempt in $(seq 1 "${attempts}"); do
  python3 -I "${PERF_SCRIPTS_DIR}/collect.py" --allow-incomplete "${campaign}" > /dev/null
  list="$(python3 -I - "${PERF_DIR}/results/${campaign}/collection.json" <<'PY'
import json, sys
for r in json.load(open(sys.argv[1]))["rerun"]:
    fields = [r["run_dir"], r["config"], r["ranks"], r["material"], r["mesh"],
              r["max_steps"], r["aka_seed"], r["fill_nodes_to_elements"],
              " ".join(r["extra_parameters"])]
    if any("\t" in str(f) or "\n" in str(f) for f in fields):
        sys.exit("unexpected whitespace in a collected run record")
    print("\t".join(str(f) for f in fields))
PY
)"
  [ -n "${list}" ] || { echo "no invalid or missing runs left in ${campaign}"; break; }
  while IFS=$'\t' read -r rel config ranks material mesh steps seed fnte extras; do
    dir="/perf/runs/${rel}"
    case "${rel}" in "${campaign}"/*) ;; *) echo "ERROR: ${rel} is not in ${campaign}" >&2; exit 2 ;; esac
    perf_check_run_dir /perf/runs "${dir}" || exit 2
    perf_check_config "${config}" || exit 2
    perf_check_posint ranks "${ranks}" || exit 2
    perf_check_material "${material}" || exit 2
    perf_check_mesh_path "${mesh}" || exit 2
    perf_check_posint steps "${steps}" || exit 2
    perf_check_uint seed "${seed}" || exit 2
    perf_check_fnte "${fnte}" || exit 2
    # shellcheck disable=SC2086  # validated plain tokens, globbing off
    set -- ${extras}
    for kv in "$@"; do perf_check_kv "${kv}" || exit 2; done
    echo "$(date '+%H:%M:%S') rerun (attempt ${attempt}) ${dir}"
    perf_docker bash -c '
      set -euo pipefail
      . /harness/scripts/in-container/validate.sh
      perf_check_run_dir /perf/runs "$1" && perf_confine_dir /perf/runs "$1" || exit 2
      if [ -e "$1" ]; then
        n=1; while [ -e "$1.invalid-$n" ]; do n=$((n+1)); done
        mv "$1" "$1.invalid-$n"
      fi' _ "${dir}"
    AKA_SEED="${seed}" AKANTU_RS_FILL_NODES_TO_ELEMENTS="${fnte}" perf_docker \
      bash /harness/scripts/in-container/run-one.sh "${dir}" "${config}" "${ranks}" \
      "${material}" "${mesh}" "${steps}" "$@" \
      || echo "$(date '+%H:%M:%S') run failed again: ${dir}" >&2
  done <<< "${list}"
done
python3 -I "${PERF_SCRIPTS_DIR}/collect.py" "${campaign}"
