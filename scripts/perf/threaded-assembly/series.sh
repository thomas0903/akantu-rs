#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the interleaved
# series of the threaded matrix assembly (docs/benchmarks/perf-campaign/
# threaded-assembly/README.md): the implicit and set-up cases of the two
# benchmark suites, the base at one thread against the head at 1, 2, 4
# and 8 threads.
#
#   series.sh BIN_DIR OUT_DIR REPS [REP_OFFSET]
#
# BIN_DIR holds base_solid, base_models (the branch point) and head_solid,
# head_models (this branch), all built with the release profile. Writes
# the wrappers BIN_DIR/<name>-t<N> (AKANTU_NUM_THREADS=N, pinned to cores
# 0..N-1, scripts/perf/exec/wrappers.sh), then runs
# scripts/perf/suite/run_ab.sh (rotated order per repetition) for each
# suite into OUT_DIR/solid and OUT_DIR/models. Run it with the machine
# otherwise idle. Summaries: scripts/perf/suite/summarize.py --ref base.
set -euo pipefail
bin="$1" out="$2" reps="$3" offset="${4:-0}"
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
for s in solid models; do
  "$root/scripts/perf/exec/wrappers.sh" "$bin" "base_$s" "1" > /dev/null
  "$root/scripts/perf/exec/wrappers.sh" "$bin" "head_$s" "1 2 4 8" > /dev/null
done
solid="setup_tet4,setup_tet10,setup_hex8,static_linear_eigen,static_newton_plastic,implicit_dynamic,cohesive_intrinsic_implicit"
models="heat_static_tri6,heat_implicit_hex8_kvar,structural_beam3_frame_static,structural_beam3_frame_dynamic,structural_dkt18_plate_static,phasefield_staggered_tri3,phasefield_staggered_tet4,embedded_rebar_tri3,embedded_rebar_tet4,contact_implicit_static_quad4"
for s in solid models; do
  if [ "$s" = solid ]; then cases="$solid"; else cases="$models"; fi
  "$root/scripts/perf/suite/run_ab.sh" --out "$out/$s" --size m --reps "$reps" \
    --rep-offset "$offset" --cases "$cases" --core 0 \
    base="$bin/base_$s-t1" t1="$bin/head_$s-t1" t2="$bin/head_$s-t2" \
    t4="$bin/head_$s-t4" t8="$bin/head_$s-t8"
done
