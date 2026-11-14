#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): bitwise check between
# builds of the explicit-step timing harness: the FNV-1a digest
# of u, v, a, f_int after N steps must be identical for every LABEL.
#   digests.sh N "LABEL..." "MESH[:fd]..."   (BIN_DIR, MESH_DIR as run_ab.sh)
set -euo pipefail
n="$1"; read -r -a labels <<<"$2"; read -r -a cases <<<"$3"
bindir="${BIN_DIR:-$HOME/perf/bin}"; meshdir="${MESH_DIR:-$HOME/perf/meshes}"
status=0
for c in "${cases[@]}"; do
  IFS=: read -r mesh fd <<<"$c"
  extra=(); [ "${fd:-}" = fd ] && extra=(--fd)
  ref=""
  for l in "${labels[@]}"; do
    d=$("$bindir/$l" --mesh "$meshdir/$mesh.msh" --digest-steps "$n" "${extra[@]}" | grep '"digest"' | sed 's/.*"fnv1a":"\([0-9a-f]*\)".*/\1/')
    echo "$mesh ${fd:-ss} $l $d"
    if [ -z "$ref" ]; then ref="$d"; elif [ "$d" != "$ref" ]; then echo "MISMATCH $mesh ${fd:-ss} $l" >&2; status=1; fi
  done
done
exit $status
