#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Parallel MPI tier, run INSIDE the akantu-rs-mpi image (by
# scripts/mpi/run-par-tests.sh): clippy (feature mpi) of the mesh, solid,
# diffusion and cohesive crates, the real-MPI binaries mpi_distribution,
# mpi_synchronizers (distributed facets, facet / grid / periodic
# synchronizers; akantu-mesh), mpi_smm (akantu-solid), mpi_diffusion
# (akantu-diffusion), mpi_cohesive (akantu-cohesive,
# the cohesive model on a distributed mesh; at 2 and 4 ranks also with two
# threads per rank, `mpi_cohesive-t2` in results.tsv), mpi_phase_field, mpi_ntn
# (akantu-ntn-contact, the split-node contact) and mpi_xmat
# (akantu-extra-materials, TEST-187 and the extra non-local
# laws) at every rank count of --ranks (default "1 2 4"), --repeat times,
# and the lifecycle scenarios. Logs and results.tsv in
# $AKANTU_PAR_LOGDIR (default /target/par-logs/<timestamp>). A binary
# passes when it exits 0 with `SUMMARY ranks=N scenarios=S failed=0`; S
# counts only the scenarios that ran (compared something): those that do
# not run at a rank count print `NOT RUN <name>: <why>` and are counted in
# the SUMMARY's `not_run=`; the SUMMARY line is echoed
# per run. Also: S >= 1 (a run in which no
# scenario ran fails), the number of NOT RUN lines equals `not_run=`, and
# the NOT RUN names are exactly those that scripts/mpi/par-not-run.txt
# expects for that binary and rank count: an oracle replay whose fixture is
# committed at that count and that turns into NOT RUN (a fixture removed, a
# rank-count lookup broken) fails the tier, and so does a scenario that
# starts running without the expectation being updated. Rank counts the
# file does not cover (8) are reported, not compared. The observed names
# go to not-run-observed.txt (the file's format); results.tsv has the
# columns `scenarios` and `not_run`.
#
# Prerequisites: MPI (Open MPI 5 with `mpirun`),
# `jq` and, for the clippy / test steps, a Rust toolchain with libclang
# (rsmpi's bindings). With --scotch also Scotch 7 (libscotch, headers) and
# Python >= 3.9 with its shared library and development files (Debian:
# `python3-dev`): the `akantu-python --features mpi` step builds test
# binaries that embed an interpreter. The project images provide all of it
# (`akantu-rs-scotch:dev`, `docker/mpi/Dockerfile` installs python3-dev);
# on another machine the --scotch tier fails (`fail=1`) without them.
set -uo pipefail
ranks="1 2 4"
repeat=1
scotch=0
while [ $# -gt 0 ]; do
  case "$1" in
    --ranks) ranks="$2"; shift 2 ;;
    --repeat) repeat="$2"; shift 2 ;;
    --scotch) scotch=1; shift ;;
    *) echo "usage: $0 [--ranks \"1 2 4\"] [--repeat N] [--scotch]" >&2; exit 2 ;;
  esac
done
logdir="${AKANTU_PAR_LOGDIR:-${CARGO_TARGET_DIR:-/target}/par-logs/$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "$logdir"
results="$logdir/results.tsv"
printf 'ranks\tbinary\trun\trc\tseconds\tverdict\tscenarios\tnot_run\n' > "$results"
expect="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/par-not-run.txt"
[ -f "$expect" ] || { echo "missing $expect" >&2; exit 2; }
observed="$logdir/not-run-observed.txt"
: > "$observed"
# expected NOT RUN names of <binary> at <ranks>, sorted (lines `<ranks> <binary> <name>`)
expected_not_run() { awk -v n="$1" -v b="$2" '$1 !~ /^#/ && $1 == n && $2 == b { print $3 }' "$expect" | LC_ALL=C sort; }
covered() { awk -v n="$1" '$1 !~ /^#/ && $1 == n { f = 1 } END { exit !f }' "$expect"; }
fail=0
{
  echo "git_head=${AKANTU_RS_GIT_HEAD:-unknown}"
  echo "image_id=${AKANTU_RS_MPI_IMAGE_ID:-unknown}"
  echo "ranks=$ranks repeat=$repeat scotch=$scotch"
  mpirun --version | head -1
} > "$logdir/provenance.txt"
features="mpi"
[ "$scotch" = 1 ] && features="mpi,scotch"
echo "== clippy (features $features)"
cargo clippy -p akantu-mesh -p akantu-solid -p akantu-diffusion -p akantu-cohesive -p akantu-couplers -p akantu-ntn-contact -p akantu-extra-materials --features "$features" --all-targets --locked -- -D warnings \
  > "$logdir/clippy.log" 2>&1 || { echo "clippy FAILED"; tail -30 "$logdir/clippy.log"; fail=1; }
if [ "$scotch" = 1 ]; then
  # the unit tests of the library with Scotch too: the FFI-level contract
  # of the Scotch call
  echo "== cargo test --features scotch (Scotch partition vs oracle, Scotch FFI unit tests)"
  SCOTCH_PTHREAD_NUMBER=1 cargo test -p akantu-mesh --features scotch --locked --lib --test parallel_oracle \
    > "$logdir/scotch-test.log" 2>&1 || { echo "scotch test FAILED"; tail -30 "$logdir/scotch-test.log"; fail=1; }
  grep -E "^test |test result" "$logdir/scotch-test.log"
  # the MPI build of the Python module (feature `mpi` = MPI + Scotch,
  # D-101 (1)): only this image has both; its Python tests need
  # NumPy/SciPy/mpi4py and run in the oracle image (docs/TESTING.md)
  echo "== akantu-python --features mpi: clippy and tests"
  cargo clippy -p akantu-python --features mpi --all-targets --locked -- -D warnings \
    > "$logdir/python-mpi-clippy.log" 2>&1 || { echo "python mpi clippy FAILED"; tail -30 "$logdir/python-mpi-clippy.log"; fail=1; }
  cargo test -p akantu-python --features mpi --locked \
    > "$logdir/python-mpi-test.log" 2>&1 || { echo "python mpi test FAILED"; tail -30 "$logdir/python-mpi-test.log"; fail=1; }
  grep -E "test result" "$logdir/python-mpi-test.log"
fi
echo "== build"
cargo test -p akantu-mesh -p akantu-solid -p akantu-diffusion -p akantu-cohesive -p akantu-couplers -p akantu-ntn-contact -p akantu-extra-materials --features "$features" --locked --no-run --message-format=json \
  > "$logdir/build.json" 2> "$logdir/build.log" || { tail -30 "$logdir/build.log"; exit 1; }
cargo test -p akantu-comm --features mpi --locked --no-run --message-format=json \
  > "$logdir/build-comm.json" 2> "$logdir/build-comm.log" || { tail -30 "$logdir/build-comm.log"; exit 1; }
bin() { jq -r --arg n "$2" 'select(.reason=="compiler-artifact" and .target.name==$n and .executable!=null) | .executable' "$1" | tail -1; }
dist=$(bin "$logdir/build.json" mpi_distribution)
syncs=$(bin "$logdir/build.json" mpi_synchronizers)
smm=$(bin "$logdir/build.json" mpi_smm)
diff=$(bin "$logdir/build.json" mpi_diffusion)
coh=$(bin "$logdir/build.json" mpi_cohesive)
pf=$(bin "$logdir/build.json" mpi_phase_field)
ntn=$(bin "$logdir/build.json" mpi_ntn)
xmat=$(bin "$logdir/build.json" mpi_xmat)
life=$(bin "$logdir/build-comm.json" mpi_lifecycle)
echo "== binaries: $dist $syncs $smm $diff $coh $pf $ntn $xmat $life"
export OMPI_MCA_rmaps_base_oversubscribe=1
# reproducible Scotch partitions (see akantu_mesh::scotch)
export SCOTCH_PTHREAD_NUMBER="${SCOTCH_PTHREAD_NUMBER:-1}"
# one run of binary <b> at <n> ranks, repetition <k>; <tag> (empty or
# `-t<threads>`) marks the threaded runs, the further arguments are passed
# to mpirun (`-x VAR=value`); the NOT RUN expectations are the binary's
one_run() {
  local n="$1" k="$2" b="$3" tag="$4"; shift 4
  local name log t0 rc t1 secs verdict summary ran nr names got want
  name=$(basename "$b" | sed 's/-[0-9a-f]*$//')
  log="$logdir/$name$tag-np$n-run$k.log"
  t0=$(date +%s.%N)
  timeout -k 10 900 mpirun -np "$n" --oversubscribe --bind-to none -x SCOTCH_PTHREAD_NUMBER "$@" "$b" > "$log" 2>&1
  rc=$?
  t1=$(date +%s.%N)
  secs=$(python3 -c "print(f'{$t1-$t0:.1f}')")
  verdict=FAIL
  summary=$(grep -a "SUMMARY ranks=$n " "$log" | head -1)
  ran=$(sed -nE 's/.* scenarios=([0-9]+) .*/\1/p' <<< "$summary")
  nr=$(sed -nE 's/.* not_run=([0-9]+).*/\1/p' <<< "$summary")
  nr=${nr:-0}
  # At least one scenario ran, failed=0 exactly
  if [ $rc = 0 ] && grep -aEq "SUMMARY ranks=$n scenarios=[1-9][0-9]* failed=0( |\$)" "$log"; then verdict=PASS; fi
  names=$(grep -a "^NOT RUN " "$log" | sed -E 's/^NOT RUN ([^:]*):.*/\1/' | LC_ALL=C sort)
  if [ "$k" = 1 ] && [ -z "$tag" ]; then
    while IFS= read -r x; do [ -n "$x" ] && echo "$n $name $x" >> "$observed"; done <<< "$names"
  fi
  got=$(grep -c . <<< "$names")
  if [ "$got" != "$nr" ]; then
    echo "np=$n $name: $got NOT RUN line(s), SUMMARY not_run=$nr"
    verdict=FAIL
  fi
  if covered "$n"; then
    want=$(expected_not_run "$n" "$name")
    if [ "$names" != "$want" ]; then
      echo "np=$n $name: NOT RUN differs from scripts/mpi/par-not-run.txt"
      echo "  new (not expected): $(LC_ALL=C comm -23 <(echo "$names") <(echo "$want") | tr '\n' ' ')"
      echo "  gone (expected):    $(LC_ALL=C comm -13 <(echo "$names") <(echo "$want") | tr '\n' ' ')"
      verdict=FAIL
    fi
  else
    echo "INFO np=$n $name: par-not-run.txt covers no run at $n ranks; $got NOT RUN name(s) not compared"
  fi
  [ $verdict = FAIL ] && fail=1
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$n" "$name$tag" "$k" "$rc" "$secs" "$verdict" "${ran:--}" "$nr" >> "$results"
  echo "np=$n run=$k $name$tag: $verdict (rc=$rc, ${secs}s)"
  grep -a "SUMMARY ranks=" "$log" | head -1
  grep -a "^FAIL\|^INFO" "$log" | head -20
}
for n in $ranks; do
  for k in $(seq 1 "$repeat"); do
    for b in "$dist" "$syncs" "$smm" "$diff" "$coh" "$pf" "$ntn" "$xmat"; do
      one_run "$n" "$k" "$b" ""
    done
    # the cohesive model distributed and threaded: mpi_cohesive again at 2
    # and 4 ranks with two threads per rank and batches split as finely as
    # possible, every comparison (oracle replays included) as at one thread;
    # idle workers park at once (4 x 2 threads on the image's 4 cores: with
    # the default spin the run takes 90 s instead of 5)
    case "$n" in 2 | 4)
      one_run "$n" "$k" "$coh" -t2 -x AKANTU_NUM_THREADS=2 -x AKANTU_EXEC_GRAIN=finest -x AKANTU_EXEC_SPIN_US=0 ;;
    esac
  done
done
for s in funneled_self_wait_on_worker_early funneled_self_wait_on_worker_late; do
  for n in 1 2; do
    log="$logdir/lifecycle-$s-np$n.log"
    timeout -k 10 120 mpirun -np "$n" --oversubscribe --bind-to none "$life" "$s" > "$log" 2>&1
    rc=$?
    verdict=FAIL
    if [ $rc = 0 ] && [ "$(grep -ac "LIFECYCLE PASS $s" "$log")" = "$n" ]; then verdict=PASS; fi
    [ $verdict = FAIL ] && fail=1
    printf '%s\t%s\t%s\t%s\t%s\t%s\t-\t-\n' "$n" "lifecycle:$s" 1 "$rc" "-" "$verdict" >> "$results"
    echo "np=$n lifecycle $s: $verdict (rc=$rc)"
    grep -a "INFO\|FAIL" "$log" | head -4
  done
done
echo "== results: $results"
cat "$results"
exit $fail
