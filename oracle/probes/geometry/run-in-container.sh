#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/geometry/generate.sh. Prepares the probe-local patched copies
# of MeshSphereIntersector (oracle/patches/geometry, UD-409/UD-414), builds
# geometry_probe and ud414_check, runs the probe twice (determinism check),
# runs the UD-414 check and promotes the fixtures fixtures/geometry/*.
set -euo pipefail
config="${1:?config}"
mode="${2:-standard}"   # standard | adversarial (review cases, 3 runs)
case "${mode}" in standard|adversarial) ;; *) echo "invalid mode ${mode}" >&2; exit 2 ;; esac
SRC=/akantu-src
PROBES=/probes
PATCHES=/patches
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
work="$(mktemp -d /tmp/geometry-probe.XXXXXX)"
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
# Probe-local copies of the header-only MeshSphereIntersector.
hdr="$(dirname "$(find "${prefix}/include" -name mesh_sphere_intersector_tmpl.hh | head -1)")"
[ -n "${hdr}" ] && [ -f "${hdr}/mesh_sphere_intersector.hh" ] || { echo "installed geometry headers not found" >&2; exit 3; }
for v in p0001 pall; do
  mkdir -p "${work}/${v}"
  cp "${hdr}/mesh_sphere_intersector.hh" "${hdr}/mesh_sphere_intersector_tmpl.hh" "${work}/${v}/"
  cmp -s "${hdr}/mesh_sphere_intersector_tmpl.hh" "${SRC}/src/geometry/mesh_sphere_intersector_tmpl.hh" ||
    { echo "installed header differs from the reference" >&2; exit 3; }
  patch -s -p3 -d "${work}/${v}" < "${PATCHES}/0001-mesh-sphere-intersector-compile-fixes.patch"
done
patch -s -p3 -d "${work}/pall" < "${PATCHES}/0002-mesh-sphere-intersector-zero-fill.patch"
GEOMETRY_PROBE_PATCHES="$(cd "${PATCHES}" && sha256sum 0001-mesh-sphere-intersector-compile-fixes.patch 0002-mesh-sphere-intersector-zero-fill.patch | awk '{printf "%s sha256=%s;", $2, $1}')"
export GEOMETRY_PROBE_PATCHES
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
cmake -S "${PROBES}/geometry" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DGEOMETRY_PATCHED_INCLUDE="${work}/pall" -DGEOMETRY_PATCHED_INCLUDE_0001="${work}/p0001" \
  -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log" ||
  { tail -40 "${work}/cmake.log"; exit 1; }
ninja -C "${work}/build" -j1 >"${work}/ninja.log" || { tail -80 "${work}/ninja.log"; exit 1; }
if [ "${mode}" = adversarial ]; then
  # The review's adversarial cases (geometry_adversarial_probe.cc): three
  # identical runs (D-035), then promotion of fixtures/geometry/adversarial/*.
  python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
    "${work}/build_env.adv.json" geometry_adversarial_probe \
    --compile-commands "${work}/build/compile_commands.json" \
    --source-suffix "geometry/geometry_adversarial_probe.cc"
  for r in stage rerun rerun2; do
    mkdir -p "${work}/${r}"
    AKANTU_PROBE_BUILD_INFO="${work}/build_env.adv.json" \
      "${work}/build/geometry_adversarial_probe" "${work}/${r}" >"${work}/${r}.log" 2>&1 ||
      { tail -20 "${work}/${r}.log"; exit 1; }
  done
  for r in rerun rerun2; do
    if ! diff -r "${work}/stage" "${work}/${r}" >"${work}/determinism.diff"; then
      head -40 "${work}/determinism.diff" >&2
      echo "geometry_adversarial_probe: runs differ; nothing is promoted" >&2
      exit 1
    fi
  done
  echo "[geometry_adversarial_probe] three identical runs"
  python3 -I "${PROBES}/python/finalize_fixtures.py" /fixtures --stage "${work}/stage" \
    --created "${AKANTU_PROBE_CREATED}"
  exit 0
fi
python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
  "${work}/build_env.geometry_probe.json" geometry_probe \
  --compile-commands "${work}/build/compile_commands.json" --source-suffix "geometry/geometry_probe.cc"
run() {
  AKANTU_PROBE_BUILD_INFO="${work}/build_env.geometry_probe.json" \
    "${work}/build/geometry_probe" "$1" "${SRC}"
}
mkdir -p "${work}/stage" "${work}/rerun"
run "${work}/stage"
run "${work}/rerun" >"${work}/rerun.log" 2>&1 || { tail -20 "${work}/rerun.log"; exit 1; }
if ! diff -r "${work}/stage" "${work}/rerun" >"${work}/determinism.diff"; then
  head -40 "${work}/determinism.diff" >&2
  echo "geometry_probe: two runs differ; nothing is promoted" >&2
  exit 1
fi
echo "[geometry_probe] two identical runs"
"${work}/build/ud414_check" "${SRC}/test/test_geometry/test_geometry_triangle.msh" | tee /fixtures-meta/ud414_check.txt
python3 -I "${PROBES}/python/finalize_fixtures.py" /fixtures --stage "${work}/stage" \
  --created "${AKANTU_PROBE_CREATED}"
