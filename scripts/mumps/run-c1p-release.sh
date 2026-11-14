#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the cohesive benchmark's
# MUMPS comparison (crates/akantu-bench/tests/perf_cohesive_mumps.rs, C1P-1)
# in an optimised build, where it runs its full 1,500 steps (the mumps tier
# builds it in the dev profile: 200 steps). One of the final campaign's runs
# that no gate makes. Runs INSIDE the
# MUMPS image:
#
#   AKANTU_RS_MPI_IMAGE=akantu-rs-mumps:dev scripts/mpi/in-docker.sh \
#       bash /src/scripts/mumps/run-c1p-release.sh [--ranks "1 2 4"]
#
# Exit 0 only if the binary builds and every rank count prints failed=0.
set -uo pipefail
ranks="1 2 4"
while [ $# -gt 0 ]; do
  case "$1" in
    --ranks) ranks="$2"; shift 2 ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
done
# shellcheck source=scripts/mumps/blas-pin.sh
. /src/scripts/mumps/blas-pin.sh
akantu_blas_export
feat="akantu-comm/mpi,akantu-linalg/mumps,akantu-solver/mumps,akantu-bench/mumps"
pk=(-p akantu-comm -p akantu-linalg -p akantu-solver -p akantu-bench)
cargo test --release --locked "${pk[@]}" --features "$feat" --test perf_cohesive_mumps --no-run || exit 1
be=$(cargo test -q --release --locked "${pk[@]}" --features "$feat" --test perf_cohesive_mumps --no-run --message-format=json 2>/dev/null \
  | sed -n 's/.*"executable":"\([^"]*perf_cohesive_mumps[^"]*\)".*/\1/p' | tail -1)
[ -x "$be" ] || { echo "no perf_cohesive_mumps executable" >&2; exit 1; }
failures=0
for n in $ranks; do
  t0=$SECONDS
  log="/tmp/c1p-release-n$n.log"
  timeout 3600 mpirun -n "$n" -x OPENBLAS_NUM_THREADS ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} "$be" > "$log" 2>&1
  rc=$?
  v=FAIL; [ $rc = 0 ] && grep -q "failed=0" "$log" && v=PASS
  printf 'perf_cohesive_mumps release ranks=%-2s rc=%-3s %5ss %s  %s\n' "$n" "$rc" $((SECONDS - t0)) "$v" "$(grep -E 'failed=|steps' "$log" | tail -1 | cut -c1-120)"
  [ "$v" = PASS ] || { failures=$((failures + 1)); tail -20 "$log"; }
done
echo "C1P-RELEASE: $([ $failures = 0 ] && echo PASS || echo FAIL)"
[ $failures = 0 ]
