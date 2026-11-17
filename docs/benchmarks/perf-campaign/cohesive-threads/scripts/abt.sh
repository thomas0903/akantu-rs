#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive model on threads); not ported from Akantu.
# abt.sh <outroot> <input.dat> <mesh> <rep> <label=binary:threads:cores>...: one repetition of an
# interleaved series of the serial driver, order rotated by <rep>; AKANTU_NUM_THREADS = threads,
# the process pinned to <cores>.
set -u
mkdir -p $1; out=$(readlink -f $1); inp=$(readlink -f $2); mesh=$(readlink -f $3); rep=$4; shift 4
vs=(); for v in "$@"; do l=${v%%=*}; sp=${v#*=}; vs+=("$l=$(readlink -f ${sp%%:*}):${sp#*:}"); done; n=${#vs[@]}
cpu=$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | sed 's/^ //')
for ((k=0; k<n; k++)); do
  v=${vs[$(( (k + rep) % n ))]}; label=${v%%=*}; spec=${v#*=}
  bin=${spec%%:*}; rest=${spec#*:}; threads=${rest%%:*}; cores=${rest#*:}
  d=$out/$label/rep$rep; rm -rf $d; mkdir -p $d/diag; cd $d
  m=$(basename $(grep -m1 "mesh =" $inp | sed "s/.*= *//")); ln -s $mesh $m
  printf '{"label":"%s","rep":%s,"order":%s,"threads":%s,"cores":"%s","sha256":"%s","start":"%s","loadavg":"%s","cpu":"%s"}\n' \
    "$label" "$rep" "$k" "$threads" "$cores" "$(sha256sum < $bin | cut -c1-64)" "$(date -u +%FT%TZ)" "$(cut -d' ' -f1-3 /proc/loadavg)" "$cpu" > run.json
  AKANTU_NUM_THREADS=$threads taskset -c $cores /usr/bin/time -v $bin --aka_input_file $inp > out.txt 2> err.txt; echo $? > exit
  rm -f $m; sed -i "s#$HOME#~#g" run.json err.txt out.txt
done
