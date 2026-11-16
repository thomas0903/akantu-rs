#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive insertion check); not ported from Akantu.
# ser.sh <outroot> <input> <mesh> <rep0> <rep1>: repetitions rep0..rep1 of the before/after series (ab.sh)
for ((r=$4; r<=$5; r++)); do ~/w/cohcheck/ab.sh $1 $2 $3 $r before=$HOME/w/cohcheck/bin/coh-before after=$HOME/w/cohcheck/bin/coh-after2; done
