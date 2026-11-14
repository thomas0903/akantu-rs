#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Repeat selected ctest cases of one oracle configuration in isolated scratch
# work directories (in-container/isolated-runs.py).
# The build volume is mounted READ-ONLY; artefacts go to a host directory
# (default oracle/isolated-runs-out/<run id>, git-ignored) and, unless
# --no-archive, are then copied into the volume at
# /build/isolated-runs/<run id>/ for retention. The summary is copied to
# oracle/results/isolated-<run id>.json, run id = <config>-<stamp>-akaseed<N>
# (or -akaseed-unset: Akantu then seeds from time(nullptr), as under ctest).
# Usage: oracle/scripts/run-isolated.sh <config> <names-file> [isolated-runs.py args...]
#   e.g. oracle/scripts/run-isolated.sh strict oracle/isolated/ob04-strict.txt --repeats 3
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

config="${1:?usage: run-isolated.sh <config> <names-file> [args...]}"
names="${2:?names file (inside oracle/)}"
shift 2
archive=1
args=()
seed_label="akaseed-unset"
prev=""
for a in "$@"; do
  if [ "$a" = "--no-archive" ]; then archive=0; else args+=("$a"); fi
  [ "${prev}" = "--aka-seed" ] && seed_label="akaseed${a}"
  prev="$a"
done
names_abs="$(cd "$(dirname "${names}")" && pwd)/$(basename "${names}")"
case "${names_abs}" in
  "${ORACLE_DIR}"/*) names_in="/oracle/${names_abs#"${ORACLE_DIR}"/}" ;;
  *) echo "ERROR: names file must be inside ${ORACLE_DIR}" >&2; exit 2 ;;
esac

check_reference_sha
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
run_id="${config}-${stamp}-${seed_label}"
out_host="${ORACLE_ISOLATED_OUT:-${ORACLE_DIR}/isolated-runs-out}/${run_id}"
mkdir -p "${out_host}"
scripts_sha="$(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null || echo unknown)"
dirty="$(git -C "${REPO_ROOT}" status --porcelain -- oracle 2>/dev/null | head -c1)"
image_id="$(docker image inspect "${ORACLE_IMAGE}" --format '{{.Id}}')"
prov="{\"image_id\": \"${image_id}\", \"scripts_sha\": \"${scripts_sha}\", \"oracle_dir_dirty\": $([ -n "${dirty}" ] && echo true || echo false), \"reference_sha\": \"${AKANTU_REF_SHA}\", \"stamp\": \"${stamp}\"}"

rc=0
docker run --rm --memory 4g "${ORACLE_RUN_OPTS[@]}" \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${ORACLE_DIR}:/oracle:ro" \
  -v "${out_host}:/out" \
  "${ORACLE_IMAGE}" \
  python3 -I /oracle/scripts/in-container/isolated-runs.py --config "${config}" \
    --names-file "${names_in}" --out /out --provenance "${prov}" \
    ${args[@]+"${args[@]}"} > /dev/null || rc=$?

mkdir -p "${ORACLE_RESULTS_DIR}"
cp "${out_host}/summary.json" "${ORACLE_RESULTS_DIR}/isolated-${run_id}.json"
if [ "${archive}" -eq 1 ]; then
  docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory 4g -v "${ORACLE_VOLUME}:/build" -v "${out_host}:/src:ro" \
    "${ORACLE_IMAGE}" bash -c \
    "mkdir -p /build/isolated-runs && cp -a /src /build/isolated-runs/${run_id}"
fi
echo "summary: ${ORACLE_RESULTS_DIR#"${REPO_ROOT}/"}/isolated-${run_id}.json (exit ${rc}); artefacts: ${out_host}"
exit "${rc}"
