#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the A/B series of the
# Eigen SparseLU translation's performance work, run from the root of a
# checkout with the suite binaries of the two commits in the directory BIN
# (before_{solid,models}: the base, final_{solid,models}: the branch head).
#   BIN=DIR docs/benchmarks/perf-campaign/sparselu/series.sh OUT REPS
set -euo pipefail
OUT=${1:?OUT}
REPS=${2:-5}
BIN=${BIN:?BIN: the directory of the suite binaries}
SOLID=static_linear_eigen,static_newton_plastic,implicit_dynamic,cohesive_intrinsic_implicit
MODELS=heat_implicit_hex8_kvar,heat_static_tri6,structural_beam3_frame_static,structural_dkt18_plate_static,phasefield_staggered_tri3,phasefield_staggered_tet4,contact_implicit_static_quad4,embedded_rebar_tet4
scripts/perf/suite/run_ab.sh --out "$OUT/solid" --size m --reps "$REPS" --cases "$SOLID" --core 2 \
  before="$BIN/before_solid" after="$BIN/final_solid"
scripts/perf/suite/run_ab.sh --out "$OUT/models" --size m --reps "$REPS" --cases "$MODELS" --core 2 \
  before="$BIN/before_models" after="$BIN/final_models"
python3 -I scripts/perf/suite/summarize.py --ref before --out "$OUT/solid.md" "$OUT/solid"
python3 -I scripts/perf/suite/summarize.py --ref before --out "$OUT/models.md" "$OUT/models"
