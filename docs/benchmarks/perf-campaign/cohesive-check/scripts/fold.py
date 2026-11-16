#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (WP-COHCHECK); not ported from Akantu.
# fold.py: perf script output (stdin) -> folded stacks "root;...;leaf count"
import sys, re, collections
c = collections.Counter()
stack = []
def short(s):
    s = re.sub(r'\+0x[0-9a-f]+$', '', s)
    s = re.sub(r'::h[0-9a-f]{16}$', '', s)
    s = re.sub(r'<[^<>]*>', '', s)  # drop simple generics
    s = re.sub(r'<[^<>]*>', '', s)
    s = re.sub(r'<[^<>]*>', '', s)
    return s.replace(';', ',').strip()
for line in sys.stdin:
    line = line.rstrip('\n')
    if not line.strip():
        if stack:
            c[';'.join(reversed(stack))] += 1
        stack = []
        continue
    if line.startswith('\t') or line.startswith(' '):
        parts = line.strip().split(None, 1)
        if len(parts) == 2:
            sym = parts[1].rsplit(' (', 1)[0]
            stack.append(short(sym))
if stack:
    c[';'.join(reversed(stack))] += 1
for k, v in c.items():
    print(k, v)
