#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Real-MPI check of `akantu::runtime::initialize` (row CORE-031;
# crates/akantu/tests/initialize_mpi.rs), run INSIDE the akantu-rs-mpi
# image:
#
#   scripts/mpi/in-docker.sh bash /src/scripts/mpi/run-initialize-mpi.sh [--ranks "1 2 4"]
#
# Builds the harness-less test binary with feature `mpi`, then for every rank
# count N runs it under `mpirun -np N` with `--aka_seed 7` only (expected
# seed 7·(rank+1)) and with `AKA_SEED=11` exported (environment wins:
# 11·(rank+1)), each in the modes `finalize` (explicit runtime::finalize) and
# `forget` (no finalize: the exit hook registered by initialize finalises
# MPI), plus one `reinit` run (finalize, then initialize
# must be refused with the UD-499 error). A run passes when mpirun exits 0
# and every rank prints `INITIALIZE_MPI PASS`. Exit status: number of failed
# runs (0 = all pass).
set -uo pipefail
ranks="1 2 4"
while [ $# -gt 0 ]; do
  case "$1" in
    --ranks) ranks="$2"; shift 2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
logdir="${CARGO_TARGET_DIR:-/target}/initialize-mpi-logs/$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$logdir"
cargo test -p akantu --features mpi --locked --test initialize_mpi --no-run \
  --message-format=json > "$logdir/build.json" 2> "$logdir/build.log"
rc=$?
if [ $rc != 0 ]; then
  tail -30 "$logdir/build.log"
  echo "build failed ($rc)"
  exit 1
fi
bin=$(jq -r 'select(.reason=="compiler-artifact" and .target.name=="initialize_mpi" and .executable!=null) | .executable' \
  "$logdir/build.json" | tail -1)
if [ -z "$bin" ] || [ ! -x "$bin" ]; then
  echo "initialize_mpi binary not found"
  exit 1
fi
failures=0
for n in $ranks; do
  for run in cli:finalize env:finalize cli:forget env:forget cli:reinit; do
    variant=${run%%:*}
    mode=${run#*:}
    log="$logdir/np$n-$variant-$mode.log"
    env_args=()
    if [ "$variant" = env ]; then env_args=(-x AKA_SEED=11); fi
    if [ "$variant" = env ]; then export AKA_SEED=11; else unset AKA_SEED; fi
    timeout -k 10 120 mpirun ${env_args[@]+"${env_args[@]}"} -np "$n" "$bin" "$mode" > "$log" 2>&1
    rc=$?
    passes=$(grep -ac "INITIALIZE_MPI PASS" "$log")
    verdict=FAIL
    if [ $rc = 0 ] && [ "$passes" = "$n" ]; then verdict=PASS; else failures=$((failures + 1)); fi
    printf 'np=%s %-3s %-8s rc=%s passes=%s %s\n' "$n" "$variant" "$mode" "$rc" "$passes" "$verdict"
    grep -a "INITIALIZE_MPI" "$log" | sort
    if [ "$verdict" = FAIL ]; then grep -av "INITIALIZE_MPI" "$log" | tail -8; fi
  done
done
unset AKA_SEED
echo "logs: $logdir"
echo "failures: $failures"
exit $failures
