#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the baseline timing
# series of the performance campaign on the shared measurement machine, each
# one command. Every piece (a round, a repetition, a campaign) is its own
# short `tjob` and leaves a marker when it is complete, so a series that the
# host's stop interrupted resumes with the same command (complete pieces are
# skipped, an incomplete one runs again).
#
#   series.sh explicit            explicit-step table: 7 rotated rounds,
#                                 ten configurations, cpp-strict cpp-default port
#   series.sh explicit-digests    the state digests of the three programs (bjob)
#   series.sh driver STEPS        cohesive driver h = 0.12, port vs C++ strict,
#                                 1 rank, materials cohesive + elastic, 5 rotated
#                                 repetitions (one tjob per repetition and material)
#   series.sh static              static solve h = 0.12, 1 rank: C++ MUMPS,
#                                 C++ SparseLU, port MUMPS, port SparseLU; 5 reps
#   series.sh scaling STEPS       cohesive benchmark h = 0.06 at 1, 2, 4, 8 ranks,
#                                 C++ strict and port, MUMPS, same partition, 3 reps
#
# Environment (defaults): OUT (~/w/infra/data), BASE (~/baseline: meshes/,
# port-*/, cpp-bin/), PORT_DIR (BASE/port-59dd2229: `port` = explicit_step_timing,
# `bench` = host cohesive_extrinsic), RUST_BIN (the MPI+MUMPS driver inside
# the akantu-rs-mumps:dev image, in the volume akantu-rs-mpi-target:
# /target/infra-bench/release/cohesive_extrinsic), PIN_CPU (2: serial runs on
# that core), LABEL (infra: tjob label prefix).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "${here}/../../../.." && pwd)"
OUT="${OUT:-$HOME/w/infra/data}"
BASE="${BASE:-$HOME/baseline}"
PORT_DIR="${PORT_DIR:-$BASE/port-59dd2229}"
export RUST_BIN="${RUST_BIN:-/target/infra-bench/release/cohesive_extrinsic}"
PIN="${PIN_CPU:-2}"
L="${LABEL:-infra}"
mkdir -p "$OUT"
say() { echo "$(date -u +%FT%TZ) $*" | tee -a "$OUT/series.log"; }

explicit_cases="tet4-20k:30 tet4-20k:30:fd tet4-200k:6 tet4-200k:6:fd tet10-20k:6 tet10-20k:6:fd hex8-20k:8 hex8-20k:8:fd hex8-200k:3 hex8-200k:3:fd"

case "${1:-}" in
  explicit)
    d="$OUT/explicit"; mkdir -p "$d"
    for r in 1 2 3 4 5 6 7; do
      [ -e "$d/r$r.ok" ] && { say "explicit round $r: done, skipped"; continue; }
      rm -f "$d/r$r.jsonl"
      say "explicit round $r: start"
      tjob "$L-explicit-r$r" -- env PIN_CPU="$PIN" ROUND_START="$r" BIN_DIR="$PORT_DIR" \
        CPP_BIN_DIR="$BASE/cpp-bin" MESH_DIR="$BASE/meshes/explicit" \
        "$repo/scripts/perf/explicit-step/timing_rotated.sh" "$d/r$r.jsonl" 1 \
        "cpp-strict cpp-default port" "$explicit_cases" > "$d/r$r.log" 2>&1
      touch "$d/r$r.ok"; say "explicit round $r: done"
    done
    cat "$d"/r[1-7].jsonl > "$d/explicit.jsonl"
    python3 -I "$repo/scripts/perf/explicit-step/summarize_timing.py" "$d/explicit.jsonl" > "$d/summary.md" 2>&1 || true
    say "explicit: $d/summary.md" ;;
  explicit-digests)
    bjob "$L-explicit-digests" -- "$here/explicit_digests.sh" "$PORT_DIR/port" "$BASE/cpp-bin" \
      "$BASE/meshes/explicit" > "$OUT/explicit-digests.txt"
    cat "$OUT/explicit-digests.txt" ;;
  driver)
    steps="${2:?steps}"
    d="$OUT/driver-s$steps"; mkdir -p "$d"
    for rep in 1 2 3 4 5; do
      for m in cohesive elastic; do
        [ -e "$d/rep$rep-$m.ok" ] && { say "driver s$steps rep $rep $m: done, skipped"; continue; }
        say "driver s$steps rep $rep $m: start"
        tjob "$L-driver-s$steps-r$rep-$m" -- env PIN_CPU="$PIN" \
          "$repo/crates/akantu-bench/scripts/timing_ab.sh" --out "$d" --meshes "$BASE/meshes/cohesive" \
          --h 0.12 --steps "$steps" --reps 1 --rep-start "$rep" --materials "$m" \
          cpp=cpp:strict "port=rust:$PORT_DIR/bench" > "$d/rep$rep-$m.log" 2>&1
        touch "$d/rep$rep-$m.ok"; say "driver s$steps rep $rep $m: done"
      done
    done
    python3 -I "$here/paired.py" "$d" cpp port > "$d/paired.md"
    python3 -I "$repo/crates/akantu-bench/scripts/compare_cpp.py" --cpp "$d/cpp" --rust "$d/port" \
      --cpp-label cpp --rust-label port > "$d/compare_cpp.md" 2>&1 || true
    say "driver s$steps: $d/paired.md" ;;
  static)
    d="$OUT/c1prep"; mkdir -p "$d"
    cases=(cpp cppeig rust rusteig)
    for rep in 1 2 3 4 5; do
      c="$L-static-h0.12-r$rep"
      grep -q "campaign done" "$d/$c/schedule.log" 2> /dev/null && { say "static rep $rep: done, skipped"; continue; }
      [ -d "$d/$c" ] && mv "$d/$c" "$d/$c.incomplete-$(date +%s)"
      rot=(); for i in 0 1 2 3; do rot+=("${cases[$(( (i + rep - 1) % 4 ))]}"); done
      say "static rep $rep: ${rot[*]}"
      tjob "$L-static-r$rep" -- env OUT="$d" PIN_CPUS_FROM="$PIN" C1PREP_LOCK="$HOME/w/infra/c1prep.lock" \
        bash "$repo/crates/akantu-bench/scripts/c1prep_campaign.sh" "$c" 0.12 1 1 1 "${rot[*]}" \
        > "$d/$c.log" 2>&1 || say "static rep $rep: campaign FAILED ($d/$c/schedule.log)"
    done
    python3 -I "$here/paired.py" --c1prep "$d" cpp rust > "$d/static-mumps.md"
    python3 -I "$here/paired.py" --c1prep "$d" cppeig rusteig > "$d/static-sparselu.md"
    python3 -I "$here/paired.py" --c1prep "$d" cpp cppeig > "$d/static-cpp-sparselu-vs-mumps.md"
    say "static: $d/static-*.md" ;;
  scaling)
    steps="${2:?steps}"
    d="$OUT/scaling"; mkdir -p "$d"
    say "scaling s$steps: start"
    env OUT="$d" PIN_CPUS_FROM=0 SKIP_DONE=1 C1PREP_LOCK="$HOME/w/infra/c1prep.lock" \
      CAMPAIGN_WRAPPER="tjob $L-scaling --" \
      bash "$repo/crates/akantu-bench/scripts/scaling_series.sh" "$L-scal-s$steps" "$steps" 3 \
      "0.06:1 0.06:2 0.06:4 0.06:8" "cpp rust" || say "scaling: series reported a failure"
    exp=(); [ "$steps" -le 500 ] || exp=(--expect-insertion)
    python3 -I "$repo/crates/akantu-bench/scripts/scaling_analyze.py" "$d" "$L-scal-s$steps" \
      --strong 0.06 ${exp[@]+"${exp[@]}"} > "$d/$L-scal-s$steps-analysis.log" 2>&1 || true
    python3 -I "$here/paired.py" --c1prep "$d" cpp rust > "$d/$L-scal-s$steps-paired.md"
    say "scaling s$steps: $d/$L-scal-s$steps-summary.md" ;;
  *) sed -n '9,26p' "$0" >&2; exit 2 ;;
esac
