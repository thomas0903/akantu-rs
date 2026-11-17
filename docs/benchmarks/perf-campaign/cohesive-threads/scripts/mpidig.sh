#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# mpidig.sh: ranks x threads digests (h = 0.12 full schedule, eigen static solve) of the head, against
# the same rank count at one thread and the base
o=~/w/cohthreads/par/dig
for spec in base:2:1 src:2:1 src:2:2 src:2:4 base:4:1 src:4:1 src:4:2 base:8:1 src:8:1; do
  IFS=: read t np nt <<< "$spec"
  ./mpirt.sh ct-$t $np $nt runs/in-5000-eigen.dat $o/$t-np$np-t$nt
done
for d in $o/*; do echo "$(basename $d) exit=$(cat $d/exit) $(cat $d/energies.csv $d/insertions.csv | sha256sum | cut -c1-16) $(tail -1 $d/insertions.csv)"; done > ~/w/cohthreads/mpidig.txt
