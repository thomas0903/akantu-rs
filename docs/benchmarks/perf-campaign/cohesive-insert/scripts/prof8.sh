#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (W2-INSERT); not ported from Akantu.
# prof8.sh <target-name> <np> <label>: whole-machine call-graph profile (frame-pointer build) of one MPI run
t=$1; np=$2; o=~/w/insert/prof/$3; rm -rf $o; mkdir -p $o
sudo perf record -q -a -C 0-$((np-1)) -F 499 -g -o $o/perf.data -- sudo -u ubuntu ~/w/insert/par-run.sh $t $np ~/w/insert/runs/in-5000.dat $o/run > /dev/null 2>&1
sudo chown ubuntu $o/perf.data
docker run --rm -v akantu-rs-mpi-target:/target:ro akantu-rs-mpi:dev cat /target/$t/release/cohesive_extrinsic > $o/bin
mkdir -p $o/symfs/target/$t/release && cp $o/bin $o/symfs/target/$t/release/cohesive_extrinsic
perf report -i $o/perf.data --symfs=$o/symfs --comm cohesive_extrin --stdio --children --sort sym -g none --percent-limit 0.3 2>/dev/null | grep -E "^ +[0-9]" | cut -c1-200 > $o/children.txt
perf report -i $o/perf.data --symfs=$o/symfs --comm cohesive_extrin --stdio --no-children --sort dso,sym --percent-limit 0.3 2>/dev/null | grep -E "^ +[0-9]" | cut -c1-200 > $o/flat.txt
rm -rf $o/symfs $o/bin
