#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Runs INSIDE the oracle image (host wrappers: oracle/scripts/run-probe.sh
# petsc [config], oracle/probes/petsc/generate.sh). Builds petsc_probe in /tmp
# against /build/install-<config> (volume read-only), runs it three times on
# one process (D-035: identical outputs required), prints the JSON with a
# provenance block on stdout. The BLAS pin (D-050) comes
# from the environment the generator passes into the container
# (OPENBLAS_CORETYPE, OPENBLAS_NUM_THREADS) and is forwarded to the probe;
# the probe pins Eigen's cache sizes itself and reports what Eigen and
# OpenBLAS use.
set -uo pipefail
config="${1:-strict}"
src="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
upstream_test_dir="${AKANTU_PROBE_UPSTREAM_TEST_DIR:-/akantu-src/test/test_model/test_common/test_model_solver}"
bld="/tmp/petsc-probe-${config}"
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo '{"error": "no READY marker"}'; exit 3; }
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
cmake -S "${src}" -B "${bld}" -G Ninja -DCMAKE_PREFIX_PATH="${prefix}" \
  -DCMAKE_BUILD_TYPE=RelWithDebInfo ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" \
  -DAKANTU_PROBE_UPSTREAM_TEST_DIR="${upstream_test_dir}" >&2 \
  && cmake --build "${bld}" -j 2 >&2 || { echo '{"error": "build failed"}'; exit 2; }
cd "${bld}"
export OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS="${OPENBLAS_NUM_THREADS:-1}"
for k in 1 2 3; do
  mpiexec --oversubscribe --bind-to none -n 1 -x OMP_NUM_THREADS -x OPENBLAS_NUM_THREADS \
    ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} ./petsc_probe > "run${k}.json" 2> "run${k}.log"
  echo "$?" > "run${k}.rc"
done
python3 -I - "${config}" "${src}/petsc_probe.cc" "${prefix}" <<'PY'
import hashlib, json, os, sys
cfg, src, inst = sys.argv[1:4]
runs = [open(f"run{k}.json", "rb").read() for k in (1, 2, 3)]
rcs = [int(open(f"run{k}.rc").read()) for k in (1, 2, 3)]
identical = len(set(runs)) == 1
doc = json.loads(runs[0]) if rcs[0] == 0 else {"error": open("run1.log").read()[-4000:]}
lib = os.path.realpath(os.path.join(inst, "lib", "libakantu.so"))
info = {}
bi = os.path.join(inst, "share", "akantu-oracle", "build-info.txt")
if os.path.exists(bi):
    for line in open(bi):
        if "=" in line and line.split("=", 1)[0] in ("CMAKE_CXX_FLAGS", "CMAKE_BUILD_TYPE:STRING", "CMAKE_CXX_FLAGS:STRING"):
            k, v = line.rstrip("\n").split("=", 1)
            info[k] = v
doc["provenance"] = {
    "probe": "petsc", "config": cfg, "platform": os.uname().machine,
    "source_sha256": hashlib.sha256(open(src, "rb").read()).hexdigest(),
    "library": lib, "library_sha256": hashlib.sha256(open(lib, "rb").read()).hexdigest(),
    "image_id": os.environ.get("ORACLE_IMAGE_ID"),
    "command": os.environ.get("AKANTU_PROBE_COMMAND", "oracle/scripts/run-probe.sh petsc " + cfg),
    "sources_commit": os.environ.get("AKANTU_PROBE_SOURCES_COMMIT"),
    "reference_sha": os.environ.get("AKANTU_PROBE_REFERENCE_SHA", "35a851780d0b9471d258ed53a003ae98aaa95ea0"),
    "build_info": info,
    "returncodes": rcs, "identical_runs": identical, "runs": 3,
    "run_sha256": [hashlib.sha256(r).hexdigest() for r in runs],
    # D-050: the pin requested and what the probe saw.
    "blas_pin": {"OPENBLAS_CORETYPE": os.environ.get("OPENBLAS_CORETYPE"),
                 "OPENBLAS_NUM_THREADS": os.environ.get("OPENBLAS_NUM_THREADS")},
    "eigen_cache_sizes_requested": [16384, 524288, 524288],
    "cpu": next((l.split(":", 1)[1].strip() for l in open("/proc/cpuinfo")
                 if l.startswith("model name")), None),
}
print(json.dumps(doc, indent=1, sort_keys=True))
PY
