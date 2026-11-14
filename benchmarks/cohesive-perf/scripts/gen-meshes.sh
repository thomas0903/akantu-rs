#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness: generate scaled cube meshes with the
# Gmsh of the oracle image (D-009) and record their statistics.
#
# Usage: scripts/gen-meshes.sh [--freeze-max-mib N] <h>...
#   e.g. scripts/gen-meshes.sh 0.12 0.06 0.04 0.03
#
# For each h: `gmsh -3 -setnumber h <h> -format msh41 inputs/cube.geo` twice
# (the second run checks that the mesh is byte-identical), the mesh is kept in
# the scratch volume at /perf/meshes/cube-h<h>.msh, its statistics are written
# to meshes/cube-h<h>.json and, if it is at most N MiB (default 5) and not yet
# frozen, the mesh is copied to fixtures/perf_cohesive/meshes/ as a frozen
# input (D-009; add its line to fixtures/SHA256SUMS); larger meshes are
# regenerated on demand (same command, checksum in the JSON). Recorded
# statistics and frozen meshes are never replaced: a mesh with other bytes
# (Gmsh on another architecture) gets meshes/cube-h<h>.<machine>.json, and a
# frozen mesh is copied into the volume so that runs use the frozen input.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

freeze_max_mib=5
if [ "${1:-}" = "--freeze-max-mib" ]; then [ $# -ge 2 ] || exit 2; freeze_max_mib="$2"; shift 2; fi
[ $# -ge 1 ] || { echo "usage: $0 [--freeze-max-mib N] <h>..." >&2; exit 2; }
perf_check_uint freeze-max-mib "${freeze_max_mib}" || exit 2
for h in "$@"; do perf_check_h "${h}" || exit 2; done

frozen_dir="${REPO_ROOT}/fixtures/perf_cohesive/meshes"
# same | new | differs-recorded | differs-frozen (bash 3.2: no here-document
# inside a command substitution, hence the variable)
read -r -d '' verdict_py <<'PY' || true
import hashlib, json, os, sys
old_p, new_p, frozen = sys.argv[1:4]
new = json.load(open(new_p))
if os.path.exists(frozen):
    with open(frozen, "rb") as f:
        if hashlib.sha256(f.read()).hexdigest() != new["sha256"]:
            print("differs-frozen"); sys.exit()
if os.path.exists(old_p) and json.load(open(old_p))["sha256"] != new["sha256"]:
    print("differs-recorded"); sys.exit()
print("same" if os.path.exists(old_p) else "new")
PY
mkdir -p "${PERF_DIR}/meshes" "${frozen_dir}"
for h in "$@"; do
  name="cube-h${h}"
  perf_docker bash -c '
    set -euo pipefail
    h="$1"; name="$2"
    mkdir -p /perf/meshes
    cd /perf/meshes
    gen() {
      /usr/bin/time -f "%e s wall, %M KiB max RSS" -o "$1.time" \
        gmsh -3 -setnumber h "$h" -format msh41 -o "$1" /harness/inputs/cube.geo > "$1.log" 2>&1
    }
    gen "${name}.msh"
    gen "${name}.check.msh"
    mv "${name}.msh.time" "${name}.time"
    if ! cmp -s "${name}.msh" "${name}.check.msh"; then
      echo "ERROR: gmsh output for h=$h is not reproducible" >&2; exit 1
    fi
    rm -f "${name}.check.msh" "${name}.check.msh.log" "${name}.check.msh.time"
    python3 -I /harness/scripts/mesh_stats.py "${name}.msh" --label "$name" --h "$h" > "${name}.json.tmp"
    python3 -I - "${name}.json.tmp" "${name}.time" "$h" > "${name}.json" <<EOF
import json, subprocess, sys
d = json.load(open(sys.argv[1]))
r = subprocess.run(["gmsh", "--version"], capture_output=True, text=True)
d["gmsh_version"] = (r.stdout + r.stderr).strip()
d["command"] = "gmsh -3 -setnumber h %s -format msh41 -o %s inputs/cube.geo" % (sys.argv[3], d["file"])
d["generation"] = open(sys.argv[2]).read().strip()
d["reproducible"] = "byte-identical over 2 generations"
print(json.dumps(d, indent=1))
EOF
    rm -f "${name}.json.tmp"
  ' _ "$h" "$name"
  stats="${PERF_DIR}/meshes/${name}.json"
  docker run --rm -v "${PERF_VOLUME}:/perf:ro" "${PERF_IMAGE}" cat "/perf/meshes/${name}.json" \
    > "${stats}.new"
  # Frozen inputs are never replaced (D-009): a mesh whose bytes differ from
  # the recorded one (e.g. Gmsh on another architecture) keeps its new
  # statistics next to the recorded ones and is not frozen.
  verdict="$(python3 -I -c "${verdict_py}" "${stats}" "${stats}.new" "${frozen_dir}/${name}.msh")"
  bytes=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["bytes"])' "${stats}.new")
  case "${verdict}" in
    same|new)
      mv "${stats}.new" "${stats}"
      if [ "${bytes}" -le $((freeze_max_mib * 1024 * 1024)) ] && [ ! -e "${frozen_dir}/${name}.msh" ]; then
        docker run --rm -v "${PERF_VOLUME}:/perf:ro" "${PERF_IMAGE}" cat "/perf/meshes/${name}.msh" \
          > "${frozen_dir}/${name}.msh"
        echo "${name}: ${bytes} bytes, frozen in fixtures/perf_cohesive/meshes/ (add it to SHA256SUMS)"
      else
        echo "${name}: ${bytes} bytes (${verdict}; frozen copy: $([ -e "${frozen_dir}/${name}.msh" ] && echo kept || echo none))"
      fi ;;
    *)
      machine="$(docker run --rm "${PERF_IMAGE}" uname -m)"
      mv "${stats}.new" "${PERF_DIR}/meshes/${name}.${machine}.json"
      echo "WARNING: ${name} generated here (${machine}) differs from the recorded/frozen mesh" \
           "(${verdict}); statistics in meshes/${name}.${machine}.json, nothing replaced." >&2
      if [ "${verdict}" = differs-frozen ]; then
        # runs must use the frozen bytes: put them into the volume
        perf_volume_write "/perf/meshes/${name}.msh" < "${frozen_dir}/${name}.msh"
        echo "${name}: the frozen mesh was copied into the volume (runs use the frozen input)" >&2
      fi ;;
  esac
done
