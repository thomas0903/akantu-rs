#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): interleaved, rotated
# A/B series of benchmark-suite binaries (suite_solid, suite_models,
# suite_setup; crates/akantu-bench/README.md "Benchmark suite").
#
#   run_ab.sh --out DIR --size s|m|l --reps N --cases a,b,...|all \
#             [--core C] [--wrap "tjob LABEL --"] [--launcher "CMD ..."] \
#             [--args "EXTRA SUITE ARGS"] NAME=BINARY [NAME=BINARY ...]
#
# Every repetition r runs every case with every binary, one process per run
# (`BINARY --case CASE --size SIZE --reps 1 --out DIR/NAME/CASE.r<r>.jsonl`),
# the binaries of one case back to back, their order rotated by one place per
# repetition (r = 0: A B C, r = 1: B C A, ...). Each run is pinned to core C
# (default 2) with taskset, unless a launcher is given (an MPI launcher binds
# its own ranks, e.g. --launcher "mpirun -n 4 --bind-to core").
#
# --wrap prefixes each group (the binaries of one case in one repetition) with
# a command, e.g. --wrap "tjob suite-solid-ab --": every group then runs alone
# on the measurement machine while the pair stays together in time.
#
# --cases all takes the case list of the first binary (`--list`).
# DIR/meta.json records the command, the date (UTC), the host CPU and each
# binary's path and SHA-256. summarize.py reads DIR.
set -euo pipefail

usage() { sed -n '5,10p' "$0" >&2; exit 2; }
orig=("$0" "$@")
out="" size="" reps="" cases="" core=2 wrap="" launcher="" extra=""
variants=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --size) size="$2"; shift 2 ;;
    --reps) reps="$2"; shift 2 ;;
    --cases) cases="$2"; shift 2 ;;
    --core) core="$2"; shift 2 ;;
    --wrap) wrap="$2"; shift 2 ;;
    --launcher) launcher="$2"; shift 2 ;;
    --args) extra="$2"; shift 2 ;;
    -h|--help) usage ;;
    *=*) variants+=("$1"); shift ;;
    *) echo "run_ab.sh: unknown argument $1" >&2; usage ;;
  esac
done
[ -n "$out" ] && [ -n "$size" ] && [ -n "$reps" ] && [ -n "$cases" ] && [ ${#variants[@]} -ge 1 ] || usage

names=(); bins=()
for v in "${variants[@]}"; do
  n="${v%%=*}"; b="${v#*=}"
  [ -x "$b" ] || { echo "run_ab.sh: $b is not executable" >&2; exit 2; }
  names+=("$n"); bins+=("$(cd "$(dirname "$b")" && pwd)/$(basename "$b")")
done
if [ "$cases" = all ]; then
  cases="$("${bins[0]}" --list | awk '{print $1}' | paste -sd, -)"
fi
IFS=, read -r -a case_list <<<"$cases"
mkdir -p "$out"
for n in "${names[@]}"; do mkdir -p "$out/$n"; done

# metadata
{
  printf '{"command":%s,' "$(printf '%q ' "${orig[@]}" | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read().strip()))')"
  printf '"date_utc":"%s",' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf '"cpu":%s,' "$( (lscpu 2>/dev/null | sed -n 's/^Model name: *//p' | head -1) | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read().strip()))')"
  printf '"size":"%s","reps":%s,"core":"%s","launcher":%s,"args":%s,"cases":%s,"binaries":{' \
    "$size" "$reps" "$core" \
    "$(printf '%s' "$launcher" | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read()))')" \
    "$(printf '%s' "$extra" | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read()))')" \
    "$(printf '%s\n' "${case_list[@]}" | python3 -c 'import json,sys;print(json.dumps(sys.stdin.read().split()))')"
  for i in "${!names[@]}"; do
    [ "$i" -gt 0 ] && printf ','
    printf '"%s":{"path":"%s","sha256":"%s"}' "${names[$i]}" "${bins[$i]}" "$( (sha256sum "${bins[$i]}" 2>/dev/null || shasum -a 256 "${bins[$i]}") | cut -d' ' -f1)"
  done
  printf '}}\n'
} > "$out/meta.json"

nv=${#names[@]}
read -r -a extra_args <<<"$extra"
read -r -a launch <<<"$launcher"
read -r -a wrapper <<<"$wrap"
for ((r = 0; r < reps; r++)); do
  for c in "${case_list[@]}"; do
    group=()
    for ((k = 0; k < nv; k++)); do
      i=$(( (k + r) % nv ))
      f="$out/${names[$i]}/$c.r$r.jsonl"
      rm -f -- "$f"
      if [ ${#launch[@]} -gt 0 ]; then
        cmd=("${launch[@]}" "${bins[$i]}")
      else
        cmd=(taskset -c "$core" "${bins[$i]}")
      fi
      cmd+=(--case "$c" --size "$size" --reps 1 --out "$f" ${extra_args[@]+"${extra_args[@]}"})
      group+=("$(printf '%q ' "${cmd[@]}")")
    done
    script="$(printf 'set -e\n'; printf '%s\n' "${group[@]}")"
    echo "[run_ab] rep $r case $c: ${names[*]}" >&2
    if [ ${#wrapper[@]} -gt 0 ]; then
      "${wrapper[@]}" bash -c "$script"
    else
      bash -c "$script"
    fi
  done
done
echo "[run_ab] done: $out" >&2
