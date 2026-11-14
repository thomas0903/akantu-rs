#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Self-test of oracle/scripts/fetch-verified.sh. No network.
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${here}/../fetch-verified.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
printf 'payload\n' > "${tmp}/good"
good_sha="$(shasum -a 256 "${tmp}/good" | cut -d' ' -f1)"
export FETCH_BACKOFF=0
fails=0
check() { # <name> <condition result 0/1>
  if [ "$2" -eq 0 ]; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi
}
calls="${tmp}/calls"
# Downloader stub: fails (or serves corrupt data) until attempt $SUCCEED_AT.
stub() {
  local n
  n=$(($(cat "${calls}" 2>/dev/null || echo 0) + 1))
  echo "${n}" > "${calls}"
  if [ "${n}" -ge "${SUCCEED_AT}" ]; then cp "${tmp}/good" "$2"; return 0; fi
  if [ "${MODE}" = corrupt ]; then printf 'corrupt\n' > "$2"; return 0; fi
  return 22
}
FETCH_CMD=stub

run() { # <mode> <succeed_at> -> rc
  rm -f "${calls}" "${tmp}/out" "${tmp}/out.part"
  MODE="$1" SUCCEED_AT="$2" fetch_verified http://x "${tmp}/out" "${good_sha}" 2>/dev/null
}
run fail 5; rc=$?
check "success on the 5th (last) attempt" $([ ${rc} -eq 0 ] && sha256_matches "${tmp}/out" "${good_sha}" && [ "$(cat "${calls}")" = 5 ]; echo $?)
run corrupt 5; rc=$?
check "corrupt downloads retried, 5th verified" $([ ${rc} -eq 0 ] && sha256_matches "${tmp}/out" "${good_sha}"; echo $?)
run fail 1; rc=$?
check "first attempt success" $([ ${rc} -eq 0 ] && [ "$(cat "${calls}")" = 1 ]; echo $?)
run corrupt 6; rc=$?
check "all attempts corrupt -> failure, nothing left" $([ ${rc} -ne 0 ] && [ ! -e "${tmp}/out" ] && [ ! -e "${tmp}/out.part" ] && [ "$(cat "${calls}")" = 5 ]; echo $?)
run fail 6; rc=$?
check "all attempts fail -> failure" $([ ${rc} -ne 0 ] && [ ! -e "${tmp}/out" ]; echo $?)
rm -f "${calls}"; cp "${tmp}/good" "${tmp}/out"
MODE=fail SUCCEED_AT=1 fetch_verified http://x "${tmp}/out" "${good_sha}" 2>/dev/null; rc=$?
check "existing verified file reused without download" $([ ${rc} -eq 0 ] && [ ! -e "${calls}" ]; echo $?)
rm -f "${calls}"; printf 'stale\n' > "${tmp}/out"
MODE=fail SUCCEED_AT=2 fetch_verified http://x "${tmp}/out" "${good_sha}" 2>/dev/null; rc=$?
check "existing corrupt file replaced" $([ ${rc} -eq 0 ] && sha256_matches "${tmp}/out" "${good_sha}"; echo $?)
echo "fetch-verified self-test: ${fails} failures"
[ "${fails}" -eq 0 ]
