#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (golden-data integrity; rows CI-020, GOLD).
# fixtures: verify every fixtures/**/SHA256SUMS manifest (sha256sum -c / shasum -a 256 -c), found with find
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin fixtures

if ci_have sha256sum; then
  sum="sha256sum -c --quiet"
elif ci_have shasum; then
  sum="shasum -a 256 -c --quiet"
else
  sum=""
fi
# The manifests are found on the file system, never through git: an exported
# tree (no .git, or no git on the host) must verify its goldens exactly like
# a checkout (F7g0). `na` only for a tree without any
# fixtures/ directory; a fixtures/ directory without a manifest is a skip
# (incomplete): the goldens would go unverified.
if [ ! -d "$CI_REPO/fixtures" ]; then
  manifests=""
  ci_na sha256 "no fixtures/ directory on this tree"
else
  manifests="$(cd "$CI_REPO" && find fixtures -name SHA256SUMS -type f | LC_ALL=C sort)"
  [ -n "$manifests" ] || ci_skip sha256 "fixtures/ exists but holds no SHA256SUMS manifest: the goldens are not verified"
fi
if [ -z "$manifests" ]; then
  :
elif [ -z "$sum" ]; then
  for m in $manifests; do ci_skip "sha256:$m" "neither sha256sum nor shasum on the host"; done
else
  for m in $manifests; do
    d="$(dirname "$m")"
    # word splitting of $sum is intended (command + options)
    # shellcheck disable=SC2086,SC2016
    ci_step "sha256:$m" --host -- sh -c 'cd "$1/$2" && '"$sum"' SHA256SUMS && echo "$(grep -c . SHA256SUMS) entries OK"' sh "$CI_REPO" "$d"
  done
fi

ci_tier_end
