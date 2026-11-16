#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): interleaved, rotated
# timing series of the build-profile study. Every repetition runs every case
# with every variant, adjacent in a rotated order (repetition r starts with
# variant r mod N), one process per run, pinned to one core.
#   series.sh OUT.jsonl REPS "VARIANT..." ["CASE..."]
# VARIANT = NAME[@ALLOC]: the binaries of BIN_ROOT/NAME (build_variant.sh),
# with ALLOC (mimalloc | jemalloc) preloaded from $ALLOC_DIR/lib<ALLOC>.so.
# CASE (default: all) = es-tet4-ss es-tet4-fd es-hex8-ss es-hex8-fd coh facet:
# the explicit-step harness on tet4-200k / hex8-200k (small strain, finite
# deformation; phases step, fint, stress), the cohesive driver on the frozen
# h = 0.12 mesh (500 steps, static preload with the SparseLU translation),
# facet_scaling. Environment: BIN_ROOT, MESH_DIR, ALLOC_DIR, CORE (default 2),
# RUN_DIR (scratch for the driver runs), REPO (inputs), FIRST_REP (default 1:
# the repetitions are FIRST_REP … FIRST_REP + REPS - 1, so that a long series
# runs as several short tjobs with the rotation continued).
set -u
broot="${BIN_ROOT:-$HOME/w/build/bin}" meshdir="${MESH_DIR:-$HOME/w/build/meshes}"
allocdir="${ALLOC_DIR:-$HOME/w/build/alloc}" core="${CORE:-2}"
rundir="${RUN_DIR:-$HOME/w/build/runs}" repo="${REPO:-$HOME/w/build/src}"
out=$1 reps=$2; read -r -a variants <<< "$3"
read -r -a cases <<< "${4:-es-tet4-ss es-tet4-fd es-hex8-ss es-hex8-fd coh facet}"
n=${#variants[@]}
mkdir -p "$rundir"
input="$rundir/input.dat"
sed -e 's|^  max_steps = .*|  max_steps = 500|' -e "s|^  mesh = .*|  mesh = $meshdir/cube-h0.12.msh|" \
  "$repo/benchmarks/cohesive-perf/inputs/material-cohesive.dat" > "$input"
run_one() { # variant case rep
  local v=$1 c=$2 r=$3 name="${1%@*}" alloc="" pre=() tag="$1|$2|r$3"
  [ "$name" = "$v" ] || alloc="${v#*@}"
  [ -z "$alloc" ] || pre=(env "LD_PRELOAD=$allocdir/lib$alloc.so")
  local b="$broot/$name"
  case "$c" in
    es-*)
      IFS=- read -r _ mesh mode <<< "$c"
      local steps=6 extra=()
      [ "$mesh" = hex8 ] && steps=3
      [ "$mode" = fd ] && extra=(--fd)
      "${pre[@]}" taskset -c "$core" "$b/explicit" --mesh "$meshdir/$mesh-200k.msh" --steps "$steps" \
        --reps 1 --warmup 3 --phases step,fint,stress --label "$tag" "${extra[@]}" | grep '"kind":"time"' ;;
    coh)
      local d="$rundir/coh"
      rm -rf "$d"; mkdir -p "$d"; cp "$input" "$d/input.dat"
      local t0 t1
      t0=$(date +%s.%N)
      (cd "$d" && OMP_NUM_THREADS=1 "${pre[@]}" taskset -c "$core" "$b/cohesive" --aka_input_file input.dat > out.txt 2> err.txt)
      local st=$?
      t1=$(date +%s.%N)
      python3 -I - "$d" "$tag" "$st" "$t0" "$t1" <<'PY'
import csv, hashlib, json, os, sys
d, tag, st, t0, t1 = sys.argv[1:6]
o = {"kind": "coh", "label": tag, "exit": int(st), "wall_s": float(t1) - float(t0)}
try:
    with open(os.path.join(d, "chrono.csv")) as f:
        for row in csv.reader(f):
            if len(row) >= 2:
                try:
                    o["t_" + row[0].strip()] = float(row[1])
                except ValueError:
                    pass
except OSError:
    pass
h = hashlib.sha256()
for name in sorted(os.listdir(d)):
    if name.endswith(".csv") and name != "chrono.csv":
        h.update(name.encode()); h.update(open(os.path.join(d, name), "rb").read())
o["outputs_sha256"] = h.hexdigest()[:16]
print(json.dumps(o))
PY
      ;;
    facet)
      "${pre[@]}" taskset -c "$core" "$b/facet" | awk -v tag="$tag" '$1 == 32 {
        gsub(/\[[^]]*\]/, ""); printf "{\"kind\":\"facet\",\"label\":\"%s\",\"csr\":%s,\"fill_e2s\":%s,\"init_mesh_facets\":%s,\"erase\":%s,\"purify\":%s}\n", tag, $4, $5, $6, $7, $8 }' ;;
  esac
}
r0="${FIRST_REP:-1}"
for r in $(seq "$r0" $((r0 + reps - 1))); do
  order=(); for ((i = 0; i < n; i++)); do order+=("${variants[$(( (i + r - 1) % n ))]}"); done
  echo "{\"kind\":\"round\",\"round\":$r,\"order\":\"${order[*]}\",\"load\":\"$(cut -d' ' -f1-3 /proc/loadavg)\",\"cpu\":\"$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | xargs)\",\"utc\":\"$(date -u +%FT%TZ)\"}" >> "$out"
  for c in "${cases[@]}"; do
    for v in "${order[@]}"; do run_one "$v" "$c" "$r" >> "$out"; done
  done
done
echo "{\"kind\":\"end\",\"utc\":\"$(date -u +%FT%TZ)\"}" >> "$out"
