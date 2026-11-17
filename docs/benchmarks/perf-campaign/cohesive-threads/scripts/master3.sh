#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# master3.sh: one-thread comparisons with the final binaries, then the C++ strict points
I=~/w/cohthreads; cd $I; S=$I/series
B=$I/bin/coh-base; A=$I/bin/coh-src
rm -rf $S/h012-5000-t1 $S/suite1
for r in 0 1 2 3 4; do tjob ct-s1-$r -- ./abt.sh $S/h012-5000-t1 runs/in-5000.dat runs/cube-h0.12.msh $r base=$B:1:2 t1=$A:1:2; done
for r in 0 1 2 3 4; do tjob ct-su1-$r -- ./suitet.sh $S/suite1-r$r cohesive_extrinsic_check,cohesive_extrinsic_insertion m 1 base=$I/bin/solid-base:1:2 t1=$I/bin/solid-src:1:2 t2=$I/bin/solid-src:2:2-3 t4=$I/bin/solid-src:4:0-3; done
# C++ strict, one rank, h = 0.12, 500 steps (no insertion); the Rust driver at one thread
mkdir -p $I/bench; cp $B $I/bench/base; cp $A $I/bench/src
cd $I/src
for r in 1 2 3; do tjob ct-cpp1-$r -- env PIN_CPU=2 crates/akantu-bench/scripts/timing_ab.sh --out $S/cpp1 --meshes ~/baseline/meshes/cohesive --h 0.12 --steps 500 --reps 1 --rep-start $r --materials cohesive cpp=cpp:strict base=rust:$I/bench/base src=rust:$I/bench/src; done
# C++ strict at 8 ranks, h = 0.06, 500 steps (MUMPS, the C++ partition replayed), the head at 8 x 1
xjob ct-cpp8 -- env OUT=$S/c1 RUST_BIN=/target/ct-src/release/cohesive_extrinsic PIN_CPUS_FROM=0 C1PREP_LOCK=$I/c1.lock \
  bash crates/akantu-bench/scripts/c1prep_campaign.sh --partitions ~/baseline/partitions/h0.06 ct-np8 0.06 500 8 3 "cpp rust"
echo done > $S/master3.done
