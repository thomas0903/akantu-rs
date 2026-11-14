#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: black-box measurement of the number of nodes of every Gmsh
# element type code that Gmsh 4.13.1 (in akantu-oracle:dev) writes, used to
# audit `gmsh_nodes_per_element` (crates/akantu-io/src/mesh_io/tables.rs).
# Not a port: Gmsh is only *run* (D-030: no Gmsh
# source is read); geometries of every element shape are meshed at orders
# 1-5, complete and incomplete (serendipity), written as ASCII MSH 2.2, and
# the node count of each element line (`id type ntags tags... nodes...`) is
# recorded per type code.
#
# Usage: oracle/probes/mesh_io/gmsh_element_nodes.sh [OUT]
# (default OUT: oracle/probes/mesh_io/gmsh_element_nodes.txt)
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out="${1:-${here}/gmsh_element_nodes.txt}"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
image_id="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")"
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} -i --init --memory=4g --memory-swap=4g --network=none \
  --user "$(id -u):$(id -g)" -e HOME=/tmp -e IMAGE_ID="${image_id}" \
  "${ORACLE_IMAGE}" bash -s >"${out}.tmp" <<'IN_CONTAINER'
set -euo pipefail
w="$(mktemp -d /tmp/gmsh-nodes.XXXXXX)"
cd "${w}"
square='Point(1) = {0, 0, 0, 0.5}; Point(2) = {1, 0, 0, 0.5};
Point(3) = {1, 1, 0, 0.5}; Point(4) = {0, 1, 0, 0.5};
Line(1) = {1, 2}; Line(2) = {2, 3}; Line(3) = {3, 4}; Line(4) = {4, 1};
Curve Loop(1) = {1, 2, 3, 4}; Plane Surface(1) = {1};'
structured='Transfinite Curve{1, 2, 3, 4} = 3; Transfinite Surface{1};'
cat >line.geo <<EOF
Point(1) = {0, 0, 0, 0.5}; Point(2) = {1, 0, 0, 0.5}; Line(1) = {1, 2};
EOF
printf '%s\n' "${square}" >triangle.geo
printf '%s\n%s\nRecombine Surface{1};\n' "${square}" "${structured}" >quadrangle.geo
printf '%s\nExtrude {0, 0, 1} { Surface{1}; }\n' "${square}" >tetrahedron.geo
printf '%s\n%s\nRecombine Surface{1};\nExtrude {0, 0, 1} { Surface{1}; Layers{2}; Recombine; }\n' \
  "${square}" "${structured}" >hexahedron.geo
printf '%s\n%s\nExtrude {0, 0, 1} { Surface{1}; Layers{2}; Recombine; }\n' \
  "${square}" "${structured}" >prism.geo
# Hexahedra extruded from a quadrangle mesh, then an unstructured volume on
# their top quadrangles: Gmsh fills the transition with pyramids.
printf '%s\n%s\nRecombine Surface{1};\nout[] = Extrude {0, 0, 1} { Surface{1}; Layers{2}; Recombine; };\nExtrude {0, 0, 1} { Surface{out[0]}; }\n' \
  "${square}" "${structured}" >pyramid.geo
echo "# Gmsh $(gmsh --version 2>&1 | head -1), image ${IMAGE_ID}"
echo "# columns: msh_type_code nodes_per_element first_source(shape:order:incomplete)"
: >failed.txt
for shape in line triangle quadrangle tetrahedron hexahedron prism pyramid; do
  dim=3
  case "${shape}" in line) dim=1 ;; triangle | quadrangle) dim=2 ;; esac
  for order in 1 2 3 4 5; do
    for inc in 0 1; do
      msh="${shape}_${order}_${inc}.msh"
      gmsh "${shape}.geo" -"${dim}" -order "${order}" -format msh22 \
        -setnumber Mesh.SecondOrderIncomplete "${inc}" -setnumber Mesh.SaveAll 1 \
        -o "${msh}" >"${msh}.log" 2>&1 || { echo "${shape}:${order}:${inc}" >>failed.txt; continue; }
      awk -v tag="${shape}:${order}:${inc}" '
        /^\$Elements/ { inel = 1; getline; next }
        /^\$EndElements/ { inel = 0 }
        inel { print $2, NF - 3 - $3, tag }' "${msh}"
    done
  done
done | sort -n -k1,1 -k2,2 -u | awk '
  { key = $1 " " $2; if (!(key in seen)) { seen[key] = 1; first[key] = $3; order[++n] = key } }
  END { for (i = 1; i <= n; ++i) print order[i], first[order[i]] }'
# Gmsh aborts on some high-order prism/pyramid meshes: listed, not hidden.
while read -r f; do echo "# gmsh failed (no data): ${f}"; done <failed.txt
IN_CONTAINER
mv "${out}.tmp" "${out}"
echo "wrote ${out}"
