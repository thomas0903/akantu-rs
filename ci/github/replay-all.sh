#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu: replay every job of the
# delivered CI, one after the other, on a machine that has Docker, a Rust
# toolchain and the repository checked out:
#
#   ci/github/replay-all.sh [--out DIR] [JOB...]
#
# Without JOB arguments the list is the 23 jobs of the final campaign, in order.
# With arguments only those jobs run (to split the list over two sandboxes);
# the last line is still `REPLAY-ALL: PASS | FAIL`.
# Each job runs `ci/github/replay.sh <job> <DIR>/<job with : as _>` (DIR defaults
# to $HOME/final-ci, outside the repository); its whole output goes to
# <DIR>/<job with : as _>.log and ONE line of result is printed:
#   REPLAY <job>: PASS|FAIL|INCOMPLETE (exit N) <seconds> s [<last CI line>]
# A failing job does not stop the run (the next ones still tell something).
# Disk routine: when the root file system is at or above
# REPLAY_DISK_MAX percent (default 70) after a job, the cargo target volume
# akantu-rs-mpi-target is removed (no job runs then; the next job rebuilds it).
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 2

jobs_default=(
  fast:selftest fast:fast fast:msrv
  features:features features:gates
  docs:rustdoc docs:manual docs:site
  python:python
  packaging:wheel-linux packaging:crates-dry-run
  distro:ubuntu-22-04
  medium:medium:release medium:medium:examples medium:medium:mpi medium:medium:par
  medium:medium:mumps medium:medium:petsc
  memcheck:memcheck:mpi memcheck:memcheck:mumps memcheck:memcheck:petsc
  expensive:miri expensive:ranks-8
  coverage:coverage
)

out_root="${HOME}/final-ci"
jobs=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out)
      [ $# -ge 2 ] || { echo "usage: $0 [--out DIR] [JOB...]" >&2; exit 2; }
      out_root="$2"
      shift 2
      ;;
    -h | --help)
      sed -n '6,19p' "$0" >&2
      exit 2
      ;;
    *)
      jobs+=("$1")
      shift
      ;;
  esac
done
[ ${#jobs[@]} -gt 0 ] || jobs=("${jobs_default[@]}")
mkdir -p "$out_root" || exit 2
out_root="$(cd "$out_root" && pwd)"
disk_max="${REPLAY_DISK_MAX:-70}"

fail=0
for j in "${jobs[@]}"; do
  dir="$out_root/${j//:/_}"
  log="$out_root/${j//:/_}.log"
  t0=$SECONDS
  ci/github/replay.sh "$j" "$dir" > "$log" 2>&1
  rc=$?
  dt=$((SECONDS - t0))
  case $rc in
    0) res=PASS ;;
    3) res=INCOMPLETE ;;
    *) res=FAIL ;;
  esac
  [ $rc = 0 ] || fail=1
  last="$(grep -aE '^(CI|selftest): ' "$log" | tail -n 1 || true)"
  echo "REPLAY $j: $res (exit $rc) $dt s ${last:+[$last]}"
  used="$(df --output=pcent / 2> /dev/null | tail -n 1 | tr -dc '0-9')"
  if [ -n "$used" ] && [ "$used" -ge "$disk_max" ]; then
    echo "REPLAY disk: ${used}% used, removing the cargo target volume"
    docker volume rm akantu-rs-mpi-target > /dev/null 2>&1 || true
  fi
done
echo "REPLAY-ALL: $([ $fail = 0 ] && echo PASS || echo FAIL)"
[ $fail = 0 ]
