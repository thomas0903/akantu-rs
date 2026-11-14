#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/gold/generate.sh. Builds the upstream drivers of
# CMakeLists.txt (copies; patches/<driver>.patch applied where the driver
# does not compile at the pinned commit) against /build/install-<config>,
# runs each one three times
# in its own scratch copy of the upstream test directory (the files of its
# FILES_TO_COPY), checks that the three runs are identical (exit status,
# stdout, stderr, every produced file) and stages the outputs of the first
# run with a PROVENANCE.json in /out/<driver>/.
set -euo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
work="$(mktemp -d /tmp/gold-probe.XXXXXX)"
# driver | upstream test directory | files copied (FILES_TO_COPY) | outputs
drivers=(
  "test_assembling_K_cohe_elements|test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit|quadrangle.msh material.dat K_matrix_verified.dat|K_matrix_test.dat"
)
# copies of the driver sources, with the minimal API patch where one exists
mkdir -p "${work}/src"
for entry in "${drivers[@]}"; do
  IFS='|' read -r exe tdir inputs outputs <<<"${entry}"
  cp "${SRC}/${tdir}/${exe}.cc" "${work}/src/${exe}.cc"
  if [ -f "${PROBES}/gold/patches/${exe}.patch" ]; then
    patch --no-backup-if-mismatch -s -d "${work}/src" -p1 <"${PROBES}/gold/patches/${exe}.patch"
  fi
done
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
cmake -S "${PROBES}/gold" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} -DGOLD_SRC_DIR="${work}/src" \
  -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log"
ninja -C "${work}/build" -j1 >"${work}/ninja.log" || { tail -60 "${work}/ninja.log"; exit 1; }

# D-050: pinned BLAS kernels and one thread (MUMPS goes through OpenBLAS).
export OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1

for entry in "${drivers[@]}"; do
  IFS='|' read -r exe tdir inputs outputs <<<"${entry}"
  for r in 1 2 3; do
    d="${work}/run-${exe}-${r}"
    mkdir -p "${d}"
    for f in ${inputs}; do cp "${SRC}/${tdir}/${f}" "${d}/${f}"; done
    rc=0
    (cd "${d}" && "${work}/build/${exe}" >stdout.txt 2>stderr.txt) || rc=$?
    echo "${rc}" >"${d}/exit_status"
    for f in ${inputs}; do
      cmp -s "${SRC}/${tdir}/${f}" "${d}/${f}" || { echo "${exe}: input ${f} modified by run ${r}" >&2; exit 1; }
    done
  done
  for r in 2 3; do
    if ! diff -r "${work}/run-${exe}-1" "${work}/run-${exe}-${r}" >"${work}/determinism-${exe}.diff"; then
      head -40 "${work}/determinism-${exe}.diff" >&2
      echo "${exe}: runs 1 and ${r} differ; nothing is staged" >&2
      exit 1
    fi
  done
  o="/out/${exe}"
  mkdir -p "${o}"
  cp "${work}/run-${exe}-1/stdout.txt" "${work}/run-${exe}-1/stderr.txt" \
     "${work}/run-${exe}-1/exit_status" "${o}/"
  for f in ${outputs}; do
    if [ -f "${work}/run-${exe}-1/${f}" ]; then cp "${work}/run-${exe}-1/${f}" "${o}/"; fi
  done
  python3 -I - "${o}" "${exe}" "${tdir}" "${inputs}" "${config}" "${fp_flags}" "${work}/src" <<'PY'
import hashlib, json, os, platform, subprocess, sys
o, exe, tdir, inputs, config, fp, srcdir = sys.argv[1:8]
def sha(p):
    with open(p, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()
cpu = ""
try:
    for line in subprocess.run(["lscpu"], capture_output=True, text=True).stdout.splitlines():
        if line.startswith("Model name:"):
            cpu = line.split(":", 1)[1].strip()
except OSError:
    pass
# Oracle variants (review condition 3): the upstream defect a
# driver patch works around; the library itself is never patched.
DEFECT_OF_PATCH = {"test_assembling_K_cohe_elements": "UD-697"}
patched = os.path.exists(f"/probes/gold/patches/{exe}.patch")
prov = {
    "generator": "oracle/probes/gold",
    "command": os.environ.get("AKANTU_PROBE_COMMAND", ""),
    "created": os.environ.get("AKANTU_PROBE_CREATED", ""),
    "probe_sources_commit": os.environ.get("AKANTU_PROBE_SOURCES_COMMIT", ""),
    "reference_sha": os.environ.get("AKANTU_PROBE_REFERENCE_SHA", ""),
    "reference_describe": os.environ.get("AKANTU_PROBE_REFERENCE_DESCRIBE", ""),
    "image_id": os.environ.get("ORACLE_IMAGE_ID", ""),
    "config": config,
    "cxx_flags": fp,
    "arch": platform.machine(),
    "cpu_model": cpu,
    "eigen_cache_sizes": "Eigen::setCpuCacheSizes(16384, 524288, 524288) before the upstream main (D-050)",
    "openblas": "OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1 (D-050)",
    "upstream_driver": f"{tdir}/{exe}.cc (compiled with -Dmain=akantu_upstream_main)",
    "upstream_driver_sha256": sha(f"/akantu-src/{tdir}/{exe}.cc"),
    "driver_patch": (f"oracle/probes/gold/patches/{exe}.patch"
                     if os.path.exists(f"/probes/gold/patches/{exe}.patch") else None),
    "driver_patch_sha256": (sha(f"/probes/gold/patches/{exe}.patch")
                            if os.path.exists(f"/probes/gold/patches/{exe}.patch") else None),
    "compiled_driver_sha256": sha(os.path.join(srcdir, f"{exe}.cc")),
    "defect": DEFECT_OF_PATCH.get(exe) if patched else None,
    "variant": ("API-patched driver, library unpatched: the minimal driver patch above "
                "(the upstream driver does not compile at the pinned commit)") if patched else None,
    "upstream_inputs": {f: sha(f"/akantu-src/{tdir}/{f}") for f in inputs.split()},
    "runs": 3,
    "runs_identical": True,
    "exit_status": int(open(os.path.join(o, "exit_status")).read().strip()),
    "outputs": {f: sha(os.path.join(o, f)) for f in sorted(os.listdir(o)) if f != "PROVENANCE.json"},
}
with open(os.path.join(o, "PROVENANCE.json"), "w") as f:
    json.dump(prov, f, indent=1, sort_keys=True)
    f.write("\n")
print(json.dumps(prov, indent=1, sort_keys=True))
PY
done
