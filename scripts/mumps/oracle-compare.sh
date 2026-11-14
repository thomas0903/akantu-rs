#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Differential run (host entry point): the Rust MUMPS acceptance
# binary exports its rank-partial systems and solutions (ranks given, RUNS
# runs: tags rust, rust2, rust3) in the akantu-rs-mumps image, the oracle
# probe (oracle/probes/mumps/run.sh) solves the same systems with upstream
# SparseSolverMumps in akantu-oracle:dev (build volume read-only), and
# scripts/mumps/compare.py writes the JSON summary.
#
#   scripts/mumps/oracle-compare.sh [exchange-dir] [ranks]
#   RUNS=3 scripts/mumps/oracle-compare.sh target/mumps-oracle 1   # D-035 goldens
#   -> <exchange-dir>/comparison.json (default target/mumps-oracle)
#
# The export runs disable mumps_mpi's own fixture
# comparison (AKANTU_MUMPS_ORACLE=off), so the script runs on any platform;
# a failing export run is recorded (rc in <exchange-dir>/in/rust-*.rc) and
# the script continues to the oracle side. BLAS pin (D-050): both containers
# get scripts/mumps/blas-pin.sh's OPENBLAS_CORETYPE / OPENBLAS_NUM_THREADS.
set -euo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
. "$here/scripts/mumps/blas-pin.sh"
x="${1:-$here/target/mumps-oracle}"
ranks="${2:-1 2 4}"
runs="${RUNS:-2}"
rm -rf "$x"; mkdir -p "$x/in" "$x/out"
image="${AKANTU_RS_MPI_IMAGE:-akantu-rs-mumps:dev}"
target="${AKANTU_RS_MPI_TARGET:-/target/mumps}"
mapfile -t blas_env < <(akantu_blas_docker_env)
echo "BLAS pin: OPENBLAS_CORETYPE=${AKANTU_BLAS_CORETYPE:-<unset>} OPENBLAS_NUM_THREADS=${AKANTU_BLAS_THREADS}"
docker run --rm --memory 6g --memory-swap 6g --cpus 4 --shm-size 512m \
  -v "$here:/src:ro" -v akantu-rs-mpi-target:/target -v akantu-rs-mpi-cargo:/usr/local/cargo/registry \
  "${blas_env[@]}" \
  -v "$x:/x" -e CARGO_TARGET_DIR="$target" -e RANKS="$ranks" -e RUNS="$runs" -w /src "$image" bash -c '
    set -euo pipefail
    cargo build -q -p akantu-linalg --features mumps --tests
    bin=$(cargo test -q -p akantu-linalg --features mumps --test mumps_mpi --no-run --message-format=json 2>/dev/null \
          | sed -n "s/.*\"executable\":\"\([^\"]*\)\".*/\1/p" | tail -1)
    blasx=(-x OPENBLAS_NUM_THREADS); [ -n "${OPENBLAS_CORETYPE:-}" ] && blasx+=(-x OPENBLAS_CORETYPE)
    for p in $RANKS; do for k in $(seq 1 "$RUNS"); do
      tag=rust; [ "$k" = 1 ] || tag=rust$k
      rc=0
      AKANTU_MUMPS_ORACLE=off AKANTU_MUMPS_EXPORT=/x/in AKANTU_MUMPS_TAG=$tag \
        AKANTU_MUMPS_ONLY=blas_pin,oracle_systems,ud216_workspace_retry \
        timeout 600 mpirun -n $p "${blasx[@]}" -x AKANTU_MUMPS_ORACLE -x AKANTU_MUMPS_EXPORT -x AKANTU_MUMPS_TAG \
        -x AKANTU_MUMPS_ONLY "$bin" > /x/in/rust-$tag-p$p.log 2>&1 || rc=$?
      echo "$rc" > /x/in/rust-$tag-p$p.rc
      [ "$rc" = 0 ] || { echo "rust run $tag p=$p: rc=$rc (recorded; continuing)"; tail -5 /x/in/rust-$tag-p$p.log; }
    done; done'
docker run --rm --memory 4g --shm-size=4g --init \
  -v akantu-oracle-build:/build:ro -v "$here/oracle:/oracle:ro" -v "$x:/x" \
  "${blas_env[@]}" -e ORACLE_RUNS="$runs" \
  -e ORACLE_IMAGE_ID="$(docker image inspect akantu-oracle:dev --format '{{.Id}}')" \
  akantu-oracle:dev bash /oracle/probes/mumps/run.sh strict "$ranks"
{
  echo "rust_image=$(docker image inspect "$image" --format '{{.Id}}')"
  echo "rust_git_head=$(git -C "$here" rev-parse HEAD 2>/dev/null || echo unknown)"
  echo "rust_OPENBLAS_CORETYPE=${AKANTU_BLAS_CORETYPE:-<unset: run-time selection>}"
  echo "rust_OPENBLAS_NUM_THREADS=${AKANTU_BLAS_THREADS}"
  grep -h -m1 'INFO blas:' "$x"/in/rust-rust-p*.log 2>/dev/null | head -1 | sed 's/^/rust_/'
} > "$x/rust-provenance.txt"
python3 -I "$here/scripts/mumps/compare.py" "$x" > "$x/comparison.json"
echo "comparison: $x/comparison.json"
