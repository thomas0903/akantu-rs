#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-010, CI-020;
# D-085: the upstream valgrind job and the ASAN/LSAN
# suppressions have no counterpart for the port's FFI `unsafe` code).
#
# Memcheck run, INSIDE a memcheck image (docker/memcheck, built by
# ci/tiers/memcheck.sh; repository read-only at /src):
#
#   ci/memcheck/run-in-image.sh <mpi|mumps|petsc>
#
# What it runs under `valgrind --tool=memcheck` (every process of every
# `mpirun`; Rust test binaries built in the current profile, line tables
# kept):
#   mpi    the real-MPI acceptance binaries of akantu-comm (mpi_acceptance,
#          the mpi_lifecycle / mpi_self_messages / mpi_progress scenarios) at
#          2 ranks, the unit tests of akantu-comm with the mpi feature
#          (`mpi_backend.rs` FFI), the libm FFI of akantu-parser (`cmath.rs`)
#          and the C ABI crate akantu-capi;
#   mumps  the MUMPS acceptance binary (akantu-linalg `mumps_mpi`, the FFI of
#          `mumps/mod.rs`) at 2 ranks and the three MPI thread levels, and the regression scenarios of
#          scripts/mumps/run-v66-regressions.sh (--valgrind), the Scotch
#          binding (`akantu-mesh/src/scotch.rs`) through its two integration
#          tests;
#   petsc  the PETSc acceptance binary (akantu-solver `petsc_mpi`) at 2 ranks,
#          thread levels funneled and serialized.
#
# Verdict per run (ci/memcheck/lib.sh `mc_judge`): valgrind's
# `--error-exitcode=99` (any memcheck error that the suppression file
# ci/memcheck/openmpi.supp does not suppress), exactly one "ERROR SUMMARY" per
# rank, the program's own result (exit 0 and its "failed=0" / "SUMMARY ...
# failed=0" line). The MUMPS regression launches (run-v66-regressions.sh) run
# under THESE valgrind words (VALGRIND_CMD) and are judged by the same rule,
# launch by launch. The valgrind words include
# `--fair-sched=yes` (round-robin thread scheduling: with the default lock a
# thread that waits for another's progress, e.g. the busy-waiting worker of
# the PETSc refusal scenario, often never runs while the main thread is in a native
# call) and `-v` (the count of every used suppression is
# recorded per run). A crate whose build fails or whose test-executable list
# is empty is a failure, never an empty loop. Leak checking is off (`--leak-check=no`: Open MPI, PMIx and Rust
# statics are reachable-at-exit by design; the unsafe code's risk is invalid
# access, use of uninitialised memory and double free, which memcheck reports
# regardless). Before the real runs a planted defect (an uninitialised buffer
# written to a syscall, a read past a heap block) must be reported: the
# negative control `planted-defect`. A run that cannot be started (binary missing, valgrind
# absent) is a failure, never a skip. Results: a table on stdout and
# $RESULTS (default /target/memcheck-logs/<UTC>/results.tsv). The exit code
# is 0 only when every run passed.
#
# A failing library test is REPORTED, never suppressed: add a suppression
# only for an error whose whole stack is inside a third-party shared library
# (Open MPI, PMIx, libevent, hwloc, libc loader) and justify it in
# ci/memcheck/openmpi.supp.
set -uo pipefail
kind="${1:?usage: run-in-image.sh <mpi|mumps|petsc>}"
case "$kind" in mpi | mumps | petsc) ;; *)
  echo "run-in-image.sh: unknown kind '$kind' (mpi mumps petsc)" >&2
  exit 2
  ;;
esac
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=ci/memcheck/lib.sh
. "$here/lib.sh"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
logdir="${RESULTS_DIR:-/target/memcheck-logs/$kind-$stamp}"
mkdir -p "$logdir"
results="$logdir/results.tsv"
printf 'run\tranks\trc\tseconds\terrors\tverdict\tsuppressions\n' > "$results"
supp="$here/openmpi.supp"
fails=0
# mpi_progress/delayed_sender_completes under valgrind: per-operation deadline
# and in-process watchdog (the reason is written at the
# progress loop below). One definition for the loop and the provenance.
dsc_timeout_s=60 dsc_watchdog_s=300

# One word per valgrind option; the program's own stderr/stdout stay in the log.
VG=(valgrind --tool=memcheck --error-exitcode=99 --leak-check=no --track-origins=no
  --num-callers=40 --error-limit=no --fair-sched=yes -v "--suppressions=$supp" --gen-suppressions=no)
used_total="$logdir/used-suppressions.txt"
: > "$used_total"
# valgrind needs the loader to find its tool binaries; libc6-dbg is installed.

{
  echo "kind=$kind"
  echo "date=$stamp"
  echo "valgrind=$(valgrind --version)"
  echo "libc6-dbg=$(dpkg-query -W -f='${Version}' libc6-dbg 2> /dev/null || echo absent)"
  echo "rustc=$(rustc --version)"
  echo "mpirun=$(mpirun --version 2> /dev/null | head -1)"
  echo "image_id=${AKANTU_RS_MPI_IMAGE_ID:-unknown}"
  echo "git_head=${AKANTU_RS_GIT_HEAD:-unknown}"
  echo "suppressions=$supp sha256=$(sha256sum < "$supp" | cut -d' ' -f1)"
  echo "suppression_entries=$(grep -c '^{' "$supp")"
  env | grep -E '^(OMPI_MCA_|PRTE_MCA_|OPENBLAS_|SCOTCH_|AKANTU_MPI_)' | sort
  echo "mpi_progress/delayed_sender_completes: AKANTU_MPI_TEST_TIMEOUT_S=$dsc_timeout_s AKANTU_MPI_WATCHDOG_S=$dsc_watchdog_s"
} | tee "$logdir/provenance.txt"

# record <run> <ranks> <rc> <seconds> <log> <expect-regex|->: judge one run (mc_judge)
record() {
  local name="$1" ranks="$2" rc="$3" secs="$4" log="$5" want="$6" verdict errs supp j
  j="$(mc_judge "$ranks" "$rc" "$log" "$want")"
  verdict="${j%% *}"
  errs="${j#* }"
  supp="$(mc_suppressions "$log")"
  [ -z "$supp" ] || printf '%s\n' "$supp" >> "$used_total"
  [ "$verdict" = PASS ] || fails=$((fails + 1))
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$ranks" "$rc" "$secs" "$errs" "$verdict" "${supp:--}" >> "$results"
  printf '%-52s ranks=%-2s rc=%-3s %5ss errors=%-4s %s\n' "$name" "$ranks" "$rc" "$secs" "$errs" "$verdict"
}

# build_artifacts <label> <cargo test args...>: the test executables of a
# --no-run build into ARTS; a failed build or an empty list is a recorded
# failure (a binary that was not built is never "nothing to run")
build_artifacts() {
  local why
  ARTS=""
  if mc_build_artifacts "$@" > "$logdir/build-why.txt"; then
    ARTS="$MC_ARTS"
    return 0
  fi
  why="$(cat "$logdir/build-why.txt")"
  printf 'build/%s\t-\t-\t0\tbuild-failed\tFAIL\t-\n' "$1" >> "$results"
  printf '%-52s %s\n' "build/$1" "FAIL: $why"
  fails=$((fails + 1))
  return 1
}
bin_of() { # name <artifact lines on stdin>
  awk -v n="$1" '$1 == n { p = $2 } END { print p }'
}

MCA=() # extra mpirun options of the next vg_mpi (e.g. --mca btl tcp,self)
# vg_mpi <run> <ranks> <timeout> <expect> <binary> [args...]
vg_mpi() {
  local name="$1" n="$2" tmo="$3" want="$4" bin="$5" log t0 rc
  shift 5
  log="$logdir/$(printf '%s' "$name" | tr -c 'A-Za-z0-9._-' '_')-np$n.log"
  t0=$SECONDS
  timeout -k 10 "$tmo" mpirun -np "$n" --oversubscribe --bind-to none ${MCA[@]+"${MCA[@]}"} \
    -x OPENBLAS_NUM_THREADS ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} -x SCOTCH_PTHREAD_NUMBER \
    ${AKANTU_MUMPS_THREAD_LEVEL:+-x AKANTU_MUMPS_THREAD_LEVEL} ${AKANTU_PETSC_THREAD_LEVEL:+-x AKANTU_PETSC_THREAD_LEVEL} \
    ${AKANTU_MPI_TEST_TIMEOUT_S:+-x AKANTU_MPI_TEST_TIMEOUT_S} ${AKANTU_MPI_WATCHDOG_S:+-x AKANTU_MPI_WATCHDOG_S} \
    "${VG[@]}" "$bin" "$@" > "$log" 2>&1
  rc=$?
  record "$name" "$n" "$rc" $((SECONDS - t0)) "$log" "$want"
}
# vg_one <run> <timeout> <expect> <binary> [args...]: no launcher (a plain test binary)
vg_one() {
  local name="$1" tmo="$2" want="$3" bin="$4" log t0 rc
  shift 4
  log="$logdir/$(printf '%s' "$name" | tr -c 'A-Za-z0-9._-' '_').log"
  t0=$SECONDS
  timeout -k 10 "$tmo" "${VG[@]}" "$bin" "$@" > "$log" 2>&1
  rc=$?
  record "$name" 1 "$rc" $((SECONDS - t0)) "$log" "$want"
}
missing() { # <run> <what>: a binary that was not built is a failure
  printf '%s\t-\t-\t0\tbinary-missing\tFAIL\n' "$1" >> "$results"
  printf '%-52s %s\n' "$1" "FAIL: $2 not found among the build artifacts"
  fails=$((fails + 1))
}

# Negative control: a planted defect must be reported with the same options
# and suppression file, so that a checker that stopped working (a missing
# valgrind tool, a suppression that is too broad, a wrapper that swallows the
# exit code) cannot make every run below a pass. The program writes an
# uninitialised heap buffer to /dev/null and reads one byte past a heap block.
planted_control() {
  local c="$logdir/planted.c" b="$logdir/planted" log="$logdir/planted-defect.log" t0=$SECONDS rc errs v=PASS
  cat > "$c" << 'EOC'
#include <stdlib.h>
#include <unistd.h>
#include <fcntl.h>
int main(void) {
  char *u = malloc(64);
  int fd = open("/dev/null", O_WRONLY);
  if (write(fd, u, 64) < 0) return 3; /* uninitialised bytes reach a syscall */
  char *h = malloc(8);
  volatile char x = h[8]; /* one byte past the block */
  (void)x;
  free(h);
  free(u);
  return 0;
}
EOC
  if ! cc -g -O0 -o "$b" "$c" 2> "$log"; then
    printf '%s\t-\t-\t0\tno-compiler\tFAIL\n' planted-defect >> "$results"
    printf '%-52s %s\n' planted-defect "FAIL: cannot compile the control program (see $log)"
    fails=$((fails + 1))
    return
  fi
  "${VG[@]}" "$b" > "$log" 2>&1
  rc=$?
  errs="$(grep -a 'ERROR SUMMARY:' "$log" | sed -E 's/.*ERROR SUMMARY: ([0-9]+) errors.*/\1/' | awk '{ s += $1 } END { print s + 0 }')"
  # expected: valgrind's exit code 99 and at least the two planted errors
  { [ "$rc" = 99 ] && [ "$errs" -ge 2 ]; } || v=FAIL
  [ "$v" = PASS ] || fails=$((fails + 1))
  printf '%s\t1\t%s\t%s\t%s\t%s\n' planted-defect "$rc" $((SECONDS - t0)) "$errs (expected: >= 2, rc 99)" "$v" >> "$results"
  printf '%-52s rc=%-3s errors=%-4s (detected as expected: rc 99, >= 2) %s\n' planted-defect "$rc" "$errs" "$v"
}
# v66_regressions <ranks> <levels> [script options...]: the MUMPS
# regression scenarios (scripts/mumps/run-v66-regressions.sh --valgrind) under the
# tier's own valgrind words (VALGRIND_CMD) into $V66_DIR; echoes nothing, sets
# V66_RC (the script's exit code) and V66_ROWS (launches recorded)
V66_DIR="" V66_RC=0 V66_ROWS=0
v66_regressions() {
  local ranks="$1" levels="$2"
  shift 2
  V66_DIR="$logdir/v66${V66_SUFFIX:-}"
  VALGRIND_CMD="${VG[*]}" bash /src/scripts/mumps/run-v66-regressions.sh --valgrind --ranks "$ranks" --levels "$levels" \
    --logdir "$V66_DIR" "$@" > "$V66_DIR.log" 2>&1
  V66_RC=$?
  V66_ROWS=0
  [ ! -f "$V66_DIR/v66.tsv" ] || V66_ROWS="$(awk -F '\t' 'NR > 1' "$V66_DIR/v66.tsv" | wc -l | tr -d ' ')"
}
# v66_judged: record every launch of the last v66_regressions run (mc_judge:
# exit code, one ERROR SUMMARY per rank, no memcheck error, "failed=0")
v66_judged() {
  local sname sranks slevel src ssecs
  if [ "$V66_RC" != 0 ]; then
    printf 'v66-regressions\t-\t%s\t0\tscript-failed\tFAIL\t-\n' "$V66_RC" >> "$results"
    printf '%-52s rc=%-3s %s\n' v66-regressions "$V66_RC" "FAIL: the script reported a failing launch or could not run (see $V66_DIR.log)"
    fails=$((fails + 1))
  fi
  if [ "$V66_ROWS" = 0 ]; then
    printf 'v66-regressions\t-\t-\t0\tno-launch\tFAIL\t-\n' >> "$results"
    printf '%-52s %s\n' v66-regressions "FAIL: the script recorded no launch"
    fails=$((fails + 1))
    return
  fi
  while IFS="$(printf '\t')" read -r sname sranks slevel src ssecs _; do
    [ "$sname" != scenario ] || continue
    record "v66/$sname/$slevel" "$sranks" "$src" "$ssecs" "$V66_DIR/$sname-n$sranks-$slevel.log" 'failed=0'
  done < "$V66_DIR/v66.tsv"
}

# Negative control of the regression launcher: a launch whose
# program prints "failed=0", exits 0 and has a memcheck error that is not an
# "Invalid read / write / free" (a conditional jump on an uninitialised
# value) must be reported by the script AND judged FAIL by the tier. The
# script's own words (no --error-exitcode) and its grep passed such a launch.
planted_v66_control() {
  local c="$logdir/planted-v66.c" b="$logdir/planted-v66" t0=$SECONDS v=PASS n judged
  cat > "$c" << 'EOC'
#include <stdio.h>
#include <stdlib.h>
int main(void) {
  int *p = malloc(4 * sizeof(int));
  if (p[1] > 3) puts("branch"); /* conditional jump on an uninitialised value */
  free(p);
  puts("planted: failed=0");
  return 0;
}
EOC
  if ! cc -g -O0 -o "$b" "$c" 2> "$logdir/planted-v66.log"; then
    printf '%s\t-\t-\t0\tno-compiler\tFAIL\t-\n' planted-v66-launch >> "$results"
    printf '%-52s %s\n' planted-v66-launch "FAIL: cannot compile the control program"
    fails=$((fails + 1))
    return
  fi
  V66_SUFFIX=-planted V66_REGR_BIN="$b" v66_regressions 2 serialized --only pe01_refusal_forward
  judged=FAIL
  if [ "$V66_ROWS" = 1 ]; then
    n="$(mc_judge 2 0 "$V66_DIR/pe01_refusal_forward-n2-serialized.log" 'failed=0')"
    judged="${n%% *}"
  fi
  # expected: the script exits non-zero and the tier's judgement is FAIL
  { [ "$V66_RC" != 0 ] && [ "$judged" = FAIL ]; } || v=FAIL
  [ "$v" = PASS ] || fails=$((fails + 1))
  printf '%s\t2\t%s\t%s\t%s\t%s\t-\n' planted-v66-launch "$V66_RC" $((SECONDS - t0)) "script rc $V66_RC, judged $judged (expected: rc != 0, FAIL)" "$v" >> "$results"
  printf '%-52s script rc=%-3s judged=%-4s (detected as expected: rc != 0, FAIL) %s\n' planted-v66-launch "$V66_RC" "$judged" "$v"
}
planted_control
[ "$kind" != mumps ] || planted_v66_control

lifecycle_scenarios="rust_first_funneled last_guard_on_worker_funneled ar3_03_two_barriers_funneled
  ar3_03_main_drain_serialized nested_release_funneled destruction_owns_last_guard
  host_first_imported thread_level_negotiation"
progress_scenarios="delayed_sender_completes serialized_progress serialized_crossed_dup
  serialized_crossed_split serialized_exclusive_rule serialized_creation_rule"
self_scenarios="self_4k self_semantics cross_4k"

case "$kind" in
  mpi)
    if build_artifacts comm -p akantu-comm --features mpi; then
      arts="$ARTS"
      acc="$(printf '%s\n' "$arts" | bin_of mpi_acceptance)"
      life="$(printf '%s\n' "$arts" | bin_of mpi_lifecycle)"
      prog="$(printf '%s\n' "$arts" | bin_of mpi_progress)"
      selfm="$(printf '%s\n' "$arts" | bin_of mpi_self_messages)"
      lib="$(printf '%s\n' "$arts" | bin_of akantu_comm)"
      if [ -n "$acc" ]; then vg_mpi mpi_acceptance 2 3000 'SUMMARY ranks=2 scenarios=[0-9]+ failed=0' "$acc"; else missing mpi_acceptance "mpi_acceptance"; fi
      if [ -n "$life" ]; then
        for s in $lifecycle_scenarios; do
          vg_mpi "mpi_lifecycle/$s" 2 900 "LIFECYCLE PASS $s" "$life" "$s"
        done
      else missing mpi_lifecycle "mpi_lifecycle"; fi
      if [ -n "$prog" ]; then
        MCA=(--mca btl "tcp,self")
        for s in $progress_scenarios; do
          # delayed_sender_completes: the per-operation deadline (a no-hang
          # guard, AKANTU_MPI_TEST_TIMEOUT_S, 20 s by default and in the mpi
          # tier) is 60 s under valgrind. Its windows hold the library's
          # element-by-element packing and unpacking of a 16 MiB Vec<u8> in the
          # debug build, 10.7 s and 13.5 s under memcheck at one CPU against
          # milliseconds natively (docs/TESTING.md); with
          # 20 s the 2-vCPU GitHub runner timed out although the sender's delay
          # is 1.5 s, and at a 0.5-CPU cap 1 of 3 runs still did after the
          # scenario's own work left the windows. The in-process watchdog
          # (AKANTU_MPI_WATCHDOG_S, 120 s) stays behind the deadlines: 300 s
          # (the whole scenario took 74-96 s at 0.5 CPU, more than 120 s at
          # 0.25). The other scenarios keep their deadlines and watchdog.
          t="" w=""
          [ "$s" != delayed_sender_completes ] || { t=$dsc_timeout_s; w=$dsc_watchdog_s; }
          AKANTU_MPI_TEST_TIMEOUT_S="$t" AKANTU_MPI_WATCHDOG_S="$w" vg_mpi "mpi_progress/$s" 2 900 "PROGRESS PASS $s" "$prog" "$s"
        done
        MCA=()
      else missing mpi_progress "mpi_progress"; fi
      if [ -n "$selfm" ]; then
        for s in $self_scenarios; do vg_mpi "mpi_self_messages/$s" 2 900 "SELF PASS $s" "$selfm" "$s"; done
      else missing mpi_self_messages "mpi_self_messages"; fi
      if [ -n "$lib" ]; then vg_one "akantu-comm unit tests (feature mpi)" 1800 'test result: ok' "$lib" --test-threads=1; else missing akantu-comm-lib "akantu-comm unit-test binary"; fi
    fi
    # libm FFI (akantu-parser cmath.rs) and the C ABI: every test binary of the crates
    for crate in akantu-parser akantu-capi; do
      build_artifacts "$crate" -p "$crate" || continue
      while read -r tname exe; do
        [ -n "$exe" ] || continue
        vg_one "$crate/$tname" 1800 'test result: ok' "$exe" --test-threads=1
      done <<EOF
$ARTS
EOF
    done
    ;;
  mumps)
    if build_artifacts linalg -p akantu-linalg --features mumps; then
      la="$(printf '%s\n' "$ARTS" | bin_of mumps_mpi)"
      if [ -n "$la" ]; then
        for level in funneled serialized multiple; do
          AKANTU_MUMPS_THREAD_LEVEL=$level vg_mpi "mumps_mpi/$level" 2 3000 'failed=0' "$la"
        done
      else missing mumps_mpi "mumps_mpi"; fi
    fi
    # The script's valgrind mode with the
    # tier's own words (VALGRIND_CMD: --error-exitcode=99, the suppression
    # file, --fair-sched=yes), every launch judged like any other run (exit
    # code, one ERROR SUMMARY per rank, no error) from its row of v66.tsv.
    V66_SUFFIX="" v66_regressions 2 "funneled serialized"
    v66_judged
    # Scotch binding: its integration tests, single process, one Scotch
    # thread. Both named targets are required (a renamed target would
    # otherwise leave this loop empty).
    if build_artifacts mesh -p akantu-mesh --features scotch; then
      for tname in upstream_mesh_partitionate_scotch mesh2_scotch_oracle; do
        exe="$(printf '%s\n' "$ARTS" | bin_of "$tname")"
        if [ -n "$exe" ]; then
          SCOTCH_PTHREAD_NUMBER=1 vg_one "akantu-mesh scotch/$tname" 3000 'test result: ok' "$exe" --test-threads=1
        else missing "akantu-mesh scotch/$tname" "$tname"; fi
      done
    fi
    ;;
  petsc)
    if build_artifacts solver -p akantu-solver --features petsc; then
      pe="$(printf '%s\n' "$ARTS" | bin_of petsc_mpi)"
      if [ -n "$pe" ]; then
        for level in funneled serialized; do
          AKANTU_PETSC_THREAD_LEVEL=$level vg_mpi "petsc_mpi/$level" 2 3000 '^petsc_mpi: failed=0' "$pe"
        done
      else missing petsc_mpi "petsc_mpi"; fi
    fi
    # the models on the PETSc DOF manager (akantu-couplers)
    if build_artifacts couplers -p akantu-couplers --features petsc; then
      pm="$(printf '%s\n' "$ARTS" | bin_of petsc_models)"
      if [ -n "$pm" ]; then
        for level in funneled serialized; do
          AKANTU_PETSC_THREAD_LEVEL=$level vg_mpi "petsc_models/$level" 2 3000 '^petsc_models: failed=0' "$pm"
        done
      else missing petsc_models "petsc_models"; fi
    fi
    ;;
esac

# Suppressions that fired, summed over every run (valgrind -v): a suppression
# that starts hiding many contexts must be visible in the results.
{
  echo "== suppressions used (name=count, all runs)"
  tr ' ' '\n' < "$used_total" | awk -F= 'NF == 2 { c[$1] += $2 } END { for (n in c) printf "%s=%d\n", n, c[n] }' | sort
} | tee "$logdir/used-suppressions-total.txt"

echo "== memcheck $kind: $results (failures=$fails)"
[ "$fails" = 0 ]
