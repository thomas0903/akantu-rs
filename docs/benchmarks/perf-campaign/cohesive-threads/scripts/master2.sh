#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# master2.sh: the measurement series after the digests (suites, 8 threads, ranks x threads, C++ strict)
I=~/w/cohthreads; cd $I; S=$I/series
B=$I/bin/coh-base; A=$I/bin/coh-src
# 1. suite_solid cohesive cases, 1 / 2 / 4 threads (one tjob per repetition)
for r in 0 1 2 3 4; do tjob ct-suite$r -- ./suitet.sh $S/suite-solid-r$r cohesive_extrinsic_check,cohesive_extrinsic_insertion m 1 base=$I/bin/solid-base:1:2 t1=$I/bin/solid-src:1:2 t2=$I/bin/solid-src:2:2-3 t4=$I/bin/solid-src:4:0-3; done
# 2. 8 threads (whole machine): driver full schedule and suite, against the base at one thread
for r in 0 1 2 3 4; do xjob ct-x8-$r -- bash -c "./abt.sh $S/h012-5000-t8 runs/in-5000.dat runs/cube-h0.12.msh $r base=$B:1:2 t8=$A:8:0-7; ./suitet.sh $S/suite-solid8-r$r cohesive_extrinsic_check,cohesive_extrinsic_insertion m 1 base=$I/bin/solid-base:1:2 t8=$I/bin/solid-src:8:0-7"; done
# 3. mid-size mesh (h = 0.06, 500 steps, C++ partitions): 8x1, 4x2, 2x4 of the head, 8x1 of the base
for r in 0 1 2; do xjob ct-mid$r -- bash -c "for spec in base:8:1 src:8:1 src:4:2 src:2:4 src:1:8 base:1:1; do IFS=: read t np nt <<< \$spec; ./mpirt.sh ct-\$t \$np \$nt runs/in06-500.dat $S/mid/\$t-np\$np-t\$nt/rep$r ~/baseline/partitions/h0.06; done"; done
echo done > $S/master2.done
