#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (WP-COHCHECK); not ported from Akantu.
# dig.sh <tree> <steps> <outdir>: digest run of the driver (energies, insertions, node state at the last step)
set -e
bin=~/w/cohcheck/$1/target/release/cohesive_extrinsic; out=$3
rm -rf $out; mkdir -p $out/diag; cd $out; cp ~/w/cohcheck/runs/cube-h0.12.msh .
/usr/bin/time -v $bin --aka_input_file ~/w/cohcheck/runs/dig-$2.dat > out.txt 2>&1
# no home-directory paths in recorded data
sed -i "s#$HOME#~#g" out.txt
