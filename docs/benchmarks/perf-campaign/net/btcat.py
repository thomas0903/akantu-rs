#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (no upstream counterpart): collectives per step and rank of the
# port by driver phase and kind, from the call stacks the interposer samples
# (PMPI_BT=Iallgather: every MPI_Iallgather, i.e. every collective of the backend).
#   python3 -I btcat.py [--detail] DRIVER_BINARY STEPS summary_rank*.pmpi-bt.txt
# --detail also lists, per phase and kind, the call sites: the innermost
# frames outside the communication layer and the standard library.
import collections
import re
import subprocess
import sys
from pathlib import Path

args = sys.argv[1:]
detail = bool(args) and args[0] == '--detail'
if detail:
    args = args[1:]
binary, steps, files = args[0], int(args[1]), args[2:]
stacks = collections.Counter()
for fn in files:
    cur, n = [], 0
    for line in Path(fn).read_text().splitlines(keepends=True):
        if line.startswith('== '):
            if cur: stacks[tuple(cur)] += n
            cur, n = [], int(line[3:])
        else:
            # frames of the driver binary (whatever its installed name), not
            # of shared libraries
            m = None if '.so' in line else re.search(r'\(\+(0x[0-9a-f]+)\)', line)
            if m: cur.append(int(m.group(1), 16))
    if cur: stacks[tuple(cur)] += n
addrs = sorted({a - 1 for s in stacks for a in s})
names = {}
for a in addrs:
    o = subprocess.run(['addr2line', '-f', '-C', '-i', '-e', binary, hex(a)], check=False, capture_output=True, text=True).stdout.splitlines()
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
SKIP = ('akantu_comm::', 'core::', 'std::', 'alloc::', '<', '{', 'akantu_solver::dof_manager::')
def site(s):
    frames = []
    for a in s:
        for f in names[a].split(' '):
            f = re.sub(r'::h[0-9a-f]{16}$', '', f)
            if '::' in f and not f.startswith(SKIP) and f not in frames:
                frames.append(f)
    return ' < '.join(f.split('::', 1)[-1] for f in frames[:3])
if detail:
    sites = collections.Counter()
    for s, n in stacks.items():
        txt = ' '.join(names[a] for a in s)
        kind = 'agreement' if 'agree_digest' in txt else 'data collective'
        sites[(phase(txt), kind, site(s))] += n
    print()
    print(f'{"phase / kind / call site":100s} per step and rank')
    for (p, k, w), n in sorted(sites.items(), key=lambda kv: (kv[0][0], kv[0][1], -kv[1])):
        if n / nr / steps >= 0.005:
            print(f'{p} / {k} / {w:70s} {n / nr / steps:8.2f}')
