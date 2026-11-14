#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-003).
# exportall: the test tier built with the test-only feature akantu-core/export-all (every SharedArray treated as exported)
#
# Same steps, analyser, baseline and floor mechanism as the `test` tier
# (ci/tiers/test.sh), with `--features akantu-core/export-all` on the build
# and the run (docs/ARCHITECTURE.md, docs/TESTING.md); its
# own floor key `exportall` in ci/floors.txt.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
export CI_TEST_TIER=exportall CI_TEST_FEATURES=--features,akantu-core/export-all CI_TEST_FLOOR_KEY=exportall
exec bash "$here/tiers/test.sh" "$@"
