#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/contact/generate.sh. Generates the adversarial meshes
# (oracle/probes/contact/meshes/*.geo, gmsh of the image), builds
# contact_probe, runs it three times (D-035 determinism check) and promotes
# the fixtures model/contact/*.
set -euo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
work="$(mktemp -d /tmp/contact-probe.XXXXXX)"
base_args=(--expect-sha "${AKANTU_PROBE_REFERENCE_SHA}" --probe-tree "${PROBES}"
           --install-prefix "${prefix}")
[ -n "${ORACLE_IMAGE_ID:-}" ] && base_args+=(--expect-image "${ORACLE_IMAGE_ID}")
[ -f "/build/${config}/compile_commands.json" ] &&
  base_args+=(--library-compile-commands "/build/${config}/compile_commands.json")
if [ -d "/build/src-${config}" ]; then
  base_args+=(--reference-tree "${SRC}" --patched-tree "/build/src-${config}" --patches-out "${work}/patches.txt")
fi
python3 -I "${PROBES}/python/build_env.py" base "${config}" installed "${flags_file}" \
  "${work}/build_env.json" "${base_args[@]}"
if [ -f "${work}/patches.txt" ]; then
  AKANTU_PROBE_PATCHES="$(cat "${work}/patches.txt")"
  export AKANTU_PROBE_PATCHES
fi
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
cmake -S "${PROBES}/contact" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log"
ninja -C "${work}/build" -j1 >"${work}/ninja.log" || { tail -60 "${work}/ninja.log"; exit 1; }
python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
  "${work}/build_env.contact_probe.json" contact_probe \
  --compile-commands "${work}/build/compile_commands.json" --source-suffix "contact/contact_probe.cc"
frozen="$(ls -d /frozen-meshes/gmsh-* | LC_ALL=C sort | tail -1)"
# adversarial meshes (generated once, used by all runs, committed as inputs)
meshes="${work}/meshes"
mkdir -p "${meshes}"
gmsh -2 -order 1 "${PROBES}/contact/meshes/ridge2d.geo" -o "${meshes}/ridge2d.msh" >"${work}/gmsh.log" 2>&1
gmsh -3 -order 1 "${PROBES}/contact/meshes/curved3d.geo" -o "${meshes}/curved3d.msh" >>"${work}/gmsh.log" 2>&1
gmsh -3 -order 2 "${PROBES}/contact/meshes/curved3d.geo" -o "${meshes}/curved3d_o2.msh" >>"${work}/gmsh.log" 2>&1
gmsh -3 -order 1 "${PROBES}/contact/meshes/curved3d_tri.geo" -o "${meshes}/curved3d_tri.msh" >>"${work}/gmsh.log" 2>&1
run() {
  AKANTU_PROBE_BUILD_INFO="${work}/build_env.contact_probe.json" \
    "${work}/build/contact_probe" "$1" "${SRC}" "${frozen}" "${meshes}"
}
mkdir -p "${work}/stage" "${work}/rerun" "${work}/rerun2"
run "${work}/stage"
for r in rerun rerun2; do
  run "${work}/${r}" >"${work}/${r}.log" 2>&1 || { tail -20 "${work}/${r}.log"; exit 1; }
  if ! diff -r "${work}/stage" "${work}/${r}" >"${work}/determinism.diff"; then
    head -40 "${work}/determinism.diff" >&2
    echo "contact_probe: runs differ (${r}); nothing is promoted" >&2
    exit 1
  fi
done
echo "[contact_probe] three identical runs: $(find "${work}/stage" -name manifest.json | wc -l) case(s)"
# Debug aid: CONTACT_PROBE_STAGE_ONLY=1 copies the staged cases into /fixtures
# without finalisation (no SHA256SUMS, no size policy) for inspection.
if [ "${CONTACT_PROBE_STAGE_ONLY:-0}" = 1 ]; then
  cp -r "${work}/stage/." /fixtures/
  exit 0
fi
python3 -I "${PROBES}/python/finalize_fixtures.py" /fixtures --stage "${work}/stage" \
  --created "${AKANTU_PROBE_CREATED}"
