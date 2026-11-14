# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (oracle tooling). Sourced by build-image.sh; tested by
# oracle/scripts/tests/test-fetch-verified.sh. macOS bash 3.2 compatible.
#
# fetch_verified <url> <file> <sha256>
#   Succeeds only when <file> exists with the expected SHA-256. An existing
#   verified file is reused; otherwise up to FETCH_ATTEMPTS (5) downloads are
#   tried, each one verified immediately after it completes
#   (a valid download on the last attempt is a success). A download with the
#   wrong checksum is discarded and retried. Nothing unverified is ever left at
#   <file>.
#   FETCH_CMD  (default fetch_with_curl) downloader: <url> <dest>
#   FETCH_BACKOFF (default 10) seconds * attempt number between attempts

sha256_matches() { # <file> <sha256>
  [ -f "$1" ] && echo "$2  $1" | shasum -a 256 -c - >/dev/null 2>&1
}

fetch_with_curl() { # <url> <dest>
  curl -sSfL --retry 3 -o "$2" "$1"
}

fetch_verified() { # <url> <file> <sha256>
  local url="$1" file="$2" sha="$3" attempts="${FETCH_ATTEMPTS:-5}" i=1
  if sha256_matches "${file}" "${sha}"; then
    return 0
  fi
  rm -f "${file}"
  while [ "${i}" -le "${attempts}" ]; do
    rm -f "${file}.part"
    if "${FETCH_CMD:-fetch_with_curl}" "${url}" "${file}.part"; then
      if sha256_matches "${file}.part" "${sha}"; then
        mv "${file}.part" "${file}"
        return 0
      fi
      echo "WARNING: ${url}: SHA-256 mismatch on attempt ${i}" >&2
    else
      echo "WARNING: ${url}: download failed on attempt ${i}" >&2
    fi
    rm -f "${file}.part"
    if [ "${i}" -lt "${attempts}" ]; then
      sleep $((i * ${FETCH_BACKOFF:-10}))
    fi
    i=$((i + 1))
  done
  echo "ERROR: could not fetch ${url} with sha256 ${sha}" >&2
  return 1
}
