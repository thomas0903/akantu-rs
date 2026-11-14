#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Build and run an oracle probe (oracle/probes/<name>/run.sh) against an
# installed configuration, with the build volume mounted READ-ONLY.
# Usage: oracle/scripts/run-probe.sh <probe> [config]
#   -> oracle/results/probe-<probe>-<config>.json (amd64: oracle/results/x86_64/)
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
probe="${1:?usage: run-probe.sh <probe> [config]}"
config="${2:-strict}"
[ -x "${ORACLE_DIR}/probes/${probe}/run.sh" ] || { echo "no probe ${probe}" >&2; exit 2; }
mkdir -p "${ORACLE_RESULTS_DIR}"
docker run --rm --memory 4g "${ORACLE_RUN_OPTS[@]}" \
  -v "${ORACLE_VOLUME}:/build:ro" -v "${ORACLE_DIR}:/oracle:ro" \
  -e ORACLE_IMAGE_ID="$(docker image inspect "${ORACLE_IMAGE}" --format "{{.Id}}")" \
  "${ORACLE_IMAGE}" bash "/oracle/probes/${probe}/run.sh" "${config}" \
  > "${ORACLE_RESULTS_DIR}/probe-${probe}-${config}.json"
python3 -m json.tool "${ORACLE_RESULTS_DIR}/probe-${probe}-${config}.json"
