#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tool (no upstream counterpart): repeated runs of the process-exit
# scenarios of crates/akantu/tests/exit_hook_mpi.rs (decision D-067),
# run INSIDE the akantu-rs-mpi image:
#
#   scripts/mpi/in-docker.sh bash /src/scripts/mpi/run-exit-hook-campaign.sh \
#       [--ranks "1 2 4"] [--runs 20] [--limit 15] [--label NAME] [--scenarios "…"]
#
# Builds the harness-less binary (facade without default features, feature
# `mpi`), then runs every scenario `--runs` times at every rank count under
# `timeout -k 5 <limit> mpirun -np N`. One RESULT line per run (rc, wall
# time in ms, ranks that reached the end, emergency notes on stderr) goes to
# stdout and results.tsv; summary.tsv has one line per (scenario, ranks):
# expected rc, runs, passes, rc histogram, median and maximum time. A run
# passes when its exit status is the expected one (glibc Linux, D-067) and
# it ended before the limit (rc 124 = killed by the limit: a hang). The
# MPI tier (run-mpi-tier.sh) runs the same scenarios once per rank count.
# Exit status: 0 if every run passed, 1 otherwise, 2 on a build failure.
set -uo pipefail
ranks="1 2 4"
runs=20
limit=15
label=campaign
scenarios="forget finalize hostfirst hostfirst-universe panic err-return exit-nonzero serial-held worker-exit"
while [ $# -gt 0 ]; do
  case "$1" in
    --ranks) ranks="$2"; shift 2 ;;
    --runs) runs="$2"; shift 2 ;;
    --limit) limit="$2"; shift 2 ;;
    --label) label="$2"; shift 2 ;;
    --scenarios) scenarios="$2"; shift 2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
expected_rc() {
  case "$1" in
    forget | finalize | hostfirst | hostfirst-universe) echo 0 ;;
    panic) echo 101 ;;
    err-return | serial-held | worker-exit) echo 1 ;;
    exit-nonzero) echo 3 ;;
    *) echo "?" ;;
  esac
}
logdir="${CARGO_TARGET_DIR:-/target}/exit-hook-logs/$label-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$logdir"
cargo test -p akantu --no-default-features --features mpi --locked --test exit_hook_mpi --no-run \
  --message-format=json > "$logdir/build.json" 2> "$logdir/build.log"
rc=$?
if [ $rc != 0 ]; then
  tail -30 "$logdir/build.log"
  echo "build failed ($rc)"
  exit 2
fi
bin=$(jq -r 'select(.reason=="compiler-artifact" and .target.name=="exit_hook_mpi" and .executable!=null) | .executable' \
  "$logdir/build.json" | tail -1)
if [ -z "$bin" ] || [ ! -x "$bin" ]; then
  echo "exit_hook_mpi binary not found"
  exit 2
fi
{
  echo "label=$label"
  echo "git_head=${AKANTU_RS_GIT_HEAD:-unknown}"
  echo "image=${AKANTU_RS_MPI_IMAGE_ID:-unknown}"
  echo "mpirun=$(mpirun --version | head -1)"
  echo "glibc=$(ldd --version | head -1)"
  echo "ranks=$ranks runs=$runs limit_s=$limit"
  echo "binary=$bin"
} > "$logdir/provenance.txt"
cat "$logdir/provenance.txt"
results="$logdir/results.tsv"
printf 'label\tscenario\tranks\trun\texpected_rc\trc\tms\treached_end\tnotes\tverdict\n' > "$results"
failures=0
for n in $ranks; do
  for s in $scenarios; do
    exp=$(expected_rc "$s")
    for k in $(seq 1 "$runs"); do
      out="$logdir/$s-np$n-run$k.out"
      err="$logdir/$s-np$n-run$k.err"
      t0=$(date +%s%N)
      timeout -k 5 "$limit" mpirun -np "$n" "$bin" "$s" > "$out" 2> "$err"
      rc=$?
      t1=$(date +%s%N)
      ms=$(((t1 - t0) / 1000000))
      reached=$(grep -ac "EXIT_HOOK $s rank=[0-9]* reached-end" "$out")
      notes=$(grep -ac "akantu: MPI left unfinalised at exit" "$err")
      verdict=FAIL
      if [ "$rc" = "$exp" ]; then verdict=PASS; else failures=$((failures + 1)); fi
      printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$label" "$s" "$n" "$k" "$exp" "$rc" "$ms" "$reached" "$notes" "$verdict" >> "$results"
      echo "RESULT label=$label scenario=$s np=$n run=$k expected=$exp rc=$rc ms=$ms reached_end=$reached notes=$notes $verdict"
      if [ "$verdict" = FAIL ] && [ "$k" = 1 ]; then
        grep -ahv "EXIT_HOOK" "$out" "$err" | sort | uniq -c | sort -rn | head -6 | sed 's/^/    /'
      fi
    done
  done
done
summary="$logdir/summary.tsv"
python3 - "$results" > "$summary" << 'PY'
import csv, statistics, sys
from collections import OrderedDict, Counter
rows = list(csv.DictReader(open(sys.argv[1]), delimiter="\t"))
groups = OrderedDict()
for r in rows:
    groups.setdefault((r["label"], r["scenario"], r["ranks"]), []).append(r)
print("label\tscenario\tranks\texpected_rc\truns\tpasses\trc_histogram\tmedian_ms\tmax_ms\treached_end\tnotes")
for (label, s, n), rs in groups.items():
    hist = Counter(r["rc"] for r in rs)
    ms = [int(r["ms"]) for r in rs]
    reached = Counter(r["reached_end"] for r in rs)
    notes = Counter(r["notes"] for r in rs)
    print("\t".join([label, s, n, rs[0]["expected_rc"], str(len(rs)),
                     str(sum(r["verdict"] == "PASS" for r in rs)),
                     " ".join(f"rc{k}x{v}" for k, v in sorted(hist.items())),
                     str(int(statistics.median(ms))), str(max(ms)),
                     " ".join(f"{k}x{v}" for k, v in sorted(reached.items())),
                     " ".join(f"{k}x{v}" for k, v in sorted(notes.items()))]))
PY
echo "== summary ($summary)"
cat "$summary"
echo "logs: $logdir"
echo "failures: $failures"
[ "$failures" = 0 ]
