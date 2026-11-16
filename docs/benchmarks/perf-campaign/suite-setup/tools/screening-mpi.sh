#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): suite-setup measurement script, kept as run.
# screening of the parallel cases on the development machine (size m, 1/2/4 ranks)
cd ~/w/suite-setup
M=$HOME/w/suite-setup/bin/suite_setup_mpi
export AKANTU_SUITE_COMMIT=f51cea974
C=(dist_mesh_tet4 dist_facets_tet4 ghost_nodes_tet4 ghost_elements_tet4 reduce_scalars explicit_step_dist_tet4)
n=${#C[@]}
for p in 1 2 4; do
  out=mpi/m-p$p/p$p; mkdir -p $out
  for r in 0 1 2 3 4; do
    for ((k = 0; k < n; k++)); do
      c=${C[$(( (k + r) % n ))]}
      docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -e SCOTCH_PTHREAD_NUMBER=1 --shm-size 2g --network none \
        -v $HOME/w/suite-setup:$HOME/w/suite-setup -w $HOME/w/suite-setup akantu-rs-scotch:dev \
        mpirun -n $p --bind-to core --map-by core $M --case $c --size m --reps 1 --out $out/$c.r$r.jsonl 2>> mpi/errors.log || echo "FAIL $p $c $r" >> mpi/errors.log
    done
    echo "p$p rep $r $(date -u +%T)"
  done
done
echo mpi-done
