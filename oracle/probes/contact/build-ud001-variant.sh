#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: build the UD-001 zero-initialisation VARIANT of the strict oracle
# (oracle/patches/variants/ud001-contact-zero-init.patch) **on a disposable
# x86_64 host that holds the oracle image and its build volume** (never on a
# shared build volume, never in the reference checkout).
#
# It patches the host's copy of /build/src-strict (the build's own patched
# source copy, not the reference checkout) and rebuilds/reinstalls the strict
# tree incrementally (only the contact translation units and the library
# relink). After it, probes run against /build/install-strict record the
# patched files in `reference.patches` of every manifest (build_env.py
# compares /akantu-src with /build/src-strict), so generate the regular
# (unpatched) contact cases FIRST, then run this script, then generate
# CONTACT_PROBE_CASES=explicit_friction.
#
# Usage (on that host, from the root of a clone):
#   oracle/probes/contact/build-ud001-variant.sh
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
# Numerical-environment pin and x86-64-v3 host check of every oracle container
# start (D-050 (5), D-073 (2)). Empty on arm64.
# shellcheck source=oracle/scripts/oracle-env.sh
source "${repo}/oracle/scripts/oracle-env.sh"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
patch_file="oracle/patches/variants/ud001-contact-zero-init.patch"
[ -f "${repo}/${patch_file}" ] || { echo "missing ${patch_file}" >&2; exit 1; }
if [ "${AKANTU_DISPOSABLE_HOST:-}" != "yes" ]; then
  echo "refusing: set AKANTU_DISPOSABLE_HOST=yes to confirm this runs on a disposable host" >&2
  exit 2
fi
exec docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --init --memory=6g --memory-swap=6g -e CCACHE_DIR=/build/ccache \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${ORACLE_VOLUME}:/build" \
  -v "${repo}/oracle:/oracle:ro" \
  "${ORACLE_IMAGE}" bash -c '
set -euo pipefail
cd /build/src-strict
if grep -q "akantu-rs UD-001 variant" src/model/contact_mechanics/resolutions/resolution_penalty.cc; then
  echo "variant already applied"
else
  # the header text before the first "--- a/" line is ignored by patch(1)
  patch -p1 --forward < /oracle/patches/variants/ud001-contact-zero-init.patch
fi
# Rebuild the library target only, with -j1 (-j2 was
# OOM-killed on contact_mechanics_model.cc in a 6 GB container) (the full `install` target also builds the
# test tree, which contains an upstream test that does not compile at this
# commit, test_material_mazars.cc), then install the rebuilt library and the
# patched header by hand.
ninja -C /build/strict -j1 akantu > /build/logs/strict-ud001-rebuild.log 2>&1 ||
  { tail -40 /build/logs/strict-ud001-rebuild.log; exit 1; }
tail -3 /build/logs/strict-ud001-rebuild.log
cp -a /build/strict/src/libakantu.so.5.0.7 /build/install-strict/lib/libakantu.so.5.0.7
cp /build/src-strict/src/model/contact_mechanics/contact_detector_inline_impl.hh \
   /build/install-strict/include/akantu/contact_detector_inline_impl.hh
sha256sum /build/install-strict/lib/libakantu.so.5.0.7
echo "UD-001 variant installed into /build/install-strict"
'
