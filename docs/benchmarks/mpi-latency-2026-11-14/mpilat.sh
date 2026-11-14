#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Runs inside akantu-rs-mpi:dev (scripts/mpi/in-docker.sh). x86_64 rerun of the
# "MPI point-to-point latency" table of docs/BENCHMARKS.md (measured on the Mac
# in OrbStack on 2026-11-11). Two methods:
#  A. separate launches, as the original table: mpi_bench <case> and the raw C
#     baseline scripts/mpi/bench_baseline.c, alternated per repetition
#     (order rotated), 5 repetitions;
#  B. one job per case (`mpi_bench calibrated`, the MPI tier's method):
#     raw MPI and akantu-comm in interleaved blocks, 9 blocks,
#     repeated 3 times.
set -uo pipefail
out=/target/mpilat
mkdir -p "$out"
cd /src
{
  echo "date_utc=$(date -u +%FT%TZ)"
  echo "git_head=$(git -c safe.directory=/src rev-parse HEAD 2>/dev/null || echo unknown)"
  echo "cpu=$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//')"
  echo "nproc=$(nproc)"
  echo "kernel=$(uname -r)"
  echo "rustc=$(rustc --version)"
  echo "mpirun=$(mpirun --version 2>&1 | head -1)"
  env | grep -E '^OMPI_MCA_|^PRTE_MCA_' | sort
} > "$out/provenance.txt"
cargo build --release -p akantu-comm --features mpi --locked --example mpi_bench --message-format=json \
  > "$out/build-bench.json" 2> "$out/build-bench.log" || { echo BUILD FAIL; exit 1; }
bench=$(jq -r 'select(.reason=="compiler-artifact" and .target.name=="mpi_bench" and .executable!=null) | .executable' \
  "$out/build-bench.json" | tail -1)
mpicc -O2 scripts/mpi/bench_baseline.c -o "$out/bench_baseline" || { echo BASELINE BUILD FAIL; exit 1; }
echo "bench_sha256=$(sha256sum "$bench" | cut -d' ' -f1)" >> "$out/provenance.txt"
echo "baseline_sha256=$(sha256sum "$out/bench_baseline" | cut -d' ' -f1)" >> "$out/provenance.txt"

# case list: np kind bytes iterations
cases=(
  "2 pingpong 8 20000"
  "2 pingpong 65536 5000"
  "2 pingpong 1048576 500"
  "2 pingpong 16777216 30"
  "2 halo 8192 5000"
  "4 halo 8192 5000"
)
resA="$out/separate.tsv"
printf 'rep\tbtl\tnp\tkind\tbytes\tprog\tus\n' > "$resA"
for rep in 1 2 3 4 5; do
  for btl in sm tcp; do
    for c in "${cases[@]}"; do
      set -- $c
      np=$1 kind=$2 bytes=$3 it=$4
      if [ $((rep % 2)) -eq 1 ]; then order="raw akantu"; else order="akantu raw"; fi
      for prog in $order; do
        if [ "$prog" = raw ]; then
          line=$(timeout -k 5 300 mpirun --mca btl "$btl,self" -np "$np" "$out/bench_baseline" "$kind" "$bytes" "$it" 2>>"$out/err.log" | grep -a '^BASE' | head -1)
          us=$(printf '%s\n' "$line" | sed -n 's/.*half-RTT \([0-9.]*\) us.*/\1/p; s/.*: \([0-9.]*\) us per exchange.*/\1/p')
        else
          line=$(timeout -k 5 300 mpirun --mca btl "$btl,self" -np "$np" "$bench" "$kind" "$bytes" "$it" 2>>"$out/err.log" | grep -a '^PERF' | head -1)
          echo "$line" >> "$out/perf-lines.txt"
          us=$(printf '%s\n' "$line" | grep -o '[0-9.]* us' | head -1 | cut -d' ' -f1)
        fi
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$rep" "$btl" "$np" "$kind" "$bytes" "$prog" "${us:-NA}" >> "$resA"
      done
    done
  done
done
resB="$out/calibrated.txt"
: > "$resB"
for rep in 1 2 3; do
  for btl in sm tcp; do
    for c in "2 pingpong 8 2000" "2 pingpong 65536 500" "2 pingpong 1048576 100" "2 pingpong 16777216 10" "2 halo 8192 1000" "4 halo 8192 1000"; do
      set -- $c
      line=$(timeout -k 5 600 mpirun --mca btl "$btl,self" -np "$1" "$bench" calibrated "$2" "$3" "$4" 9 2>>"$out/err.log" | grep -a '^CAL summary' | head -1)
      echo "rep=$rep btl=$btl np=$1 $line" >> "$resB"
    done
  done
done
echo DONE > "$out/done"
