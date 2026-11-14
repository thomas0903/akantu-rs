#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (D-093 (2)): valgrind memcheck of the oracle
# run of the couplers case friction_cohesive_contact_explicit, shortened to
# its first N steps (CPL_PROBE_FRICTION_STEPS; insertion at step 2224, stick
# from 2226, slip from 3946 and through the shear phase from 5000 in the
# golden), on a disposable host with the pinned oracle
# (akantu-oracle:dev + its build volume, docs/ORACLE_BUILD.md).
#
#   oracle/probes/couplers/memcheck-friction.sh <label> <steps> <out dir>
#
# <label> names the build the install currently holds: `variant` (after
# oracle/probes/contact/build-ud001-variant.sh: the evidence for the golden)
# or `regular` (before it: the negative control, which must show UD-001's
# uninitialised reads). Valgrind is installed into a throwaway ROOT
# container of the oracle image only (the image is not changed); the probe
# is built against /build/install-strict as run-in-container.sh builds it,
# and run once under `valgrind --tool=memcheck --track-origins=yes` with a
# scratch fixtures root: the staged case is labelled "MEMCHECK EVIDENCE RUN,
# NOT A GOLDEN" and is never promoted; no value is taken from it (D-093).
# Output: <out dir>/<label>/{vg.log, run.log, summary.txt, libakantu.sha256}.
set -euo pipefail
label=${1:?label: variant or regular}
steps=${2:?steps}
out=${3:?out dir}
case "${label}" in variant | regular) ;; *) echo "label must be variant or regular" >&2; exit 2 ;; esac
[[ "${steps}" =~ ^[1-9][0-9]*$ ]] || { echo "invalid steps ${steps}" >&2; exit 2; }
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
# shellcheck source=oracle/scripts/oracle-env.sh
source "${repo}/oracle/scripts/oracle-env.sh"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
[ "$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)" = "${REF_SHA}" ] ||
  { echo "reference checkout is not at ${REF_SHA}" >&2; exit 1; }
if [ "${AKANTU_DISPOSABLE_HOST:-}" != "yes" ]; then
  echo "refusing: set AKANTU_DISPOSABLE_HOST=yes to confirm this runs on a disposable host" >&2
  exit 2
fi
vpatch="oracle/patches/variants/ud001-contact-zero-init.patch"
vsha="$(sha256sum "${repo}/${vpatch}" | cut -d' ' -f1)"
mkdir -p "${out}/${label}"
out="$(cd "${out}" && pwd)"
image_id="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")"
exec docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --init --user 0 --memory=6g --memory-swap=6g \
  -e OMPI_ALLOW_RUN_AS_ROOT=1 -e OMPI_ALLOW_RUN_AS_ROOT_CONFIRM=1 \
  -e OPENBLAS_CORETYPE="${OPENBLAS_CORETYPE:-Haswell}" -e OPENBLAS_NUM_THREADS=1 -e OMP_NUM_THREADS=1 \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" -e AKANTU_PROBE_REFERENCE_DESCRIBE=memcheck \
  -e AKANTU_PROBE_CREATED="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  -e AKANTU_PROBE_COMMAND="oracle/probes/couplers/memcheck-friction.sh ${label} ${steps}" \
  -e AKANTU_PROBE_SOURCES_COMMIT="$(git -C "${repo}" rev-parse HEAD)" \
  -e ORACLE_IMAGE_ID="${image_id}" \
  -e CPL_PROBE_CASES=friction_cohesive_contact_explicit \
  -e CPL_PROBE_VARIANT_PATCH="${vpatch}" -e CPL_PROBE_VARIANT_SHA256="${vsha}" \
  -e CPL_PROBE_VARIANT_DEFECT=UD-001 -e CPL_PROBE_FRICTION_STEPS="${steps}" \
  $([ "${label}" = regular ] && echo "-e CPL_PROBE_MEMCHECK_UNPATCHED=1") \
  -e LABEL="${label}" \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  -v "${repo}/fixtures/meshes:/frozen-meshes:ro" \
  -v "${out}:/out" \
  "${ORACLE_IMAGE}" bash -c '
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null 2>&1
apt-get install -y -qq valgrind >/dev/null 2>&1
o=/out/${LABEL}
valgrind --version > "${o}/valgrind-version.txt"
prefix=/build/install-strict
sha256sum "${prefix}/lib/libakantu.so.5.0.7" > "${o}/libakantu.sha256"
grep -c "akantu-rs UD-001 variant" /build/src-strict/src/model/contact_mechanics/resolutions/resolution_penalty.cc \
  > "${o}/variant-markers.txt" || true
work=$(mktemp -d /tmp/vg.XXXXXX)
flags_file="${prefix}/share/akantu-oracle/build-info.txt"
python3 -I /probes/python/build_env.py base strict installed "${flags_file}" "${work}/build_env.json" \
  --expect-sha "${AKANTU_PROBE_REFERENCE_SHA}" --probe-tree /probes --install-prefix "${prefix}" \
  --reference-tree /akantu-src --patched-tree /build/src-strict --patches-out "${work}/patches.txt"
[ -f "${work}/patches.txt" ] && export AKANTU_PROBE_PATCHES="$(cat "${work}/patches.txt")"
fp_flags="$(sed -nE "s/^CMAKE_CXX_FLAGS(:[A-Z]+)?=(.*)$/\2/p" "${flags_file}" | head -1)"
cgal_dir="$(sed -n "s/^CGAL_DIR:PATH=//p" "${flags_file}" | head -1)"
cmake -S /probes/couplers -B "${work}/build" -G Ninja -DCMAKE_PREFIX_PATH="${prefix}" \
  -DCMAKE_BUILD_TYPE=RelWithDebInfo ${cgal_dir:+-DCGAL_DIR="${cgal_dir}"} \
  -DAKANTU_PROBE_CONFIG=strict -DAKANTU_PROBE_CXX_FLAGS="${fp_flags}" > "${o}/cmake.log"
ninja -C "${work}/build" -j1 > "${o}/ninja.log" || { tail -40 "${o}/ninja.log"; exit 1; }
python3 -I /probes/python/build_env.py probe "${work}/build_env.json" "${work}/build_env.couplers_probe.json" \
  couplers_probe --compile-commands "${work}/build/compile_commands.json" --source-suffix couplers/couplers_probe.cc
frozen="$(ls -d /frozen-meshes/gmsh-* | LC_ALL=C sort | tail -1)"
mkdir -p "${work}/stage"
set +e
AKANTU_PROBE_BUILD_INFO="${work}/build_env.couplers_probe.json" \
  valgrind --tool=memcheck --track-origins=yes --num-callers=40 --error-limit=no \
  --log-file="${o}/vg.log" "${work}/build/couplers_probe" "${work}/stage" /akantu-src "${frozen}" \
  > "${o}/run.log" 2>&1
rc=$?
echo "probe exit ${rc}" >> "${o}/run.log"
# the summary greps may match nothing (a clean run): no `set -e` from here
{
  echo "label ${LABEL}, steps ${CPL_PROBE_FRICTION_STEPS}, probe exit ${rc}"
  cat "${o}/libakantu.sha256"
  echo "variant markers in resolution_penalty.cc: $(cat "${o}/variant-markers.txt")"
  grep -E "ERROR SUMMARY|in use at exit|definitely lost" "${o}/vg.log" | sed -E "s/==[0-9]+==//"
  python3 -I - "${work}/stage" <<PY
import json, sys, pathlib
for m in pathlib.Path(sys.argv[1]).rglob("manifest.json"):
    j = json.load(open(m))
    p = j.get("parameters", {})
    print("staged", j["case_id"], "steps", p.get("steps"), "memcheck_evidence_run", p.get("memcheck_evidence_run"))
PY
  echo "-- origins of uninitialised values (stack allocations), counted"
  grep -h -A1 "Uninitialised value was created" "${o}/vg.log" | grep -v "Uninitialised value was created\|^--" |
    sed -E "s/==[0-9]+==//; s/0x[0-9A-F]+//" | cut -c1-200 | sort | uniq -c | sort -rn | head -20
  echo "-- error kinds, counted"
  grep -hE "^==[0-9]+== (Conditional jump|Use of uninitialised|Invalid (read|write)|Syscall param|Mismatched|Source and destination)" "${o}/vg.log" |
    sed -E "s/==[0-9]+== //" | sort | uniq -c | sort -rn
} > "${o}/summary.txt" 2>&1
cat "${o}/summary.txt"
exit "${rc}"
'
