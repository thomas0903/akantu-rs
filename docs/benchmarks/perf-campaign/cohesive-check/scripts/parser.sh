#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive insertion check); not ported from Akantu.
# parser.sh <np> <rep0> <rep1> <input>: interleaved before (parbase) / after (src) MPI runs, order rotated per repetition
np=$1; inp=$(readlink -f $4)
for ((r=$2; r<=$3; r++)); do
  if (( r % 2 == 0 )); then vs="parbase src"; else vs="src parbase"; fi
  for t in $vs; do ~/w/cohcheck/par-run.sh $t $np $inp ~/w/cohcheck/series/par-np$np/$t/rep$r; done
done
