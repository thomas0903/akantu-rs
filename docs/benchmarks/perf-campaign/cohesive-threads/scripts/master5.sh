#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# master5.sh: the fix round at the head of src: builds, digests (serial at both grains, ranks x threads),
# the one-thread and thread series (5 repetitions, whole machine), then the gates.
I=~/w/cohthreads; cd $I; S=$I/series; L=$I/m5; mkdir -p $L
log() { echo "$(date -u +%T) $*" >> $L/steps.txt; }
git -C src log --oneline -1 > $L/head.txt
log builds
bjob ct-r3b -- ./build.sh src > $L/build-src.log 2>&1 &
bjob ct-r3m -- ./build-mpi.sh src > $L/build-mpi.log 2>&1 &
bjob ct-r3bm -- ./build-mpi.sh base > $L/build-mpi-base.log 2>&1 &
wait
cp target-src/release/cohesive_extrinsic bin/coh-r3; cp target-src/release/suite_solid bin/solid-r3
log "builds done $(sha256sum bin/coh-r3 | cut -c1-16)"
# serial digests, h = 0.12 full schedule and h = 0.06, 1/2/4/8 threads, finest and automatic grain
for n in 1 2 4 8; do for g in finest auto; do
  bjob ct-r3d -- env GRAIN=$g ./dig.sh r3-5000-n$n-$g bin/coh-r3 runs/dig-5000.dat 4-7 $n >> $L/dig.txt 2>&1
  bjob ct-r3d -- env GRAIN=$g ./dig.sh r3-06-n$n-$g bin/coh-r3 runs/dig06-200.dat 4-7 $n >> $L/dig.txt 2>&1
done; done
log "serial digests done"
# the series: base at one thread against the head at 1, 2, 4, 8 threads, whole machine, rotated order
rm -rf $S/r3-h012-5000 $S/r3-suite-r*
for r in 0 1 2 3 4; do
  xjob ct-r3s$r -- bash -c "./abt.sh $S/r3-h012-5000 runs/in-5000.dat runs/cube-h0.12.msh $r base=bin/coh-base:1:2 t1=bin/coh-r3:1:2 t2=bin/coh-r3:2:2-3 t4=bin/coh-r3:4:0-3 t8=bin/coh-r3:8:0-7; ./suitet.sh $S/r3-suite-r$r cohesive_extrinsic_check,cohesive_extrinsic_insertion m 1 base=$I/bin/solid-base:1:2 t1=$I/bin/solid-r3:1:2 t2=$I/bin/solid-r3:2:2-3 t4=$I/bin/solid-r3:4:0-3 t8=$I/bin/solid-r3:8:0-7"
done
rm -rf $S/r3-h012-500
for r in 0 1 2 3 4; do xjob ct-r3f$r -- ./abt.sh $S/r3-h012-500 runs/in-500.dat runs/cube-h0.12.msh $r base=bin/coh-base:1:2 t1=bin/coh-r3:1:2; done
log "series done"
# ranks x threads digests (h = 0.12 full schedule, eigen static solve), both grains
o=$I/par/dig3; rm -rf $o
xjob ct-r3md -- bash -c "for spec in base:2:1:auto src:2:1:auto src:2:2:auto src:2:2:finest src:2:4:auto src:2:4:finest base:4:1:auto src:4:1:auto src:4:2:auto src:4:2:finest base:8:1:auto src:8:1:auto; do IFS=: read t np nt g <<< \$spec; GRAIN=\$g ./mpirt.sh ct-\$t \$np \$nt runs/in-5000-eigen.dat $o/\$t-np\$np-t\$nt-\$g; done"
for d in $o/*; do echo "$(basename $d) exit=$(cat $d/exit) $(cat $d/energies.csv $d/insertions.csv | sha256sum | cut -c1-16) $(tail -1 $d/insertions.csv)"; done > $L/mpidig.txt
log "mpi digests done"
mv gates gates-36faa51f 2>/dev/null; ./gates.sh > $L/gates.log 2>&1
log "gates done"
echo done > $L/done
