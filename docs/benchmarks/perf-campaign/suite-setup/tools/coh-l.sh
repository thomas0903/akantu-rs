#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): suite-setup measurement script, kept as run.
cd $HOME/w/suite-setup
export AKANTU_SUITE_COMMIT=e2bebff49
B=$HOME/w/suite-setup/target/release/suite_setup
s="set -e"
for r in 0 1 2; do s+="; rm -f results/coh-l/base/cohesive_insert_tet4.r$r.jsonl; /usr/bin/time -v taskset -c 2 $B --case cohesive_insert_tet4 --size l --reps 1 --out results/coh-l/base/cohesive_insert_tet4.r$r.jsonl 2> results/coh-l/time.r$r.txt"; done
tjob suite-setup-coh-l -- bash -c "$s"
echo coh-done
