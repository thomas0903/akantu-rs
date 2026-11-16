#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the explicit-step state
# digests (FNV-1a of u, v, a, f_int after n steps) of the port and of the C++
# strict and default builds for the ten configurations of
# docs/benchmarks/explicit-step/final/digests.txt, one line per case, and the
# comparison with the recorded digests (same n per case).
#   explicit_digests.sh PORT_BIN CPP_BIN_DIR MESH_DIR > digests.txt
# shellcheck disable=SC2094 # the recorded digests file is only read (the loop and awk), never written
set -euo pipefail
X="$(cd "$(dirname "$0")/../../../.." && pwd)/scripts/perf/explicit-step"
port="$1" cppdir="$2" meshdir="$3"
rec="$(cd "$(dirname "$0")/../../.." && pwd)/benchmarks/explicit-step/final/digests.txt"
echo "# FNV-1a digests of u, v, a, f_int after n explicit steps; $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | xargs), $(uname -m), $(date -u +%FT%TZ)"
echo "# port=$(sha256sum "$port" | cut -c1-16) cpp-strict=$(sha256sum "$cppdir/explicit_step_timing_strict" | cut -c1-16) cpp-default=$(sha256sum "$cppdir/explicit_step_timing_default" | cut -c1-16)"
echo "mesh mode n port cpp-strict cpp-default port=cpp-strict port=recorded cpp-strict=recorded cpp-default=recorded"
status=0
while read -r mesh mode n _; do
  case "$mesh" in \#*|mesh) continue ;; esac
  n=${n#n=}
  fd=0 extra=(); [ "$mode" = fd ] && { fd=1; extra=(--fd); }
  p=$("$port" --mesh "$meshdir/$mesh.msh" --digest-steps "$n" "${extra[@]}" | sed -n 's/.*"fnv1a":"\([0-9a-f]*\)".*/\1/p')
  s=$("$X/cpp_run.sh" "$cppdir" strict "$meshdir" "$mesh" 1 1 0 "$fd" check "$n" | sed -n 's/.*"fnv1a":"\([0-9a-f]*\)".*/\1/p')
  d=$("$X/cpp_run.sh" "$cppdir" default "$meshdir" "$mesh" 1 1 0 "$fd" check "$n" | sed -n 's/.*"fnv1a":"\([0-9a-f]*\)".*/\1/p')
  r=$(awk -v m="$mesh" -v o="$mode" '$1 == m && $2 == o {print $4, $7, $8}' "$rec")
  read -r rp rs rd <<<"$r"
  eq() { [ -n "$1" ] && [ "$1" = "$2" ] && echo True || echo False; }
  echo "$mesh $mode n=$n $p $s $d $(eq "$p" "$s") $(eq "$p" "$rp") $(eq "$s" "$rs") $(eq "$d" "$rd")"
  [ "$p" = "$s" ] || status=1
done < "$rec"
exit $status
