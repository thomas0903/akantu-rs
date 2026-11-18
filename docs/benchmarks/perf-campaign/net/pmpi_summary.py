#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (no upstream counterpart): the counters of the PMPI interposer
# (pmpistat.c, summary_rank<r>.pmpi.json of every rank) of one run, per step and rank.
#   python3 -I pmpi_summary.py RUNDIR [STEPS]
import glob
import json
import os
import statistics
import sys
from pathlib import Path

d = sys.argv[1].rstrip('/')
steps = int(sys.argv[2]) if len(sys.argv) > 2 else json.loads(Path(d + '/summary.json').read_text())['max_steps']
recs = [json.loads(Path(f).read_text()) for f in glob.glob(d + '/summary_rank*.pmpi.json')]
n = len(recs)
ops = sorted({k for r in recs for k in r['ops']})
print(f'{os.path.basename(d)}: {n} ranks, {steps} steps; per step and rank (mean): calls, hits, '
      f'ms inside, KiB sent, completions, ms lifetime (mean), max lifetime ms')
for op in ops:
    v = [r['ops'].get(op, {}) for r in recs]
    def g(k, v=v):
        return statistics.mean(x.get(k, 0) for x in v)
    mx = max(x.get('life_max_ns', 0) for x in v)
    print(f'  {op:12s} {g("calls")/steps:10.2f} {g("hits")/steps:8.2f} {g("ns")/steps/1e6:9.4f} '
          f'{g("bytes")/steps/1024:9.2f} {g("life_n")/steps:8.2f} {g("life_ns")/steps/1e6:9.4f} {mx/1e6:9.2f}'
          + (f'  hist {[round(sum(x.get("hist",[0]*5)[i] for x in v)/n/steps,2) for i in range(5)]}' if g("bytes") else ''))
