#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Build a candidate oracle Docker image from oracle/docker/ and compare its
# package manifest with the recorded one (oracle/docker/packages.tsv).
#
# Reproducibility scope (D-034): the oracle results are tied
# to the existing image recorded in oracle/docker/image-id.txt. The Dockerfile
# pins the base image by digest and CGAL by SHA-256, but installs Debian
# packages from the live trixie repositories without versions, so a clean
# rebuild can resolve different package versions, i.e. a potentially
# different numerical environment. This script therefore never overwrites the
# recorded manifest or image id silently:
#   * it builds ${ORACLE_IMAGE}-candidate (default akantu-oracle:dev-candidate);
#   * it writes the candidate's manifest to oracle/docker/packages.candidate.tsv
#     (git-ignored) and diffs it against packages.tsv;
#   * only with --promote is the candidate tagged ${ORACLE_IMAGE} and
#     image-id.txt rewritten; if the manifests differ, --promote also requires
#     --accept-package-drift, and packages.tsv is then replaced (commit it and
#     treat every oracle result as needing regeneration).
# Exit status: 0 identical manifest (or promoted), 3 package drift (candidate
# kept, nothing recorded), other non-zero on errors.
#
# Architecture (D-043): the platform defaults to the Docker server's
# architecture (override: ORACLE_PLATFORM=linux/arm64|linux/amd64). Records are
# per architecture: arm64 keeps oracle/docker/{packages.tsv,image-id.txt};
# amd64 uses oracle/docker/{packages.amd64.tsv,image-id.amd64.txt}. With no
# record yet for the architecture, --promote creates it. For amd64 the
# (package, version) pairs are additionally compared with the arm64 manifest
# (informative: cross-architecture version drift, docs/ORACLE_BUILD.md).
#
# Usage: oracle/scripts/build-image.sh [--promote] [--accept-package-drift]
#                                      [extra docker build args]
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
source "${ORACLE_SCRIPTS_DIR}/fetch-verified.sh"

promote=0
accept_drift=0
build_args=()
for a in "$@"; do
  case "$a" in
    --promote) promote=1 ;;
    --accept-package-drift) accept_drift=1 ;;
    *) build_args+=("$a") ;;
  esac
done
candidate="${ORACLE_IMAGE}-candidate"
ORACLE_PLATFORM="${ORACLE_PLATFORM:-linux/$(docker version --format '{{.Server.Arch}}')}"
case "${ORACLE_PLATFORM}" in
  linux/arm64) rec_suffix="" ;;
  linux/amd64) rec_suffix=".amd64" ;;
  *) echo "ERROR: unsupported ORACLE_PLATFORM ${ORACLE_PLATFORM}" >&2; exit 2 ;;
esac

# Fetch pinned third-party tarballs into the (git-ignored) build-context cache.
cache_dir="${ORACLE_DIR}/docker/cache"
mkdir -p "${cache_dir}"
fetch_verified \
  https://github.com/CGAL/cgal/releases/download/v5.6.2/CGAL-5.6.2-library.tar.xz \
  "${cache_dir}/CGAL-5.6.2-library.tar.xz" \
  2d93d96c8c73c73c9999556d512775648d26e1e04035ceb958208d25e6e2d10f

docker build \
  --platform "${ORACLE_PLATFORM}" \
  -t "${candidate}" \
  -f "${ORACLE_DIR}/docker/Dockerfile" \
  ${build_args[@]+"${build_args[@]}"} \
  "${ORACLE_DIR}/docker"

docker volume inspect "${ORACLE_VOLUME}" >/dev/null 2>&1 \
  || docker volume create "${ORACLE_VOLUME}" >/dev/null

recorded="${ORACLE_DIR}/docker/packages${rec_suffix}.tsv"
id_file="${ORACLE_DIR}/docker/image-id${rec_suffix}.txt"
cand_tsv="${ORACLE_DIR}/docker/packages.candidate${rec_suffix}.tsv"
docker run --rm "${candidate}" cat /usr/share/akantu-oracle-packages.tsv > "${cand_tsv}"
cand_id="$(docker image inspect "${candidate}" --format '{{.Id}}')"
rec_id="$(cut -d' ' -f1 "${id_file}" 2>/dev/null || true)"
echo "platform ${ORACLE_PLATFORM}"
echo "candidate image ${candidate}: ${cand_id}"
echo "recorded image:  ${rec_id:-<none>} (oracle/docker/$(basename "${id_file}"))"

drift=0
first_record=0
if [ ! -f "${recorded}" ]; then
  first_record=1
  echo "package manifest: no record for ${ORACLE_PLATFORM} yet ($(basename "${recorded}")); --promote creates it"
elif cmp -s "${recorded}" "${cand_tsv}"; then
  echo "package manifest: IDENTICAL to oracle/docker/$(basename "${recorded}") ($(wc -l < "${recorded}" | tr -d ' ') packages)"
else
  drift=1
  echo "package manifest: DIFFERS from oracle/docker/$(basename "${recorded}"):" >&2
  diff "${recorded}" "${cand_tsv}" >&2 || true
fi
if [ -n "${rec_suffix}" ] && [ -f "${ORACLE_DIR}/docker/packages.tsv" ]; then
  # Cross-architecture comparison of (package, version) pairs with the arm64
  # record (architecture column ignored). Informative only.
  if diff <(cut -f1,2 "${ORACLE_DIR}/docker/packages.tsv") <(cut -f1,2 "${cand_tsv}") \
       > "${cand_tsv}.vs-arm64.diff"; then
    echo "package versions: IDENTICAL to the arm64 record (oracle/docker/packages.tsv)"
  else
    echo "package versions differ from the arm64 record (oracle/docker/packages.tsv):"
    cat "${cand_tsv}.vs-arm64.diff"
  fi
fi

if [ "${promote}" -eq 0 ]; then
  if [ "${drift}" -eq 1 ]; then
    echo "Candidate kept as ${candidate}; nothing recorded. It is a potentially different" >&2
    echo "numerical environment (docs/ORACLE_BUILD.md)." >&2
    exit 3
  fi
  echo "Candidate kept as ${candidate}; re-run with --promote to make it ${ORACLE_IMAGE}."
  exit 0
fi
if [ "${drift}" -eq 1 ] && [ "${accept_drift}" -eq 0 ]; then
  echo "ERROR: --promote with package drift requires --accept-package-drift" >&2
  exit 3
fi
docker tag "${candidate}" "${ORACLE_IMAGE}"
if [ "${drift}" -eq 1 ]; then
  cp "${cand_tsv}" "${recorded}"
  echo "$(basename "${recorded}") REPLACED: commit it; all oracle results need regeneration." >&2
elif [ "${first_record}" -eq 1 ]; then
  cp "${cand_tsv}" "${recorded}"
  echo "$(basename "${recorded}") CREATED: commit it." >&2
fi
docker image inspect "${ORACLE_IMAGE}" --format '{{.Id}} {{.Created}}' > "${id_file}"
echo "Promoted ${candidate} to ${ORACLE_IMAGE}: $(cat "${id_file}")"
