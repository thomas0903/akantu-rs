#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Configure (and by default build + install) one oracle configuration in a
# DETACHED container. Poll with:  docker logs -f akantu-oracle-build-<config>
#                          or:    oracle/scripts/shell.sh tail -f /build/logs/<config>/build.log
# Usage: oracle/scripts/configure-build.sh <config> [-j N] [--configure-only|--build-only|--install-only] [--foreground]
#   <config>: strict | default  (oracle/configs/<config>.cmake)
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

config="${1:?usage: configure-build.sh <config> [-j N] [--configure-only|--build-only|--install-only] [--foreground]}"
shift
jobs=2
mode=all
detach=(-d)
while [ $# -gt 0 ]; do
  case "$1" in
    -j) jobs="$2"; shift 2 ;;
    -j*) jobs="${1#-j}"; shift ;;
    --configure-only) mode=configure; shift ;;
    --build-only) mode=build; shift ;;
    --install-only) mode=install; shift ;;
    --foreground) detach=(); shift ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done

check_reference_sha
name="akantu-oracle-build-${config}"
docker rm -f "${name}" >/dev/null 2>&1 || true
docker run ${detach[@]+"${detach[@]}"} "${ORACLE_RUN_OPTS[@]}" --name "${name}" \
  "${ORACLE_MOUNTS[@]}" "${ORACLE_IMAGE}" \
  bash /oracle/scripts/in-container/build.sh "${config}" "${jobs}" "${mode}"
if [ -n "${detach[*]+x}" ] && [ ${#detach[@]} -gt 0 ]; then
  echo "started ${name}; follow with: docker logs -f ${name}"
fi
