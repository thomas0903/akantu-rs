#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): suite-setup measurement script, kept as run.
# Interleaved A/B of the Rust twin and the Python explicit loop (size m)
cd $HOME/w/suite-setup
out=$HOME/w/suite-setup/results/twin-ab; mkdir -p $out/rust $out/python
B=$HOME/w/suite-setup/target/release/suite_setup
export AKANTU_SUITE_COMMIT=e2bebff49
s="set -e"; for r in 2 3 4; do
  rust="rm -f $out/rust/twin.r$r.jsonl; taskset -c 2 $B --case py_twin_explicit_tri3 --size m --out $out/rust/twin.r$r.jsonl"
  py="rm -f $out/python/twin.r$r.jsonl; taskset -c 2 $HOME/w/suite-setup/pybench.sh --case explicit_tri3 --size m --out $out/python/twin.r$r.jsonl"
  if [ $((r % 2)) = 0 ]; then s+="; $rust; $py"; else s+="; $py; $rust"; fi
  :
done
tjob suite-setup-twin-ab -- bash -c "$s"
echo twin-done
