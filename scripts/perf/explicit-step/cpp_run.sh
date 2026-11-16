#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): run the C++ reference
# timing driver inside the oracle image, one process.
#   cpp_run.sh BIN_DIR CONFIG MESH_DIR MESH STEPS REPS WARMUP FD LABEL [DIGEST_STEPS]
# PIN_CPU (optional, e.g. 2): run the container on that core only
# (docker --cpuset-cpus).
set -euo pipefail
bindir="$1" config="$2" meshdir="$3"; shift 3
pin=(); [ -z "${PIN_CPU:-}" ] || pin=(--cpuset-cpus "${PIN_CPU}")
docker run --rm ${pin[@]+"${pin[@]}"} -v akantu-oracle-build:/build -v "$bindir":/bin-cpp:ro -v "$meshdir":/meshes:ro \
  -w /tmp akantu-oracle:dev bash -c '. /build/install-'"$config"'/share/akantu-oracle/env.sh
    m="$1"; shift; exec /bin-cpp/explicit_step_timing_'"$config"' /meshes/"$m".msh "$@"' _ "$@" | grep "^{"
