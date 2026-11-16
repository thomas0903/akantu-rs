#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): suite-setup measurement script, kept as run.
# grouped.sh <out> <name> <size> <reps> <cases> <wrap> <launcher|-> <binary>
# One wrapper job per repetition: every case once, one process per run, the
# case order rotated by one place per repetition. Layout as run_ab.sh:
# <out>/<name>/<case>.r<r>.jsonl and <out>/meta.json.
set -euo pipefail
out="$1"; name="$2"; size="$3"; reps="$4"; cases="$5"; wrap="$6"; launcher="$7"; bin="$8"
[ -f "$out.commit" ] && export AKANTU_SUITE_COMMIT="$(cat "$out.commit")"
IFS=, read -r -a cl <<<"$cases"
mkdir -p "$out/$name"
cat > "$out/meta.json" <<J
{"command":"grouped.sh $*","date_utc":"$(date -u +%Y-%m-%dT%H:%M:%SZ)","cpu":"$(lscpu | sed -n 's/^Model name: *//p' | head -1)","size":"$size","reps":$reps,"launcher":"$launcher","commit":"${AKANTU_SUITE_COMMIT:-}","binaries":{"$name":{"path":"$bin","sha256":"$(sha256sum "$bin" | cut -d' ' -f1)"}}}
J
n=${#cl[@]}
[ -f "$out.skip" ] && { echo "[grouped] skipped: $out" >&2; exit 0; }
per=1; [ -f "$out.perjob" ] && per="$(cat "$out.perjob")"
script="set -e"; inbatch=0
for ((r = 0; r < reps; r++)); do
  for ((k = 0; k < n; k++)); do
    c="${cl[$(( (k + r) % n ))]}"
    f="$out/$name/$c.r$r.jsonl"
    if [ "$launcher" = "-" ]; then pre="taskset -c 2"; else pre="$launcher"; fi
    script+=$'\n'"rm -f $f; $pre $bin --case $c --size $size --reps 1 --out $f"
  done
  inbatch=$((inbatch + 1))
  if [ "$inbatch" -ge "$per" ] || [ "$r" -eq $((reps - 1)) ]; then
    echo "[grouped] reps up to $r" >&2
    $wrap bash -c "$script"
    script="set -e"; inbatch=0
  fi
done
echo "[grouped] done: $out" >&2
