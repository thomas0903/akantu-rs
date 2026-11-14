#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): timing series of the
# explicit-step harnesses with the order of the programs ROTATED cyclically
# per round (round r starts with program r mod N), one process per run,
# nothing else running; the load average and CPU model are recorded per
# round. The method of the 2026-11-12 merged-tree series in docs/BENCHMARKS.md
# (docs/benchmarks/explicit-step/final/); summarise with
# summarize_timing.py.
#   timing_rotated.sh OUT.jsonl ROUNDS "LABEL..." "MESH:STEPS[:fd]..."
# LABEL cpp-<config> runs $CPP_BIN_DIR/explicit_step_timing_<config> in the
# oracle image (cpp_run.sh); any other LABEL runs $BIN_DIR/<LABEL>. Meshes:
# $MESH_DIR/<MESH>.msh. Defaults as in run_ab.sh: ~/perf/bin, ~/perf/cpp-bin,
# ~/perf/meshes.
set -u
X="$(cd "$(dirname "$0")" && pwd)"
bindir="${BIN_DIR:-$HOME/perf/bin}"; meshdir="${MESH_DIR:-$HOME/perf/meshes}"
cppdir="${CPP_BIN_DIR:-$HOME/perf/cpp-bin}"
out=$1 rounds=$2; read -r -a labels <<< "$3"; read -r -a cases <<< "$4"
n=${#labels[@]}
for r in $(seq 1 "$rounds"); do
  order=(); for ((i = 0; i < n; i++)); do order+=("${labels[$(( (i + r - 1) % n ))]}"); done
  echo "{\"kind\":\"round\",\"round\":$r,\"order\":\"${order[*]}\",\"load\":\"$(cut -d' ' -f1-3 /proc/loadavg)\",\"cpu\":\"$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | xargs)\",\"utc\":\"$(date -u +%FT%TZ)\"}" >> "$out"
  for c in "${cases[@]}"; do
    IFS=: read -r mesh steps fd <<< "$c"
    mode=ss; [ "${fd:-}" = fd ] && mode=fd
    for l in "${order[@]}"; do
      tag="$l|$mesh|$mode|r$r"
      case "$l" in
        cpp-*)
          f=0; [ "$mode" = fd ] && f=1
          "$X/cpp_run.sh" "$cppdir" "${l#cpp-}" "$meshdir" "$mesh" "$steps" 1 3 "$f" "$tag" >> "$out" ;;
        *)
          extra=(); [ "$mode" = fd ] && extra=(--fd)
          "$bindir/$l" --mesh "$meshdir/$mesh.msh" --steps "$steps" --reps 1 --warmup 3 \
            --phases step,fint,stress --label "$tag" "${extra[@]}" >> "$out" ;;
      esac
    done
  done
done
echo "done"
