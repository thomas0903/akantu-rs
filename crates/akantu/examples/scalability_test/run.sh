#!/usr/bin/bash -l
# SPDX-License-Identifier: LGPL-3.0-or-later
# Ported from Akantu (EPFL/LSMS): examples/c++/scalability_test/run.sh
# (Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne),
# Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides);
# Akantu is free software under the GNU Lesser General Public License,
# version 3 or later).
#
# SLURM job of one scalability run of the Rust example `scalability_test`
# (EX-018/EX-049), submitted by scalability.sh. Build the example with MPI
# and Scotch first (Linux cluster, `mpicc` and Scotch in the environment):
#   cargo build -p akantu --release --example scalability_test --features mpi,scotch
# and run from a directory holding the input files and the mesh
# (material-elastic.dat, material-cohesive.dat, cube.msh: the full mesh of
# upstream's study is generated with `gmsh -3 cube.geo -o cube.msh`).
# AKANTU_RS_SCALABILITY_BIN overrides the path of the binary (default: the
# cargo release build next to this script). The chronometer CSV is printed
# on the standard error (the job's .err file), as upstream.
#SBATCH --time 1-0:0:0

# upstream: `module purge; module -q restore akantu` (the cluster's module
# set; keep the modules your MPI build needs)
if command -v module >/dev/null 2>&1; then
  module purge
  module -q restore akantu-rs || true
fi

export OMP_NUM_THREADS=1
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bin="${AKANTU_RS_SCALABILITY_BIN:-${here}/../../../../target/release/examples/scalability_test}"
srun "${bin}" --aka_input_file ./material-elastic.dat "$@"
