#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): bitwise result checks
# of the build-profile variants (correctness, not timing). For every
# variant directory BIN_ROOT/<v> (explicit, cohesive): the explicit-step
# FNV-1a digest of u, v, a, f_int after N steps (tet4-200k and hex8-200k,
# small strain and finite deformation), and the cohesive driver on the frozen
# h = 0.12 mesh for STEPS steps (default the input's 5000: fragmentation
# with insertions) with the SHA-256 of its physical results (summary.json
# energies, displacement norms and sums, insertion counts; insertions.csv).
#   result_digests.sh OUT.tsv N STEPS VARIANT...   (BIN_ROOT, MESH_DIR, REPO)
set -uo pipefail
out=$1 n=$2 steps=$3; shift 3
broot="${BIN_ROOT:?}" meshdir="${MESH_DIR:?}" repo="${REPO:?}"
work="$(mktemp -d)"
for v in "$@"; do
  for c in "tet4-200k:" "tet4-200k:--fd" "hex8-200k:" "hex8-200k:--fd"; do
    m="${c%%:*}" f="${c#*:}"
    # shellcheck disable=SC2086 # empty or one flag
    d=$("$broot/$v/explicit" --mesh "$meshdir/$m.msh" --digest-steps "$n" $f | sed -n 's/.*"fnv1a":"\([0-9a-f]*\)".*/\1/p')
    printf '%s\texplicit\t%s%s\t%s\n' "$v" "$m" "${f:+ fd}" "$d" >> "$out"
  done
  w="$work/$v"; mkdir -p "$w"
  sed -e "s|^  max_steps = .*|  max_steps = $steps|" -e "s|^  mesh = .*|  mesh = $meshdir/cube-h0.12.msh|" \
    "$repo/benchmarks/cohesive-perf/inputs/material-cohesive.dat" > "$w/input.dat"
  (cd "$w" && "$broot/$v/cohesive" --aka_input_file input.dat > out.txt 2> err.txt)
  st=$?
  dg=$(python3 -I - "$w" <<'PY'
import hashlib, json, os, sys
d = sys.argv[1]
s = json.load(open(os.path.join(d, "summary.json")), parse_float=str)
h = hashlib.sha256()
keys = ("nb_cohesive_elements", "nb_insertion_steps", "first_insertion_step", "energy_dissipated",
        "energy_potential", "energy_kinetic", "displacement_l2_norm", "displacement_max_abs", "displacement_sum")
for k in keys:
    h.update(f"{k}={s.get(k)}".encode())
h.update(open(os.path.join(d, "insertions.csv"), "rb").read())
print(h.hexdigest()[:16], f"cohesive={s.get('nb_cohesive_elements')} insertion_steps={s.get('nb_insertion_steps')}")
PY
)
  printf '%s\tcohesive\t%s steps (exit %s)\t%s\n' "$v" "$steps" "$st" "$dg" >> "$out"
done
