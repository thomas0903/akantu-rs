#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness: timed series of the upstream driver.
#
# Usage: scripts/run-series.sh --campaign NAME --h H --steps N
#          [--config default|strict] [--materials "elastic cohesive"]
#          [--ranks "1 2 4 8"] [--reps 3] [--memory 8g]
#          [--fnte upstream|joss] [--seed 1] [-- key=value ...]
#
# --fnte joss: skip Mesh::fillNodesToElements() in the mesh-event handlers, as
#   the JOSS runs did (akantu-v5.0.4.patch), via the driver's interposer;
#   default upstream (oracle behaviour, calls timed).
# --seed: AKA_SEED (Scotch partition seed; default 1, upstream = time-based).
# Every run also gets SCOTCH_PTHREAD_NUMBER=1 (reproducible partitions).
#
# Single node only: every run is one `mpiexec
# --oversubscribe -n N` inside one container on this host; there is no
# hostfile, remote launch or EFA transport. Runs with more ranks than the
# container's CPUs are recorded as oversubscribed and kept out of the timing
# tables by collect.py.
#
# One container per run (cgroup memory peak per run). Repetitions are the
# outer loop (rep 1 of every rank count, then rep 2, ...) so that slow drifts
# of the host load spread over all points. Run directories live in the scratch
# volume: /perf/runs/<campaign>/<variant>/<material>/h<H>/np<N>/rep<k>/. The
# series is recorded first as a plan (/perf/runs/<campaign>/_plans/*.json, the
# list of expected runs) so that collect.py reports missing or invalid
# repetitions as failures. Collect into results/<campaign>/ with
# scripts/collect.py (run by this script at the end, also callable alone).
#
# All arguments are validated before anything runs: campaign
# [A-Za-z0-9][A-Za-z0-9._-]{0,63}, positive counts, h like 0.12, enums, and
# key=value with plain-token values.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

campaign=""; h=""; steps=""; config="default"; materials="elastic cohesive"
ranks="1 2 4 8"; reps=3
fnte="${AKANTU_RS_FILL_NODES_TO_ELEMENTS:-upstream}"; seed="${AKA_SEED:-1}"
need() { [ $# -ge 2 ] || { echo "ERROR: option $1 needs a value" >&2; exit 2; }; }
while [ $# -gt 0 ]; do
  case "$1" in
    --campaign) need "$@"; campaign="$2"; shift 2 ;;
    --h) need "$@"; h="$2"; shift 2 ;;
    --steps) need "$@"; steps="$2"; shift 2 ;;
    --config) need "$@"; config="$2"; shift 2 ;;
    --materials) need "$@"; materials="$2"; shift 2 ;;
    --ranks) need "$@"; ranks="$2"; shift 2 ;;
    --reps) need "$@"; reps="$2"; shift 2 ;;
    --memory) need "$@"; PERF_MEMORY="$2"; shift 2 ;;
    --fnte) need "$@"; fnte="$2"; shift 2 ;;
    --seed) need "$@"; seed="$2"; shift 2 ;;
    --) shift; break ;;
    *) echo "ERROR: unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "${campaign}" ] && [ -n "${h}" ] && [ -n "${steps}" ] \
  || { echo "usage: $0 --campaign NAME --h H --steps N [...]" >&2; exit 2; }
perf_check_campaign "${campaign}" || exit 2
perf_check_h "${h}" || exit 2
perf_check_posint steps "${steps}" || exit 2
perf_check_config "${config}" || exit 2
perf_check_enum_list materials "${materials}" elastic cohesive || exit 2
perf_check_posint_list ranks "${ranks}" || exit 2
perf_check_posint reps "${reps}" || exit 2
perf_check_fnte "${fnte}" || exit 2
perf_check_uint seed "${seed}" || exit 2
seen_keys=" "
for kv in "$@"; do
  perf_check_kv "${kv}" || exit 2
  case "${seen_keys}" in *" ${kv%%=*} "*) echo "ERROR: parameter ${kv%%=*} given twice" >&2; exit 2 ;; esac
  seen_keys="${seen_keys}${kv%%=*} "
done
perf_set_run_opts
export AKANTU_RS_FILL_NODES_TO_ELEMENTS="${fnte}" AKA_SEED="${seed}"

variant="${config}"
[ "${fnte}" = upstream ] || variant="${variant}-fnte_${fnte}"
for kv in "$@"; do [ "${kv}" != "blocked_node=global" ] || variant="${variant}-global"; done

mesh="/perf/meshes/cube-h${h}.msh"
if ! docker run --rm -v "${PERF_VOLUME}:/perf:ro" "${PERF_IMAGE}" test -f "${mesh}"; then
  echo "mesh ${mesh} missing: generating it" >&2
  "${PERF_SCRIPTS_DIR}/gen-meshes.sh" "${h}"
fi

# The plan: one record per expected run (collect.py checks every one).
stamp="$(date -u +%Y%m%dT%H%M%SZ)-$$"
python3 -I - "${campaign}" "${variant}" "${config}" "${h}" "${steps}" "${mesh}" \
  "${fnte}" "${seed}" "${reps}" "${materials}" "${ranks}" "$@" <<'PY' \
  | perf_volume_write "/perf/runs/${campaign}/_plans/${stamp}.json"
import datetime, json, sys
(campaign, variant, config, h, steps, mesh, fnte, seed, reps, materials, ranks), extra = \
    sys.argv[1:12], sys.argv[12:]
runs = []
for rep in range(1, int(reps) + 1):
    for material in materials.split():
        for n in ranks.split():
            runs.append({"run_dir": f"{campaign}/{variant}/{material}/h{h}/np{n}/rep{rep}",
                         "config": config, "material": material, "ranks": int(n),
                         "mesh": mesh, "max_steps": int(steps), "aka_seed": seed,
                         "fill_nodes_to_elements": fnte, "extra_parameters": extra})
print(json.dumps({"format": "cohesive-perf plan v1", "campaign": campaign,
                  "created_utc": datetime.datetime.now(datetime.timezone.utc)
                  .strftime("%Y-%m-%dT%H:%M:%SZ"),
                  "command": "run-series.sh " + " ".join(sys.argv[1:]),
                  "expected_runs": runs}, indent=1))
PY

for rep in $(seq 1 "${reps}"); do
  for material in ${materials}; do
    for n in ${ranks}; do
      dir="/perf/runs/${campaign}/${variant}/${material}/h${h}/np${n}/rep${rep}"
      echo "$(date '+%H:%M:%S') ${campaign} ${variant} ${material} h=${h} np=${n} rep=${rep} steps=${steps}"
      # A failed run is recorded (run.json, exit status) and reported by
      # collect.py; the series goes on with the other points.
      perf_docker bash /harness/scripts/in-container/run-one.sh \
        "${dir}" "${config}" "${n}" "${material}" "${mesh}" "${steps}" "$@" \
        || echo "$(date '+%H:%M:%S') run failed: ${dir} (see collect.py report)" >&2
    done
  done
done

python3 -I "${PERF_SCRIPTS_DIR}/collect.py" "${campaign}"
