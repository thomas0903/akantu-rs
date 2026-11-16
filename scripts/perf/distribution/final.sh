#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
# final.sh: the before / after series of the report (nothing else ran on the machine meanwhile)
cd ~/w/dist || exit 1
./ab.sh fin-m suite_setup explicit_step_dist_tet4,dist_mesh_tet4,ghost_nodes_tet4,ghost_elements_tet4,reduce_scalars,dist_facets_tet4 "1 2 4 8" 5 m base0 h9
./ab.sh fin-heat suite_models heat_explicit_tet4,phasefield_staggered_tri3 "1 2 4 8" 5 m base0 h9
./ab.sh fin-tiny suite_setup py_twin_explicit_tri3 "1" 5 m base0 h9
./ab.sh fin-l suite_setup dist_mesh_tet4 "2 4 8" 3 l base0 h9
echo FINAL_DONE
