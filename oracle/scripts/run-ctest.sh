#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Run the ctest suite (or a subset) of one oracle configuration.
# Usage: oracle/scripts/run-ctest.sh <config> [--detach] [--collect] [ctest args...]
#   <config>    strict | default
#   --detach    run in a detached container (name akantu-oracle-ctest-<config>);
#               follow with `docker logs -f akantu-oracle-ctest-<config>`
#   --collect   do not run anything: copy the latest summary into the repo
#               (oracle/results/ctest-<config>.json and .junit.xml;
#               amd64: oracle/results/x86_64/)
#   ctest args  passed through, e.g. -R test_solid -j 12 --timeout 600
# Defaults: --output-on-failure --no-compress-output --timeout 3600 (serial
# unless -j is given). Results: /build/logs/<config>/ctest/latest/ (see
# in-container/ctest.sh). Examples: docs/ORACLE_BUILD.md.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

config="${1:?usage: run-ctest.sh <config> [--detach] [--collect] [ctest args...]}"
shift
detach=()
collect=0
while [ $# -gt 0 ]; do
  case "$1" in
    --detach) detach=(-d); shift ;;
    --collect) collect=1; shift ;;
    *) break ;;
  esac
done

if [ "${collect}" -eq 1 ]; then
  mkdir -p "${ORACLE_RESULTS_DIR}"
  docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} "${ORACLE_MOUNTS[@]}" "${ORACLE_IMAGE}" \
    cat "/build/logs/${config}/ctest/latest/summary.json" \
    > "${ORACLE_RESULTS_DIR}/ctest-${config}.json"
  docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} "${ORACLE_MOUNTS[@]}" "${ORACLE_IMAGE}" \
    cat "/build/logs/${config}/ctest/latest/junit.xml" \
    > "${ORACLE_RESULTS_DIR}/ctest-${config}.junit.xml" || true
  python3 - "${ORACLE_RESULTS_DIR}/ctest-${config}.json" \
    "${ORACLE_DIR}/results/known-failures.json" <<'EOF'
import collections, json, re, sys
d = json.load(open(sys.argv[1]))
kf = json.load(open(sys.argv[2]))["known_failures"]
print(d["config"], d["totals"], "wall", d["wall_seconds"], "s")
cats = collections.Counter()
for t in d["tests"]:
    if t["status"] == "failed":
        k = next((k for k in kf if re.fullmatch(k["pattern"], t["name"])), None)
        cats[k["category"] if k else "UNEXPECTED"] += 1
        if not k:
            print(f'  UNEXPECTED failure: {t["name"]} ranks={t["ranks"]} {t.get("exit_code")}')
print("failures by category:", dict(cats))
EOF
  exit 0
fi

check_reference_sha
scripts_sha="$(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null || echo unknown)"
image_id="$(docker image inspect "${ORACLE_IMAGE}" --format '{{.Id}}')"
name="akantu-oracle-ctest-${config}"
docker rm -f "${name}" >/dev/null 2>&1 || true
tty=()
[ ${#detach[@]} -eq 0 ] && [ -t 0 ] && [ -t 1 ] && tty=(-it)
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} ${detach[@]+"${detach[@]}"} ${tty[@]+"${tty[@]}"} \
  "${ORACLE_RUN_OPTS[@]}" --name "${name}" "${ORACLE_MOUNTS[@]}" \
  -e ORACLE_SCRIPTS_SHA="${scripts_sha}" -e ORACLE_IMAGE_ID="${image_id}" \
  "${ORACLE_IMAGE}" bash /oracle/scripts/in-container/ctest.sh "${config}" "$@"
if [ ${#detach[@]} -gt 0 ]; then
  echo "started ${name}; follow with: docker logs -f ${name}"
fi
