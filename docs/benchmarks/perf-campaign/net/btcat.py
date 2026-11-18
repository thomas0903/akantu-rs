#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (no upstream counterpart): collectives per step and rank of the
# port by driver phase and kind, from the call stacks the interposer samples
# (PMPI_BT=Iallgather: every MPI_Iallgather, i.e. every collective of the backend).
#   python3 -I btcat.py DRIVER_BINARY STEPS summary_rank*.pmpi-bt.txt
import collections, re, subprocess, sys
binary, steps, files = sys.argv[1], int(sys.argv[2]), sys.argv[3:]
stacks = collections.Counter()
for fn in files:
    cur, n = [], 0
    for line in open(fn):
        if line.startswith('== '):
            if cur: stacks[tuple(cur)] += n
            cur, n = [], int(line[3:])
        else:
            m = re.search(r'cohesive_extrinsic\(\+(0x[0-9a-f]+)\)', line)
            if m: cur.append(int(m.group(1), 16))
    if cur: stacks[tuple(cur)] += n
addrs = sorted({a - 1 for s in stacks for a in s})
names = {}
for a in addrs:
    o = subprocess.run(['addr2line', '-f', '-C', '-i', '-e', binary, hex(a)], capture_output=True, text=True).stdout.splitlines()
    names[a + 1] = ' '.join(o[i] for i in range(0, len(o), 2))
def phase(txt):
    if 'driver::global_cohesive_count' in txt: return 'check_cohesive_stress (driver count)'
    if 'check_cohesive_stress' in txt: return 'check_cohesive_stress'
    if '::energy' in txt or 'kinetic_energy' in txt or 'external_work' in txt: return 'energies'
    if '::solve_step' in txt: return 'solve_step'
    return 'other (init, output)'
agg = collections.Counter()
for s, n in stacks.items():
    txt = ' '.join(names[a] for a in s)
    kind = 'agreement' if 'agree_digest' in txt else 'data collective'
    agg[(phase(txt), kind)] += n
nr = len(files)
tot = sum(agg.values())
print(f'{"phase":40s} {"kind":16s} per step and rank')
for (p, k), n in sorted(agg.items()):
    print(f'{p:40s} {k:16s} {n / nr / steps:8.2f}')
print(f'{"total":57s} {tot / nr / steps:8.2f}')
