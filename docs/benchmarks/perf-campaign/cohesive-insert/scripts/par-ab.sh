#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (W2-INSERT); not ported from Akantu.
# par-ab.sh <outroot> <input.dat> <np> <reps> <label=target>...: interleaved MPI A/B (MUMPS image,
# Scotch partition), order rotated per repetition, ranks bound to cores 0..np-1 (par-run.sh).
set -u
out=$1; inp=$2; np=$3; reps=$4; shift 4
vs=("$@"); n=${#vs[@]}
for ((r=0; r<reps; r++)); do
  for ((k=0; k<n; k++)); do
    v=${vs[$(( (k + r) % n ))]}; label=${v%%=*}; t=${v#*=}
    d=$out/np$np/$label/rep$r
    ~/w/insert/par-run.sh $t $np $inp $d
    sed -i "s#$HOME#~#g" $d/run.json $d/err.txt $d/out.txt 2>/dev/null
  done
done
