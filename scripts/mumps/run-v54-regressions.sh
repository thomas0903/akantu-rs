#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# MUMPS regression scenarios, run INSIDE the akantu-rs-mumps
# image (scripts/mumps/run-mumps-tier.sh calls it; standalone:
# scripts/mpi/in-docker.sh bash /src/scripts/mumps/run-v54-regressions.sh).
# One mpirun per scenario × rank count × thread level, each under
# `timeout` (a scenario that hangs fails without blocking the others):
#   * crates/akantu-comm/tests/review_v54_native.rs (native job
#     identity; MPI_THREAD_SERIALIZED);
#   * crates/akantu-linalg/tests/review_v54_regressions.rs
#     (funneled/serialized/multiple).
#   scripts/mumps/run-v54-regressions.sh [--ranks "1 2 4"] [--levels "funneled serialized multiple"]
#                                        [--logdir DIR] [--timeout S] [--only "scenario ..."]
# Writes <logdir>/v54.tsv (scenario, ranks, level, rc, seconds, verdict)
# and exits 1 if any launch failed.
set -uo pipefail
ranks="1 2 4"; levels="funneled serialized multiple"; logdir=""; tmo=240; only=""
while [ $# -gt 0 ]; do
  case "$1" in
    --ranks) ranks="$2"; shift 2 ;;
    --levels) levels="$2"; shift 2 ;;
    --logdir) logdir="$2"; shift 2 ;;
    --timeout) tmo="$2"; shift 2 ;;
    --only) only="$2"; shift 2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
[ -n "$logdir" ] || logdir=/target/mumps-logs/v54-$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "$logdir"
tsv="$logdir/v54.tsv"
[ -f "$tsv" ] || printf 'scenario\tranks\tlevel\trc\tseconds\tverdict\n' > "$tsv"
bin_of() { # package test-name features
  cargo test -q -p "$1" --features "$3" --test "$2" --no-run --message-format=json 2>/dev/null \
    | sed -n 's/.*"executable":"\([^"]*\/'"$2"'-[^"/]*\)".*/\1/p' | tail -1
}
cargo test -q -p akantu-comm --features mpi --test review_v54_native --no-run || exit 1
cargo test -q -p akantu-linalg --features mumps --test review_v54_regressions --no-run || exit 1
native=$(bin_of akantu-comm review_v54_native mpi)
regr=$(bin_of akantu-linalg review_v54_regressions mumps)
fail=0
launch() { # scenario ranks level binary
  # shellcheck disable=SC2086 # $only is a blank-separated list of scenarios
  if [ -n "$only" ] && ! printf ' %s ' $only | grep -q " $1 "; then return 0; fi
  local t0=$SECONDS log="$logdir/$1-n$2-$3.log"
  AKANTU_MUMPS_THREAD_LEVEL=$3 timeout --kill-after=5 "$tmo" \
    mpirun -n "$2" -x AKANTU_MUMPS_THREAD_LEVEL "$4" "$1" > "$log" 2>&1
  local rc=$? v=FAIL
  [ $rc = 0 ] && v=PASS
  [ $v = PASS ] || fail=$((fail + 1))
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$rc" $((SECONDS - t0)) "$v" >> "$tsv"
  printf '%-36s n=%-2s %-10s rc=%-3s %4ss %s\n' "$1" "$2" "$3" "$rc" $((SECONDS - t0)) "$v"
}
for n in $ranks; do
  for s in mu03_c2f_inside_serial_lock mu02_native_job_identity; do
    launch "$s" "$n" serialized "$native"
  done
  for level in $levels; do
    for s in mu01_reordered_communicator mu01_subgroup mu02_divergent_ceiling \
             mu02_divergent_retry_rhs mu02_divergent_icntl mu02_divergent_cntl \
             mu02_divergent_persisted_icntl14 mu02_one_rank_mutation \
             mu04_refused_reinit mu05_admission_interleavings; do
      case "$s" in mu04_*|mu05_*) [ "$level" = serialized ] || continue ;; esac
      launch "$s" "$n" "$level" "$regr"
    done
  done
done
echo "== v54 regressions: $tsv (failures=$fail)"
exit $((fail > 0))
