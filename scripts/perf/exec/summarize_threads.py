#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): thread scaling of the
# explicit step (D-123) from timing_rotated.sh output whose labels are
# <REF> (the branch point, one thread) and <NAME>-t<N> (wrappers.sh).
# Per (mesh, mode, phase, label): ns per element and step, median [min, max]
# over the rounds; speed-up = REF / label per round (paired), median
# [min, max]; efficiency = median speed-up / N. The phase "rest" is step −
# fint (integration scheme, residual, lumped solve) per run.
#   summarize_threads.py TIMING.jsonl REF NAME
import json
import statistics as st
import sys
from collections import defaultdict

path, ref, name = sys.argv[1:4]
rows = defaultdict(dict)  # (mesh, mode, phase) -> {(label, round): ns}
meta = {}
rounds = []
for line in open(path):
    if not line.startswith("{"):
        continue
    o = json.loads(line)
    if o["kind"] == "round":
        rounds.append(o)
        continue
    label, mesh, mode, r = o["label"].split("|")
    if o["kind"] == "meta":
        meta[mesh] = (o["types"][0].strip("_"), o["nb_elements"])
    elif o["kind"] == "time":
        rows[(mesh, mode, o["phase"])][(label, r)] = o["ns_el_step"]
for (mesh, mode, ph), d in list(rows.items()):
    if ph != "step" or (mesh, mode, "fint") not in rows:
        continue
    f = rows[(mesh, mode, "fint")]
    rows[(mesh, mode, "rest")] = {k: v - f[k] for k, v in d.items() if k in f}

labels = sorted({l for d in rows.values() for (l, _) in d if l != ref},
                key=lambda l: int(l.rsplit("-t", 1)[1]) if "-t" in l else 0)
threads = {l: int(l.rsplit("-t", 1)[1]) for l in labels if l.startswith(name + "-t")}


def med(v):
    return f"{st.median(v):.1f} [{min(v):.1f}, {max(v):.1f}]" if v else "—"


print(f"rounds: {len(rounds)}; cpu: {rounds[0]['cpu'] if rounds else '?'}; "
      f"first {rounds[0]['utc'] if rounds else '?'}, last {rounds[-1]['utc'] if rounds else '?'}")
print(f"load at round start: " + ", ".join(r["load"].split()[0] for r in rounds))
print()
print(f"| mesh | elements | mode | phase | {ref} (ns/el/step) | " + " | ".join(
    f"{l} (ns) | speed-up | eff." for l in labels) + " |")
print("|---|---|---|---|---|" + "---|---|---|" * len(labels))
order = ["tet4-20k", "tet4-200k", "hex8-20k", "hex8-200k"]
for mesh in order:
    for mode in ["ss", "fd"]:
        for ph in ["step", "fint", "stress", "rest"]:
            d = rows.get((mesh, mode, ph))
            if not d:
                continue
            rs = sorted({r for (_, r) in d})
            base = [d[(ref, r)] for r in rs if (ref, r) in d]
            cells = []
            for l in labels:
                vals = [d[(l, r)] for r in rs if (l, r) in d]
                sp = [d[(ref, r)] / d[(l, r)] for r in rs if (ref, r) in d and (l, r) in d and d[(l, r)] > 0]
                n = threads.get(l, 1)
                if sp:
                    cells.append(f"{med(vals)} | **{st.median(sp):.2f}** [{min(sp):.2f}, {max(sp):.2f}] | {st.median(sp) / n:.2f}")
                else:
                    cells.append("— | — | —")
            ty, ne = meta.get(mesh, ("?", 0))
            print(f"| {mesh} | {ne:,} | {mode} | {ph} | {med(base)} | " + " | ".join(cells) + " |")
