#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs INSIDE akantu-oracle:dev, started by
# oracle/probes/non_local/adversarial/generate.sh. Builds adversarial_probe
# like oracle/probes/non_local/run-in-container.sh, runs every case three times
# in its own process (D-035; a crash of one case is recorded and does not stop
# the others), stages the cases with three identical runs and a manifest, and
# promotes them (finalize_fixtures.py) unless NL_PROBE_STAGE_ONLY=1.
set -uo pipefail
config="${1:?config}"
SRC=/akantu-src
PROBES=/probes
prefix="/build/install-${config}"
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
[ -f "${prefix}/READY" ] || { echo "no ${prefix}/READY marker" >&2; exit 3; }
work="$(mktemp -d /tmp/non-local-adv.XXXXXX)"
base_args=(--expect-sha "${AKANTU_PROBE_REFERENCE_SHA}" --probe-tree "${PROBES}"
           --install-prefix "${prefix}")
[ -n "${ORACLE_IMAGE_ID:-}" ] && base_args+=(--expect-image "${ORACLE_IMAGE_ID}")
[ -f "/build/${config}/compile_commands.json" ] &&
  base_args+=(--library-compile-commands "/build/${config}/compile_commands.json")
if [ -d "/build/src-${config}" ]; then
  base_args+=(--reference-tree "${SRC}" --patched-tree "/build/src-${config}" --patches-out "${work}/patches.txt")
fi
python3 -I "${PROBES}/python/build_env.py" base "${config}" installed "${flags_file}" \
  "${work}/build_env.json" "${base_args[@]}" || exit 3
if [ -f "${work}/patches.txt" ]; then
  AKANTU_PROBE_PATCHES="$(cat "${work}/patches.txt")"
  export AKANTU_PROBE_PATCHES
fi
fp_flags="$(sed -nE 's/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p' "${flags_file}" | head -1)"
cgal_dir="$(sed -n 's/^CGAL_DIR:PATH=//p' "${flags_file}" | head -1)"
cmake -S "${PROBES}/non_local/adversarial" -B "${work}/build" -G Ninja \
  -DCMAKE_PREFIX_PATH="${prefix}" -DCMAKE_BUILD_TYPE=RelWithDebInfo \
  ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CONFIG="${config}" -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" >"${work}/cmake.log" ||
  { cat "${work}/cmake.log"; exit 1; }
ninja -C "${work}/build" -j1 >"${work}/ninja.log" || { tail -60 "${work}/ninja.log"; exit 1; }
python3 -I "${PROBES}/python/build_env.py" probe "${work}/build_env.json" \
  "${work}/build_env.adversarial_probe.json" adversarial_probe \
  --compile-commands "${work}/build/compile_commands.json" \
  --source-suffix "non_local/adversarial/adversarial_probe.cc" || exit 3
cases="mgr_plate_r10 mgr_plate_r0p01 mgr_plate_r0p25 avg_line_r0p1 avg_line_exact_r0p5
avg_line_mixed_remove_wf avg_mixed3d_damage_wf avg_plate_two_nh avg_plate_rate3
avg_plate_rate0 avg_plate_dnear1_r0p5 avg_plate_dnear1_r5 avg_plate_dbands_r0p5
avg_plate_dbands_r100 law_ud558_two_mazars law_ud558_ref_ehat law_ud558_ref_damage
law_ud561_mixed"
status="${work}/adversarial-case-status.tsv"
printf '# case\texit code (run 1)\tthree identical runs\tlog tail\n' > "${status}"
mkdir -p "${work}/stage/model/non_local"
failed=0
for c in ${cases}; do
  for r in 1 2 3; do
    mkdir -p "${work}/run${r}"
    AKANTU_PROBE_BUILD_INFO="${work}/build_env.adversarial_probe.json" timeout 900 \
      "${work}/build/adversarial_probe" "${work}/run${r}" "${PROBES}/non_local/inputs" \
      "${work}/tmp${r}" "${c}" >"${work}/${c}.${r}.log" 2>&1
    echo "$?" > "${work}/${c}.${r}.rc"
  done
  rc1="$(cat "${work}/${c}.1.rc")"
  same=yes
  for r in 2 3; do
    [ "$(cat "${work}/${c}.${r}.rc")" = "${rc1}" ] || same=no
    if [ -d "${work}/run1/model/non_local/${c}" ]; then
      diff -r -x manifest.json "${work}/run1/model/non_local/${c}" \
        "${work}/run${r}/model/non_local/${c}" >/dev/null 2>&1 || same=no
    fi
  done
  printf '%s\t%s\t%s\t%s\n' "${c}" "${rc1}" "${same}" \
    "$(tail -c 300 "${work}/${c}.1.log" | tr '\n\t' '  ')" >> "${status}"
  if [ "${same}" != yes ]; then
    echo "adversarial_probe: case ${c} is not deterministic over three runs" >&2
    failed=1
  elif [ -f "${work}/run1/model/non_local/${c}/manifest.json" ]; then
    cp -r "${work}/run1/model/non_local/${c}" "${work}/stage/model/non_local/"
  fi
done
cp "${status}" /status/adversarial-case-status.tsv
for c in ${cases}; do cp "${work}/${c}.1.log" "/status/${c}.log"; done
cat "${status}"
[ "${failed}" = 0 ] || { echo "non-deterministic case(s): nothing is promoted" >&2; exit 1; }
echo "[adversarial_probe] staged $(find "${work}/stage" -name manifest.json | wc -l) case(s)"
if [ "${NL_PROBE_STAGE_ONLY:-0}" = 1 ]; then
  cp -r "${work}/stage/." /fixtures/
  exit 0
fi
python3 -I "${PROBES}/python/finalize_fixtures.py" /fixtures --stage "${work}/stage" \
  --created "${AKANTU_PROBE_CREATED}"
