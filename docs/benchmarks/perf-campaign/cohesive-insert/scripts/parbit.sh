#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (W2-INSERT); not ported from Akantu.
# parbit.sh <outroot> <tag> <np>...: base vs src at each rank count, eigen static solve (repeatable)
o=$1; tag=$2; shift 2
for np in "$@"; do for t in base src; do ./par-run.sh insert-$t $np runs/in-5000-eigen.dat $o/np$np/$t; done; done
for np in "$@"; do for t in base src; do d=$o/np$np/$t; echo "np$np $t exit=$(cat $d/exit) $(cat $d/energies.csv $d/insertions.csv | sha256sum | cut -c1-16) $(tail -1 $d/insertions.csv)"; done; done > $o/digests.txt
