#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# digs.sh <tag> <bin> <threads...>: digests on both meshes at each thread count (cores 4-7, finest grain)
t=$1; b=$2; shift 2
for n in "$@"; do GRAIN=finest ./dig.sh $t-5000-n$n $b runs/dig-5000.dat 4-7 $n; GRAIN=finest ./dig.sh $t-06-n$n $b runs/dig06-200.dat 4-7 $n; done
