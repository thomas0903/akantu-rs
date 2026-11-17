#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# mpirt.sh <target> <np> <threads> <input.dat> <outdir> [partition-dir]: one MPI run of the driver in the
# MUMPS image, np ranks of <threads> threads each on cores 0..np*threads-1 (a rank on its own cores).
set -u
t=$1; np=$2; nt=$3; inp=$(readlink -f $4); out=$5; pdir=${6:-}
rm -rf $out; mkdir -p $out/diag
mesh=$(grep -m1 "mesh =" $inp | sed "s/.*= *//"); cp ~/w/cohthreads/runs/$mesh $out/; cp $inp $out/in.dat
if [ -n "$pdir" ] && [ $np -gt 1 ]; then cp $pdir/partition-np$np.csv $out/partition.csv; sed -i "s#^  mesh = .*#&\n  rust_partitioner = file:partition.csv#" $out/in.dat; fi
echo "{\"target\":\"$t\",\"np\":$np,\"threads\":$nt,\"start\":\"$(date -u +%FT%TZ)\",\"load\":\"$(cut -d" " -f1-3 /proc/loadavg)\"}" > $out/run.json
docker run --rm --cpuset-cpus 0-$((np*nt-1)) --shm-size 2g --memory 24g -v akantu-rs-mpi-target:/target:ro -v $out:/run -w /run \
  -e OMP_NUM_THREADS=1 -e OPENBLAS_NUM_THREADS=1 akantu-rs-mumps:dev \
  bash -c "/usr/bin/time -v mpirun -np $np --map-by slot:PE=$nt --bind-to core -x AKANTU_NUM_THREADS=$nt -x AKANTU_EXEC_GRAIN=${GRAIN:-auto} /target/$t/release/cohesive_extrinsic --aka_input_file in.dat > out.txt 2> err.txt; echo \$? > exit"
rm -f $out/$mesh
