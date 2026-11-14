#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code): tests of
# the shell validators in in-container/validate.sh.
# Runs with macOS bash 3.2 and Linux bash 5; exit status 0 = all passed.
set -u
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "${here}/../in-container/validate.sh"
fails=0
ok()  { if "$@" 2>/dev/null; then :; else echo "FAIL (rejected): $*"; fails=$((fails+1)); fi; }
bad() { if "$@" 2>/dev/null; then echo "FAIL (accepted): $*"; fails=$((fails+1)); fi; }

for c in baseline full-schedule review-v49 x86_64.base_2 a; do ok perf_check_campaign "$c"; done
for c in ../../../v49-outside-results /tmp/v49-outside-results "a b" "x'y" 'a;b' '$(id)' \
         a/b .. .hidden a..b "" "$(printf 'x%.0s' $(seq 1 65))" 'a|b' 'a\b'; do
  bad perf_check_campaign "$c"
done
for v in 1 8 500 5000 999999999; do ok perf_check_posint n "$v"; done
for v in 0 -1 01 1.5 "" " 1" 1e3 1000000000 '1;2'; do bad perf_check_posint n "$v"; done
ok perf_check_uint seed 0; ok perf_check_uint seed 42; bad perf_check_uint seed -3; bad perf_check_uint seed x
ok perf_check_posint_list ranks "1 2 4 8"; bad perf_check_posint_list ranks ""; bad perf_check_posint_list ranks "1 0"
bad perf_check_posint_list ranks "1 \$(id)"
for h in 0.12 0.06 0.04 0.03 0.02 1 1.5 12; do ok perf_check_h "$h"; done
for h in 0 0.0 0.120 .12 -0.1 1e-2 0.12/.. "0.1 2" 00.1 ""; do bad perf_check_h "$h"; done
ok perf_check_config default; ok perf_check_config strict; bad perf_check_config Default; bad perf_check_config ""
ok perf_check_enum_list materials "elastic cohesive" elastic cohesive
ok perf_check_enum_list materials cohesive elastic cohesive
bad perf_check_enum_list materials "elastic x" elastic cohesive
bad perf_check_enum_list materials "" elastic cohesive
ok perf_check_fnte joss; ok perf_check_fnte upstream; bad perf_check_fnte none
for m in 8g 14g 512m 100000000; do ok perf_check_memory "$m"; done
for m in 8G 0g -1g 8gb "8 g" ""; do bad perf_check_memory "$m"; done
for kv in output_energy=true energy_precision=17 blocked_node=global export_partition=true x=1e-3 a_b=-1.5; do
  ok perf_check_kv "$kv"
done
for kv in mesh=/etc/passwd max_steps=10 "a=b c" 'a=b|c' 'a=b/c' 'a=b\c' 'a=b&c' "a='b'" A=b 1a=b =b a= 'a=$(id)'; do
  bad perf_check_kv "$kv"
done
ok perf_check_mesh_path /perf/meshes/cube-h0.12.msh; ok perf_check_mesh_path /perf/meshes/fixture-cube-h0.12.msh
bad perf_check_mesh_path /perf/meshes/../x.msh; bad perf_check_mesh_path /etc/x.msh; bad perf_check_mesh_path /perf/meshes/a/b.msh

R=/perf/runs
ok perf_check_run_dir "$R" "$R/baseline/default/cohesive/h0.12/np1/rep1"
ok perf_check_run_dir "$R" "$R/fixtures/strict/coh_np1/rep3"
for d in "$R" "$R/" "$R/baseline" "$R/baseline/rep1" "$R/../x/y/rep1" "$R/a/../b/rep1" "/tmp/a/b/rep1" \
         "$R/a//b/rep1" "$R/a/b/rep1/" "$R/a/b/rep1.invalid-1" "$R/a/b/rep0" "$R/a/b/notrep" \
         "$R/a b/c/rep1" "$R/.a/b/rep1" "/perf/runsX/a/b/rep1"; do
  bad perf_check_run_dir "$R" "$d"
done
ok perf_check_attempt_dir "$R" "$R/a/b/rep1.invalid-2"; ok perf_check_attempt_dir "$R" "$R/a/b/rep1"
bad perf_check_attempt_dir "$R" "$R/a/b/rep1.invalid-0"; bad perf_check_attempt_dir "$R" "$R/a/b/rep1.old"

# symlink-resolving confinement on a real directory tree
tmp="$(mktemp -d)"; trap 'rm -rf "${tmp}"' EXIT
root="${tmp}/runs"; mkdir -p "${root}/camp/x" "${tmp}/outside"
ln -s "${tmp}/outside" "${root}/camp/link"
ok perf_confine_dir "${root}" "${root}/camp/x/rep1"
ok perf_confine_dir "${root}" "${root}/camp/new/rep1"
bad perf_confine_dir "${root}" "${root}/camp/link/rep1"
bad perf_confine_dir "${root}" "${root}/camp/../../outside/rep1"
bad perf_confine_dir "${root}" "${root}"
bad perf_confine_dir "${root}" "${tmp}/outside/rep1"
bad perf_confine_dir "${root}" "camp/x/rep1"

if [ "${fails}" -ne 0 ]; then echo "${fails} validator test(s) failed"; exit 1; fi
echo "validate.sh: all checks passed"
