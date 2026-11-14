#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# The MUMPS test tier, run INSIDE the akantu-rs-mumps image (use
# scripts/mumps/run-mumps-tests.sh from the host). Steps:
#   * cargo fmt --check, clippy -D warnings with the `mumps` feature
#     (akantu-comm/mpi, akantu-linalg/mumps, akantu-solver/mumps);
#   * cargo test of akantu-comm (mpi), akantu-linalg, akantu-solver and
#     akantu-bench (mumps): unit tests, the harness tests, and the MPI
#     binaries as singletons;
#   * mpirun of crates/akantu-linalg/tests/mumps_mpi.rs on every rank count
#     (thread levels funneled, serialized, multiple), of
#     crates/akantu-solver/tests/mumps_mpi_solver.rs and of
#     crates/akantu-bench/tests/perf_cohesive_mumps.rs (the cohesive benchmark
#     driver's MUMPS static solve vs the oracle; funneled),
#     --repeat times each; a run passes on exit 0 with "failed=0";
#   * the MUMPS instances of upstream's patch tests
#     (crates/akantu-solid/tests/patch_mumps.rs, feature akantu-solid/mumps,
#     harness = false, one process like upstream's patch_test_linear);
#   * the regression scenarios of scripts/mumps/run-v54-regressions.sh (one
#     mpirun per scenario × rank count × thread level) and of
#     scripts/mumps/run-v66-regressions.sh.
# The BLAS pin (D-050, scripts/mumps/blas-pin.sh) is (re-)established here
# and recorded in provenance.txt; mumps_mpi's `blas_pin` scenario fails if
# it is absent or different.
#   scripts/mumps/run-mumps-tier.sh [--ranks "1 2 4"] [--repeat N] [--skip-lint]
# Logs and results.tsv go to /target/mumps-logs/<UTC timestamp>/.
set -uo pipefail
ranks="1 2 4"; repeat=1; lint=1
while [ $# -gt 0 ]; do
  case "$1" in
    --ranks) ranks="$2"; shift 2 ;;
    --repeat) repeat="$2"; shift 2 ;;
    --skip-lint) lint=0; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
# shellcheck source=scripts/mumps/blas-pin.sh
. /src/scripts/mumps/blas-pin.sh
akantu_blas_export
stamp=$(date -u +%Y%m%dT%H%M%SZ)
logdir=/target/mumps-logs/$stamp
mkdir -p "$logdir"
results="$logdir/results.tsv"
printf 'step\tranks\tlevel\trun\trc\tseconds\tverdict\n' > "$results"
failures=0
record() { # step ranks level run rc seconds verdict
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$@" >> "$results"
  printf '%-28s ranks=%-3s level=%-10s run=%s rc=%-3s %6ss %s\n' "$@"
  [ "$7" = PASS ] || failures=$((failures + 1))
}
step() { # name command...
  local name="$1"; shift
  local t0=$SECONDS
  "$@" > "$logdir/$name.log" 2>&1
  local rc=$?
  record "$name" - - 1 "$rc" $((SECONDS - t0)) "$([ $rc = 0 ] && echo PASS || echo FAIL)"
}
{
  echo "date=$stamp"
  echo "rustc=$(rustc --version)"
  echo "mpirun=$(mpirun --version | head -1)"
  echo "git_head=${AKANTU_RS_GIT_HEAD:-unknown}"
  echo "image=${AKANTU_RS_MPI_IMAGE_ID:-unknown}"
  grep -E '^libmumps|^libopenblas0|^libscotch|^libptscotch|^libmetis5|^libscalapack-openmpi2' \
    /usr/share/akantu-rs-mumps-packages.tsv
  cat /usr/share/akantu-rs-mumps-alternatives.txt
  echo "SCOTCH_PTHREAD_NUMBER=${SCOTCH_PTHREAD_NUMBER:-} OPENBLAS_NUM_THREADS=${OPENBLAS_NUM_THREADS:-} OMP_NUM_THREADS=${OMP_NUM_THREADS:-}"
  echo "platform=$(uname -m) OPENBLAS_CORETYPE=${OPENBLAS_CORETYPE:-<unset: run-time selection>} (D-050 pin: ${AKANTU_BLAS_CORETYPE:-<unset>})"
  echo "cpu=$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ //')"
} > "$logdir/provenance.txt"
cat "$logdir/provenance.txt"
feat=(-p akantu-comm -p akantu-linalg -p akantu-solver -p akantu-bench
  --features "akantu-comm/mpi,akantu-linalg/mumps,akantu-solver/mumps,akantu-bench/mumps")
# The patch tests' `default/auto[mumps]` instances (one target of
# akantu-solid, not the whole crate: its other targets run in the check tier)
solid=(-p akantu-solid --features mumps --test patch_mumps)
if [ $lint = 1 ]; then
  step fmt cargo fmt --all -- --check
  step clippy cargo clippy "${feat[@]}" --all-targets -- -D warnings
  step clippy-patch-mumps cargo clippy "${solid[@]}" -- -D warnings
fi
# --no-fail-fast: on x86_64 the 1-rank bitwise checks against the committed
# aarch64 oracle fixtures fail (D-043); every other test binary still runs.
step cargo-test cargo test --no-fail-fast "${feat[@]}"
# a singleton run (one MPI process); exit 0 only with "failed=0"
step patch-mumps cargo test --no-fail-fast "${solid[@]}"
# The executable is picked by the test's name: a package with a [[bin]] target
# (akantu-bench: cohesive_extrinsic) also reports that binary, and cargo's
# message order is not fixed (final campaign, 2026-11-13: the last message was
# the binary, so mpirun launched cohesive_extrinsic instead of the test).
bin_of() { # package test-name features
  cargo test -q -p "$1" --features "$3" --test "$2" --no-run --message-format=json 2>/dev/null \
    | sed -n 's/.*"executable":"\([^"]*\/'"$2"'-[^"/]*\)".*/\1/p' | tail -1
}
la=$(bin_of akantu-linalg mumps_mpi akantu-linalg/mumps)
so=$(bin_of akantu-solver mumps_mpi_solver akantu-solver/mumps)
# The cohesive benchmark driver's MUMPS static solve vs the oracle
be=$(bin_of akantu-bench perf_cohesive_mumps akantu-bench/mumps)
for n in $ranks; do
  for run in $(seq 1 "$repeat"); do
    for level in funneled serialized multiple; do
      t0=$SECONDS
      log="$logdir/mumps_mpi-n$n-$level-$run.log"
      AKANTU_MUMPS_THREAD_LEVEL=$level timeout 900 mpirun -n "$n" -x AKANTU_MUMPS_THREAD_LEVEL \
        -x OPENBLAS_NUM_THREADS ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} "$la" > "$log" 2>&1
      rc=$?
      v=FAIL; [ $rc = 0 ] && grep -q "failed=0" "$log" && v=PASS
      record mumps_mpi "$n" "$level" "$run" "$rc" $((SECONDS - t0)) "$v"
    done
    t0=$SECONDS
    log="$logdir/mumps_mpi_solver-n$n-$run.log"
    timeout 900 mpirun -n "$n" "$so" > "$log" 2>&1
    rc=$?
    v=FAIL; [ $rc = 0 ] && grep -q "failed=0" "$log" && v=PASS
    record mumps_mpi_solver "$n" funneled "$run" "$rc" $((SECONDS - t0)) "$v"
    t0=$SECONDS
    log="$logdir/perf_cohesive_mumps-n$n-$run.log"
    timeout 1800 mpirun -n "$n" -x OPENBLAS_NUM_THREADS ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} \
      "$be" > "$log" 2>&1
    rc=$?
    v=FAIL; [ $rc = 0 ] && grep -q "failed=0" "$log" && v=PASS
    record perf_cohesive_mumps "$n" funneled "$run" "$rc" $((SECONDS - t0)) "$v"
  done
done
t0=$SECONDS
bash /src/scripts/mumps/run-v54-regressions.sh --ranks "$ranks" --logdir "$logdir/v54" \
  > "$logdir/v54-regressions.log" 2>&1
rc=$?
record v54-regressions "$ranks" all 1 "$rc" $((SECONDS - t0)) "$([ $rc = 0 ] && echo PASS || echo FAIL)"
t0=$SECONDS
bash /src/scripts/mumps/run-v66-regressions.sh --ranks "$ranks" --logdir "$logdir/v66" \
  > "$logdir/v66-regressions.log" 2>&1
rc=$?
record v66-regressions "$ranks" all 1 "$rc" $((SECONDS - t0)) "$([ $rc = 0 ] && echo PASS || echo FAIL)"
# D-121: every scenario that compares with an oracle record prints one line
# per launch, `ORACLE <scenario>: compared ...` or `ORACLE <scenario>: not
# compared: <reason>` (outside the records' environment, e.g. on aarch64);
# the counts are part of the summary, and each scenario not compared is named.
oracle_counts() {
  local lines
  lines="$(grep -h -r -E '^[[:space:]]*ORACLE [A-Za-z0-9_]+: (compared|not compared)' "$logdir" 2>/dev/null || true)"
  echo "== oracle comparisons: $(printf '%s\n' "$lines" | grep -c ': compared' || true) compared, $(printf '%s\n' "$lines" | grep -c ': not compared' || true) not compared (lines of all launches)"
  printf '%s\n' "$lines" | grep ': not compared' | sed -E 's/^[[:space:]]*ORACLE ([A-Za-z0-9_]+): not compared: (.*)$/   not compared: \1 (\2)/' | sort | uniq -c || true
}
oracle_counts
echo "== results: $results (failures=$failures)"
exit $((failures > 0))
