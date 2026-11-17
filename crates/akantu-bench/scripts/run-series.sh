#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: timed series of the Rust cohesive scalability driver
# (crates/akantu-bench, bin cohesive_extrinsic), the Rust counterpart of
# benchmarks/cohesive-perf/scripts/run-series.sh (not a port of Akantu code).
#
# Usage (on an x86_64 Linux host):
#   run-series.sh --bin <cohesive_extrinsic> --meshes <dir with cube-h<h>.msh>
#                 --out <campaign dir> [--h "0.12 0.06"] [--steps 500]
#                 [--reps 3] [--materials "cohesive elastic"]
#                 [--solver eigen|faer] [key=value ...]
#
# Every run gets its own directory <out>/<material>/h<h>/rep<k>/ with the
# input derived from the upstream benchmarks/cohesive-perf/inputs/
# material-<material>.dat exactly as the C++ harness does (only `mesh`,
# `max_steps` and optional user parameters change), stdout (out.txt), stderr
# (err.txt, holds the Chrono CSV), GNU time -v (time.txt: wall, max RSS),
# the driver's chrono.csv/summary*.json/insertions.csv and run.json
# (provenance). Repetitions are interleaved (rep 1 of every point, then
# rep 2, ...). Collect with collect.py.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
bin=""; meshes=""; out=""; hs="0.12"; steps=500; reps=3
materials="cohesive elastic"; solver=""; extra=()
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) bin="$2"; shift 2 ;;
    --meshes) meshes="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    --h) hs="$2"; shift 2 ;;
    --steps) steps="$2"; shift 2 ;;
    --reps) reps="$2"; shift 2 ;;
    --materials) materials="$2"; shift 2 ;;
    --solver) solver="$2"; shift 2 ;;
    *=*) extra+=("$1"); shift ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -x "${bin}" ] || { echo "--bin: ${bin} is not executable" >&2; exit 2; }
[ -d "${meshes}" ] || { echo "--meshes: ${meshes} missing" >&2; exit 2; }
[ -n "${out}" ] || { echo "--out is required" >&2; exit 2; }
case "${out}" in /|"${HOME}"|"${repo}") echo "refusing --out ${out}" >&2; exit 2 ;; esac
mkdir -p "${out}"
bin="$(cd "$(dirname "${bin}")" && pwd)/$(basename "${bin}")"
bin_sha=$(sha256sum "${bin}" | cut -d' ' -f1)
git_head=$(git -C "${repo}" rev-parse HEAD 2>/dev/null || echo unknown)

for rep in $(seq 1 "${reps}"); do
  for h in ${hs}; do
    mesh="${meshes}/cube-h${h}.msh"
    [ -f "${mesh}" ] || { echo "mesh ${mesh} missing" >&2; exit 1; }
    for material in ${materials}; do
      src="${repo}/benchmarks/cohesive-perf/inputs/material-${material}.dat"
      dir="${out}/${material}/h${h}/rep${rep}"
      rm -rf "${dir}"; mkdir -p "${dir}"
      sedargs=(-e "s|^  max_steps = .*|  max_steps = ${steps}|" -e "s|^  mesh = .*|  mesh = ${mesh}|")
      add=""
      for kv in ${extra[@]+"${extra[@]}"}; do
        k="${kv%%=*}"; v="${kv#*=}"
        if grep -qE "^  ${k} = " "${src}"; then
          sedargs+=(-e "s|^  ${k} = .*|  ${k} = ${v}|")
        else
          add="${add}\\n  ${k} = ${v}"
        fi
      done
      [ -z "${add}" ] || sedargs+=(-e "s|^user section \\[\$|&${add}|")
      sed "${sedargs[@]}" "${src}" > "${dir}/input.dat"
      args=(--aka_input_file input.dat)
      [ -z "${solver}" ] || args+=(--rust_sparse_solver "${solver}")
      echo "$(date '+%H:%M:%S') ${material} h=${h} rep${rep}"
      load=$(cut -d' ' -f1 /proc/loadavg)
      status=0
      (cd "${dir}" && /usr/bin/time -v -o time.txt "${bin}" "${args[@]}" > out.txt 2> err.txt) || status=$?
      python3 -I - "${dir}" <<EOF
import json, sys
json.dump({"material": "${material}", "h": "${h}", "rep": ${rep}, "steps": ${steps},
           "mesh": "${mesh}", "solver": "${solver}" or "input",
           "extra": "${extra[*]+${extra[*]}}", "exit_status": ${status},
           "bin": "${bin}", "bin_sha256": "${bin_sha}", "git_head": "${git_head}",
           "load_at_start": "${load}", "host": "(redacted)",
           "cpu": "$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | sed 's/^ //')",
           "nproc": "$(nproc)"}, open(sys.argv[1] + "/run.json", "w"), indent=1)
EOF
      [ "${status}" = 0 ] || echo "  run failed (exit ${status}), see ${dir}/err.txt" >&2
    done
  done
done
