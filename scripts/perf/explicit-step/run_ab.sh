#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): interleaved A/B series
# of the explicit-step timing harnesses (docs/BENCHMARKS.md, "Explicit dynamics").
#   run_ab.sh OUT.jsonl ROUNDS "LABEL..." "MESH:STEPS[:fd]..." [PHASES]
# LABEL selects the program:
#   cpp-<config>  the C++ reference driver $CPP_BIN_DIR/explicit_step_timing_<config>
#                 in the oracle image (cpp_run.sh; phases step, fint, stress);
#   coh-*         akantu-cohesive's cohesive_step_timing ($BIN_DIR/<LABEL>; phases
#                 check, step; mode ss = --small-strain);
#   anything else akantu-solid's explicit_step_timing ($BIN_DIR/<LABEL>).
# Meshes: $MESH_DIR/<MESH>.msh (default ~/perf/meshes); BIN_DIR default
# ~/perf/bin, CPP_BIN_DIR default ~/perf/cpp-bin. Every round runs every case
# with every program (one process per run: warm-up 3 steps, then one
# repetition of each phase); the program order alternates between rounds.
# One run at a time.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="$1"; rounds="$2"; read -r -a labels <<<"$3"; read -r -a cases <<<"$4"
phases="${5:-step,fint,stress,grad,btd,integ,asm}"
bindir="${BIN_DIR:-$HOME/perf/bin}"; meshdir="${MESH_DIR:-$HOME/perf/meshes}"
cppdir="${CPP_BIN_DIR:-$HOME/perf/cpp-bin}"
for r in $(seq 1 "$rounds"); do
  order=("${labels[@]}")
  if [ $((r % 2)) -eq 0 ]; then
    order=(); for ((i = ${#labels[@]} - 1; i >= 0; i--)); do order+=("${labels[$i]}"); done
  fi
  for c in "${cases[@]}"; do
    IFS=: read -r mesh steps fd <<<"$c"
    mode=ss; [ "${fd:-}" = fd ] && mode=fd
    for l in "${order[@]}"; do
      tag="$l|$mesh|$mode|r$r"
      case "$l" in
        cpp-*)
          f=0; [ "$mode" = fd ] && f=1
          "$here/cpp_run.sh" "$cppdir" "${l#cpp-}" "$meshdir" "$mesh" "$steps" 1 3 "$f" "$tag" >> "$out" ;;
        coh-*)
          extra=(); [ "$mode" = ss ] && extra=(--small-strain)
          "$bindir/$l" --mesh "$meshdir/$mesh.msh" --steps "$steps" --reps 1 --warmup 3 \
            --label "$tag" "${extra[@]}" >> "$out" ;;
        *)
          extra=(); [ "$mode" = fd ] && extra=(--fd)
          "$bindir/$l" --mesh "$meshdir/$mesh.msh" --steps "$steps" --reps 1 --warmup 3 \
            --phases "$phases" --label "$tag" "${extra[@]}" >> "$out" ;;
      esac
    done
  done
done
