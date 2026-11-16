#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (W2-INSERT); not ported from Akantu.
# prof.sh <bin> <input.dat> <outdir>: fp call-graph profile of one serial driver run (core 3)
set -u
bin=$(readlink -f $1); inp=$(readlink -f $2); o=$3
rm -rf $o; mkdir -p $o/diag; cd $o; cp ~/w/insert/runs/cube-h0.12.msh ~/w/insert/runs/cube-h0.06.msh . 2>/dev/null
perf record -q -F 499 -g -o perf.data -- taskset -c 3 $bin --aka_input_file $inp > out.txt 2> err.txt
perf report -i perf.data --stdio --no-children --sort sym --percent-limit 0.3 2>/dev/null | grep -E "^ +[0-9]" | cut -c1-220 > flat.txt
perf report -i perf.data --stdio --children --sort sym --percent-limit 0.5 -g none 2>/dev/null | grep -E "^ +[0-9]" | cut -c1-220 > children.txt
rm -f cube-h0.*.msh
