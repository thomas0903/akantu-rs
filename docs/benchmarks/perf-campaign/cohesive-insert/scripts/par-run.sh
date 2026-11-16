#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive insertion step); not ported from Akantu.
# par-run.sh <target-name> <np> <input.dat> <outdir>: one MPI run of the driver in the MUMPS image,
# ranks bound to cores 0..np-1 (Scotch partition)
set -u
t=$1; np=$2; inp=$(readlink -f $3); out=$4
rm -rf $out; mkdir -p $out/diag; cp ~/w/insert/runs/cube-h0.12.msh $out/; cp $inp $out/in.dat
echo "{\"target\":\"$t\",\"np\":$np,\"start\":\"$(date -u +%FT%TZ)\",\"load\":\"$(cut -d" " -f1-3 /proc/loadavg)\"}" > $out/run.json
docker run --rm --cpuset-cpus 0-$((np-1)) --shm-size 2g --memory 12g -v akantu-rs-mpi-target:/target:ro -v $out:/run -w /run \
  -e OMP_NUM_THREADS=1 -e OPENBLAS_NUM_THREADS=1 akantu-rs-mumps:dev \
  bash -c "/usr/bin/time -v mpirun -np $np --bind-to core --map-by core /target/$t/release/cohesive_extrinsic --aka_input_file in.dat > out.txt 2> err.txt; echo \$? > exit"
rm -f $out/cube-h0.12.msh
