#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs measurement script of the performance campaign (the cohesive insertion check); not ported from Akantu.
# q.py folded pattern [depth] [mode]: among stacks containing pattern, aggregate
# by the frame `depth` levels below the last frame matching pattern (depth 0 =
# that frame), or by leaf (mode=leaf). Prints counts and shares.
import sys, collections
f, pat = sys.argv[1], sys.argv[2]
depth = int(sys.argv[3]) if len(sys.argv) > 3 else 1
mode = sys.argv[4] if len(sys.argv) > 4 else 'child'
tot = 0; sel = 0; agg = collections.Counter()
for line in open(f):
    st, n = line.rsplit(' ', 1); n = int(n); tot += n
    fr = st.split(';')
    idx = [i for i, x in enumerate(fr) if pat in x]
    if not idx:
        continue
    sel += n
    i = idx[-1]
    if mode == 'leaf':
        agg[fr[-1]] += n
    else:
        agg[fr[i + depth] if i + depth < len(fr) else '<self>'] += n
print(f'total {tot}  matching {sel} ({100*sel/max(tot,1):.1f}%)')
for k, v in agg.most_common(40):
    print(f'{100*v/max(sel,1):6.2f}%  {v:7d}  {k[:200]}')
