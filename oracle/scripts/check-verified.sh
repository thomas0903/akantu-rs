#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Run in-container/check-verified.py for one configuration:
# compare test outputs with the `.verified` references that upstream's driver
# never checks (UD-018). Default mode re-runs every test afresh in a scratch
# copy of its work directory with the build volume mounted READ-ONLY.
# Outputs (stdout/stderr per test) go to oracle/isolated-runs-out/verified-
# <config>-<stamp>/ (git-ignored) and are archived in the volume at
# /build/verified-runs/<config>-<stamp>/; the report (with SHA-256 of every
# output, executable and reference) is written to
# oracle/results/verified-check-<config>.json (amd64: oracle/results/x86_64/).
# Usage: oracle/scripts/check-verified.sh <config> [--mode ctest-run] [...]
# Exit status: that of check-verified.py (0 only if every reference matches).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

config="${1:?usage: check-verified.sh <config> [check-verified.py args...]}"
shift
check_reference_sha
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
out_host="${ORACLE_ISOLATED_OUT:-${ORACLE_DIR}/isolated-runs-out}/verified-${config}-${stamp}"
mkdir -p "${out_host}"
scripts_sha="$(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null || echo unknown)"
image_id="$(docker image inspect "${ORACLE_IMAGE}" --format '{{.Id}}')"
mkdir -p "${ORACLE_RESULTS_DIR}"
report="${ORACLE_RESULTS_DIR}/verified-check-${config}.json"

rc=0
docker run --rm --memory 4g "${ORACLE_RUN_OPTS[@]}" \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${ORACLE_DIR}:/oracle:ro" \
  -v "${out_host}:/out" \
  -e ORACLE_SCRIPTS_SHA="${scripts_sha}" -e ORACLE_IMAGE_ID="${image_id}" \
  "${ORACLE_IMAGE}" \
  python3 -I /oracle/scripts/in-container/check-verified.py "${config}" --out /out "$@" \
  > "${report}" || rc=$?
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory 4g -v "${ORACLE_VOLUME}:/build" -v "${out_host}:/src:ro" \
  "${ORACLE_IMAGE}" bash -c \
  "mkdir -p /build/verified-runs && cp -a /src /build/verified-runs/${config}-${stamp}"
echo "report: ${report} (exit ${rc}); outputs: ${out_host}"
exit "${rc}"
