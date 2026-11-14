#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): generate the explicit-step
# benchmark meshes with the local gmsh into DIR (frozen for a session: the
# same .geo gives different meshes on other gmsh versions/platforms).
#   make_meshes.sh DIR [sizes...]   sizes: tet4-20k tet4-200k tet4-1m tet10-20k tet10-200k hex8-20k hex8-200k hex8-1m
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="$1"; shift
mkdir -p "$out"
sizes=("$@")
[ ${#sizes[@]} -gt 0 ] || sizes=(tet4-20k tet4-200k tet4-1m tet10-20k tet10-200k hex8-20k hex8-200k hex8-1m)
for s in "${sizes[@]}"; do
  f="$out/$s.msh"
  [ -s "$f" ] && { echo "keep $f"; continue; }
  case "$s" in
    tet4-20k)   gmsh -3 -v 2 -setnumber h 0.064 -setnumber order 1 "$here/cube_tet.geo" -o "$f" ;;
    tet4-200k)  gmsh -3 -v 2 -setnumber h 0.0297 -setnumber order 1 "$here/cube_tet.geo" -o "$f" ;;
    tet4-1m)    gmsh -3 -v 2 -setnumber h 0.0174 -setnumber order 1 "$here/cube_tet.geo" -o "$f" ;;
    tet10-20k)  gmsh -3 -v 2 -setnumber h 0.064 -setnumber order 2 "$here/cube_tet.geo" -o "$f" ;;
    tet10-200k) gmsh -3 -v 2 -setnumber h 0.0297 -setnumber order 2 "$here/cube_tet.geo" -o "$f" ;;
    hex8-20k)   gmsh -3 -v 2 -setnumber n 27 "$here/cube_hex.geo" -o "$f" ;;
    hex8-200k)  gmsh -3 -v 2 -setnumber n 58 "$here/cube_hex.geo" -o "$f" ;;
    hex8-1m)    gmsh -3 -v 2 -setnumber n 100 "$here/cube_hex.geo" -o "$f" ;;
    *) echo "unknown size $s" >&2; exit 2 ;;
  esac
done
gmsh --version > "$out/gmsh-version.txt" 2>&1
sha256sum "$out"/*.msh > "$out/SHA256SUMS"
cat "$out/SHA256SUMS"
