#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# final.sh: digests, one-thread series and gates at the head of src (= gtree)
I=~/w/cohthreads; cd $I; S=$I/series
(cd gtree && git pull -q); git -C src log --oneline -1 > final-head.txt
bjob ct-fmpi -- ./build-mpi.sh src > build-mpi-src.log 2>&1
cp target-src/release/cohesive_extrinsic bin/coh-final; cp target-src/release/suite_solid bin/solid-final
bjob ct-fdig -- bash -c "./digs.sh final bin/coh-final 1 2 4 8" > final-dig.log 2>&1
xjob ct-fmpidig -- ./mpidig.sh > /dev/null 2>&1; cp mpidig.txt final-mpidig.txt
rm -rf $S/f-h012-5000 $S/f-suite1-r* $S/f-mid
for r in 0 1 2 3 4; do tjob ct-f1-$r -- ./abt.sh $S/f-h012-5000 runs/in-5000.dat runs/cube-h0.12.msh $r base=bin/coh-base:1:2 t1=bin/coh-final:1:2; done
for r in 0 1 2 3 4; do tjob ct-fs-$r -- ./suitet.sh $S/f-suite1-r$r cohesive_extrinsic_check,cohesive_extrinsic_insertion m 1 base=$I/bin/solid-base:1:2 t1=$I/bin/solid-final:1:2; done
for r in 0 1 2; do xjob ct-fmid$r -- bash -c "for spec in base:8:1 src:8:1; do IFS=: read t np nt <<< \$spec; ./mpirt.sh ct-\$t \$np \$nt runs/in06-500.dat $S/f-mid/\$t-np\$np-t\$nt/rep$r ~/baseline/partitions/h0.06; done"; done
mv gates gates-45897328; ./gates.sh > gates.log 2>&1
echo done > final.done
