#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# pf.sh <label> <binary>: perf stat and perf record (flat) of the driver at one thread, h = 0.12 full schedule
I=~/w/cohthreads; d=$I/prof/$1; rm -rf $d; mkdir -p $d/diag; cd $d
ln -s $I/runs/cube-h0.12.msh .
AKANTU_NUM_THREADS=1 taskset -c 2 perf stat -e instructions,cycles -o stat.txt $I/$2 --aka_input_file $I/runs/in-5000.dat > /dev/null 2>&1
AKANTU_NUM_THREADS=1 taskset -c 2 perf record -q -F 2000 -o perf.data $I/$2 --aka_input_file $I/runs/in-5000.dat > /dev/null 2>&1
perf report -i perf.data --no-children --sort symbol --stdio 2>/dev/null | grep -v "^#" | grep . | head -25 > flat.txt
rm -f cube-h0.12.msh
