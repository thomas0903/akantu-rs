#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Isolated regression cases of the PETSc fix round
# (UD-426; crates/akantu-solver/tests/petsc_mpi_cases), run INSIDE the
# akantu-rs-petsc image — from the host:
#   scripts/mpi/in-docker.sh bash /src/scripts/petsc/run-petsc-regressions.sh [options]
# (AKANTU_RS_MPI_IMAGE=akantu-rs-petsc:dev), or as a step of
# scripts/petsc/run-petsc-tier.sh.
#
#   --cases "a b …"     default: every case except pe16_sparse_8 (8 ranks)
#   --ranks "1 2 4"     rank counts
#   --levels "funneled serialized"
#   --repeat N          repetitions of every (case, ranks, level)
#   --bin PATH          a prebuilt petsc_mpi binary (default: build it)
#
# One `mpirun` per (case, ranks, level, run), wrapped in `timeout`; a case
# passes iff the job exits 0 and rank 0 printed `CASE PASS <case>`; a hang
# (`CASE HANG`, exit 75, or the timeout) is a failure. Cases:
# `gpe01_precedence` runs with PETSC_OPTIONS="-tao_max_it 1" (exported to
# every rank); `ape01_glue_panic` passes iff the job ends with
# TRANSPORT_ABORT_CODE (71) within 30 s and the log holds the abort's
# emergency note naming the glue panic on the last rank ("a panic in the
# PETSc callback glue code on rank <N-1>", the injected
# glue panic, not some other transport failure, aborted it);
# `pt03_pending_global_option` runs with PETSC_OPTIONS
# unset, like every case but gpe01;
# `gpe03_ksp_setup_failure` passes iff it ends with 71 after rank 0 printed
# `GPE03 CHECK PASS rank 0` (expected error, then the abort policy). The D-050 BLAS pin is
# (re-)established and passed to every rank. Results: results.tsv in
# /target/petsc-logs/regressions-<UTC timestamp>/.
set -uo pipefail
cases=""
ranks="1 2 4"
levels="funneled serialized"
repeat=1
bin=""
while [ $# -gt 0 ]; do
  case "$1" in
    --cases) cases="$2"; shift 2 ;;
    --ranks) ranks="$2"; shift 2 ;;
    --levels) levels="$2"; shift 2 ;;
    --repeat) repeat="$2"; shift 2 ;;
    --bin) bin="$2"; shift 2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
# shellcheck source=scripts/mumps/blas-pin.sh
. /src/scripts/mumps/blas-pin.sh
akantu_blas_export
stamp=$(date -u +%Y%m%dT%H%M%SZ)
logdir=/target/petsc-logs/regressions-$stamp
mkdir -p "$logdir"
results="$logdir/results.tsv"
printf 'case\tranks\tlevel\trun\trc\tseconds\tverdict\n' > "$results"
{
  echo "git_head=${AKANTU_RS_GIT_HEAD:-unknown}"
  echo "image=${AKANTU_RS_MPI_IMAGE_ID:-unknown}"
  echo "platform=$(uname -m) OPENBLAS_CORETYPE=${OPENBLAS_CORETYPE:-<unset>} OPENBLAS_NUM_THREADS=${OPENBLAS_NUM_THREADS:-}"
  echo "cpu=$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ //')"
} > "$logdir/provenance.txt"
if [ -z "$bin" ]; then
  cargo build -p akantu-solver --features petsc --test petsc_mpi --locked \
    --message-format=json-render-diagnostics > "$logdir/build.log" 2>&1
  bin=$(grep -o '"executable":"[^"]*petsc_mpi[^"]*"' "$logdir/build.log" | tail -1 |
    sed 's/"executable":"\(.*\)"/\1/')
fi
if [ -z "$bin" ] || [ ! -x "$bin" ]; then
  echo "petsc_mpi binary not found" | tee -a "$logdir/build.log"
  exit 1
fi
if [ -z "$cases" ]; then
  cases="pe01_refusal_dup pe01_refusal_world pe01_refusal_native pe02_deadlock pe02_stress
    pe04_vectors pe05_panic pe06_foreign_comm pe07_policy pe09_world_dup pe09_finalize_order
    pe13_command_line pt04_pending_options pe15_world_in_region ud426_j_petsc ud426_j_petsc_read_only blas_pin
    gpe01_precedence gpe03_ksp_setup_failure ape01_glue_panic pt03_pending_global_option"
fi
failures=0
for case in $cases; do
  for level in $levels; do
    for n in $ranks; do
      for k in $(seq 1 "$repeat"); do
        log="$logdir/$case-np$n-$level-run$k.log"
        t0=$SECONDS
        extra=()
        if [ "$case" = gpe01_precedence ]; then
          export PETSC_OPTIONS="-tao_max_it 1"
          extra=(-x PETSC_OPTIONS)
        else
          unset PETSC_OPTIONS
        fi
        AKANTU_PETSC_CASE=$case AKANTU_PETSC_THREAD_LEVEL=$level timeout --kill-after=10 600 \
          mpirun -n "$n" -x AKANTU_PETSC_CASE -x AKANTU_PETSC_THREAD_LEVEL ${extra[@]+"${extra[@]}"} \
            -x OPENBLAS_NUM_THREADS -x OMP_NUM_THREADS ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} \
            "$bin" > "$log" 2>&1
        rc=$?
        unset PETSC_OPTIONS
        v=PASS
        if [ "$case" = ape01_glue_panic ]; then
          # The injected glue panic on the last rank must abort the job
          # (TRANSPORT_ABORT_CODE) with its emergency note.
          if [ $rc -ne 71 ] || [ $((SECONDS - t0)) -gt 30 ] ||
            ! grep -aq "a panic in the PETSc callback glue code on rank $((n - 1))" "$log"; then
            v=FAIL
            failures=$((failures + 1))
          fi
        elif [ "$case" = gpe03_ksp_setup_failure ]; then
          # Expected error on rank 0, then the transport abort policy.
          if [ $rc -ne 71 ] || ! grep -q "^GPE03 CHECK PASS rank 0$" "$log"; then
            v=FAIL
            failures=$((failures + 1))
          fi
        elif [ $rc -ne 0 ] || ! grep -q "^CASE PASS $case " "$log"; then
          v=FAIL
          failures=$((failures + 1))
        fi
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$case" "$n" "$level" "$k" "$rc" $((SECONDS - t0)) "$v" >> "$results"
        printf '%-26s ranks=%-2s level=%-11s run=%-2s rc=%-3s %5ss %s\n' "$case" "$n" "$level" "$k" "$rc" $((SECONDS - t0)) "$v"
        if [ $v = FAIL ]; then grep -E '^(CASE|rank [0-9]+: CASE|INFO)' "$log" | head -12 | sed 's/^/    /'; fi
      done
    done
  done
done
echo "logs: $logdir"
echo "regression failures=$failures"
[ $failures -eq 0 ]
