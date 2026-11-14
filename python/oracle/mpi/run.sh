#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (D-101 (1), PY-053): the mpirun harness of the MPI
# build of the Python module, on a sandbox with the pinned oracle image
# (akantu-oracle:dev and its build volume, docs/ORACLE_BUILD.md).
#
#   python/oracle/mpi/run.sh <repo> <port-python-dir> <out-dir> [ranks...]
#
# <port-python-dir>: a copy of python/ whose akantu/ holds the MPI build of
# the extension (python/oracle/mpi/build-module.sh). Both modules run in
# the same oracle image (Debian trixie: Open MPI 5.0.7, Scotch 7.0.7,
# CPython 3.13, NumPy, SciPy, mpi4py 4.0.3), with AKA_SEED=10 and
# SCOTCH_PTHREAD_NUMBER=1 (reproducible Scotch partitions).
# * mesh_distribute.py at every rank count (default 1 2 4): three oracle
#   runs (identical per rank, D-035), one port run, compare.py at
#   tolerance 0 on every rank's output (bitwise floats; no key missing and,
#   none extra: the key sets are equal), and the port's
#   own global node ids against the coordinate-derived ones;
# * lifecycle.py: every scenario with the port at 1 / 2 / 4 ranks
#   (funneled_* at 1 / 2), the upstream-API scenarios with the oracle too;
#   a scenario passes when every rank prints its PASS line and mpirun
#   returns 0.
# Summary lines go to <out-dir>/summary.txt; the last line is
# `HARNESS: PASS` or `HARNESS: FAIL`.
set -u
REPO=$1 PORT=$2 OUT=$3
shift 3
RANKS=${*:-1 2 4}
IMG=${AKANTU_ORACLE_IMAGE:-akantu-oracle:dev}
mkdir -p "$OUT"
SUM=$OUT/summary.txt
: > "$SUM"
fail=0
echo "image $IMG $(docker images --no-trunc "$IMG" --format '{{.ID}}')" >> "$SUM"
echo "repo $(git -C "$REPO" rev-parse HEAD 2>/dev/null || cat "$REPO/../sha" 2>/dev/null)" >> "$SUM"
echo "port_module_sha256 $(sha256sum "$PORT"/akantu/py11_akantu*.so | cut -c1-16)" >> "$SUM"

prep() {
  rm -rf "$1"
  mkdir -p "$1"
  cp "$REPO"/python/oracle/snippets/_out.py "$REPO"/python/oracle/mpi/*.py "$1/"
  while read -r name path; do
    case "$name" in '' | '#'*) continue ;; esac
    cp "$REPO/$path" "$1/$name"
  done < "$REPO/python/oracle/inputs.txt"
}

# run <work dir> <oracle|port> <np> <python args...>
run() {
  local w=$1 side=$2 np=$3
  shift 3
  local setup
  if [ "$side" = oracle ]; then
    setup='. /build/install-strict/share/akantu-oracle/env.sh &&'
  else
    setup='export PYTHONPATH=/port &&'
  fi
  docker run --rm --init --memory 12g --shm-size 1g \
    -e OMPI_MCA_btl=tcp,self,sm -e OMPI_MCA_btl_sm_single_copy_mechanism=none \
    -e OMPI_MCA_btl_base_warn_component_unused=0 -e OMPI_MCA_rmaps_base_oversubscribe=1 \
    -e PRTE_MCA_rmaps_default_mapping_policy=:oversubscribe \
    -e OMPI_MCA_hwloc_base_binding_policy=none -e PRTE_MCA_hwloc_default_binding_policy=none \
    -e OPENBLAS_NUM_THREADS=1 -e OPENBLAS_CORETYPE=Haswell -e OMP_NUM_THREADS=1 \
    -e AKA_SEED=10 -e SCOTCH_PTHREAD_NUMBER=1 \
    -v akantu-oracle-build:/build:ro -v "$PORT:/port:ro" -v "$w:/work" -w /work "$IMG" \
    timeout -k 10 600 mpirun -np "$np" --oversubscribe --bind-to none \
    bash -c "$setup exec python3 $*"
}

for n in $RANKS; do
  for k in 1 2 3; do
    w=$OUT/dist-np$n-o$k
    prep "$w"
    run "$w" oracle "$n" mesh_distribute.py > "$w.log" 2>&1
    echo "rc=$?" >> "$w.log"
  done
  w=$OUT/dist-np$n-p
  prep "$w"
  run "$w" port "$n" mesh_distribute.py > "$w.log" 2>&1
  echo "rc=$?" >> "$w.log"
  for r in $(seq 0 $((n - 1))); do
    same=identical
    for k in 2 3; do
      cmp -s "$OUT/dist-np$n-o1/out.r$r.txt" "$OUT/dist-np$n-o$k/out.r$r.txt" || same=DIFFER
    done
    python3 "$REPO/python/oracle/compare.py" "$OUT/dist-np$n-o1/out.r$r.txt" \
      "$OUT/dist-np$n-p/out.r$r.txt" 0 > "$OUT/dist-np$n-r$r.cmp" 2>&1
    rc=$?
    gid=$(python3 - "$OUT/dist-np$n-p/port.r$r.txt" <<'PY'
import ast, sys
d = {}
for line in open(sys.argv[1]):
    k, v = line.rstrip("\n").split(" = ", 1)
    d[k] = v
ok = True
for name in ("plate", "cube"):
    ok &= ast.literal_eval(d[f"{name}.derived"]) == ast.literal_eval(d[f"{name}.native"])
    a, b = d[f"{name}.nb_global_nodes"].split()
    ok &= a == b
print("ok" if ok else "MISMATCH")
PY
)
    keys=$(grep -c ' = ' "$OUT/dist-np$n-o1/out.r$r.txt")
    bitwise=$(grep -c '^OK ' "$OUT/dist-np$n-r$r.cmp")
    # Compare.py fails on MISSING / DIFFER / TOL lines
    # only; a key the port prints and the oracle does not (an extra element
    # type, block or group on a rank) is an EXTRA line, which fails here, so
    # the two key sets are equal
    extra=$(grep -c '^EXTRA ' "$OUT/dist-np$n-r$r.cmp")
    verdict=PASS
    if [ "$rc" != 0 ] || [ "$same" != identical ] || [ "$gid" != ok ] || [ "$keys" -lt 10 ] || [ "$extra" != 0 ]; then verdict=FAIL; fail=1; fi
    echo "distribute np=$n rank=$r oracle_runs=$same compare_rc=$rc keys=$keys ok=$bitwise extra=$extra port_global_ids=$gid oracle_sha=$(sha256sum < "$OUT/dist-np$n-o1/out.r$r.txt" | cut -c1-16) $verdict" >> "$SUM"
  done
done

scen() {
  local side=$1 s=$2 n=$3
  local w=$OUT/life-$side-$s-np$n
  prep "$w"
  run "$w" "$side" "$n" lifecycle.py "$s" > "$w.log" 2>&1
  local rc=$?
  local passes
  passes=$(grep -ac "LIFECYCLE PASS $s " "$w.log")
  local verdict=PASS
  if [ "$rc" != 0 ] || [ "$passes" != "$n" ]; then verdict=FAIL; fail=1; fi
  echo "lifecycle $side $s np=$n rc=$rc passes=$passes/$n $verdict" >> "$SUM"
}
for n in $RANKS; do
  for s in mpi4py_first mpi4py_first_kept akantu_first akantu_first_kept stale_view; do
    scen port "$s" "$n"
  done
  for s in mpi4py_first mpi4py_first_kept; do
    scen oracle "$s" "$n"
  done
done
for n in 1 2; do
  for s in funneled_thread funneled_mpi4py; do
    scen port "$s" "$n"
  done
done
if [ "$fail" = 0 ]; then echo "HARNESS: PASS" >> "$SUM"; else echo "HARNESS: FAIL" >> "$SUM"; fi
exit $fail
