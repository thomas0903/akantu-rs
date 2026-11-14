#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs adversarial coupler probe: runs INSIDE akantu-oracle:dev (started by run.sh).
# Builds v80_probe against /build/install-strict, runs each mode three times
# (D-035) and the unmodified upstream test binary test_multi_material
# (TEST-097) from a scratch copy of its build directory.
set -uo pipefail
prefix=/build/install-strict
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
work=/tmp/v80
mkdir -p "${work}"
cmake -S /v80 -B "${work}/build" -G Ninja -DCMAKE_PREFIX_PATH="${prefix}" \
  -DCMAKE_BUILD_TYPE=RelWithDebInfo ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" > /out/cmake.log 2>&1 || { tail -30 /out/cmake.log; exit 1; }
ninja -C "${work}/build" -j2 > /out/ninja.log 2>&1 || { tail -60 /out/ninja.log; exit 1; }
echo "fp_flags=${fp_flags}" > /out/build.txt
grep -m1 -o '"command": "[^"]*v80_probe.cc[^"]*"' "${work}/build/compile_commands.json" >> /out/build.txt || true
frozen="$(ls -d /frozen-meshes/gmsh-* | LC_ALL=C sort | tail -1)"
for mode in ${V80_MODES:-cohesive_static pf_selector_mismatch pf_initfull cohesive_static_k0}; do
  for r in 1 2 3; do
    (cd "${work}" && "${work}/build/v80_probe" "${mode}" /akantu-src "${frozen}" \
       > "/out/${mode}.run${r}.txt" 2> "/out/${mode}.run${r}.err"; echo $? > "/out/${mode}.run${r}.rc")
  done
done
[ -n "${V80_MODES:-}" ] && { echo done > /out/DONE; exit 0; }
# UD-621: expected crash; run it once in its own process, record the status
(cd "${work}" && "${work}/build/v80_probe" coupler_section /akantu-src "${frozen}" \
   > /out/coupler_section.txt 2> /out/coupler_section.err; echo $? > /out/coupler_section.rc)
# TEST-097: the unmodified upstream test binary, from a scratch copy of its
# build directory (the build volume is mounted read-only)
tdir=/build/strict/test/test_model/test_phase_field_model
mkdir -p "${work}/t097"
cp -a "${tdir}/." "${work}/t097/"
ls -la "${work}/t097" > /out/test097.ls
(cd "${work}/t097" && ./test_multi_material > /out/test097.out 2> /out/test097.err; echo $? > /out/test097.rc)
sha256sum "${tdir}/test_multi_material" > /out/test097.sha256
grep -n 'test_multi_material' /build/strict/test/test_model/test_phase_field_model/CTestTestfile.cmake > /out/test097.ctest 2>&1 || true
echo done > /out/DONE
