#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# master6.sh: after master5, the mid-size mesh again at the head of src (3 repetitions, whole machine)
I=~/w/cohthreads; cd $I; S=$I/series
while [ ! -f m5/done ]; do sleep 30; done
rm -rf $S/r3-mid
for r in 0 1 2; do xjob ct-r3mid$r -- bash -c "for spec in base:8:1 src:8:1 src:4:2 src:2:4 src:1:8; do IFS=: read t np nt <<< \$spec; ./mpirt.sh ct-\$t \$np \$nt runs/in06-500.dat $S/r3-mid/\$t-np\$np-t\$nt/rep$r ~/baseline/partitions/h0.06; done"; done
echo done > m5/mid.done
