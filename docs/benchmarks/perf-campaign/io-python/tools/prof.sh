#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): measurement script of the
# I/O and Python optimisation (docs/benchmarks/perf-campaign/io-python), kept as run.
# prof.sh <binary> <outdir> <secs> cases... : record + report each case on the first job core
set -uo pipefail
bin="$1"; dir="$2"; secs="$3"; shift 3
core="${AKANTU_JOB_CPUS%%[,-]*}"
P=~/w/io/akantu-rs/scripts/perf/suite/profile.sh
for c in "$@"; do
  bash $P record "$bin" "$c" "$dir" "$secs" m "$core" && bash $P report "$c" "$dir"
done
