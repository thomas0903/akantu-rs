#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): perf profiles of
# benchmark-suite cases (crates/akantu-bench/README.md "Benchmark suite").
#
#   profile.sh record BINARY CASE DIR [SECONDS] [SIZE] [CORE]   # run under tjob
#   profile.sh report CASE DIR                                  # run under bjob
#
# record: `perf record -F 499 --call-graph dwarf,16384` of
#   `BINARY --case CASE --size SIZE --loop-seconds SECONDS` (default 10 s,
#   size m) pinned to CORE (default 2); DIR/CASE.perf.data and the suite's
#   JSON lines DIR/CASE.jsonl. Cases without a primary loop (set-up cases)
#   run with --reps 3 instead of --loop-seconds.
# report: DIR/CASE.txt with the command and environment, the top 25 symbols
#   by self time, the top 25 by inclusive time (children), and the call
#   chains (caller graph, entries above 2 %); the perf.data file is removed
#   afterwards (it is large) unless KEEP_PERF_DATA=1.
set -euo pipefail
mode="${1:?usage}"; shift
case "$mode" in
  record)
    bin="$1"; c="$2"; dir="$3"; secs="${4:-10}"; size="${5:-m}"; core="${6:-2}"
    mkdir -p "$dir"
    rm -f "$dir/$c.jsonl"
    case "$c" in
      setup_*) extra=(--reps 3) ;;
      *) extra=(--loop-seconds "$secs") ;;
    esac
    {
      echo "binary: $bin ($(sha256sum "$bin" | cut -c1-16))"
      echo "command: taskset -c $core $bin --case $c --size $size ${extra[*]}"
      echo "perf: perf record -F 499 --call-graph dwarf,16384"
      echo "cpu: $(lscpu | sed -n 's/^Model name: *//p' | head -1)"
      echo "date_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } > "$dir/$c.meta"
    perf record -q -F 499 --call-graph dwarf,16384 -o "$dir/$c.perf.data" -- \
      taskset -c "$core" "$bin" --case "$c" --size "$size" "${extra[@]}" --out "$dir/$c.jsonl"
    ;;
  report)
    c="$1"; dir="$2"
    data="$dir/$c.perf.data"
    out="$dir/$c.txt"
    {
      echo "# Profile of suite case $c"
      echo
      cat "$dir/$c.meta"
      echo "commit: $(head -1 "$dir/$c.jsonl" | python3 -c 'import json,sys;print(json.loads(sys.stdin.read())["commit"])')"
      echo
      echo "## Phases of the profiled run (seconds, calls)"
      echo
      python3 -c '
import json,sys
for l in open(sys.argv[1]):
    d=json.loads(l)
    print(f"{d[\"phase\"]:<24} {d[\"seconds\"]:10.4f} s  {d[\"calls\"]:6d} calls")' "$dir/$c.jsonl"
      echo
      echo "## Top 25 symbols, self time"
      echo
      perf report -i "$data" --no-children --sort symbol --stdio -g none --percent-limit 0.1 2>/dev/null \
        | grep -E '^ +[0-9]' | head -25
      echo
      echo "## Top 25 symbols, inclusive time (children)"
      echo
      perf report -i "$data" --children --sort symbol --stdio -g none --percent-limit 0.1 2>/dev/null \
        | grep -E '^ +[0-9]' | head -25
      echo
      echo "## Call chains (callers of the hottest self-time symbols, entries above 2 %)"
      echo
      perf report -i "$data" --no-children --sort symbol --stdio -g caller,2,callee,function,percent \
        --percent-limit 2 2>/dev/null | grep -v '^$' | grep -v '^#' | head -400
    } > "$out"
    [ "${KEEP_PERF_DATA:-0}" = 1 ] || rm -f "$data" "$data.old"
    ;;
  *) echo "profile.sh: mode record or report" >&2; exit 2 ;;
esac
