#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Runs INSIDE the akantu-oracle:dev container (called by run-ctest.sh).
# Usage: ctest.sh <config> [ctest args...]
# Runs ctest in /build/<config> in dashboard mode (-T test) and writes, under
# /build/logs/<config>/ctest/<stamp>/ (symlink .../ctest/latest):
#   Test.xml     raw CTest dashboard XML (all outputs)
#   tests.json   `ctest --show-only=json-v1` (test properties, e.g. PROCESSORS)
#   summary.json per-test name/status/duration/ranks + totals (ctest_summary.py)
#   junit.xml    JUnit conversion (upstream test/ci/ctest2junit.xsl)
#   ctest.log    console output
#   outputs/     copy of every *.lastout/*.csv + MANIFEST.json (snapshot-outputs.py)
set -uo pipefail

config="${1:?config name}"
shift
build_dir="/build/${config}"
[ -d "${build_dir}" ] || { echo "no build tree ${build_dir}" >&2; exit 2; }

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
out_dir="/build/logs/${config}/ctest/${stamp}"
mkdir -p "${out_dir}"

# Reproducibility / no thread oversubscription under MPI (also set in image).
export OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1

# Defaults first so that user-supplied arguments override them
# (later --timeout / -j win in ctest).
defaults=(--output-on-failure --no-compress-output --timeout 3600)

cd "${build_dir}"
# Remove previous dashboard results so the Test.xml we pick is this run's.
rm -rf Testing/20*
ctest --show-only=json-v1 > "${out_dir}/tests.json" 2>/dev/null

start=$(date +%s)
ctest -T test "${defaults[@]}" "$@" 2>&1 | tee "${out_dir}/ctest.log"
rc=${PIPESTATUS[0]}
end=$(date +%s)

tag="$(head -n1 Testing/TAG 2>/dev/null || true)"
if [ -n "${tag}" ] && [ -f "Testing/${tag}/Test.xml" ]; then
  cp "Testing/${tag}/Test.xml" "${out_dir}/Test.xml"
  xsltproc -o "${out_dir}/junit.xml" \
    "$(sed -n 's/^CMAKE_HOME_DIRECTORY:INTERNAL=//p' CMakeCache.txt)/test/ci/ctest2junit.xsl" \
    "${out_dir}/Test.xml" || echo "junit conversion failed" >&2
  python3 /oracle/scripts/in-container/ctest_summary.py \
    --config "${config}" --test-xml "${out_dir}/Test.xml" \
    --tests-json "${out_dir}/tests.json" --cache "${build_dir}/CMakeCache.txt" \
    --wall-seconds "$((end - start))" --ctest-args "${defaults[*]} $*" \
    --ctest-rc "${rc}" > "${out_dir}/summary.json"
else
  echo "no Test.xml produced" >&2
fi
# Immutable copy of this run's output captures + provenance manifest:
# compare-outputs.py <run>/outputs... reports files not
# produced by this run as stale.
python3 -I /oracle/scripts/in-container/snapshot-outputs.py \
  --build-dir "${build_dir}" --out "${out_dir}/outputs" \
  --start "${start}" --end "${end}" --source-root /akantu-src \
  --meta "{\"config\": \"${config}\", \"stamp\": \"${stamp}\", \"scripts_sha\": \"${ORACLE_SCRIPTS_SHA:-}\", \"image_id\": \"${ORACLE_IMAGE_ID:-}\"}" \
  || echo "output snapshot failed" >&2
ln -sfn "${stamp}" "/build/logs/${config}/ctest/latest"
echo "ctest rc=${rc}; results in ${out_dir}"
exit "${rc}"
