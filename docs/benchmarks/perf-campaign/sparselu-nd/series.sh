#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the COLAMD / nested
# dissection series of the SparseLU ordering work (README of this
# directory). Variants are wrappers around the suite binaries that add
# `--ordering` and record each run's peak RSS (/usr/bin/time).
#
#   series.sh BIN_DIR OUT_DIR
#
# BIN_DIR holds base_suite_{solid,models} (the branch point) and
# head_suite_{solid,models} (this branch). T threads run on cores 0..T-1.
set -euo pipefail
B=$(cd "$1" && pwd); OUT=$2
mkdir -p "$OUT/wrap"
OUT=$(cd "$OUT" && pwd)
HERE=$(cd "$(dirname "$0")" && pwd)
AB="$HERE/../../../../scripts/perf/suite/run_ab.sh"
REPS=${REPS:-5}
SOLID="static_linear_eigen,static_newton_plastic,implicit_dynamic,cohesive_intrinsic_implicit"
MODELS="heat_static_tri6,heat_implicit_hex8_kvar,structural_beam3_frame_static,structural_beam3_frame_dynamic,structural_dkt18_plate_static,phasefield_staggered_tri3,phasefield_staggered_tet4,contact_implicit_static_quad4,embedded_rebar_tri3,embedded_rebar_tet4"

# wrapper NAME BINARY ORDERING|-: runs BINARY with --ordering, appends
# "case rss_kib" to OUT/rss/NAME.txt
wrapper() {
  local w="$OUT/wrap/$1"
  {
    echo '#!/usr/bin/env bash'
    echo 'c=""; p=""; for a in "$@"; do [ "$p" = "--case" ] && c=$a; p=$a; done'
    echo "mkdir -p '$OUT/rss'; t=\$(mktemp)"
    if [ "$3" = "-" ]; then
      echo "/usr/bin/time -f %M -o \"\$t\" '$2' \"\$@\""
    else
      echo "/usr/bin/time -f %M -o \"\$t\" '$2' \"\$@\" --ordering $3"
    fi
    echo "s=\$?; echo \"\$c \$(tail -1 \"\$t\")\" >> '$OUT/rss/$1.txt'; rm -f \"\$t\"; exit \$s"
  } > "$w"
  chmod +x "$w"
  echo "$w"
}

for s in solid models; do
  wrapper "base_$s" "$B/base_suite_$s" - > /dev/null
  for o in colamd nested_dissection auto; do
    wrapper "${o}_$s" "$B/head_suite_$s" "$o" > /dev/null
  done
done
W="$OUT/wrap"

run() { # run TAG SIZE THREADS CASES SUITE VARIANTS...
  local tag=$1 size=$2 t=$3 cases=$4 suite=$5; shift 5
  local vs=()
  for v in "$@"; do vs+=("$v=$W/${v}_$suite"); done
  "$AB" --out "$OUT/$tag-t$t" --size "$size" --reps "$REPS" --cases "$cases" \
    --core "0-$((t - 1))" --args "--threads $t" "${vs[@]}" > "$OUT/$tag-t$t.log" 2>&1
}

# one thread: the branch point, the head's three orderings
run solid m 1 "$SOLID" solid base colamd nested_dissection auto
run models m 1 "$MODELS" models base colamd nested_dissection auto
for t in 2 4 8; do
  run solid m "$t" "$SOLID" solid colamd nested_dissection auto
  run models m "$t" "$MODELS" models colamd nested_dissection auto
done
for t in 1 2 4 8; do
  run large l "$t" static_linear_eigen solid colamd nested_dissection
done
echo SERIES-DONE
