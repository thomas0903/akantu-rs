#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (W2-INSERT); not ported from Akantu.
# dig.sh <label> <bin> <input.dat> <core>: one serial driver run with the state digest output
# (energies of every step, insertions, the nodal state of the last step); prints the digests
set -u
I=~/w/insert; d=$I/dig/$1; rm -rf $d; mkdir -p $d/diag; cd $d
cp $I/runs/cube-h0.12.msh $I/runs/cube-h0.06.msh .
taskset -c $4 $(readlink -f $I/$2) --aka_input_file $(readlink -f $I/$3) > out.txt 2>&1; echo $? > exit
rm -f cube-h0.12.msh cube-h0.06.msh
( cat energies.csv insertions.csv; cat diag/*.csv 2>/dev/null ) | sha256sum | cut -c1-16 > digest
echo "$1 exit=$(cat exit) digest=$(cat digest) $(sha256sum energies.csv | cut -c1-12) $(tail -1 insertions.csv)"
