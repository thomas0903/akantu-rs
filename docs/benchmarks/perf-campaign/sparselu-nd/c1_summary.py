# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the cohesive static
# solve series of c1_nd.sh: per variant the median static_solve time, the
# paired ratios to C++ and to COLAMD at one thread, the peak RSS, the fill
# and the energies after the 10 explicit steps.
#
#   python3 -I c1_summary.py OUT H
import json, glob, os, re, statistics as st, sys, collections
out, h = sys.argv[1], sys.argv[2]
rows = collections.defaultdict(list)
for d in sorted(glob.glob(f"{out}/h{h}-*-r*")):
    m = re.match(rf".*/h{re.escape(h)}-(.+)-r(\d+)$", d)
    if not m or not os.path.exists(d + "/summary.json"):
        continue
    var, rep = m.group(1), int(m.group(2))
    j = json.load(open(d + "/summary.json"))
    for r in j["runs"]:
        s = r.get("summary") or {}
        rows[var].append(dict(rep=rep, static=r["chrono"].get("static_solve"),
            ep=s.get("energy_potential"), ek=s.get("energy_kinetic"), ed=s.get("energy_dissipated"),
            coh=s.get("nb_cohesive_elements"), hwm=(s.get("rust_memory_kib") or {}).get("hwm_static", {}).get("max"),
            peak=(r.get("mem") or {}).get("run_rank_peak_max"), stats=s.get("rust_static_solver_stats"),
            status=r.get("status")))
cpp = {x["rep"]: x["static"] for x in rows.get("cpp", [])}
order = ["cpp"] + [f"{o}-t{t}" for o in ("colamd", "nd") for t in (1, 2, 4, 8)]
print(f"h = {h}")
print("| variant | static_solve median [min, max] | / C++ per rep | / COLAMD t1 per rep | peak RSS (MiB) | nnz(L+U) | E_pot | E_kin | cohesive |")
print("|---|---|---|---|---|---|---|---|---|")
col1 = {x["rep"]: x["static"] for x in rows.get("colamd-t1", [])}
for v in order:
    xs = rows.get(v, [])
    if not xs:
        continue
    t = [x["static"] for x in xs]
    rc = [x["static"] / cpp[x["rep"]] for x in xs if x["rep"] in cpp]
    r1 = [x["static"] / col1[x["rep"]] for x in xs if x["rep"] in col1]
    f = lambda a: "%.3f [%.3f, %.3f]" % (st.median(a), min(a), max(a)) if a else "–"
    pk = [x["peak"] or x["hwm"] for x in xs if (x["peak"] or x["hwm"])]
    nz = xs[0]["stats"]
    nzs = "%d" % (nz["nnz_l"] + nz["nnz_u"]) if nz else "–"
    print(f"| {v} | {st.median(t):.3f} s [{min(t):.3f}, {max(t):.3f}] | {f(rc)} | {f(r1)} | {st.median(pk)/1024 if pk else float('nan'):.0f} | {nzs} | {xs[0]['ep']!r} | {xs[0]['ek']!r} | {xs[0]['coh']} |")
