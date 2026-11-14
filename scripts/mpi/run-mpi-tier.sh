#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# The MPI test tier of akantu-comm (INT-009), run INSIDE the
# akantu-rs-mpi image (use scripts/mpi/run-tests.sh from the host).
#
#   scripts/mpi/run-mpi-tier.sh [--ranks "1 2 4"] [--repeat N] [--repeat-abort N] [--skip-lint]
#
# Steps: fmt + clippy (feature mpi), `cargo test --features mpi` (unit tests
# and the MPI binaries as singletons), a build of every real-MPI binary and
# of the abort interception shim, then for every rank count N:
#   * mpi_acceptance (exit 0 and "failed=0"), repeated --repeat times;
#   * every mpi_abort_paths scenario with the MPI_Abort interception shim
#     LD_PRELOADed (exit 71 = TRANSPORT_ABORT_CODE, per-rank evidence and
#     abort legality checked by check_evidence.py, every rank process ended
#     within 30 s, well before the peers' 60 s deadline: measured by
#     abort_watch.py on the rank processes, not on the launcher's return),
#     each run --repeat-abort times (default 1;
#     the launcher teardown hang is rare, see repeat-abort-scenario.sh);
#   * the PMIx fallback under adversarial PMIx_Abort interposers
#     (scripts/mpi/pmix_abort_shims.c): worker_abort_funneled
#     with a PMIx_Abort that never returns and with one that fails at once,
#     in forms that avoid the PRRTE teardown race (the
#     race-prone form with the interposer on every rank is kept only in
#     repeat-abort-scenario.sh, PMIX_FORM=all): at 1 rank as singletons
#     without a launcher (exit 71, evidence, the interposer reached, < 30 s;
#     the blocking one only after the fallback's 2 s bound), at N >= 2 ranks
#     with the interposer on rank 0 only (preload-rank0.sh, standing in for
#     the MPMD form, which the image's prterun cannot launch; the other
#     ranks make the real PMIx_Abort request: exit 71, evidence, < 30 s, the
#     number of interposer calls recorded), each --repeat-abort times; and
#     the singleton without a launcher and without interposer (exit 71);
#   * the mpi_lifecycle scenarios, the mpi_progress scenarios (over TCP;
#     payload progress and the SERIALIZED two-thread choreographies)
#     and the mpi4py-first probe;
#   * the mpi_self_messages scenarios (truncated and
#     exact messages to self and across processes, 4 KiB / 64 KiB / 1 MiB,
#     under the binary's red-zone allocator; cross-process ones over shared
#     memory and over TCP);
#   * the facade's process-exit scenarios (decision D-067;
#     crates/akantu/tests/initialize_mpi.rs modes finalize /
#     forget / reinit and crates/akantu/tests/exit_hook_mpi.rs): exit status
#     as expected (0 for forget, finalize, hostfirst, hostfirst-universe; 101
#     panic; 1 err-return; 3 exit-nonzero; 1 serial-held, with the hook's
#     emergency note; 1 worker-exit), every rank process ended within the
#     abort limit (30 s, measured as for the abort scenarios).
# Then the performance gate (when the
# rank list has 2 / 4): the release example `mpi_bench calibrated` over
# shared memory — 1 MiB and 16 MiB ping-pong at 2 ranks, 8 KiB halo
# exchange at 4 ranks — measures raw MPI and akantu-comm in ONE job (same
# processes, placement and memory state) in 9 interleaved blocks, plus, for
# the ping-pongs, a calibration block: the public API's own buffer work for
# the same message (send + receive_vec to self, no MPI). Gate: the median
# over the blocks of the backend ratio (akantu - calibration) / raw within
# PERF_FACTOR (default 8, AKANTU_PERF_FACTOR) — what the backend's waits and
# matching engine add to raw MPI's exchange the earlier polling back-off:
# hundreds). The plain ratio akantu / raw is reported in perf.tsv, not
# gated: the per-message copies, allocations and the allocator's page
# faults dominate it and move it between about 5 and 11 for the 16 MiB
# ping-pong with the allocator's state.
# Provenance also records whether the image's MPI overruns truncated
# self-receives (scripts/mpi/openmpi_self_truncation.c).
# Every exit status is the command's own: a failed lint,
# test or build step is a failure, binaries are taken from the current
# build's artifact list only. Every mpirun is wrapped in `timeout`; the
# binaries also carry per-operation deadlines and an in-process watchdog.
# Logs, provenance (toolchain, MPI, MCA settings, git head, image) and a
# results table go to /target/mpi-logs/<UTC timestamp>/.
set -uo pipefail
ranks="1 2 4"
repeat=1
repeat_abort=1
lint=1
while [ $# -gt 0 ]; do
  case "$1" in
    --ranks) ranks="$2"; shift 2 ;;
    --repeat) repeat="$2"; shift 2 ;;
    --repeat-abort) repeat_abort="$2"; shift 2 ;;
    --skip-lint) lint=0; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
here="$(cd "$(dirname "$0")" && pwd)"
stamp=$(date -u +%Y%m%dT%H%M%SZ)
logdir=/target/mpi-logs/$stamp
mkdir -p "$logdir"
results="$logdir/results.tsv"
printf 'ranks\tbinary\tscenario\trun\texpected_rc\trc\tseconds\tverdict\n' > "$results"
failures=0
record() { # ranks binary scenario run expected rc seconds verdict
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$@" >> "$results"
  if [ "$8" != PASS ]; then failures=$((failures + 1)); fi
  printf '%-4s %-16s %-38s run=%s expected=%-3s rc=%-3s %6ss  %s\n' "$@"
}
step() { # name rc
  if [ "$2" = 0 ]; then verdict=PASS; else verdict=FAIL; fi
  record - step "$1" 1 0 "$2" - "$verdict"
}

{
  echo "date=$stamp"
  echo "rustc=$(rustc --version)"
  echo "mpirun=$(mpirun --version | head -1)"
  echo "mpi4py=$(python3 -c 'import mpi4py; print(mpi4py.__version__)')"
  echo "git_head=${AKANTU_RS_GIT_HEAD:-unknown}"
  echo "image=${AKANTU_RS_MPI_IMAGE_ID:-unknown}"
  echo "cargo_target_dir=${CARGO_TARGET_DIR:-/target}"
  env | grep -E '^OMPI_MCA_|^PRTE_MCA_' | sort
} > "$logdir/provenance.txt"
echo "== provenance ($logdir/provenance.txt)"; cat "$logdir/provenance.txt"

if [ $lint = 1 ]; then
  echo "== cargo fmt --check"
  cargo fmt -p akantu-comm -p akantu -- --check > "$logdir/fmt.log" 2>&1
  step fmt $?
  echo "== cargo clippy (feature mpi)"
  cargo clippy -p akantu-comm --features mpi --all-targets --locked -- -D warnings > "$logdir/clippy.log" 2>&1
  step clippy-mpi $?
  # the facade's MPI code (lib) and MPI tests; its other targets need the
  # default features and are linted by the check tier
  cargo clippy -p akantu --no-default-features --features mpi --locked --lib --test exit_hook_mpi \
    --test initialize_mpi -- -D warnings > "$logdir/clippy-facade.log" 2>&1
  step clippy-facade-mpi $?
fi

echo "== evidence checker self-test"
python3 "$here/test_check_evidence.py" > "$logdir/check-evidence-selftest.log" 2>&1
step check-evidence-selftest $?

echo "== cargo test --features mpi (unit tests; MPI binaries as singletons)"
cargo test -p akantu-comm --features mpi --locked > "$logdir/cargo-test.log" 2>&1
rc=$?
grep -E "^test result|SUMMARY|LIFECYCLE|^error" "$logdir/cargo-test.log"
step cargo-test-mpi $rc

echo "== build the MPI binaries (current build only)"
cargo test -p akantu-comm --features mpi --locked --no-run --message-format=json \
  > "$logdir/build-tests.json" 2> "$logdir/build-tests.log"
step build-tests $?
cargo build -p akantu-comm --features mpi --locked --example mpi4py_probe --message-format=json \
  > "$logdir/build-probe.json" 2> "$logdir/build-probe.log"
step build-probe $?
cargo build --release -p akantu-comm --features mpi --locked --example mpi_bench --message-format=json \
  > "$logdir/build-bench.json" 2> "$logdir/build-bench.log"
step build-bench $?
cargo test -p akantu --no-default-features --features mpi --locked --test exit_hook_mpi --test initialize_mpi \
  --no-run --message-format=json > "$logdir/build-facade.json" 2> "$logdir/build-facade.log"
step build-facade $?
bin() { # test target name [artifact list]
  jq -r --arg n "$1" 'select(.reason=="compiler-artifact" and .target.name==$n and .executable!=null) | .executable' \
    "${2:-$logdir/build-tests.json}" | tail -1
}
acc=$(bin mpi_acceptance); abort=$(bin mpi_abort_paths); life=$(bin mpi_lifecycle); prog=$(bin mpi_progress)
selfm=$(bin mpi_self_messages); natv=$(bin native_regions)
hook=$(bin exit_hook_mpi "$logdir/build-facade.json"); init=$(bin initialize_mpi "$logdir/build-facade.json")
probe=$(jq -r 'select(.reason=="compiler-artifact" and .target.name=="mpi4py_probe") | .filenames[] | select(endswith(".so"))' \
  "$logdir/build-probe.json" | tail -1)
shim="$logdir/abort_shim.so"
mpicc -shared -fPIC "$here/abort_shim.c" -o "$shim" > "$logdir/build-shim.log" 2>&1
step build-shim $?
pmix_block="$logdir/pmix_shim_block.so"; pmix_err="$logdir/pmix_shim_err.so"
cc -shared -fPIC -DPMIX_SHIM_BLOCK "$here/pmix_abort_shims.c" -o "$pmix_block" > "$logdir/build-pmix-shims.log" 2>&1 \
  && cc -shared -fPIC -DPMIX_SHIM_ERR "$here/pmix_abort_shims.c" -o "$pmix_err" >> "$logdir/build-pmix-shims.log" 2>&1
step build-pmix-shims $?
bench=$(jq -r 'select(.reason=="compiler-artifact" and .target.name=="mpi_bench" and .executable!=null) | .executable' \
  "$logdir/build-bench.json" | tail -1)
# Third-party defect probe: self (1 process) and inter-process
# (2 processes, shared memory and TCP) truncation; recorded in
# provenance.txt, never a failure by itself.
ompi_probe="$logdir/openmpi_self_truncation"
if mpicc "$here/openmpi_self_truncation.c" -o "$ompi_probe" > "$logdir/build-ompi-probe.log" 2>&1; then
  for v in 1:sm 2:sm 2:tcp; do
    pn=${v%%:*}; pb=${v#*:}
    plog="$logdir/openmpi-truncation-np$pn-$pb.log"
    timeout -k 5 60 mpirun -np "$pn" --mca btl "$pb,self" "$ompi_probe" > "$plog" 2>&1
    grep -a OMPI_TRUNCATION "$plog" | sed "s/^/np=$pn btl=$pb,self /" | tee -a "$logdir/provenance.txt"
  done
fi
missing=0
for f in "$acc" "$abort" "$life" "$prog" "$selfm" "$natv" "$probe" "$shim" "$pmix_block" "$pmix_err" "$bench" "$hook" "$init"; do
  if [ -z "$f" ] || [ ! -f "$f" ]; then echo "missing binary: '$f'"; missing=1; fi
done
step binaries-present $missing
echo "== binaries: $acc $abort $life $prog $selfm $natv $probe $shim $bench $hook $init"
if [ $missing = 1 ]; then
  echo "== results: $results"; echo "== failures: $failures"; exit 1
fi

LAST_RC=0
LAST_SECONDS=0
run() { # out err timeout_s cmd...
  local out=$1 err=$2 t=$3; shift 3
  local t0 t1
  t0=$(date +%s.%N)
  timeout -k 10 "$t" "$@" > "$out" 2> "$err"
  LAST_RC=$?
  t1=$(date +%s.%N)
  LAST_SECONDS=$(python3 -c "print(f'{$t1-$t0:.1f}')")
}
# Rows bounded in time (abort paths, process exit):
# scripts/mpi/abort_watch.py runs the command as `run` does (timeout, KILL
# 10 s later: exit 124) and also reports when the job's rank processes
# (command lines naming the binary) had all ended. The bound applies to that
# moment, the property it stands for (the abort or exit path terminated
# every rank, well before the peers' own 60 s deadline), and the launcher's
# exit status is still required; the launcher's own teardown after the last
# rank ended is recorded in launch-times.tsv, not bounded: Open MPI 5.0.7's
# prterun can take long to return after every rank ended
# (docs/DEPENDENCIES.md). A launcher that never returns fails at the timeout.
LAST_ENDED=-1
launch_times="$logdir/launch-times.tsv"
printf 'ranks\tbinary\tscenario\trun\trc\tlauncher_s\tranks_ended_s\n' > "$launch_times"
watched() { # binary out err timeout_s cmd...
  local bin=$1 out=$2 err=$3 t=$4 line
  shift 4
  line=$(python3 "$here/abort_watch.py" --timeout "$t" --kill-after 10 --match "$bin" \
    --out "$out" --err "$err" -- "$@")
  LAST_RC=$(printf '%s\n' "$line" | tr ' ' '\n' | sed -n 's/^rc=//p')
  LAST_SECONDS=$(printf '%s\n' "$line" | tr ' ' '\n' | sed -n 's/^seconds=//p')
  LAST_ENDED=$(printf '%s\n' "$line" | tr ' ' '\n' | sed -n 's/^ranks_ended=//p')
  [ -n "$LAST_RC" ] || { LAST_RC=125; LAST_SECONDS=0; LAST_ENDED=-1; }
}
ended_within() { python3 -c "import sys; e = float('${LAST_ENDED:--1}'); sys.exit(0 if 0 <= e < $1 else 1)"; }
launch_time() { # ranks binary scenario run
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$LAST_RC" "$LAST_SECONDS" "$LAST_ENDED" >> "$launch_times"
}

abort_scenarios="transport_local_result unagreeable_fatal standalone_sync_peer_missing
  rendezvous_naive_deadlock transport_breaks_derived mpi_error_translation funneled_worker_policy
  dropped_barrier_breaks_family after_header_omitted_payload payload_stall_deadline
  worker_abort_funneled worker_abort_serialized serialized_lock_held_abort stdout_lock_held_abort
  region_abort_serialized"
not_applicable_on_one=" standalone_sync_peer_missing dropped_barrier_breaks_family after_header_omitted_payload payload_stall_deadline "
lifecycle_scenarios="rust_first_funneled last_guard_on_worker_funneled last_guard_on_worker_multiple
  ar3_03_two_barriers_funneled ar3_03_main_drain_serialized ar3_03_worker_drain_serialized
  nested_release_funneled nested_release_serialized destruction_owns_last_guard
  host_first_imported host_first_funneled_worker_rejected thread_level_negotiation
  funneled_self_wait_on_worker_early funneled_self_wait_on_worker_late"

for n in $ranks; do
  for k in $(seq 1 "$repeat"); do
    log="$logdir/acceptance-np$n-run$k.log"
    run "$log" "$log" 1200 mpirun -np "$n" "$acc"
    verdict=FAIL
    if [ "$LAST_RC" = 0 ] && grep -aq "SUMMARY ranks=$n scenarios=[0-9]* failed=0" "$log"; then verdict=PASS; fi
    record "$n" mpi_acceptance all-scenarios "$k" 0 "$LAST_RC" "$LAST_SECONDS" "$verdict"
  done

  for s in $abort_scenarios; do
    expected=71
    case "$not_applicable_on_one" in *" $s "*) [ "$n" = 1 ] && expected=0 ;; esac
    btl=()
    if [ "$s" = payload_stall_deadline ]; then btl=(--mca btl "tcp,self"); fi
    for k in $(seq 1 "$repeat_abort"); do
      base="$logdir/abort-$s-np$n"
      [ "$repeat_abort" = 1 ] || base="$base-run$k"
      watched "$abort" "$base.out" "$base.err" 120 mpirun ${btl[@]+"${btl[@]}"} -x LD_PRELOAD="$shim" -np "$n" "$abort" "$s"
      python3 "$here/check_evidence.py" "$s" "$n" "$base.out" "$base.err" > "$base.check" 2>&1
      checked=$?
      cat "$base.check"
      verdict=FAIL
      if [ "$LAST_RC" = "$expected" ] && [ $checked = 0 ] && ended_within 30; then verdict=PASS; fi
      launch_time "$n" mpi_abort_paths "$s" "$k"
      record "$n" mpi_abort_paths "$s" "$k" "$expected" "$LAST_RC" "$LAST_SECONDS" "$verdict"
    done
  done

  # PMIx fallback under adversarial PMIx_Abort interposers,
  # in forms without the PRRTE teardown race
  # (with the interposer on every rank no rank makes the real request and
  # every rank _exits at once, which meets the launcher teardown race at ~1 % at 8 ranks; that
  # form stays in repeat-abort-scenario.sh only):
  # * 1 rank: singletons without a launcher (no PRRTE at all): exit 71 with
  #   the evidence, the interposer reached, well before 30 s; the blocking
  #   interposer only after the fallback's 2 s bound (the bound under test);
  # * N >= 2 ranks: the interposer on rank 0 only (preload-rank0.sh; the
  #   MPMD spelling `-np 1 … : -np N-1 …` makes the image's prterun 3.0.8
  #   dump core); ranks 1..N-1 make the real PMIx_Abort request, so the
  #   launcher aborts the job (exit 71, evidence, < 30 s). Rank 0 may be
  #   killed by that abort before it reaches its interposer: the calls are
  #   recorded, not required.
  for v in block err; do
    case $v in block) pre="$pmix_block"; tag=PMIX_SHIM_BLOCK ;; *) pre="$pmix_err"; tag=PMIX_SHIM_ERR ;; esac
    for k in $(seq 1 "$repeat_abort"); do
      if [ "$n" = 1 ]; then
        form=singleton
        base="$logdir/abort-pmix-$v-singleton-run$k"
        watched "$abort" "$base.out" "$base.err" 60 env LD_PRELOAD="$shim:$pre" "$abort" worker_abort_funneled
      else
        form=rank0
        base="$logdir/abort-pmix-$v-rank0-np$n-run$k"
        watched "$abort" "$base.out" "$base.err" 120 mpirun -x AKANTU_TEST_PRELOAD="$shim" -x AKANTU_TEST_PRELOAD_RANK0="$pre" \
          -np "$n" sh "$here/preload-rank0.sh" "$abort" worker_abort_funneled
      fi
      python3 "$here/check_evidence.py" worker_abort_funneled "$n" "$base.out" "$base.err" > "$base.check" 2>&1
      checked=$?
      calls=$(grep -ac "$tag PMIx_Abort(71)" "$base.err")
      echo "pmix-$v-$form np=$n run=$k rc=$LAST_RC seconds=$LAST_SECONDS ranks_ended=$LAST_ENDED evidence=$checked interposer_calls=$calls" | tee -a "$base.check"
      verdict=FAIL
      if [ "$LAST_RC" = 71 ] && [ $checked = 0 ] && ended_within 30; then
        if [ "$form" = rank0 ]; then
          verdict=PASS
        elif [ "$calls" -ge 1 ] && { [ $v = err ] || python3 -c "import sys; sys.exit(0 if $LAST_SECONDS >= 2.0 else 1)"; }; then
          verdict=PASS
        fi
      fi
      launch_time "$n" mpi_abort_paths "worker_abort_funneled/pmix-$v-$form" "$k"
      record "$n" mpi_abort_paths "worker_abort_funneled/pmix-$v-$form" "$k" 71 "$LAST_RC" "$LAST_SECONDS" "$verdict"
    done
  done
  if [ "$n" = 1 ]; then
    # singleton without a launcher: PMIx present, no server
    base="$logdir/abort-pmix-singleton"
    watched "$abort" "$base.out" "$base.err" 60 "$abort" worker_abort_funneled
    verdict=FAIL
    if [ "$LAST_RC" = 71 ] && ended_within 30; then verdict=PASS; fi
    launch_time 1 mpi_abort_paths "worker_abort_funneled/singleton-no-launcher" 1
    record 1 mpi_abort_paths "worker_abort_funneled/singleton-no-launcher" 1 71 "$LAST_RC" "$LAST_SECONDS" "$verdict"
  fi

  for s in $lifecycle_scenarios; do
    log="$logdir/lifecycle-$s-np$n.log"
    run "$log" "$log" 180 mpirun -np "$n" "$life" "$s"
    passes=$(grep -ac "LIFECYCLE PASS $s" "$log")
    verdict=FAIL
    if [ "$LAST_RC" = 0 ] && [ "$passes" = "$n" ]; then verdict=PASS; fi
    record "$n" mpi_lifecycle "$s" 1 0 "$LAST_RC" "$LAST_SECONDS" "$verdict"
  done

  # The choreographies (communicator creation x split,
  # randomised timing) run --repeat times; the others once.
  for s in delayed_sender_completes serialized_progress serialized_crossed_dup \
      serialized_crossed_split serialized_crossed_native serialized_crossed_mixed \
      serialized_exclusive_rule serialized_split_vs_dup_low serialized_split_vs_dup_high \
      serialized_split_vs_world serialized_creation_rule serialized_self_wait_progress \
      idle_wait_cpu; do
    reps=1
    case "$s" in serialized_split_vs_*) reps=$repeat ;; esac
    for k in $(seq 1 "$reps"); do
      log="$logdir/progress-$s-np$n-run$k.log"
      run "$log" "$log" 180 mpirun --mca btl tcp,self -np "$n" "$prog" "$s"
      passes=$(grep -ac "PROGRESS PASS $s" "$log")
      verdict=FAIL
      if [ "$LAST_RC" = 0 ] && [ "$passes" = "$n" ]; then verdict=PASS; fi
      record "$n" mpi_progress "$s" "$k" 0 "$LAST_RC" "$LAST_SECONDS" "$verdict"
    done
  done

  for s in redzone_selftest self_4k self_64k self_1m self_semantics cross_4k cross_64k cross_1m; do
    for btl in default tcp; do
      case "$s" in self_*|redzone_*) [ $btl = tcp ] && continue ;; esac
      mca=()
      if [ $btl = tcp ]; then mca=(--mca btl "tcp,self"); fi
      log="$logdir/self-$s-$btl-np$n.log"
      if [ "$s" = redzone_selftest ]; then
        [ "$n" = 1 ] || continue # process-local detector check, once
        run "$log" "$log" 60 "$selfm" "$s"
        expected_passes=1
      else
        run "$log" "$log" 180 mpirun ${mca[@]+"${mca[@]}"} -np "$n" "$selfm" "$s"
        expected_passes=$n
      fi
      passes=$(grep -ac "SELF PASS $s" "$log")
      verdict=FAIL
      if [ "$LAST_RC" = 0 ] && [ "$passes" = "$expected_passes" ] && ! grep -aq "REDZONE VIOLATION" "$log"; then verdict=PASS; fi
      record "$n" mpi_self_messages "$s/$btl" 1 0 "$LAST_RC" "$LAST_SECONDS" "$verdict"
    done
  done

  # Native library entries.
  for s in ticket_native_reduce_serialized ticket_native_reduce_multiple \
      ticket_native_reduce_funneled region_world_serialized region_world_funneled \
      nested_native_multiple nested_native_serialized; do
    log="$logdir/native-$s-np$n.log"
    run "$log" "$log" 180 mpirun -np "$n" "$natv" "$s"
    passes=$(grep -ac "NATIVE PASS $s" "$log")
    verdict=FAIL
    if [ "$LAST_RC" = 0 ] && [ "$passes" = "$n" ]; then verdict=PASS; fi
    record "$n" native_regions "$s" 1 0 "$LAST_RC" "$LAST_SECONDS" "$verdict"
  done

  for level in multiple funneled; do
    log="$logdir/mpi4py-$level-np$n.log"
    run "$log" "$log" 180 mpirun -np "$n" python3 "$here/mpi4py_first.py" "$probe" "$level"
    passes=$(grep -ac "MPI4PY PASS" "$log")
    verdict=FAIL
    if [ "$LAST_RC" = 0 ] && [ "$passes" = "$n" ]; then verdict=PASS; fi
    record "$n" mpi4py_first "$level" 1 0 "$LAST_RC" "$LAST_SECONDS" "$verdict"
  done

  # Process exit of the facade's runtime (D-067).
  for m in finalize forget reinit; do
    log="$logdir/initialize-$m-np$n.log"
    watched "$init" "$log" "$log" 120 mpirun -np "$n" "$init" "$m"
    passes=$(grep -ac "INITIALIZE_MPI PASS" "$log")
    verdict=FAIL
    if [ "$LAST_RC" = 0 ] && [ "$passes" = "$n" ] && ended_within 30; then verdict=PASS; fi
    launch_time "$n" initialize_mpi "$m" 1
    record "$n" initialize_mpi "$m" 1 0 "$LAST_RC" "$LAST_SECONDS" "$verdict"
  done
  for s in forget finalize hostfirst hostfirst-universe panic err-return exit-nonzero serial-held worker-exit; do
    case "$s" in
      panic) expected=101 ;;
      exit-nonzero) expected=3 ;;
      err-return | serial-held | worker-exit) expected=1 ;;
      *) expected=0 ;;
    esac
    base="$logdir/exit-hook-$s-np$n"
    watched "$hook" "$base.out" "$base.err" 120 mpirun -np "$n" "$hook" "$s"
    reached=$(grep -ac "EXIT_HOOK $s rank=[0-9]* reached-end" "$base.out")
    notes=$(grep -ac "akantu: MPI left unfinalised at exit" "$base.err")
    extra=1
    case "$s" in
      forget | finalize | hostfirst | hostfirst-universe) [ "$reached" = "$n" ] || extra=0 ;;
      serial-held) [ "$notes" -ge 1 ] || extra=0 ;;
    esac
    verdict=FAIL
    if [ "$LAST_RC" = "$expected" ] && [ $extra = 1 ] && ended_within 30; then verdict=PASS; fi
    launch_time "$n" exit_hook_mpi "$s" 1
    record "$n" exit_hook_mpi "$s(reached=$reached,notes=$notes)" 1 "$expected" "$LAST_RC" "$LAST_SECONDS" "$verdict"
  done
done

# Performance gate, shared memory: see the
# header. One job per case; perf.tsv holds the medians over the blocks.
perf_factor=${AKANTU_PERF_FACTOR:-8}
perf="$logdir/perf.tsv"
printf 'ranks\tcase\tbytes\takantu_us\traw_us\tcalib_us\tratio\tbackend_ratio\tfactor\tverdict\n' > "$perf"
perf_case() { # ranks kind bytes iterations-per-block blocks
  local log="$logdir/perf-$2-$3-np$1.log" sum a r k ratio br verdict=FAIL
  case " $ranks " in *" $1 "*) ;; *) return ;; esac
  run "$log" "$log" 600 mpirun --mca btl sm,self -np "$1" "$bench" calibrated "$2" "$3" "$4" "$5"
  sum=$(grep -a '^CAL summary ' "$log" | head -1)
  field() { printf '%s\n' "$sum" | tr ' ' '\n' | sed -n "s/^$1=//p"; }
  a=$(field akantu_med_us); r=$(field raw_med_us); k=$(field calib_med_us)
  ratio=$(field ratio_med); br=$(field backend_ratio_med)
  if [ "$LAST_RC" = 0 ] && [ -n "$br" ] && python3 -c "import math, sys; b = float('$br'); sys.exit(0 if math.isfinite(b) and b <= $perf_factor else 1)"; then
    verdict=PASS
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "${a:-?}" "${r:-?}" "${k:-?}" "${ratio:-?}" "${br:-?}" "$perf_factor" "$verdict" >> "$perf"
  record "$1" mpi_bench "$2-$3-sm(akantu=${a:-?}us,raw=${r:-?}us,calib=${k:-?}us,x${ratio:-?},backend=x${br:-?})" 1 0 "$LAST_RC" "$LAST_SECONDS" "$verdict"
}
echo "== performance gate (factor $perf_factor on the backend ratio, $perf)"
perf_case 2 pingpong 1048576 100 9
perf_case 2 pingpong 16777216 10 9
perf_case 4 halo 8192 1000 9

echo "== results: $results"
echo "== failures: $failures"
exit $(( failures > 0 ))
