#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (W2-INSERT); not ported from Akantu.
# final.sh: the before / after series of the report, one after the other, nothing else running.
set -u
I=~/w/insert; S=$I/series; cd $I
B=$I/bin/f-base; A=$I/bin/f-src
for r in 0 1 2 3 4; do ./ab.sh $S/h012-500 runs/in-500.dat runs/cube-h0.12.msh $r before=$B after=$A; done
for r in 0 1 2 3 4; do ./ab.sh $S/h012-5000 runs/in-5000.dat runs/cube-h0.12.msh $r before=$B after=$A; done
for r in 0 1 2 3 4; do ./ab.sh $S/h006-ns-200 runs/in06-ns-200.dat runs/cube-h0.06.msh $r before=$B after=$A; done
./suite.sh $S/suite-setup facets_tet4,facets_hex8,groups_tet4,cohesive_insert_tet4 m 5 before=$I/bin/fs-base after=$I/bin/fs-src
./suite.sh $S/suite-solid cohesive_extrinsic_check,cohesive_extrinsic_insertion m 5 before=$I/bin/fo-base after=$I/bin/fo-src
for np in 2 4 8; do ./par-ab.sh $S/par runs/in-5000.dat $np 5 before=insert-fbase after=insert-fsrc; done
echo done > $S/done
