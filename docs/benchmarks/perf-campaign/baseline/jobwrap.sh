#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): CAMPAIGN_WRAPPER of
# scaling_series.sh on the shared measurement machine. Runs the campaign
# under `xjob` (whole machine) when its name says more than 4 ranks
# (`-np<N>`, N > 4), else under `tjob` (cores 0-3).
#   jobwrap.sh LABEL COMMAND...
set -euo pipefail
label="$1"; shift
np=1
for a in "$@"; do
  if [[ "$a" =~ -np([0-9]+)(-|$) ]]; then np="${BASH_REMATCH[1]}"; fi
done
if [ "$np" -gt 4 ]; then exec xjob "$label-np$np" -- "$@"; else exec tjob "$label-np$np" -- "$@"; fi
