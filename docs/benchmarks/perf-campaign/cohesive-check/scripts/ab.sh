#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (WP-COHCHECK); not ported from Akantu.
# ab.sh <outroot> <input.dat> <mesh> <rep> <label=binary>...: one repetition of an
# interleaved A/B series: every variant once, order rotated by <rep>, pinned to core 3.
set -u
out=$1; inp=$(readlink -f $2); mesh=$(readlink -f $3); rep=$4; shift 4
vs=("$@"); n=${#vs[@]}
cpu=$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | sed 's/^ //')
for ((k=0; k<n; k++)); do
  v=${vs[$(( (k + rep) % n ))]}; label=${v%%=*}; bin=${v#*=}
  d=$out/$label/rep$rep; rm -rf $d; mkdir -p $d/diag; cd $d
  ln -s $mesh $(basename $(grep -m1 "mesh =" $inp | sed "s/.*= *//"))
  sha=$(sha256sum < $bin | cut -c1-64)
  printf '{"label":"%s","rep":%s,"order":%s,"binary":"%s","sha256":"%s","start":"%s","loadavg":"%s","cpu":"%s"}\n' \
    "$label" "$rep" "$k" "$bin" "$sha" "$(date -u +%FT%TZ)" "$(cut -d' ' -f1-3 /proc/loadavg)" "$cpu" > run.json
  taskset -c 3 /usr/bin/time -v $bin --aka_input_file $inp > out.txt 2> err.txt; echo $? > exit
done
