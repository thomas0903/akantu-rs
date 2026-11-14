#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (EX-012): oracle runs of upstream's compiled example
# examples/c++/solid_mechanics_model/boundary_conditions/python_user_defined_bc
# (the binary of the oracle build tree, unchanged; it embeds CPython through
# pybind11::embed and imports boundary_condition.py) and the Gmsh mesh of
# examples/python/solid_mechanics_model/eigen_modes (the .geo the script
# writes, meshed by the oracle image's Gmsh, x86_64).
#
# Runs on an x86_64 Linux host with the oracle (image
# akantu-oracle:dev, volume akantu-oracle-build with the strict build tree):
#   python/examples/oracle/ex012-oracle.sh <out dir>
# Three runs of the binary (D-035) in fresh directories holding upstream's
# boundary_condition.py and material.dat and the frozen Gmsh 4.13.1 mesh
# fine_mesh.msh (D-009, the mesh the port's example reads). The embedded
# interpreter imports boundary_condition from the run directory
# (PYTHONPATH=<run dir>; upstream's ctest relies on pybind11 adding the
# binary's directory, where CMake copies the file).
set -euo pipefail
out="${1:?usage: ex012-oracle.sh <out dir>}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
src="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
updir=examples/c++/solid_mechanics_model/boundary_conditions/python_user_defined_bc
mkdir -p "$out"
for r in 1 2 3; do
  d="$out/ex012/run$r"
  mkdir -p "$d"
  cp "$src/$updir/boundary_condition.py" "$src/$updir/material.dat" "$d/"
  cp "$repo/fixtures/meshes/gmsh-4.13.1/$updir/fine_mesh.msh" "$d/"
  docker run --rm --memory 4g --init -v akantu-oracle-build:/build:ro \
    -v "$d:/work" -w /work akantu-oracle:dev bash -c \
    ". /build/install-strict/share/akantu-oracle/env.sh && export PYTHONPATH=/work:\$PYTHONPATH && /build/strict/$updir/python_user_defined_bc > stdout.txt 2> stderr.txt; echo \$? > exit_code"
done
# eigen_modes: the .geo written by eigen_modes.py (mesh_h = 0.2) meshed by
# the oracle's Gmsh, three times.
for r in 1 2 3; do
  d="$out/eigen_modes_gmsh/run$r"
  mkdir -p "$d"
  docker run --rm --memory 4g --init -v akantu-oracle-build:/build:ro \
    -v "$d:/work" -w /work -v "$repo/python/examples:/ex:ro" akantu-oracle:dev bash -c \
    ". /build/install-strict/share/akantu-oracle/env.sh && python3 /ex/oracle/freeze_gmsh.py /ex/solid_mechanics_model/eigen_modes/eigen_modes.py > stdout.txt 2> stderr.txt; echo \$? > exit_code; gmsh --version > gmsh_version.txt 2>&1"
done
echo "image $(docker images akantu-oracle:dev --format '{{.ID}}')" > "$out/provenance.txt"
echo "host $(uname -m) $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2)" >> "$out/provenance.txt"
