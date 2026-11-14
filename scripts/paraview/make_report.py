# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): turns the results of the
# ParaView visual check (run-paraview.sh / run-paraview-python.sh: one
# directory per example with result.json and PNGs; runinfo.py: runinfo.json)
# into the committed deliverables: the PNGs, gallery.html, the per-file array
# tables, arrays.md and the generated tables of the report. Plain python3,
# standard library only.
#
#   python3 -I scripts/paraview/make_report.py <res-dir> <runinfo.json> [<vtu-diff-dir>] [<repo-root>]
#
# <vtu-diff-dir>: the outputs of vtu_diff.py (<id>.tsv, <id>.txt per example;
# `-` for none). Writes below <repo-root>/docs/reports/paraview-visual-check/
# (PNGs, gallery.md, gallery.html, details.md, arrays.md, data/) and the file
# <repo-root>/docs/reports/2026-11-12-paraview-visual-check.md from the
# template scripts/paraview/report_template.md (placeholders {{NAME}}); the
# summary table takes its verdicts from scripts/paraview/verdicts.tsv.
# Compressed tables are written with a zero gzip time stamp, so an unchanged
# table gives an unchanged file.
import gzip
import html
import json
import os
import shutil
import sys

res_dir = sys.argv[1]
runinfo = json.load(open(sys.argv[2]))
vtu_diff_dir = sys.argv[3] if len(sys.argv) > 3 and sys.argv[3] != "-" else None
here = os.path.dirname(os.path.abspath(__file__))
repo = os.path.abspath(sys.argv[4]) if len(sys.argv) > 4 else os.path.abspath(os.path.join(here, "..", ".."))
outdir = os.path.join(repo, "docs", "reports", "paraview-visual-check")
report = os.path.join(repo, "docs", "reports", "2026-11-12-paraview-visual-check.md")


def gz_text(path, text):
    """`text` gzip-compressed with a zero time stamp (reproducible bytes)."""
    with open(path, "wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", mtime=0, filename="") as f:
        f.write(text.encode())


def table(path):
    rows = []
    for ln in open(path):
        if ln.startswith("#") or not ln.strip():
            continue
        rows.append(ln.rstrip("\n").split("\t"))
    return rows


cpp = [dict(id=r[0], binary=r[1], ocase=r[2], args=r[3], desc=r[4]) for r in table(os.path.join(here, "cases.tsv"))]
py = [dict(id="py-" + r[0], pcase=r[0], ocase=r[1], desc=r[2]) for r in table(os.path.join(here, "python-cases.tsv"))]

KIND = {
    "disp": "Deformed shape, coloured by the displacement magnitude (how far each point has moved, in metres)",
    "stress": "Von Mises stress (one number that sums up the stress state, in pascals)",
    "damage": "Damage (0 = intact, 1 = fully broken)",
    "temperature": "Temperature (kelvin)",
    "cohesive": "Cohesive elements (the lines where the body can crack), coloured by their damage (0 = intact, 1 = broken), drawn over the grey body",
}
STEP = {"first": "first dump (the initial state)", "middle": "a middle dump", "last": "last dump"}


def load(ex):
    p = os.path.join(res_dir, ex, "result.json")
    return json.load(open(p)) if os.path.exists(p) else None


def fmt(x):
    if x == 0:
        return "0"
    if abs(x) >= 1e5 or abs(x) < 1e-3:
        return f"{x:.3e}"
    return f"{x:.4g}"


def exts(d):
    n = d["n_files"]
    return f"{n['pvd']} .pvd, {n['pvtu']} .pvtu, {n['vtu']} .vtu"


def strip_png(src, dst):
    """Copy a PNG keeping only the chunks needed to display it (no text chunks:
    ParaView's single-panel screenshots carry a comment)."""
    import struct

    b = open(src, "rb").read()
    out = [b[:8]]
    i = 8
    while i < len(b):
        n, t = struct.unpack(">I4s", b[i:i + 8])
        if t in (b"IHDR", b"PLTE", b"tRNS", b"IDAT", b"IEND"):
            out.append(b[i:i + 12 + n])
        i += 12 + n
    open(dst, "wb").write(b"".join(out))


rows = []
size_total = 0
os.makedirs(outdir, exist_ok=True)
for e in cpp + py:
    d = load(e["id"])
    e["d"] = d
    if d is None:
        continue
    # pictures
    pics = []
    exdir = os.path.join(outdir, e["id"])
    shutil.rmtree(exdir, ignore_errors=True)
    for r in d["renders"]:
        if "png" not in r:
            continue
        src = os.path.join(res_dir, e["id"], r["png"])
        name = r["png"][len(e["id"]) + 1:]
        os.makedirs(exdir, exist_ok=True)
        strip_png(src, os.path.join(exdir, name))
        sz = os.path.getsize(os.path.join(exdir, name))
        size_total += sz
        pics.append(dict(r, file=f"{e['id']}/{name}", size=sz))
    e["pics"] = pics
    # per-file table of arrays (gz): every file of both sides
    os.makedirs(os.path.join(outdir, "data"), exist_ok=True)
    lines_ = ["side\tfile\tsha256_12\tpoints\tcells\tarray\tncomp\tmin\tmax\n"]
    sdp = d["sides"].get("port", {}).get("files", {})
    sdu = d["sides"].get("upstream", {}).get("files", {})
    for fn in sorted(set(sdp) | set(sdu)):
        rp, ru = sdp.get(fn), sdu.get(fn)
        # a file whose bytes are equal on both sides is written once, as side "both"
        todo = [("both", rp)] if rp and ru and rp["sha256"] == ru["sha256"] else [(sn, r_) for sn, r_ in (("port", rp), ("upstream", ru)) if r_]
        for sn, rec in todo:
            arrs = rec.get("arrays", {})
            head_ = f"{sn}\t{fn}\t{rec['sha256'][:12]}\t{rec.get('points', '')}\t{rec.get('cells', '')}"
            if not arrs:
                lines_.append(head_ + "\t\t\t\t\n")
            for k, a in arrs.items():
                lo = min(c[0] for c in a["components"])
                hi = max(c[1] for c in a["components"])
                lines_.append(f"{head_}\t{k}\t{a['ncomp']}\t{lo!r}\t{hi!r}\n")
    gz_text(os.path.join(outdir, "data", f"{e['id']}-files.tsv.gz"), "".join(lines_))
    # raw classification of the differing files (vtu_diff.py)
    e["vtu_diff"] = None
    if vtu_diff_dir and os.path.exists(os.path.join(vtu_diff_dir, f"{e['id']}.txt")):
        e["vtu_diff"] = open(os.path.join(vtu_diff_dir, f"{e['id']}.txt")).read().rstrip("\n").splitlines()
        tsv = os.path.join(vtu_diff_dir, f"{e['id']}.tsv")
        if os.path.exists(tsv) and len(open(tsv).read().splitlines()) > 1:
            gz_text(os.path.join(outdir, "data", f"{e['id']}-vtu-diff.tsv.gz"), open(tsv).read())


def run_of(e):
    if e["id"].startswith("py-"):
        return runinfo["python"].get(e["pcase"], {})
    return {"port": runinfo["port"].get(e["id"], {}), "oracle": runinfo["oracle"].get(e["ocase"], {}) if e["ocase"] != "-" else {}}


def verdict(e):
    d = e["d"]
    if d is None:
        return "not processed"
    c = d.get("comparison")
    bits = []
    ld = d.get("vtk_messages_loading", d["vtk_messages_total"])
    bits.append(f"{ld} ParaView messages while loading")
    if c:
        bits.append(f"{c['identical_bytes']} of {c['common']} files byte-identical")
    return "; ".join(bits)


# ------------------------------------------------------------------ gallery
def caption(e, r):
    d = e["d"]
    kind = KIND[r["kind"]]
    step = STEP[r["step_name"]]
    warp = d.get("warp", {})
    s = f"{kind}, {step}, dump {r['dump']} of {r['of']}"
    if r["kind"] != "temperature" and warp.get("scale_factor"):
        s += f"; the shape is magnified {warp['scale_factor']:.4g}× so that the movement can be seen"
    pc = r.get("pixel_comparison")
    miss = r.get("sides_without_the_array") or []
    if miss:
        s += f". ParaView could not read this array from the {' / '.join(miss)} file; the other side is shown alone"
    elif pc is None:
        s += ". Rust port only"
    elif pc["pixels_differing"] == 0:
        s += ". Left (Rust port) and right (upstream C++) are pixel-identical"
    else:
        s += (f". Left (Rust port) and right (upstream C++) differ in {pc['pixels_differing']:,} of {pc['pixels_compared']:,} pixels "
              f"(largest colour difference {pc['max_channel_difference']} of 255)")
    if r.get("upstream_outside_scale"):
        up = r["data_range_by_side"]["upstream"]
        s += f". The colour scale is the port's range; upstream's values reach {fmt(up[1])} (smallest {fmt(up[0])}), outside it, and are drawn in the end colours"
    if r.get("cut_in_half"):
        s += ". The body is cut in half through its middle so that the inside is visible"
    un = r.get("unused_points_left_out") or {}
    if any(un.values()):
        s += (f". {max(un.values())} points that no cell uses (pure ghost points of the distributed run) are left out of the picture")
    return s + "."


def gallery_html():
    out = [
        "<!doctype html>",
        '<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">',
        "<title>akantu-rs: ParaView gallery</title>",
        "<style>",
        ":root{--bg:#fff;--fg:#1b1b1b;--mut:#555;--line:#d0d0d0;--card:#f6f6f6;--warn:#8a4b00}",
        "@media (prefers-color-scheme:dark){:root{--bg:#161616;--fg:#e8e8e8;--mut:#aaa;--line:#444;--card:#202020;--warn:#ffb866}}",
        "body{margin:0;padding:16px;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,sans-serif;max-width:1100px;margin-inline:auto}",
        "h1{font-size:1.5rem}h2{font-size:1.2rem;margin-top:2.2rem;border-top:1px solid var(--line);padding-top:1rem}",
        "figure{margin:1rem 0;background:var(--card);padding:8px;border:1px solid var(--line);border-radius:6px}",
        "img{width:100%;height:auto;display:block;background:#fff}figcaption{color:var(--mut);font-size:.9rem;margin-top:6px}",
        ".facts{color:var(--mut);font-size:.9rem}.warn{color:var(--warn)}nav a{margin-right:.6rem;white-space:nowrap}code{font-size:.9em}",
        "</style></head><body>",
        "<h1>ParaView gallery of the example outputs</h1>",
        "<p>Every picture is made with a headless ParaView 5.13.3: the <b>Rust port on the left</b>, <b>upstream Akantu (C++) on the right</b>, "
        "the same camera and the same colour scale. Read the report "
        '(<code>docs/reports/2026-11-12-paraview-visual-check.md</code>) for what was done, what was not checked and what ParaView complained about.</p>',
        "<nav>" + " ".join(f'<a href="#{e["id"]}">{html.escape(e["id"])}</a>' for e in cpp + py if e.get("pics")) + "</nav>",
    ]
    for e in cpp + py:
        d = e.get("d")
        if not d:
            continue
        out.append(f'<h2 id="{e["id"]}">{html.escape(e["id"])}</h2>')
        out.append(f"<p>{html.escape(e['desc'])}</p>")
        out.append(f'<p class="facts">{html.escape(verdict(e))}.</p>')
        if not e["pics"]:
            out.append('<p class="warn">No picture was made for this example (see the report).</p>')
        for r in e["pics"]:
            out.append(
                f'<figure><img loading="lazy" src="{r["file"]}" alt="{html.escape(e["id"])} {r["kind"]} {r["step_name"]}">'
                f"<figcaption>{html.escape(caption(e, r))}</figcaption></figure>"
            )
    out.append("</body></html>")
    open(os.path.join(outdir, "gallery.html"), "w").write("\n".join(out))


gallery_html()


# ------------------------------------------------------------------ arrays.md
def arrays_md():
    out = [
        "# Arrays of every dump file, port against upstream",
        "",
        "Generated by `scripts/paraview/make_report.py` from what ParaView reports for **every** file written by the runs "
        "(all time steps of every `.pvd`, every `.pvtu`, every `.vtu` piece). For each array: association (point or cell data), "
        "number of components, and the smallest and largest value over all components and over all files of the run. "
        "Per-file ranges of every file are in `data/<example>-files.tsv.gz` (columns: side, file, first 12 hex digits of the SHA-256, points, cells, array, components, min, max; side `both` = the two files are byte-identical, listed once). "
        "A cell marked `–` means the array does not exist on that side.",
        "",
    ]
    for e in cpp + py:
        d = e.get("d")
        if not d:
            continue
        out += [f"## {e['id']}", ""]
        sp = d["sides"].get("port", {}).get("array_summary", {})
        su = d["sides"].get("upstream", {}).get("array_summary", {})
        keys = sorted(set(sp) | set(su))
        if not keys:
            out += ["No arrays (no ParaView file was written by the port" + (" or upstream" if not su else "") + ").", ""]
            continue
        out += ["| array | components | port: min .. max | upstream: min .. max |", "|---|---|---|---|"]

        def rng(a):
            if not a:
                return "–"
            lo = min(c[0] for c in a["components"])
            hi = max(c[1] for c in a["components"])
            return f"{fmt(lo)} .. {fmt(hi)}"

        for k in keys:
            nc = (sp.get(k) or su.get(k))["ncomp"]
            out.append(f"| `{k}` | {nc} | {rng(sp.get(k))} | {rng(su.get(k))} |")
        out.append("")
    open(os.path.join(outdir, "arrays.md"), "w").write("\n".join(out))


arrays_md()

# ------------------------------------------------------------------ report tables
def how_compared(e):
    if e["id"].startswith("py-"):
        return f"upstream module, same script (case `{e['ocase']}`)"
    if e["ocase"] == "-":
        return "**port only**: upstream's embedded package does not compile at this commit; built with the project's minimal compile patches (D-060), the example stops with a segmentation fault in `initFull` (UD-851; cause: UD-1140)"
    if e["binary"].startswith("python:"):
        return "the upstream binary of the oracle build (`run-ex012.sh`)"
    return f"upstream binary or its Eigen-pinned variant (case `{e['ocase']}`)"


def port_args(e):
    if e["id"].startswith("py-") or e["args"] == "-":
        return "default"
    return "`" + e["args"].replace("{oracle}/", "") + "`"


main_rows = ["| Example | What it simulates | Port arguments | Files the port wrote | Compared with | ParaView load of every file | Pictures |", "|---|---|---|---|---|---|---|"]
for e in cpp + py:
    d = e.get("d")
    if not d:
        continue
    sp = d["sides"]["port"]
    su = d["sides"].get("upstream")
    n_p = exts(sp) if sum(sp["n_files"].values()) else "none (see findings)"
    ld = d.get("vtk_messages_loading", d["vtk_messages_total"])
    c = d.get("comparison")
    errs = len(sp["errors"]) + (len(su["errors"]) if su else 0)
    load_s = f"{ld} messages, {errs} files refused"
    if c and c["common"]:
        load_s += f"; {c['identical_bytes']}/{c['common']} files byte-identical"
    main_rows.append(
        f"| `{e['id']}` | {e['desc']} | {port_args(e)} | {n_p} | {how_compared(e)} | {load_s} | {len(e['pics'])} |"
    )

# findings, generated
find = []
for e in cpp + py:
    d = e.get("d")
    if not d:
        continue
    c = d.get("comparison")
    ld = d.get("vtk_messages_loading", d["vtk_messages_total"])
    lines = []
    for side, sd in d["sides"].items():
        if sd["errors"]:
            lines.append(f"ParaView raised an error opening {len(sd['errors'])} {side} file(s), e.g. `{sd['errors'][0]}`.")
        if sd["files_with_messages"]:
            lines.append(
                f"ParaView printed messages while loading {len(sd['files_with_messages'])} of the {side} files "
                f"(first: `{sd['files_with_messages'][0]}`)."
            )
    if c and c["different"]:
        n = len(c["different"])
        lines.append(f"{n} of {c['common']} files differ in bytes between port and upstream (first: `{c['different'][0]}`).")
        agg = {}
        for dd in c.get("detail", []):
            for k, v in (dd.get("arrays") or {}).items():
                if isinstance(v, dict):
                    if v["max_abs_diff"] > 0:
                        a = agg.setdefault(k, [0.0, 0.0])
                        a[0] = max(a[0], v["max_abs_diff"])
                        a[1] = max(a[1], v["max_abs_value_upstream"])
                else:
                    agg.setdefault(k + " (" + v + ")", None)
        if agg:
            parts = []
            for k, v in sorted(agg.items()):
                parts.append(f"`{k}`" if v is None else f"`{k}` (largest difference {fmt(v[0])} where the upstream values reach {fmt(v[1])})")
            lines.append(f"In the {len(c['detail'])} differing files read back through ParaView (first, last and an even sample), these arrays differ: " + ", ".join(parts) + ".")
        else:
            lines.append(f"In the {len(c['detail'])} differing files read back through ParaView (first, last and an even sample) every array has the same values: only the bytes differ.")
    if c and c["only_in_port"]:
        lines.append(f"Files only in the port output: {len(c['only_in_port'])} (e.g. `{c['only_in_port'][0]}`).")
    if c and c["only_in_upstream"]:
        lines.append(f"Files only in the upstream output: {len(c['only_in_upstream'])} (e.g. `{c['only_in_upstream'][0]}`).")
    pd = [r for r in e["pics"] if (r.get("pixel_comparison") or {}).get("pixels_differing", 0) > 0]
    if pd:
        lines.append(
            f"{len(pd)} of {len(e['pics'])} pictures differ between left and right: "
            + ", ".join(f"{r['kind']}/{r['step_name']} ({r['pixel_comparison']['pixels_differing']:,} px)" for r in pd)
            + "."
        )
    skipped = [r for r in d["renders"] if "png" not in r]
    if skipped:
        lines.append("Not drawn: " + "; ".join(sorted({f"{r.get('kind')} ({r.get('skipped') or r.get('error')})" for r in skipped})) + ".")
    if lines:
        find.append((e, lines))

fl = []
for e, lines in find:
    fl.append(f"**`{e['id']}`**")
    fl.append("")
    for ln in lines:
        fl.append(f"- {ln}")
    fl.append("")

# ------------------------------------------------------------------ verdicts
VERDICT = {
    "identical": "identical",
    "compat": "identical with upstream's behaviour switched on",
    "differs": "differs: upstream's file cannot be reproduced",
    "gap": "gap of the port",
    "port-only": "port only",
}
verdicts = {}
for r in table(os.path.join(here, "verdicts.tsv")):
    verdicts[r[0]] = (r[1], r[2] if len(r) > 2 else "-")


def files_cell(e):
    d = e["d"]
    c = d.get("comparison")
    if not c or not c["common"]:
        n = sum(d["sides"]["port"]["n_files"].values())
        return f"{n} (port only)" if n else "none"
    s_ = f"{c['identical_bytes']:,} / {c['common']:,}"
    extra = len(c["only_in_port"]) + len(c["only_in_upstream"])
    if extra:
        s_ += f" (+{extra} on one side)"
    return s_


sum_rows = ["| Example (run) | Verdict | Byte-identical files | Cause (defect entry) |", "|---|---|---|---|"]
for e in cpp + py:
    d = e.get("d")
    if not d:
        continue
    v, cause = verdicts.get(e["id"], ("?", "verdict missing in scripts/paraview/verdicts.tsv"))
    ld = d.get("vtk_messages_loading", d["vtk_messages_total"])
    vt = VERDICT.get(v, v)
    if ld:
        vt += f"; ParaView messages: {ld}"
    sum_rows.append(f"| `{e['id']}` | {vt} | {files_cell(e)} | {cause} |")

gal = []
for e in cpp + py:
    d = e.get("d")
    if not d or not e["pics"]:
        continue
    gal.append(f"## `{e['id']}`")
    gal.append("")
    gal.append(e["desc"])
    gal.append("")
    for r in e["pics"]:
        gal.append(f"![{e['id']} {r['kind']} {r['step_name']}]({r['file']})")
        gal.append("")
        gal.append(f"*{caption(e, r)}*")
        gal.append("")
open(os.path.join(outdir, "gallery.md"), "w").write(
    "# Gallery of the ParaView visual check\n\n"
    "Generated by `scripts/paraview/make_report.py`. Port on the left, upstream C++ on the right (a single panel where only "
    "one side could be drawn), the same camera and colour scale. The report "
    "(`../2026-11-12-paraview-visual-check.md`) explains every difference.\n\n" + "\n".join(gal)
)

vd = []
for e in cpp + py:
    if not e.get("d") or not e.get("vtu_diff"):
        continue
    vd.append(f"**`{e['id']}`** (`data/{e['id']}-vtu-diff.tsv.gz`: one row per differing array)")
    vd.append("")
    vd.append("```text")
    vd += e["vtu_diff"]
    vd.append("```")
    vd.append("")

def tot(side):
    return sum(sum(e["d"]["sides"][side]["n_files"].values()) for e in cpp + py if e.get("d") and side in e["d"]["sides"])


common = sum(e["d"]["comparison"]["common"] for e in cpp + py if e.get("d") and e["d"].get("comparison"))
ident = sum(e["d"]["comparison"]["identical_bytes"] for e in cpp + py if e.get("d") and e["d"].get("comparison"))
dirty = [e["id"] for e in cpp + py if e.get("d") and (e["d"].get("vtk_messages_loading", 0) > 0)]
stats = {
    "N_CPP": str(sum(1 for e in cpp if e.get("d"))),
    "N_PY": str(sum(1 for e in py if e.get("d"))),
    "FILES_PORT": f"{tot('port'):,}",
    "FILES_UP": f"{tot('upstream'):,}",
    "COMMON": f"{common:,}",
    "IDENT": f"{ident:,}",
    "DIRTY": ", ".join(f"`{x}`" for x in dirty) or "none",
}
tmpl = open(os.path.join(here, "report_template.md")).read()
# the hand-written explanations of section 3 (scripts/paraview/differences.md)
tmpl = tmpl.replace("{{DIFFERENCES}}", open(os.path.join(here, "differences.md")).read().rstrip("\n"))
for k, v in stats.items():
    tmpl = tmpl.replace("{{" + k + "}}", v)
npics = sum(len(e.get("pics", [])) for e in cpp + py)
biggest = max((p["size"] for e in cpp + py for p in e.get("pics", [])), default=0)
d0 = next(e["d"] for e in cpp if e.get("d"))
details = open(os.path.join(here, "details_template.md")).read()
for k, v in stats.items():
    details = details.replace("{{" + k + "}}", v)
details = (
    details.replace("{{MAIN_TABLE}}", "\n".join(main_rows))
    .replace("{{FINDINGS_AUTO}}", "\n".join(fl))
    .replace("{{VTU_DIFF}}", "\n".join(vd) or "(not run)")
)
open(os.path.join(outdir, "details.md"), "w").write(details)
rep = (
    tmpl.replace("{{SUMMARY_TABLE}}", "\n".join(sum_rows))
    .replace("{{NPICS}}", str(npics))
    .replace("{{PNG_TOTAL_MB}}", f"{size_total / 1e6:.1f}")
    .replace("{{PNG_MAX_KB}}", f"{biggest / 1e3:.0f}")
    .replace("{{PV_VERSION}}", d0["paraview_version"])
)
open(report, "w").write(rep)
print(f"{npics} pictures, {size_total / 1e6:.2f} MB, largest {biggest / 1e3:.0f} kB; report {report}")
