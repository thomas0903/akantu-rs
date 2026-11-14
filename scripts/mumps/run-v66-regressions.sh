#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# MUMPS regression scenarios, run INSIDE the akantu-rs-mumps
# image (scripts/mumps/run-mumps-tier.sh calls it; standalone:
# scripts/mpi/in-docker.sh bash /src/scripts/mumps/run-v66-regressions.sh).
# One mpirun per scenario × rank count × thread level of
# crates/akantu-linalg/tests/review_v66_regressions.rs, each under
# `timeout` (a scenario that hangs or crashes fails without blocking the
# others):
#   mu2_01_stale_coordinates_{shorter,longer}  (>= 2 ranks; n/a on one)
#   mu2_02_release_collision, mu2_04_teardown_on_broken_communicator
#   mu2_05_concurrent_solvers                  (multiple only; n/a elsewhere)
#   pe01_refusal_{forward,reverse} (serialized only, >= 2 ranks)
#   scripts/mumps/run-v66-regressions.sh [--ranks "1 2 4"] [--levels "funneled serialized multiple"]
#                                        [--logdir DIR] [--timeout S] [--only "scenario ..."]
#                                        [--valgrind]
# Environment: VALGRIND_CMD replaces the built-in valgrind words of --valgrind
# (the memcheck tier of the delivered CI hands over its own: --error-exitcode,
# the suppression file, --fair-sched=yes); V66_REGR_BIN runs that
# executable instead of the test binary (the memcheck tier's planted-defect
# control of this launcher; it builds nothing).
# Writes <logdir>/v66.tsv (scenario, ranks, level, rc, seconds, verdict)
# and exits 1 if any launch failed. --valgrind runs every rank under
# valgrind memcheck (must be installed) and fails a launch that reports an
# invalid read, write or free.
set -uo pipefail
ranks="1 2 4"; levels="funneled serialized multiple"; logdir=""; tmo=300; only=""; vg=""
while [ $# -gt 0 ]; do
  case "$1" in
    --ranks) ranks="$2"; shift 2 ;;
    --levels) levels="$2"; shift 2 ;;
    --logdir) logdir="$2"; shift 2 ;;
    --timeout) tmo="$2"; shift 2 ;;
    --only) only="$2"; shift 2 ;;
    --valgrind) vg="${VALGRIND_CMD:-valgrind --error-limit=no --leak-check=no --track-origins=no -q}"; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
[ -n "$logdir" ] || logdir=/target/mumps-logs/v66-$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "$logdir"
tsv="$logdir/v66.tsv"
[ -f "$tsv" ] || printf 'scenario\tranks\tlevel\trc\tseconds\tverdict\n' > "$tsv"
if [ -n "${V66_REGR_BIN:-}" ]; then
  regr="$V66_REGR_BIN"
else
  cargo test -q -p akantu-linalg --features mumps --test review_v66_regressions --no-run || exit 1
  regr=$(cargo test -q -p akantu-linalg --features mumps --test review_v66_regressions --no-run \
           --message-format=json 2>/dev/null | sed -n 's/.*"executable":"\([^"]*\/review_v66_regressions-[^"/]*\)".*/\1/p' | tail -1)
fi
fail=0
launch() { # scenario ranks level
  # shellcheck disable=SC2086 # $only is a blank-separated list of scenarios
  if [ -n "$only" ] && ! printf ' %s ' $only | grep -q " $1 "; then return 0; fi
  local t0=$SECONDS log="$logdir/$1-n$2-$3.log"
  # shellcheck disable=SC2086 # $vg is a word list on purpose
  AKANTU_MUMPS_THREAD_LEVEL=$3 timeout --kill-after=5 "$tmo" \
    mpirun -n "$2" -x AKANTU_MUMPS_THREAD_LEVEL -x OPENBLAS_NUM_THREADS ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} \
      $vg "$regr" "$1" > "$log" 2>&1
  local rc=$? v=FAIL
  [ $rc = 0 ] && grep -q "failed=0" "$log" && v=PASS
  if [ -n "$vg" ] && grep -q -E "Invalid (read|write|free)" "$log"; then v=FAIL; fi
  [ $v = PASS ] || fail=$((fail + 1))
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$rc" $((SECONDS - t0)) "$v" >> "$tsv"
  printf '%-40s n=%-2s %-10s rc=%-3s %4ss %s\n' "$1" "$2" "$3" "$rc" $((SECONDS - t0)) "$v"
}
for n in $ranks; do
  for level in $levels; do
    for s in mu2_01_stale_coordinates_shorter mu2_01_stale_coordinates_longer \
             mu2_02_release_collision mu2_04_teardown_on_broken_communicator \
             mu2_05_concurrent_solvers pe01_refusal_forward pe01_refusal_reverse; do
      case "$s" in
        mu2_05_*) [ "$level" = multiple ] || continue ;;
        pe01_*) [ "$level" = serialized ] || continue ;;
      esac
      launch "$s" "$n" "$level"
    done
  done
done
echo "== v66 regressions: $tsv (failures=$fail)"
exit $((fail > 0))
