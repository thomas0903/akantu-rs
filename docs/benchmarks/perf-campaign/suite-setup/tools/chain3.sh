#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): suite-setup measurement script, kept as run.
# parallel baseline: one tjob (Scotch m/l, parallel cases m at 1/2/4 ranks, dist_mesh l at 2/4), one xjob (8 ranks)
cd $HOME/w/suite-setup
M=$HOME/w/suite-setup/bin/suite_setup_mpi
PAR=dist_mesh_tet4,dist_facets_tet4,ghost_nodes_tet4,ghost_elements_tet4,reduce_scalars,explicit_step_dist_tet4
export AKANTU_SUITE_COMMIT=6b12c7d8c
rm -f results/*.perjob results/*.commit
cat > inner-t.sh <<EOT
set -e
cd $HOME/w/suite-setup
./grouped.sh results/scotch-m mpi1 m 5 partition_scotch_tet4,partition_rcb_tet4 "" "$HOME/w/suite-setup/mpi.sh 1" $M
./grouped.sh results/scotch-l mpi1 l 3 partition_scotch_tet4,partition_rcb_tet4 "" "$HOME/w/suite-setup/mpi.sh 1" $M
for p in 1 2 4; do ./grouped.sh results/mpi-m-p\$p p\$p m 5 $PAR "" "$HOME/w/suite-setup/mpi.sh \$p" $M; done
for p in 2 4; do ./grouped.sh results/dist-l-p\$p p\$p l 3 dist_mesh_tet4 "" "$HOME/w/suite-setup/mpi.sh \$p" $M; done
EOT
cat > inner-x.sh <<EOT
set -e
cd $HOME/w/suite-setup
./grouped.sh results/mpi-m-p8 p8 m 5 $PAR "" "$HOME/w/suite-setup/mpi.sh 8" $M
./grouped.sh results/dist-l-p8 p8 l 3 dist_mesh_tet4 "" "$HOME/w/suite-setup/mpi.sh 8" $M
EOT
chmod +x inner-t.sh inner-x.sh
tjob suite-setup-mpi -- ./inner-t.sh > logs/inner-t.log 2>&1
xjob suite-setup-mpi8 -- ./inner-x.sh > logs/inner-x.log 2>&1
echo chain3-done > logs/chain3.done
