#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu.
# Workflow helper: free disk space on a GitHub-hosted ubuntu runner
# before the project images and the cargo target volume are stored on it. The
# four project images are 3.7-4.4 GB each (layers shared with the MPI image)
# and the target volume of a workspace build reaches 21-27 GB (runs of
# `ci/run-all.sh --backend docker`, docs/TESTING.md), while a
# standard ubuntu-24.04 runner has about 14-20 GB free on `/`. Removes only
# toolchains the project never uses that ship with the runner image (.NET SDK,
# Android SDK, GHC, CodeQL bundles); nothing under the workspace is touched.
# A no-op outside GitHub Actions (GITHUB_ACTIONS unset) so that it is never
# run by accident on a developer machine.
set -euo pipefail
if [ "${GITHUB_ACTIONS:-}" != true ]; then
  echo "free-disk.sh: not on GitHub Actions: nothing removed"
  exit 0
fi
df -h / | tail -n 1
sudo rm -rf /usr/share/dotnet /usr/local/lib/android /opt/ghc /opt/hostedtoolcache/CodeQL
df -h / | tail -n 1
