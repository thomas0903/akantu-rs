#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs upstream's independent Python oracle
# test/test_model/test_common/test_model_solver/test_model_solver.py
# (with test/python_fe, NumPy/SciPy of the oracle image) on the oracle's
# full-precision static `test_model_solver` displacement
# (fixtures/solver_model/static_model_solver/data/displacement.npy), as
# disp.csv with 17 significant digits. Prints the script's relative error.
# Usage: oracle/probes/gold/python_fe_check.sh [--out DIR]   (host with Docker)
# The check runs three times, each in a fresh container with the OpenBLAS pin
# of D-050; the runs must be identical (exit status, and output once the
# random scratch-directory name /tmp/pyfe.XXXXXX of the traceback line is
# normalised), otherwise nothing is staged and the script exits 3. With --out,
# DIR/test_model_solver_py/ receives output.txt (everything run 1 printed),
# exit_status and PROVENANCE.json.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
# Numerical-environment pin and x86-64-v3 host check of every oracle container
# start (D-050 (5), D-073 (2)). Empty on arm64.
# shellcheck source=oracle/scripts/oracle-env.sh
source "${repo}/oracle/scripts/oracle-env.sh"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
RUNS=3
out=""
cmdline="oracle/probes/gold/python_fe_check.sh $*"
if [ "${1:-}" = "--out" ]; then out="${2:?--out DIR}"; fi
image_id="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")"
ref_sha="$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)"
probes_commit="$(git -C "${repo}" rev-parse HEAD 2>/dev/null || echo unknown)"
if [ -n "$(git -C "${repo}" status --porcelain -- oracle/probes 2>/dev/null)" ]; then
  probes_commit="${probes_commit}+uncommitted-probe-changes"
fi
logdir="$(mktemp -d)"
run_check() { # $1: log file; returns the check's exit status
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --init --memory=2g --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${repo}/fixtures/solver_model/static_model_solver:/case:ro" \
  "${ORACLE_IMAGE}" bash -c '
set -euo pipefail
w=$(mktemp -d /tmp/pyfe.XXXXXX)
cd "$w"
python3 -I - <<PY
import numpy as np
u = np.load("/case/data/displacement.npy")
n = len(u)
with open("disp.csv", "w") as f:
    f.write("node,disp,force,blocked\n")
    for i in range(n):
        f.write("%d,%.17g,%.17g,%d\n" % (i, u[i], -10.0 if i == n - 1 else 0.0, 1 if i == 0 else 0))
PY
cp /akantu-src/test/test_model/test_common/test_model_solver/test_model_solver.py .
# Changes to a copy of the script (UD-699): (1) python_fe/__init__.py does
# not import its sub-modules, so `pfe.TrussFE` / `pfe.StaticSolver` only
# exist after importing them; (2) `np.where(c)` (a tuple of index arrays) is
# used as a node index, which NumPy (2.2.4, verified) rejects in
# `fe.Solver._zero_rows` (`A.indptr[r]` with an array `r`): `np.where(c)[0]`
# is what was meant (needed for `blocked`; the same rewrite of
# `np.where(F == f)` is a no-op: the script gives the identical result
# without it; kept so that the staged output.txt stays reproducible);
# (3) the script only raises, so its relative error is printed as well.
sed -e "s/^import python_fe as pfe$/import python_fe as pfe\nimport python_fe.truss_fe, python_fe.static_solver  # noqa: E401/" \
    -e "s/np\.where(\([^)]*\))/np.where(\1)[0]/g" \
    -e "s/^if n > 1e-14:/print(\"relative error\", repr(n))\nif n > 1e-14:/" test_model_solver.py > check.py
diff test_model_solver.py check.py || true
python3 -c "import numpy, scipy; print(\"numpy\", numpy.__version__, \"scipy\", scipy.__version__)"
rc=0
PYTHONPATH=/akantu-src/test python3 check.py 2>&1 || rc=$?
echo "exit status ${rc}"
exit ${rc}
' >"$1" 2>&1
}
for r in $(seq 1 "${RUNS}"); do
  rc_r=0
  run_check "${logdir}/run${r}.log" || rc_r=$?
  echo "${rc_r}" >"${logdir}/run${r}.rc"
done
# The traceback names the script by its absolute path in the run's mktemp
# directory: compare the runs with that name normalised.
for r in $(seq 1 "${RUNS}"); do
  sed -E 's#/tmp/pyfe\.[A-Za-z0-9]{6}/#/tmp/pyfe.XXXXXX/#g' "${logdir}/run${r}.log" \
    >"${logdir}/run${r}.norm"
done
for r in $(seq 2 "${RUNS}"); do
  if ! cmp -s "${logdir}/run1.norm" "${logdir}/run${r}.norm" ||
     ! cmp -s "${logdir}/run1.rc" "${logdir}/run${r}.rc"; then
    echo "python_fe_check: runs 1 and ${r} differ; nothing is staged" >&2
    rm -rf "${logdir}"
    exit 3
  fi
done
rc="$(cat "${logdir}/run1.rc")"
cat "${logdir}/run1.log"
echo "(${RUNS} identical runs)"
if [ -n "${out}" ]; then
  d="${out}/test_model_solver_py"
  mkdir -p "${d}"
  cp "${logdir}/run1.log" "${d}/output.txt"
  echo "${rc}" >"${d}/exit_status"
  python3 - "${d}" "${image_id}" "${ref_sha}" "$(uname -m)" "${RUNS}" "${cmdline}" "${probes_commit}" <<'PY'
import hashlib, json, os, sys, datetime
d, image, ref, arch, runs, cmdline, probes_commit = sys.argv[1:8]
def sha(p):
    return hashlib.sha256(open(p, "rb").read()).hexdigest()
prov = {
    "generator": "oracle/probes/gold/python_fe_check.sh",
    "created": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "reference_sha": ref,
    "image_id": image,
    "arch": arch,
    "input": "fixtures/solver_model/static_model_solver/data/displacement.npy (oracle, full precision) as disp.csv (%.17g), force -10 at the last node, node 0 blocked",
    "script": "test/test_model/test_common/test_model_solver/test_model_solver.py with test/python_fe, copy changed as documented in the generator (UD-699)",
    "command": cmdline,
    "probe_sources_commit": probes_commit,
    "openblas": "OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1 (D-050)",
    "runs": int(runs),
    "runs_identical": True,
    "runs_identical_modulo": "the random scratch-directory name /tmp/pyfe.XXXXXX in the traceback line (output.txt is run 1's log)",
    "exit_status": int(open(os.path.join(d, "exit_status")).read()),
    "outputs": {f: sha(os.path.join(d, f)) for f in sorted(os.listdir(d)) if f != "PROVENANCE.json"},
}
json.dump(prov, open(os.path.join(d, "PROVENANCE.json"), "w"), indent=1, sort_keys=True)
open(os.path.join(d, "PROVENANCE.json"), "a").write("\n")
PY
fi
rm -rf "${logdir}"
exit "${rc}"
