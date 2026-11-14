#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# The PETSc test tier, run INSIDE the akantu-rs-petsc image (use
# scripts/petsc/run-petsc-tests.sh from the host; amd64 host).
#
#   scripts/petsc/run-petsc-tier.sh [--ranks "1 2 4"] [--levels "funneled serialized"] [--skip-lint]
#
# Steps: fmt check, clippy -D warnings of akantu-comm / akantu-linalg /
# akantu-solver / akantu-phase-field / akantu-solid / akantu-couplers with
# their `petsc` features, `cargo test` of akantu-linalg and akantu-solver
# with the feature (unit tests; the petsc_mpi binary is not a harness test),
# a build of the petsc_mpi acceptance binary, then for every MPI thread
# level and rank count N: `mpirun -np N petsc_mpi` (exit 0 and
# "failed=0"), then the isolated regression cases of the fix round
# (scripts/petsc/run-petsc-regressions.sh, same ranks and levels), then the
# models on the PETSc DOF manager (akantu-couplers' petsc_models
# binary, `mpirun -np N`, same ranks and levels) and its isolated cases
# (`AKANTU_PETSC_CASE`), then
# the PETSc instances of upstream's patch tests (akantu-solid's
# patch_petsc binary, `mpirun -np 1`, when 1 is among the ranks). Every
# mpirun is wrapped in `timeout`; the binary also has a
# watchdog. Logs, provenance (toolchain, MPI, PETSc, git head, image, BLAS
# pin, CPU) and a results table go to /target/petsc-logs/<UTC timestamp>/.
# The BLAS pin (D-050; single definition
# scripts/mumps/blas-pin.sh, shared with the MUMPS tier) is (re-)established
# here, passed to every rank (`mpirun -x`) and recorded in provenance.txt;
# petsc_mpi's `blas_pin` scenario fails if it is absent or different, and
# `oracle_serial` compares bitwise only under the recorded pin.
set -uo pipefail
ranks="1 2 4"
levels="funneled serialized"
lint=1
while [ $# -gt 0 ]; do
  case "$1" in
    --ranks) ranks="$2"; shift 2 ;;
    --levels) levels="$2"; shift 2 ;;
    --skip-lint) lint=0; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
# shellcheck source=scripts/mumps/blas-pin.sh
. /src/scripts/mumps/blas-pin.sh
akantu_blas_export
stamp=$(date -u +%Y%m%dT%H%M%SZ)
logdir=/target/petsc-logs/$stamp
mkdir -p "$logdir"
results="$logdir/results.tsv"
printf 'step\tranks\tlevel\trc\tseconds\tverdict\n' > "$results"
failures=0
record() { # step ranks level rc seconds verdict
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$@" >> "$results"
  if [ "$6" != PASS ]; then failures=$((failures + 1)); fi
  printf '%-28s ranks=%-3s level=%-11s rc=%-3s %6ss  %s\n' "$@"
}
step() { # name log cmd...
  local name=$1 log=$2; shift 2
  local t0=$SECONDS
  "$@" > "$logdir/$log" 2>&1
  local rc=$?
  local v=PASS; [ $rc -eq 0 ] || v=FAIL
  record "$name" - - "$rc" $((SECONDS - t0)) "$v"
}
{
  echo "git_head=${AKANTU_RS_GIT_HEAD:-unknown}"
  echo "image=${AKANTU_RS_MPI_IMAGE_ID:-unknown}"
  rustc --version; cargo --version
  mpirun --version | head -1
  echo "petsc=$(pkg-config --modversion PETSc)"
  dpkg-query -W -f='${Package}\t${Version}\n' 'libpetsc-real3.22' 'libopenmpi40' 'libopenblas0-pthread' 2>/dev/null
  uname -m
  echo "OPENBLAS_NUM_THREADS=${OPENBLAS_NUM_THREADS:-} OMP_NUM_THREADS=${OMP_NUM_THREADS:-}"
  echo "platform=$(uname -m) OPENBLAS_CORETYPE=${OPENBLAS_CORETYPE:-<unset: run-time selection>} (D-050 pin: ${AKANTU_BLAS_CORETYPE:-<unset>})"
  echo "cpu=$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ //')"
} > "$logdir/provenance.txt" 2>&1
cat "$logdir/provenance.txt"
pkgs="-p akantu-comm -p akantu-linalg -p akantu-solver -p akantu-phase-field -p akantu-solid -p akantu-couplers"
features="akantu-solver/petsc,akantu-phase-field/petsc,akantu-solid/petsc,akantu-couplers/petsc"
if [ $lint -eq 1 ]; then
  step fmt fmt.log cargo fmt --all --check
  # shellcheck disable=SC2086 # $pkgs is a list of -p options on purpose
  step clippy clippy.log cargo clippy $pkgs --features "$features" --all-targets --locked -- -D warnings
fi
step cargo-test cargo-test.log cargo test -p akantu-linalg -p akantu-solver --features akantu-solver/petsc --locked
step build build.log cargo build -p akantu-solver --features petsc --test petsc_mpi --locked --message-format=json-render-diagnostics
bin=$(grep -o '"executable":"[^"]*petsc_mpi[^"]*"' "$logdir/build.log" | tail -1 | sed 's/"executable":"\(.*\)"/\1/')
if [ -z "$bin" ] || [ ! -x "$bin" ]; then
  echo "petsc_mpi binary not found in the build artifacts" | tee -a "$logdir/build.log"
  record petsc_mpi-binary - - 1 0 FAIL
else
  for level in $levels; do
    for n in $ranks; do
      log="petsc_mpi-np$n-$level.log"
      t0=$SECONDS
      AKANTU_PETSC_THREAD_LEVEL=$level timeout --kill-after=10 1200 \
        mpirun -n "$n" -x AKANTU_PETSC_THREAD_LEVEL -x OPENBLAS_NUM_THREADS -x OMP_NUM_THREADS \
          ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} "$bin" > "$logdir/$log" 2>&1
      rc=$?
      v=PASS
      if [ $rc -ne 0 ] || ! grep -q '^petsc_mpi: failed=0' "$logdir/$log"; then v=FAIL; fi
      record petsc_mpi "$n" "$level" "$rc" $((SECONDS - t0)) "$v"
    done
  done
  # Isolated regression cases of the fix round, one mpirun each.
  t0=$SECONDS
  bash /src/scripts/petsc/run-petsc-regressions.sh --bin "$bin" --ranks "$ranks" \
    --levels "$levels" > "$logdir/regressions.log" 2>&1
  rc=$?
  v=PASS; [ $rc -eq 0 ] || v=FAIL
  record regressions - - "$rc" $((SECONDS - t0)) "$v"
  grep -E 'FAIL|regression failures' "$logdir/regressions.log" | head -40
fi
# The models on the PETSc DOF manager.
step build-models build-models.log cargo build -p akantu-couplers --features petsc --test petsc_models --locked --message-format=json-render-diagnostics
mbin=$(grep -o '"executable":"[^"]*petsc_models[^"]*"' "$logdir/build-models.log" | tail -1 | sed 's/"executable":"\(.*\)"/\1/')
if [ -z "$mbin" ] || [ ! -x "$mbin" ]; then
  echo "petsc_models binary not found in the build artifacts" | tee -a "$logdir/build-models.log"
  record petsc_models-binary - - 1 0 FAIL
else
  for level in $levels; do
    for n in $ranks; do
      log="petsc_models-np$n-$level.log"
      t0=$SECONDS
      AKANTU_PETSC_THREAD_LEVEL=$level timeout --kill-after=10 1200 \
        mpirun -n "$n" -x AKANTU_PETSC_THREAD_LEVEL -x OPENBLAS_NUM_THREADS -x OMP_NUM_THREADS \
          ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} "$mbin" > "$logdir/$log" 2>&1
      rc=$?
      v=PASS
      if [ $rc -ne 0 ] || ! grep -q '^petsc_models: failed=0' "$logdir/$log"; then v=FAIL; fi
      record petsc_models "$n" "$level" "$rc" $((SECONDS - t0)) "$v"
    done
  done
  # Isolated cases of the models:
  # one mpirun each; the program holds no MPI handle of its own and starts
  # no PETSc session before the case (the serial-mesh rule's process-wide
  # world uses the default MPI_THREAD_FUNNELED). A case passes iff the job
  # exits 0 and printed `CASE PASS <case>`.
  for spec in "pt02_serial_models_in_sequence:1" "pt02_refusal_recoverable:2 4"; do
    icase=${spec%%:*}
    for n in ${spec#*:}; do
      case " $ranks " in *" $n "*) ;; *) continue ;; esac
      log="petsc_models-$icase-np$n.log"
      t0=$SECONDS
      AKANTU_PETSC_CASE=$icase timeout --kill-after=10 700 \
        mpirun -n "$n" -x AKANTU_PETSC_CASE -x OPENBLAS_NUM_THREADS -x OMP_NUM_THREADS \
          ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} "$mbin" > "$logdir/$log" 2>&1
      rc=$?
      v=PASS
      if [ $rc -ne 0 ] || ! grep -q "^CASE PASS $icase " "$logdir/$log"; then v=FAIL; fi
      record "$icase" "$n" funneled "$rc" $((SECONDS - t0)) "$v"
    done
  done
fi
# The PETSc instances of upstream's patch tests (TEST-050…053:
# `petsc/auto[petsc]`, `petsc/petsc_snes[petsc]`): one process, as upstream's
# serial patch_test_linear (a serial mesh's PETSc DOF manager runs on the MPI
# world of one process, D-092). Passes iff the job exits 0 and printed
# `patch_petsc: failed=0`.
case " $ranks " in
  *" 1 "*)
    step build-patch build-patch.log cargo build -p akantu-solid --features petsc --test patch_petsc --locked --message-format=json-render-diagnostics
    pbin=$(grep -o '"executable":"[^"]*patch_petsc[^"]*"' "$logdir/build-patch.log" | tail -1 | sed 's/"executable":"\(.*\)"/\1/')
    if [ -z "$pbin" ] || [ ! -x "$pbin" ]; then
      echo "patch_petsc binary not found in the build artifacts" | tee -a "$logdir/build-patch.log"
      record patch_petsc-binary - - 1 0 FAIL
    else
      t0=$SECONDS
      timeout --kill-after=10 2700 \
        mpirun -n 1 -x OPENBLAS_NUM_THREADS -x OMP_NUM_THREADS \
          ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} "$pbin" > "$logdir/patch_petsc.log" 2>&1
      rc=$?
      v=PASS
      if [ $rc -ne 0 ] || ! grep -q '^patch_petsc: failed=0' "$logdir/patch_petsc.log"; then v=FAIL; fi
      record patch_petsc 1 funneled "$rc" $((SECONDS - t0)) "$v"
      grep -E '^(PASS|FAIL|SKIP) ' "$logdir/patch_petsc.log"
    fi
    ;;
esac
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
echo "logs: $logdir"
echo "failures=$failures"
[ $failures -eq 0 ]
