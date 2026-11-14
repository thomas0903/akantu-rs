#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the example that embeds
# CPython, python_user_defined_bc (EX-012), on both sides of the ParaView
# visual check (scripts/paraview/README.md). Its port needs the feature
# `python` and NumPy in the embedded interpreter, so it is not built by
# run-all.sh's `examples` step and run-port.sh skips its `python:` row.
#   AKANTU_SRC_HOST=<reference checkout> scripts/paraview/run-ex012.sh <work-dir>
# Writes <work-dir>/oracle/python_user_defined_bc and
# <work-dir>/port/python_user_defined_bc (every file the runs wrote,
# stdout.txt, stderr.txt, exit_code, command.txt).
# Upstream: the unchanged binary of the oracle's strict build tree, one run,
# as python/examples/oracle/ex012-oracle.sh (fresh directory with upstream's
# boundary_condition.py and material.dat and the frozen fine_mesh.msh;
# PYTHONPATH = the run directory). Port: built with `--features python` in
# the project image (CPython 3.13 with Python.h) and run in a throw-away root
# container of that image with Debian's python3-numpy installed (the image
# has no NumPy; the embedded interpreter imports it for the row views).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../.." && pwd)"
work="${1:?usage: run-ex012.sh <work-dir>}"
: "${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the reference checkout}"
mkdir -p "${work}"; work="$(cd "${work}" && pwd)"
updir=examples/c++/solid_mechanics_model/boundary_conditions/python_user_defined_bc
image="${AKANTU_RS_MPI_IMAGE:-akantu-rs-mpi:dev}"

d="${work}/oracle/python_user_defined_bc"
rm -rf "${d}"; mkdir -p "${d}"
cp "${AKANTU_SRC_HOST}/${updir}/boundary_condition.py" "${AKANTU_SRC_HOST}/${updir}/material.dat" "${d}/"
cp "${repo}/fixtures/meshes/gmsh-4.13.1/${updir}/fine_mesh.msh" "${d}/"
echo "/build/strict/${updir}/python_user_defined_bc (PYTHONPATH=<run dir>)" > "${d}/command.txt"
docker run --rm --init --memory 4g --memory-swap 4g --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 -e PYTHONDONTWRITEBYTECODE=1 \
  -v akantu-oracle-build:/build:ro -v "${d}:/work" -w /work akantu-oracle:dev bash -c \
  ". /build/install-strict/share/akantu-oracle/env.sh && export PYTHONPATH=/work:\${PYTHONPATH:-} && /build/strict/${updir}/python_user_defined_bc > stdout.txt 2> stderr.txt; echo \$? > exit_code"
echo "python_user_defined_bc (upstream): exit $(cat "${d}/exit_code"), $(find "${d}" -name '*.vtu' -o -name '*.pvtu' | wc -l) ParaView files"

d="${work}/port/python_user_defined_bc"
rm -rf "${d}"; mkdir -p "${d}"
echo "/target/python-example/release/examples/python_user_defined_bc --output-dir <dir> (cargo build --release -p akantu --features python)" > "${d}/command.txt"
docker run --rm --init --user root --memory 6g --memory-swap 6g --cpus 4 \
  -e HOME=/tmp -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" -e PYTHONDONTWRITEBYTECODE=1 \
  -v "${repo}:/src:ro" -v akantu-rs-mpi-target:/target -v akantu-rs-mpi-cargo:/usr/local/cargo/registry \
  -v "${d}:/out" -e CARGO_TARGET_DIR=/target/python-example -e CARGO_INCREMENTAL=0 \
  -w /src "${image}" bash -c \
  "apt-get -qq update && apt-get -qq -y --no-install-recommends install python3-numpy >/dev/null && PYO3_PYTHON=python3 cargo build --release -p akantu --features python --example python_user_defined_bc --locked > /out/build.log 2>&1 && cd /out && /target/python-example/release/examples/python_user_defined_bc --output-dir /out > /out/stdout.txt 2> /out/stderr.txt; echo \$? > /out/exit_code; chown -R \${HOST_UID}:\${HOST_GID} /out"
echo "python_user_defined_bc (port): exit $(cat "${d}/exit_code" 2>/dev/null || echo '?'), $(find "${d}" -name '*.vtu' -o -name '*.pvtu' | wc -l) ParaView files"
