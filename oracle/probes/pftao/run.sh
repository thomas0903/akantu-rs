#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Runs INSIDE the oracle image (host wrapper: oracle/probes/pftao/generate.sh).
# Builds pftao_probe in /tmp against /build/install-<config> (volume
# read-only), runs every case three times on one process (D-035: identical
# outputs required), writes one JSON per case with a provenance block into
# the output directory given as $2 (mounted by generate.sh). The BLAS pin
# (D-050) comes from the environment the generator passes in and is
# forwarded to the probe; the probe pins Eigen's cache sizes itself.
set -uo pipefail
config="${1:-strict}"
out="${2:-/out}"
cases="${PFTAO_CASES:-optimal_linear tao_linear_no_split tao_linear_volumetric_deviatoric tao_quadratic_no_split tao_quadratic_volumetric_deviatoric cpl_tao_linear_no_split cpl_tao_linear_volumetric_deviatoric cpl_tao_quadratic_no_split cpl_tao_quadratic_volumetric_deviatoric}"
src="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bld="/tmp/pftao-probe-${config}"
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no READY marker" >&2; exit 3; }
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
cmake -S "${src}" -B "${bld}" -G Ninja -DCMAKE_PREFIX_PATH="${prefix}" \
  -DCMAKE_BUILD_TYPE=RelWithDebInfo ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >&2 \
  && cmake --build "${bld}" -j 2 >&2 || { echo "build failed" >&2; exit 2; }
cd "${bld}"
export OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS="${OPENBLAS_NUM_THREADS:-1}"
rc_all=0
for c in ${cases}; do
  for k in 1 2 3; do
    mpiexec --oversubscribe --bind-to none -n 1 -x OMP_NUM_THREADS -x OPENBLAS_NUM_THREADS \
      ${OPENBLAS_CORETYPE:+-x OPENBLAS_CORETYPE} ./pftao_probe "${c}" /akantu-src \
      > "${c}.run${k}.json" 2> "${c}.run${k}.log"
    echo "$?" > "${c}.run${k}.rc"
  done
  python3 -I - "${config}" "${src}/pftao_probe.cc" "${prefix}" "${c}" "${out}/${c}.json" <<'PY'
import hashlib, json, os, sys
cfg, src, inst, case, dest = sys.argv[1:6]
runs = [open(f"{case}.run{k}.json", "rb").read() for k in (1, 2, 3)]
rcs = [int(open(f"{case}.run{k}.rc").read()) for k in (1, 2, 3)]
identical = len(set(runs)) == 1
doc = json.loads(runs[0]) if rcs[0] == 0 else {"error": open(f"{case}.run1.log").read()[-4000:]}
lib = os.path.realpath(os.path.join(inst, "lib", "libakantu.so"))
info = {}
bi = os.path.join(inst, "share", "akantu-oracle", "build-info.txt")
if os.path.exists(bi):
    for line in open(bi):
        if "=" in line and line.split("=", 1)[0] in ("CMAKE_CXX_FLAGS", "CMAKE_BUILD_TYPE:STRING", "CMAKE_CXX_FLAGS:STRING"):
            k, v = line.rstrip("\n").split("=", 1)
            info[k] = v
doc["provenance"] = {
    "probe": "pftao", "config": cfg, "platform": os.uname().machine,
    "source_sha256": hashlib.sha256(open(src, "rb").read()).hexdigest(),
    "library": lib, "library_sha256": hashlib.sha256(open(lib, "rb").read()).hexdigest(),
    "image_id": os.environ.get("ORACLE_IMAGE_ID"),
    "command": os.environ.get("AKANTU_PROBE_COMMAND", "oracle/probes/pftao/generate.sh"),
    "sources_commit": os.environ.get("AKANTU_PROBE_SOURCES_COMMIT"),
    "reference_sha": os.environ.get("AKANTU_PROBE_REFERENCE_SHA", "35a851780d0b9471d258ed53a003ae98aaa95ea0"),
    "build_info": info,
    "returncodes": rcs, "identical_runs": identical, "runs": 3,
    "run_sha256": [hashlib.sha256(r).hexdigest() for r in runs],
    "blas_pin": {"OPENBLAS_CORETYPE": os.environ.get("OPENBLAS_CORETYPE"),
                 "OPENBLAS_NUM_THREADS": os.environ.get("OPENBLAS_NUM_THREADS")},
    "eigen_cache_sizes_requested": [16384, 524288, 524288],
    "x86_64_level": os.environ.get("ORACLE_X86_64_LEVEL"),
    "cpu": next((l.split(":", 1)[1].strip() for l in open("/proc/cpuinfo")
                 if l.startswith("model name")), None),
}
with open(dest, "w") as f:
    f.write(json.dumps(doc, indent=1, sort_keys=True) + "\n")
ok = "error" not in doc and rcs == [0, 0, 0] and identical
print(f"{case}: rc={rcs} identical={identical} -> {dest}", file=sys.stderr)
sys.exit(0 if ok else 1)
PY
  [ $? -eq 0 ] || rc_all=1
done
exit "${rc_all}"
