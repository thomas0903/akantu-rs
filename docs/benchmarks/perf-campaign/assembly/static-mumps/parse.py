#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): static-solve and
# memory table of the interleaved c1prep campaigns of the sparse-assembly
# changes (campaign names s-<aft|bef>-h<H>-np<N>-r<R>; README.md of this directory).
import glob, json, os, re, statistics as st, sys
base = os.path.expanduser(sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.abspath(__file__)))
rows = {}
for f in sorted(glob.glob(base + "/s-*-h*-np*-r*/summary.json")):
    m = re.search(r"s-(aft|bef)-h([\d.]+)-np(\d+)-r(\d+)/summary.json", f)
    var, h, np_, r = m.groups()
    for run in json.load(open(f))["runs"]:
        if run.get("status") != 0: continue
        lab = "cpp" if run["case"] == "cpp" else ("after" if var == "aft" else "before")
        c, mm = run["chrono"], run["mem"]
        rows.setdefault((h, np_), {}).setdefault(lab, {})[int(r)] = dict(
            ss=c["static_solve"], init=c["init_full"],
            peak=mm["run_rank_peak_max"] / 1024, peak_tot=mm["run_total_rss_peak"] / 1024,
            speak=mm["static_rank_peak_max"] / 1024, stot=mm["static_total_rss_peak"] / 1024)
def med(xs): return f"{st.median(xs):.3f} [{min(xs):.3f}, {max(xs):.3f}]"
for (h, np_), v in sorted(rows.items()):
    print(f"## h={h} np={np_}")
    for lab in ("cpp", "before", "after"):
        if lab not in v: continue
        g = lambda k: [x[k] for x in v[lab].values()]
        print(f"  {lab:7s} n={len(g('ss'))} static_solve s {med(g('ss'))}  init_full s {med(g('init'))}  "
              f"peak rank MiB {st.median(g('peak')):.0f}  peak total {st.median(g('peak_tot')):.0f}  "
              f"static-phase peak rank {st.median(g('speak')):.0f} total {st.median(g('stot')):.0f}")
    for a, b in (("after", "before"), ("after", "cpp"), ("before", "cpp")):
        if a in v and b in v:
            ks = [k for k in v[a] if k in v[b]]
            if not ks: continue
            for key in ("ss", "peak_tot", "stot"):
                rr = [v[a][k][key] / v[b][k][key] for k in ks]
                print(f"  {a}/{b} {key}: {med(rr)} n={len(rr)}")
