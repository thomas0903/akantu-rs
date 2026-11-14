#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/embedded/generate.sh. Builds the patched oracle variant
# `embedded` (D-060, docs/ORACLE_BUILD.md): copies of the reference
# embedded sources, of model_options.hh and of upstream's embedded tests are
# patched with oracle/patches/embedded/0001…0008 (the reference checkout and
# the installed library are never modified), compiled against the installed
# oracle with its floating-point flags, then
#   1. upstream's three embedded tests run with their own criteria (and the
#      patched .sh driver: diff of the saved stiffness with the 2015 golden
#      `matrix`); results in /fixtures-meta/upstream-tests.txt;
#   2. embedded_probe runs every case three times (D-035); the cases whose
#      three runs are byte-identical are promoted to /fixtures
#      (model/embedded/*); a case that fails or differs is reported in
#      /fixtures-meta/probe-runs.txt and not promoted.
set -euo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
PATCHES=/patches
META=/fixtures-meta
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
work="$(mktemp -d /tmp/embedded-variant.XXXXXX)"
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
patches=""
[ -f "${work}/patches.txt" ] && patches="$(cat "${work}/patches.txt")"

# --- patched copies (never the reference checkout) ---------------------------
E=src/model/solid_mechanics/solid_mechanics_model_embedded_interface
M=src/model/solid_mechanics/materials/material_embedded
T=test/test_model/test_solid_mechanics_model/test_embedded_interface
tree="${work}/tree"
for f in src/model/model_options.hh ${E}/embedded_interface_model.hh ${E}/embedded_interface_model.cc \
         ${E}/embedded_interface_intersector.hh ${E}/embedded_interface_intersector.cc \
         ${M}/material_reinforcement.hh ${M}/material_reinforcement_tmpl.hh; do
  mkdir -p "${tree}/$(dirname "$f")"
  cp "${SRC}/$f" "${tree}/$f"
done
mkdir -p "${tree}/${T}"
cp "${SRC}/${T}"/* "${tree}/${T}/"
# the installed copy of every header the variant replaces must be the reference one
for h in model_options.hh; do
  cmp "${prefix}/include/akantu/${h}" "${SRC}/src/model/${h}" ||
    { echo "installed ${h} differs from the reference" >&2; exit 3; }
done
variant_list=""
for p in "${PATCHES}"/0*.patch; do
  patch -s -p1 --no-backup-if-mismatch -d "${tree}" < "${p}"
  variant_list="${variant_list}oracle/patches/embedded/$(basename "${p}") sha256=$(sha256sum "${p}" | cut -d' ' -f1),"
done
EMBEDDED_VARIANT_SHA256="$(cat "${PATCHES}"/0*.patch | sha256sum | cut -d' ' -f1)"
export EMBEDDED_VARIANT_SHA256
AKANTU_PROBE_PATCHES="${patches:+${patches},}${variant_list%,}"
export AKANTU_PROBE_PATCHES
mkdir -p "${work}/inc" "${work}/psrc"
cp -r "${prefix}/include/akantu/." "${work}/inc/"
cp "${tree}/src/model/model_options.hh" "${tree}/${E}/"*.hh "${tree}/${M}/"*.hh "${work}/inc/"
cp "${tree}/${E}/"*.cc "${work}/psrc/"

# --- build (same compiler and FP flags as the install) -----------------------
# Akantu translation units need several GB each: -j1 by default (three in
# parallel were OOM-killed in the 6 GB container on a 4-CPU host);
# EMBEDDED_BUILD_JOBS raises it (at most 3).
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
cmake -S "${PROBES}/embedded" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DEMBEDDED_PATCHED_INC="${work}/inc" -DEMBEDDED_PATCHED_SRC="${work}/psrc" \
  -DEMBEDDED_UPSTREAM_TESTS="${tree}/${T}" \
  -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log" 2>&1 ||
  { tail -40 "${work}/cmake.log"; exit 1; }
jobs="${EMBEDDED_BUILD_JOBS:-1}"
[[ "${jobs}" =~ ^[123]$ ]] || { echo "EMBEDDED_BUILD_JOBS must be 1, 2 or 3" >&2; exit 2; }
ninja -C "${work}/build" -j"${jobs}" >"${work}/ninja.log" 2>&1 ||
  { grep -E "error|Error" "${work}/ninja.log" | head -60; exit 1; }
{
  echo "fp flags: ${fp_flags}"
  echo "variant patches sha256 (concatenated, in order): ${EMBEDDED_VARIANT_SHA256}"
  echo "${variant_list%,}" | tr ',' '\n'
  sha256sum "${prefix}/lib/libakantu.so" || true
  echo "install READY: $(cat "${prefix}/READY")"
  echo "image: ${ORACLE_IMAGE_ID:-unknown}"
  echo "OPENBLAS_CORETYPE=${OPENBLAS_CORETYPE:-unset} OPENBLAS_NUM_THREADS=${OPENBLAS_NUM_THREADS:-unset}"
  echo "host cpu: $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ //')"
} > "${META}/provenance.txt"
grep -c "cpuid" <(objdump -d "${work}/build/embedded_probe" 2>/dev/null) > "${META}/probe-cpuid-count.txt" || true

# --- upstream tests (patched copies, their own criteria) ---------------------
run="${work}/run"
mkdir -p "${run}/paraview"
cp "${tree}/${T}"/*.msh "${tree}/${T}"/*.dat "${tree}/${T}/matrix" "${tree}/${T}/test_embedded_interface_model.sh" "${run}/"
: > "${META}/upstream-tests.txt"
for t in test_embedded_element_matrix test_embedded_interface_model test_embedded_interface_model_prestress; do
  cp "${work}/build/${t}" "${run}/"
  set +e; (cd "${run}" && "./${t}") > "${META}/${t}.log" 2>&1; rc=$?; set -e
  echo "${t} exit ${rc}" | tee -a "${META}/upstream-tests.txt"
done
set +e; (cd "${run}" && bash ./test_embedded_interface_model.sh) > "${META}/test_embedded_interface_model.sh.log" 2>&1; rc=$?; set -e
echo "test_embedded_interface_model.sh (patched 0008: diff matrix matrix_test.mtx) exit ${rc}" | tee -a "${META}/upstream-tests.txt"
set +e; (cd "${run}" && diff matrix matrix_test) > /dev/null 2>&1; rc=$?; set -e
echo "unpatched driver command 'diff matrix matrix_test' exit ${rc} (2 = file missing: saveMatrix writes matrix_test.mtx)" | tee -a "${META}/upstream-tests.txt"
cp "${run}/matrix_test.mtx" "${META}/matrix_test.mtx" 2>/dev/null || true
cp "${run}/stiffness.mtx" "${META}/stiffness.mtx" 2>/dev/null || true

# --- probe: three runs per case (D-035) --------------------------------------
python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
  "${work}/build_env.embedded_probe.json" embedded_probe \
  --compile-commands "${work}/build/compile_commands.json" --source-suffix "embedded/embedded_probe.cc"
cases="${EMBEDDED_PROBE_CASES:-t125 t126 oblique t127 prestress_2d_rest adversarial_2d grazing_2d_1e-4 grazing_2d_1e-10 triangle6_oblique adversarial_3d prestress_3d_rest grazing_3d_1e-4 grazing_3d_1e-10 face_z0_3d faces_grazing_3d}"
: > "${META}/probe-runs.txt"
ok=""
for c in ${cases}; do
  good=1
  for r in stage rerun rerun2; do
    mkdir -p "${work}/${r}" "${work}/cwd/${r}/${c}"
    set +e
    (cd "${work}/cwd/${r}/${c}" &&
       AKANTU_PROBE_BUILD_INFO="${work}/build_env.embedded_probe.json" \
       "${work}/build/embedded_probe" "${c}" "${run}" "${work}/${r}") \
       > "${META}/probe-${c}-${r}.log" 2>&1
    rc=$?
    set -e
    if [ "${rc}" -ne 0 ]; then
      echo "${c}: run ${r} exit ${rc} (not promoted)" | tee -a "${META}/probe-runs.txt"
      tail -5 "${META}/probe-${c}-${r}.log" | sed 's/^/    /' | tee -a "${META}/probe-runs.txt"
      good=0
      break
    fi
  done
  if [ "${good}" = 1 ]; then
    if diff -r "${work}/stage/model/embedded/${c}" "${work}/rerun/model/embedded/${c}" >/dev/null &&
       diff -r "${work}/stage/model/embedded/${c}" "${work}/rerun2/model/embedded/${c}" >/dev/null; then
      if [ -d "${work}/stage/model/embedded/${c}/data" ]; then
        n="$(find "${work}/stage/model/embedded/${c}/data" -name '*.npy' | wc -l)"
        echo "${c}: 3 identical runs (${n} quantities)" | tee -a "${META}/probe-runs.txt"
      else
        echo "${c}: 3 identical runs (expected error: $(grep -o '"error_kind": "[^"]*"' "${work}/stage/model/embedded/${c}/manifest.json" || true))" | tee -a "${META}/probe-runs.txt"
      fi
      ok="${ok} ${c}"
    else
      echo "${c}: runs differ (not promoted)" | tee -a "${META}/probe-runs.txt"
      rm -rf "${work}/stage/model/embedded/${c}"
    fi
  else
    rm -rf "${work}/stage/model/embedded/${c}"
  fi
done
# diagnostics (not promoted): does the probe's D-050 pin reach libakantu's
# SparseLU? t127 with Zen 5's CPUID cache sizes instead of the pin
if [ -d "${work}/stage/model/embedded/t127" ]; then
  mkdir -p "${work}/diag" "${work}/cwd/diag"
  set +e
  (cd "${work}/cwd/diag" && EMBEDDED_PROBE_EIGEN_CACHE="65536 524288 16777216" \
     AKANTU_PROBE_BUILD_INFO="${work}/build_env.embedded_probe.json" \
     "${work}/build/embedded_probe" t127 "${run}" "${work}/diag") > "${META}/probe-t127-diag.log" 2>&1
  set -e
  for q in u f_int; do
    if cmp -s "${work}/stage/model/embedded/t127/data/${q}.npy" "${work}/diag/model/embedded/t127/data/${q}.npy"; then
      echo "diag t127 ${q}: identical with cache sizes 65536/524288/16777216" | tee -a "${META}/probe-runs.txt"
    else
      echo "diag t127 ${q}: DIFFERENT with cache sizes 65536/524288/16777216 (the pin is effective and needed)" | tee -a "${META}/probe-runs.txt"
    fi
  done
fi
[ -n "${ok}" ] || { echo "no case to promote" >&2; exit 1; }
python3 -I "${PROBES}/python/finalize_fixtures.py" /fixtures --stage "${work}/stage" \
  --created "${AKANTU_PROBE_CREATED}"
echo "promoted:${ok}" | tee -a "${META}/probe-runs.txt"
