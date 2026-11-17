#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# asmstat.sh: build akantu-cohesive (release) from ~/w/cohthreads/asm and count the
# packed / scalar double operations of the insertion check's functions.
source ~/.cargo/env
cd ~/w/cohthreads/asm
CARGO_TARGET_DIR=~/w/cohthreads/target-asm cargo build -q --release --locked -p akantu-cohesive 2>&1 | grep -E "^(error|warning)" | head
r=$(ls -t ~/w/cohthreads/target-asm/release/deps/libakantu_cohesive-*.rlib | head -1)
objdump -d --no-show-raw-insn -C "$r" 2>/dev/null > /tmp/asm.txt
for f in "Linear>::check_serial_dim::<3>>:" "Linear>::check_insertion>:" "Linear>::insertion_decisions::{closure#1}>:" ; do
  awk -v f="$f" 'index($0,f){p=1;next} p&&/^$/{p=0} p{print $2}' /tmp/asm.txt | grep -E "^(mul|add|div|sub)(p|s)d|^call" | sort | uniq -c | sort -rn | tr '\n' ' ' | sed "s/^/$f /"; echo
done
