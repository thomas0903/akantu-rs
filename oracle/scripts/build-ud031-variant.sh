#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: build the UD-031 VARIANT of the strict oracle **on a
# disposable host that holds the oracle image and its build volume** (never on
# a shared build volume, never in the reference checkout). Convention: D-060 /
# D-063 and docs/ORACLE_BUILD.md.
#
# The variant carries two patches, applied in this order (each one defect,
# each documented hunk by hunk in its header):
#   1. oracle/patches/variants/ud031-anisotropic-damage-zero-init.patch
#      (UD-031: zero-initialised A_diag in tensorPlusOp);
#   2. oracle/patches/variants/ud814-eig-noconvergence-zero-init.patch
#      (UD-814: MatrixBase::eig returns zero eigenpairs after NoConvergence
#      instead of the solver's uninitialised members).
# VARIANT_PATCHES="ud031" builds the first one alone (evidence runs only:
# the goldens need both).
#
# Steps: record the regular library (build tree and install: sha256,
# RUNPATH); patch the host's copy of /build/src-strict (the build's own
# patched source copy, not the reference checkout); rebuild the library
# target of the strict tree (patch 2 touches src/common/aka_types.hh, which
# every translation unit includes: a full library rebuild, about 25-40 min at
# -j3 on an 8-core host); install the rebuilt library with the RUNPATH of
# the regular install (CMake's own file(RPATH_CHANGE), what `cmake --install`
# does; a plain copy kept the build-tree RUNPATH, so
# libiohelper could resolve to the build-tree copy); copy the patched headers
# into the install; write /build/install-strict/VARIANT with the patches,
# their sha256 and the sha256 of the INSTALLED variant library (the
# probe guard material_cases.py checks the installed library against it, not
# the source marker). Probes run against that install record the patched
# files in `reference.patches` (build_env.py compares /akantu-src with
# /build/src-strict), and material_cases.py refuses to write regular goldens
# from it. Generate the regular cases FIRST, then run this script, then the
# variant cases (material:material/anisotropic_damage_mazars/variant_ud031/*).
#
# Usage (on that host, from the root of a clone):
#   AKANTU_DISPOSABLE_HOST=yes oracle/scripts/build-ud031-variant.sh [-jN] (N <= 3)
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
MEMORY="${VARIANT_BUILD_MEMORY:-12g}"
VARIANT_PATCHES="${VARIANT_PATCHES:-ud031 ud814}"
jobs=1
case "${1:-}" in
  -j1|-j2|-j3) jobs="${1#-j}" ;;
  "") ;;
  *) echo "usage: $0 [-j1|-j2|-j3] (AGENTS.md section 8: at most -j3)" >&2; exit 2 ;;
esac
patches=()
for p in ${VARIANT_PATCHES}; do
  case "${p}" in
    ud031) patches+=("ud031-anisotropic-damage-zero-init.patch") ;;
    ud814) patches+=("ud814-eig-noconvergence-zero-init.patch") ;;
    *) echo "unknown variant patch ${p} (ud031, ud814)" >&2; exit 2 ;;
  esac
done
[ "${patches[0]}" = "ud031-anisotropic-damage-zero-init.patch" ] ||
  { echo "the UD-031 patch comes first" >&2; exit 2; }
for p in "${patches[@]}"; do
  [ -f "${repo}/oracle/patches/variants/${p}" ] || { echo "missing oracle/patches/variants/${p}" >&2; exit 1; }
done
if [ "${AKANTU_DISPOSABLE_HOST:-}" != "yes" ]; then
  echo "refusing: set AKANTU_DISPOSABLE_HOST=yes to confirm this runs on a disposable host" >&2
  exit 2
fi
for p in "${patches[@]}"; do sha256sum "${repo}/oracle/patches/variants/${p}"; done
exec docker run --rm --init --memory="${MEMORY}" --memory-swap="${MEMORY}" -e CCACHE_DIR=/build/ccache \
  --user "$(id -u):$(id -g)" -e HOME=/tmp -e JOBS="${jobs}" -e PATCHES="${patches[*]}" \
  -v "${ORACLE_VOLUME}:/build" \
  -v "${repo}/oracle:/oracle:ro" \
  "${ORACLE_IMAGE}" bash -c '
set -euo pipefail
command -v readelf >/dev/null || { echo "readelf missing in the image" >&2; exit 1; }
runpath() { readelf -d "$1" | sed -n "s/.*(RUNPATH).*\[\(.*\)\]$/\1/p"; }
cd /build/src-strict
lib=$(cd /build/strict/src && ls libakantu.so.*.*.* | head -1)
blib="/build/strict/src/${lib}"
ilib="/build/install-strict/lib/${lib}"
regular=/build/logs/variant-regular-library.txt
if [ -f /build/install-strict/VARIANT ]; then
  echo "a variant is installed already (patches are added incrementally):"
  cat /build/install-strict/VARIANT
elif [ ! -f "${regular}" ]; then
  # the regular library, before anything is patched (both layouts)
  {
    echo "regular_build_tree_library ${lib} sha256=$(sha256sum "${blib}" | cut -d" " -f1) runpath=$(runpath "${blib}")"
    echo "regular_install_library ${lib} sha256=$(sha256sum "${ilib}" | cut -d" " -f1) runpath=$(runpath "${ilib}")"
  } > "${regular}"
fi
cat "${regular}"
install_runpath=$(runpath "${ilib}")
[ -n "${install_runpath}" ] || { echo "the installed library has no RUNPATH" >&2; exit 1; }
for p in ${PATCHES}; do
  marker=$(sed -n "s/^+.*\(akantu-rs UD-[0-9]* variant\).*$/\1/p" "/oracle/patches/variants/${p}" | head -1)
  file=$(sed -n "s#^+++ b/\([^[:space:]]*\).*#\1#p" "/oracle/patches/variants/${p}" | head -1)
  [ -n "${marker}" ] && [ -n "${file}" ] || { echo "cannot read marker/file of ${p}" >&2; exit 1; }
  if grep -q "${marker}" "${file}"; then
    echo "${p}: already applied"
  else
    # the header text before the first "--- a/" line is ignored by patch(1)
    patch -p1 --forward < "/oracle/patches/variants/${p}"
  fi
  grep -n "${marker}" "${file}"
done
# Library target only (the install target also builds the test tree, which
# holds an upstream test that does not compile at this commit,
# test_material_mazars.cc); then install the rebuilt library and the patched
# headers by hand, with the install RUNPATH.
start=$(date +%s)
ninja -C /build/strict -j"${JOBS}" akantu > /build/logs/strict-variant-rebuild.log 2>&1 ||
  { tail -40 /build/logs/strict-variant-rebuild.log; exit 1; }
echo "rebuild: $(( $(date +%s) - start )) s"
tail -3 /build/logs/strict-variant-rebuild.log
build_runpath=$(runpath "${blib}")
cp -a "${blib}" /tmp/variant-lib
printf "%s\n" "file(RPATH_CHANGE FILE \"/tmp/variant-lib\" OLD_RPATH \"${build_runpath}\" NEW_RPATH \"${install_runpath}\")" > /tmp/rpath.cmake
cmake -P /tmp/rpath.cmake
[ "$(runpath /tmp/variant-lib)" = "${install_runpath}" ] ||
  { echo "RUNPATH not rewritten: $(runpath /tmp/variant-lib)" >&2; exit 1; }
cp -a /tmp/variant-lib "${ilib}"
for p in ${PATCHES}; do
  file=$(sed -n "s#^+++ b/\([^[:space:]]*\).*#\1#p" "/oracle/patches/variants/${p}" | head -1)
  inc=/build/install-strict/include/akantu/$(basename "${file}")
  if [ -f "${inc}" ]; then cp "${file}" "${inc}"; fi
done
{
  echo "akantu-rs oracle variant (docs/ORACLE_BUILD.md section 12.1)"
  for p in ${PATCHES}; do
    echo "patch oracle/patches/variants/${p} sha256=$(sha256sum "/oracle/patches/variants/${p}" | cut -d" " -f1)"
  done
  echo "library ${lib} sha256=$(sha256sum "${ilib}" | cut -d" " -f1) runpath=$(runpath "${ilib}")"
  echo "build_tree_library ${lib} sha256=$(sha256sum "${blib}" | cut -d" " -f1) runpath=${build_runpath}"
  cat "${regular}"
  echo "built $(date -u +%Y-%m-%dT%H:%M:%SZ) jobs=${JOBS}"
} > /build/install-strict/VARIANT
cat /build/install-strict/VARIANT
echo "variant installed into /build/install-strict"
'
