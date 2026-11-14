#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Make /build/install-<config> usable for out-of-tree programs, smoke-test it
# (C++ find_package(Akantu) consumer + Python import) and write the
# /build/install-<config>/READY marker containing the akantu-rs commit SHA of
# the oracle scripts. Run after configure-build.sh finished successfully.
# Usage: oracle/scripts/finalize-install.sh <config>
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

config="${1:?usage: finalize-install.sh <config>}"
check_reference_sha
scripts_sha="$(git -C "${REPO_ROOT}" rev-parse HEAD)"
if [ -n "$(git -C "${REPO_ROOT}" status --porcelain -- oracle)" ]; then
  echo "WARNING: oracle/ has uncommitted changes; READY will name ${scripts_sha}" >&2
fi
image_id="$(docker image inspect "${ORACLE_IMAGE}" --format '{{.Id}}')"
docker run --rm "${ORACLE_RUN_OPTS[@]}" "${ORACLE_MOUNTS[@]}" \
  -e ORACLE_IMAGE_ID="${image_id}" "${ORACLE_IMAGE}" \
  bash /oracle/scripts/in-container/finalize-install.sh "${config}" "${scripts_sha}"
