# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the ParaView side of the
# visual check (scripts/paraview/README.md). Run with `pvpython` of the
# docker/paraview image, once per example:
#
#   pvpython pv_example.py --example NAME --port DIR --oracle DIR|- --out DIR
#                          [--title TEXT] [--views disp,stress,...]
#
# 1. loads EVERY ParaView file (.pvd/.pvtu/.vtu) below the run directories of both runs (.pvd with
#    all its time steps, every .pvtu, every .vtu piece), records errors and
#    warnings (the VTK output window is redirected to a file; a message is
#    a failure) and the arrays with their ranges;
# 2. compares the two sides (file names, SHA-256, and for files that differ,
#    the largest difference of every array read through ParaView);
# 3. renders the side-by-side pictures (port left, upstream right, same camera,
#    same colour range, legend, title) at the first, a middle and the last dump.
#
# Writes <out>/result.json, <out>/messages.txt and the PNGs <out>/*.png.
# Nothing is "fixed" or smoothed over: every difference is reported as found.
import argparse
import hashlib
import json
import math
import os
import re
import sys
import time
import traceback

import numpy as np
from vtkmodules.vtkCommonCore import vtkFileOutputWindow, vtkOutputWindow

from paraview import simple as pvs

# pvpython's start-up namespace (and builtins) shadow sum/min/max/... with
# numpy-interface reductions that fail on plain Python values: own helpers
def _sum(it):
    t = 0
    for x in it:
        t += x
    return t


def _min(*a):
    v = list(a[0]) if len(a) == 1 else list(a)
    r = v[0]
    for x in v[1:]:
        if x < r:
            r = x
    return r


def _max(*a):
    v = list(a[0]) if len(a) == 1 else list(a)
    r = v[0]
    for x in v[1:]:
        if x > r:
            r = x
    return r


def _any(it):
    for x in it:
        if x:
            return True
    return False


def _abs(x):
    return -x if x < 0 else x


def _round(x, n):
    return float("%.*f" % (n, x))

ap = argparse.ArgumentParser()
ap.add_argument("--example", required=True)
ap.add_argument("--port", required=True)
ap.add_argument("--oracle", default="-")
ap.add_argument("--out", required=True)
ap.add_argument("--title", default="")
ap.add_argument("--views", default="")
ap.add_argument("--steps", default="first,middle,last")
ap.add_argument("--max-diff-files", type=int, default=12)
ap.add_argument("--no-render", action="store_true")
args = ap.parse_args()
os.makedirs(args.out, exist_ok=True)

# ---------------------------------------------------------------- messages
MSG = os.path.join(args.out, "messages.txt")
open(MSG, "w").close()
_ow = vtkFileOutputWindow()
_ow.SetFileName(MSG)
_ow.SetFlush(1)
_ow.SetAppend(1)
vtkOutputWindow.SetInstance(_ow)


LOG = open(os.path.join(args.out, "log.txt"), "w")


def plog(msg):
    # pvpython sends print() to the VTK output window; keep messages.txt VTK-only
    LOG.write(msg + "\n")
    LOG.flush()


def message_count():
    with open(MSG) as f:
        return _sum(1 for line in f if line.strip())


# ----------------------------------------------------------- file discovery
NUM = re.compile(r"^(?P<stem>.*?)_(?P<step>\d+)(?P<proc>\.proc\d+)?$")


def discover(root):
    pv = root
    found = {"pvd": [], "pvtu": [], "vtu": []}
    if not os.path.isdir(pv):
        return found
    for dp, _dn, fn in os.walk(pv):
        for f in fn:
            ext = f.rsplit(".", 1)[-1]
            if ext in found:
                found[ext].append(os.path.relpath(os.path.join(dp, f), pv))
    for k in found:
        found[k].sort()
    return found


def pieces_of(pv, pvtu):
    txt = open(os.path.join(pv, pvtu)).read()
    base = os.path.dirname(pvtu)
    return [os.path.normpath(os.path.join(base, s)) for s in re.findall(r'<Piece\s+Source="([^"]+)"', txt)]


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def series_of(files):
    """Primary dump files (.pvtu, plus .vtu not referenced by a .pvtu) grouped
    into series by (directory, stem); each series is a list of (step, file)."""
    series = {}
    for rel in files:
        stem = rel.rsplit(".", 1)[0]
        m = NUM.match(os.path.basename(stem))
        key = (os.path.dirname(rel), m.group("stem") if m else os.path.basename(stem))
        step = int(m.group("step")) if m else 0
        series.setdefault(key, []).append((step, rel))
    for k in series:
        series[k].sort()
    return series


# ------------------------------------------------------------ loading
def array_table(src):
    """{assoc/name: {ncomp, components: [[min,max],...], magnitude: [min,max]}}"""
    t = {}
    for assoc, coll in (("point", src.PointData), ("cell", src.CellData), ("field", src.FieldData)):
        for name in coll.keys():
            a = coll[name]
            nc = a.GetNumberOfComponents()
            comps = [list(a.GetRange(i)) for i in range(nc)]
            e = {"ncomp": nc, "components": comps}
            if nc > 1:
                e["magnitude"] = list(a.GetRange(-1))
            t[f"{assoc}/{name}"] = e
    return t


def load_one(path):
    """Open one file in ParaView; returns (info, source). Exceptions are
    caught by the caller and recorded."""
    src = pvs.OpenDataFile(path)
    if src is None:
        raise RuntimeError("OpenDataFile returned None")
    src.UpdatePipeline()
    di = src.GetDataInformation()
    info = {
        "points": di.GetNumberOfPoints(),
        "cells": di.GetNumberOfCells(),
        "bounds": list(di.GetBounds()),
        "arrays": array_table(src),
    }
    return info, src


def load_everything(root, label):
    """Open every file of one run; returns {rel: record} and the series."""
    pv = root
    found = discover(root)
    res = {"files": {}, "pvd": {}, "n_files": {k: len(v) for k, v in found.items()}}
    referenced = set()
    for p in found["pvtu"]:
        try:
            for piece in pieces_of(pv, p):
                referenced.add(piece)
        except Exception:
            pass
    t0 = time.time()
    for rel in found["pvtu"] + found["vtu"]:
        before = message_count()
        rec = {"sha256": sha256(os.path.join(pv, rel)), "size": os.path.getsize(os.path.join(pv, rel))}
        try:
            info, src = load_one(os.path.join(pv, rel))
            rec.update(info)
            pvs.Delete(src)
            del src
        except Exception as e:  # recorded, never swallowed silently
            rec["error"] = f"{type(e).__name__}: {e}"
        rec["messages"] = message_count() - before
        rec["piece_of_pvtu"] = rel in referenced
        res["files"][rel] = rec
    for rel in found["pvd"]:
        before = message_count()
        rec = {"sha256": sha256(os.path.join(pv, rel)), "steps": []}
        try:
            src = pvs.OpenDataFile(os.path.join(pv, rel))
            times = list(src.TimestepValues) if hasattr(src, "TimestepValues") else []
            if not isinstance(times, list):
                times = [times]
            rec["n_steps"] = len(times)
            rec["time_first_last"] = [times[0], times[-1]] if times else None
            for t in times:
                src.UpdatePipeline(t)
                di = src.GetDataInformation()
                rec["steps"].append(
                    {"t": t, "points": di.GetNumberOfPoints(), "cells": di.GetNumberOfCells(), "arrays": array_table(src)}
                )
            rec["times"] = times
            pvs.Delete(src)
        except Exception as e:
            rec["error"] = f"{type(e).__name__}: {e}"
        rec["messages"] = message_count() - before
        res["pvd"][rel] = rec
    res["load_seconds"] = _round(time.time() - t0, 1)
    res["found"] = found
    plog(f"[{label}] loaded {_sum(len(v) for v in found.values())} files in {res['load_seconds']} s")
    return res


def summarize_arrays(res):
    """Per array: union over all primary files of ncomp and the ranges."""
    summ = {}
    for rel, rec in res["files"].items():
        if rec.get("piece_of_pvtu") or "arrays" not in rec:
            continue
        for key, a in rec["arrays"].items():
            s = summ.setdefault(key, {"ncomp": a["ncomp"], "components": [list(c) for c in a["components"]], "files": 0})
            s["files"] += 1
            for i, c in enumerate(a["components"]):
                if i < len(s["components"]):
                    s["components"][i][0] = _min(s["components"][i][0], c[0])
                    s["components"][i][1] = _max(s["components"][i][1], c[1])
    return summ


# ------------------------------------------------------------ comparison
def fetch_arrays(path):
    from paraview import servermanager as sm
    from vtkmodules.numpy_interface import dataset_adapter as dsa

    src = pvs.OpenDataFile(path)
    src.UpdatePipeline()
    d = dsa.WrapDataObject(sm.Fetch(src))
    out = {"points": np.asarray(d.Points)}
    for assoc, coll in (("point", d.PointData), ("cell", d.CellData)):
        for k in coll.keys():
            out[f"{assoc}/{k}"] = np.asarray(coll[k])
    pvs.Delete(src)
    return out


def compare_sides(rp, ro, root_p, root_o):
    pv_p, pv_o = root_p, root_o
    fp, fo = set(rp["files"]) | set(rp["pvd"]), set(ro["files"]) | set(ro["pvd"])
    cmp = {
        "only_in_port": sorted(fp - fo),
        "only_in_upstream": sorted(fo - fp),
        "common": len(fp & fo),
        "identical_bytes": 0,
        "different": [],
    }
    for rel in sorted(fp & fo):
        a = (rp["files"].get(rel) or rp["pvd"].get(rel))["sha256"]
        b = (ro["files"].get(rel) or ro["pvd"].get(rel))["sha256"]
        if a == b:
            cmp["identical_bytes"] += 1
        else:
            cmp["different"].append(rel)
    detail = []
    diffs = cmp["different"]
    if len(diffs) > args.max_diff_files:  # first, last and an even sample
        idx = sorted(set(np.linspace(0, len(diffs) - 1, args.max_diff_files).astype(int)))
        pick = [diffs[i] for i in idx]
    else:
        pick = diffs
    for rel in pick:
        if not rel.endswith((".vtu", ".pvtu")):
            detail.append({"file": rel, "note": "not a data file (e.g. .pvd): bytes differ"})
            continue
        try:
            A, B = fetch_arrays(os.path.join(pv_p, rel)), fetch_arrays(os.path.join(pv_o, rel))
        except Exception as e:
            detail.append({"file": rel, "error": f"{type(e).__name__}: {e}"})
            continue
        dd = {"file": rel, "arrays": {}}
        for k in sorted(set(A) | set(B)):
            if k not in A or k not in B:
                dd["arrays"][k] = "present on one side only"
            elif A[k].shape != B[k].shape:
                dd["arrays"][k] = f"shape {A[k].shape} vs {B[k].shape}"
            else:
                x, y = A[k].astype(float), B[k].astype(float)
                both_nan = np.isnan(x) & np.isnan(y)
                diff = np.where(both_nan, 0.0, np.abs(x - y))
                mx = float(diff.max()) if diff.size else 0.0
                ref = float(np.nanmax(np.abs(y))) if y.size else 0.0
                dd["arrays"][k] = {"max_abs_diff": mx, "max_abs_value_upstream": ref}
        detail.append(dd)
    cmp["detail"] = detail
    cmp["detail_sampled_of"] = len(diffs)
    return cmp


# ------------------------------------------------------------ rendering
PROG = r"""
import numpy as np
from vtkmodules.vtkFiltersCore import vtkRemoveUnusedPoints
inp0 = inputs[0]
# points that no cell references (e.g. the pure-ghost points of a distributed
# dump, UD-475) are left out of the picture: they carry meaningless values and
# would stretch the camera and the colour scale; their number is recorded
rm = vtkRemoveUnusedPoints()
rm.SetInputData(inp0.VTKObject)
rm.Update()
out = output
out.ShallowCopy(rm.GetOutput())
dropped = int(inp0.GetNumberOfPoints() - out.GetNumberOfPoints())
out.FieldData.append(np.array([dropped]), 'unused_points')
pts = np.asarray(out.Points)
planar = bool(pts.shape[0] > 0 and np.ptp(pts[:, 2]) == 0.0)
pd = out.PointData
if 'displacement' in pd.keys() and out.GetNumberOfPoints() > 0:
    u = np.asarray(pd['displacement']); u = u.reshape(len(u), -1)
    u3 = np.zeros((u.shape[0], 3)); k = 3 if u.shape[1] > 3 else u.shape[1]; u3[:, :k] = u[:, :k]
    out.PointData.append(u3, 'u3'); out.PointData.append(np.linalg.norm(u3, axis=1), 'umag')
for assoc in ('CellData', 'PointData'):
    data = getattr(out, assoc)
    if 'stress' in data.keys() and out.GetNumberOfCells() > 0:
        s = np.asarray(data['stress']); s = s.reshape(len(s), -1)
        if s.shape[1] == 9 and planar:
            s = s[:, [0, 1, 3, 4]]  # a plane problem: the out-of-plane entries of the padded 3 x 3 are not stresses (UD-111)
        if s.shape[1] == 4:
            sxx, syy, sxy = s[:, 0], s[:, 3], 0.5 * (s[:, 1] + s[:, 2])
            vm = np.sqrt(np.maximum(sxx ** 2 - sxx * syy + syy ** 2 + 3.0 * sxy ** 2, 0.0))
        elif s.shape[1] == 9:
            sxx, syy, szz = s[:, 0], s[:, 4], s[:, 8]
            sxy, syz, sxz = 0.5 * (s[:, 1] + s[:, 3]), 0.5 * (s[:, 5] + s[:, 7]), 0.5 * (s[:, 2] + s[:, 6])
            vm = np.sqrt(np.maximum(0.5 * ((sxx - syy) ** 2 + (syy - szz) ** 2 + (szz - sxx) ** 2) + 3.0 * (sxy ** 2 + syz ** 2 + sxz ** 2), 0.0))
        else:
            continue
        getattr(out, assoc).append(vm, 'von_mises')
"""

SIDE_NAME = {"port": "akantu-rs (Rust port)", "upstream": "Akantu C++ (upstream)"}
LABEL = {
    "disp": ("umag", "POINTS", "displacement magnitude [m]"),
    "stress": ("von_mises", "CELLS", "von Mises stress [Pa]"),
    "damage": ("damage", None, "damage [0..1]"),
    "temperature": ("temperature", "POINTS", "temperature [K]"),
}


def field_range(infos, assoc_name):
    lo, hi = math.inf, -math.inf
    for rg in infos:
        if rg is None:
            continue
        lo, hi = _min(lo, rg[0]), _max(hi, rg[1])
    return (lo, hi) if lo <= hi else (0.0, 1.0)


_CHAINS = {}


def drop(w):
    """Delete a pipeline made by pipeline(), last filter first."""
    for x in reversed(_CHAINS.pop(id(w))):
        pvs.Delete(x)


def pipeline(path, scale, warp=True, clip=False):
    r = pvs.OpenDataFile(path)
    pf = pvs.ProgrammableFilter(Input=r)
    pf.Script = PROG
    pf.UpdatePipeline()
    chain = [r, pf]
    if not warp or "u3" not in pf.PointData.keys():
        w = pf
    else:
        w = pvs.WarpByVector(Input=pf)
        w.Vectors = ["POINTS", "u3"]
        w.ScaleFactor = float(scale)
        chain.append(w)
    if clip:  # a 3-D body cut in half through its middle (y >= centre kept), to show the inside
        b = w.GetDataInformation().GetBounds()
        cl = pvs.Clip(Input=w)
        cl.ClipType = "Plane"
        cl.ClipType.Origin = [(b[0] + b[1]) / 2, (b[2] + b[3]) / 2, (b[4] + b[5]) / 2]
        cl.ClipType.Normal = [0.0, 1.0, 0.0]
        cl.Invert = 0
        cl.Crinkleclip = 0
        chain.append(cl)
        _CHAINS[id(cl)] = chain
        return r, pf, cl
    _CHAINS[id(w)] = chain
    return r, pf, w


def data_range(src, assoc, name):
    src.UpdatePipeline()
    coll = src.PointData if assoc == "POINTS" else src.CellData
    if name not in coll.keys():
        return None
    a = coll[name]
    return list(a.GetRange(0)) if a.GetNumberOfComponents() == 1 else list(a.GetRange(-1))


def assoc_of(src, name):
    src.UpdatePipeline()
    if name in src.PointData.keys():
        return "POINTS"
    if name in src.CellData.keys():
        return "CELLS"
    return None


_VIEW = {}


def the_view():
    """One render view for the whole run (several views and layouts made the
    OSMesa build of ParaView 5.13.3 abort with a corrupted heap)."""
    if "v" not in _VIEW:
        v = pvs.CreateView("RenderView")
        v.ViewSize = [500, 500]
        v.OrientationAxesVisibility = 0
        try:
            v.UseColorPaletteForBackground = 0
        except Exception:
            pass
        v.Background = [1.0, 1.0, 1.0]
        pvs.SetActiveView(v)
        # ParaView resets the camera on the first render of a view: do that on a
        # throw-away sphere so that every picture gets the camera set for it
        sp = pvs.Sphere()
        pvs.Show(sp, v)
        pvs.Render(v)
        pvs.Delete(sp)
        _VIEW["v"] = v
    return _VIEW["v"]


def quantize256(rgb):
    """(palette, index image, exact) of an RGB image: exact when it has at most
    256 colours, else the frequent colours are kept and the rest is median-cut."""
    h, w, _ = rgb.shape
    flat = rgb.reshape(-1, 3)
    key = (flat[:, 0].astype(np.int32) << 16) | (flat[:, 1].astype(np.int32) << 8) | flat[:, 2].astype(np.int32)
    uniq, inv, cnt = np.unique(key, return_inverse=True, return_counts=True)
    cols = np.stack([(uniq >> 16) & 255, (uniq >> 8) & 255, uniq & 255], axis=1).astype(np.float64)
    if len(uniq) <= 256:
        return cols.astype(np.uint8), inv.reshape(h, w).astype(np.uint8), True
    total = flat.shape[0]
    freq = np.nonzero(cnt > total * 0.002)[0]
    if len(freq) > 96:
        freq = np.argsort(-cnt)[:96]
    rest = np.setdiff1d(np.arange(len(uniq)), freq)
    boxes = [rest]
    while len(boxes) < 256 - len(freq):
        ext = [(cols[b_].max(axis=0) - cols[b_].min(axis=0)).max() if len(b_) > 1 else -1 for b_ in boxes]
        i = int(np.argmax(ext))
        if ext[i] <= 0:
            break
        b_ = boxes.pop(i)
        ch = int(np.argmax(cols[b_].max(axis=0) - cols[b_].min(axis=0)))
        order = b_[np.argsort(cols[b_][:, ch])]
        cs = np.cumsum(cnt[order])
        k = int(np.searchsorted(cs, cs[-1] / 2)) + 1
        k = _min(_max(k, 1), len(order) - 1)
        boxes += [order[:k], order[k:]]
    lut = np.zeros(len(uniq), np.uint8)
    pal = np.zeros((256, 3), np.uint8)
    for j, f in enumerate(freq):
        lut[f] = j
        pal[j] = cols[f]
    for t, b_ in enumerate(boxes):
        lut[b_] = len(freq) + t
        pal[len(freq) + t] = np.round((cols[b_] * cnt[b_][:, None]).sum(axis=0) / cnt[b_].sum())
    return pal, lut[inv].reshape(h, w), False


def write_indexed_png(path, idx, pal):
    """8-bit indexed PNG (no text chunks: nothing but the pixels)."""
    import struct
    import zlib

    def chunk(t, data):
        c = struct.pack(">I", len(data)) + t + data
        return c + struct.pack(">I", zlib.crc32(t + data) & 0xFFFFFFFF)

    h, w = idx.shape
    raw = b"".join(b"\x00" + idx[y].tobytes() for y in range(h))
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 3, 0, 0, 0)))
        f.write(chunk(b"PLTE", pal.tobytes()))
        f.write(chunk(b"IDAT", zlib.compress(raw, 9)))
        f.write(chunk(b"IEND", b""))


def compose(left, right, png):
    """Put two PNGs side by side (port left, upstream right) with VTK."""
    from vtkmodules.util.numpy_support import numpy_to_vtk, vtk_to_numpy
    from vtkmodules.vtkIOImage import vtkPNGReader, vtkPNGWriter

    arrs = []
    for f in (left, right):
        rd = vtkPNGReader()
        rd.SetFileName(f)
        rd.Update()
        im = rd.GetOutput()
        w, h, _ = im.GetDimensions()
        nc = im.GetNumberOfScalarComponents()
        arrs.append(vtk_to_numpy(im.GetPointData().GetScalars()).reshape(h, w, nc))
    # pixel comparison of the two halves except the top 75 rows (title text) (the title text
    # differs by design): number of differing pixels and the largest channel difference
    a, b = arrs[0][:-75].astype(np.int16), arrs[1][:-75].astype(np.int16)  # VTK rows run bottom-up: the title is the last 75 rows
    dmax = int(np.abs(a - b).max()) if a.size else 0
    dpix = int((np.abs(a - b).max(axis=2) > 0).sum()) if a.size else 0
    both = np.ascontiguousarray(np.concatenate(arrs, axis=1))
    h, w, nc = both.shape
    # repository-sized files: an indexed-colour PNG of at most 256 colours (the frequent colours
    # of the picture exactly, the others by median cut)
    rgb = np.ascontiguousarray(both[::-1, :, :3])  # VTK rows run bottom-up
    pal, idx, exact = quantize256(rgb)
    write_indexed_png(png, idx, pal)
    mask = 0xFF if exact else 0x00
    return {"colours_exact": bool(mask), "pixels_differing": dpix, "max_channel_difference": dmax, "pixels_compared": int(a.shape[0] * a.shape[1])}


def render_pair(png, kind, path_p, path_o, title_lines, scale, three_d):
    """One side-by-side picture; returns a small dict for the manifest."""
    name = LABEL[kind][0]
    sides = [("port", path_p), ("upstream", path_o)]
    pls = []
    for side, path in sides:
        if path is None:
            continue
        pls.append((side,) + pipeline(path, scale, warp=(kind != "temperature"), clip=(three_d and kind == "temperature")))
    assoc = None
    ranges = []
    missing = []
    keep = []
    for side, r, pf, w in pls:
        a = assoc_of(w, name)
        if a is None:  # this side has no such array (or ParaView could not read it): left out, and said so
            missing.append(side)
            drop(w)
            continue
        assoc = a
        ranges.append(data_range(w, a, name))
        keep.append((side, r, pf, w))
    pls = keep
    if not pls:
        return {"skipped": f"array {name} missing on side(s) {', '.join(missing)}"}
    # one colour scale for both panels: the port's range (the port is what is checked); upstream
    # values outside it are shown in the end colours, and the picture's caption says so
    by_side = {pl[0]: rg for pl, rg in zip(pls, ranges)}
    lo, hi = field_range([by_side.get("port", ranges[0])], name)
    up = by_side.get("upstream")
    exceeds = bool(up is not None and "port" in by_side and (up[1] > hi * (1 + 1e-9) + 1e-300 or up[0] < lo - 1e-300 - abs(lo) * 1e-9))
    if kind in ("disp", "stress"):  # magnitudes: the scale starts at 0 (a uniform field shows as uniform)
        lo = 0.0
    if kind == "damage":
        lo, hi = 0.0, _max(hi, 1.0)
    if hi - lo < 1e-300:
        hi = lo + 1.0 if lo == 0.0 else lo + _abs(lo) * 1e-9
    lut = pvs.GetColorTransferFunction(name)
    lut.ApplyPreset("Turbo", True)
    lut.AutomaticRescaleRangeMode = "Never"
    lut.RescaleTransferFunction(lo, hi)
    pvs.GetOpacityTransferFunction(name).RescaleTransferFunction(lo, hi)
    # union bounds of the (warped) data for one common camera
    bs = [w.GetDataInformation().GetBounds() for _s, _r, _pf, w in pls]
    b = [_min(x[0] for x in bs), _max(x[1] for x in bs), _min(x[2] for x in bs), _max(x[3] for x in bs), _min(x[4] for x in bs), _max(x[5] for x in bs)]
    c = [(b[0] + b[1]) / 2, (b[2] + b[3]) / 2, (b[4] + b[5]) / 2]
    ext = [b[1] - b[0], b[3] - b[2], b[5] - b[4]]
    diag = math.sqrt(_sum(e * e for e in ext)) or 1.0
    v = the_view()
    tmp = []
    dropped_pts = {}
    for side, r, pf, w in pls:
        d = pvs.Show(w, v)
        pvs.ColorBy(d, (assoc, name))
        d.LookupTable = lut
        lut.AutomaticRescaleRangeMode = "Never"  # ColorBy may have rescaled to the data range
        lut.RescaleTransferFunction(lo, hi)
        n_cells = w.GetDataInformation().GetNumberOfCells()
        d.SetRepresentationType("Surface With Edges" if n_cells < 3000 else "Surface")
        d.EdgeColor = [0.25, 0.25, 0.25]
        d.SetScalarBarVisibility(v, True)
        bar = pvs.GetScalarBar(lut, v)
        bar.Title = LABEL[kind][2]
        bar.ComponentTitle = ""
        bar.TitleColor = [0, 0, 0]
        bar.LabelColor = [0, 0, 0]
        bar.TitleFontSize = 11
        bar.LabelFontSize = 10
        bar.WindowLocation = "Any Location"
        bar.Orientation = "Horizontal"
        bar.Position = [0.2, 0.03]
        bar.ScalarBarLength = 0.6
        bar.RangeLabelFormat = "%-#6.3g"
        txt = pvs.Text(Text="\n".join([SIDE_NAME[side]] + title_lines))
        td = pvs.Show(txt, v)
        td.Color = [0, 0, 0]
        td.FontSize = 11
        td.WindowLocation = "Upper Left Corner"
        if three_d:
            dv = np.array([0.6, -0.8, 0.5])
            dv /= np.linalg.norm(dv)
            v.CameraParallelProjection = 0
            v.CameraFocalPoint = c
            v.CameraViewUp = [0, 0, 1]
            v.CameraPosition = list(np.array(c) + dv * diag * 2.6)
            v.CameraViewAngle = 30
        else:
            v.CameraParallelProjection = 1
            v.CameraFocalPoint = c
            v.CameraPosition = [c[0], c[1], c[2] + diag * 3 + 1.0]
            v.CameraViewUp = [0, 1, 0]
            v.CameraParallelScale = 0.68 * _max(ext[0], ext[1], 1e-30)
        pvs.Render(v)
        f = f"{png}.{side}.tmp.png"
        pvs.SaveScreenshot(f, v, ImageResolution=[500, 500])
        tmp.append(f)
        d.SetScalarBarVisibility(v, False)
        pvs.Hide(w, v)
        pvs.Delete(txt)
        dropped = pf.FieldData["unused_points"].GetRange(0)[1] if "unused_points" in pf.FieldData.keys() else 0
        dropped_pts[side] = int(dropped)
    pix = None
    if len(tmp) == 2:
        pix = compose(tmp[0], tmp[1], png)
    else:  # port only: the picture is the left half (500 x 500)
        os.replace(tmp[0], png)
    for f in tmp:
        if os.path.exists(f):
            os.remove(f)
    out = {"png": os.path.basename(png), "range": [lo, hi], "assoc": assoc, "unused_points_left_out": dropped_pts, "pixel_comparison": pix, "sides_without_the_array": missing, "data_range_by_side": by_side, "upstream_outside_scale": exceeds, "cut_in_half": bool(three_d and kind == "temperature")}
    for side, r, pf, w in pls:
        drop(w)
    return out


def pick_series(rec_files, found):
    """Primary series: the one with the most cells in its last file (the bulk
    body), then the most files. Returns (best, all series)."""
    prim = [f for f, r in rec_files.items() if not r.get("piece_of_pvtu") and "arrays" in r]
    ser = series_of(prim)
    best, score = None, (-1, -1, -1)
    for key, lst in ser.items():
        last = rec_files[lst[-1][1]]
        has = _any(k in last["arrays"] for k in ("point/displacement", "point/temperature", "cell/damage", "point/damage"))
        s = (last.get("cells", 0), 1 if has else 0, len(lst))
        if s > score:
            best, score = (key, lst), s
    return best, ser


def cohesive_series(rec_files, ser, primary_key):
    """A second series of the same directory whose last file has cohesive
    elements with a damage array (the cohesive-element dumper of the Python
    examples); None if there is none."""
    for key, lst in sorted(ser.items()):
        if key == primary_key or key[0] != primary_key[0]:
            continue
        last = rec_files[lst[-1][1]]
        if "cell/damage" in last["arrays"] and last.get("cells", 0) > 0:
            return key, lst
    return None


def render_cohesive(png, bulk, coh, title_lines, scale, three_d):
    """Bulk body (grey) with its cohesive elements as lines coloured by damage;
    bulk = (path_port, path_upstream), coh likewise (a path may be None)."""
    chains = []
    for i, side in enumerate(("port", "upstream")):
        if bulk[i] is None or coh[i] is None:
            continue
        rb, pfb, wb = pipeline(bulk[i], scale, warp=True)
        rc, pfc, wc = pipeline(coh[i], scale, warp=True)
        chains.append((side, wb, wc))
    out = {"png": os.path.basename(png), "pixel_comparison": None}
    keep, ranges = [], []
    for side, wb, wc in chains:
        rg = data_range(wc, "CELLS", "damage")
        if rg is None:  # no cohesive element with a damage array in this dump
            drop(wc)
            drop(wb)
            continue
        ranges.append(rg)
        keep.append((side, wb, wc))
    if not keep:
        return {"skipped": "no cohesive elements with a damage array in this dump"}
    lo, hi = 0.0, _max(1.0, _max(r[1] for r in ranges))
    lut = pvs.GetColorTransferFunction("damage")
    lut.ApplyPreset("Turbo", True)
    lut.AutomaticRescaleRangeMode = "Never"
    lut.RescaleTransferFunction(lo, hi)
    pvs.GetOpacityTransferFunction("damage").RescaleTransferFunction(lo, hi)
    for _s, wb, _wc in keep:
        wb.UpdatePipeline()
    bs = [wb.GetDataInformation().GetBounds() for _s, wb, _wc in keep]
    b = [_min(x[0] for x in bs), _max(x[1] for x in bs), _min(x[2] for x in bs), _max(x[3] for x in bs), _min(x[4] for x in bs), _max(x[5] for x in bs)]
    c = [(b[0] + b[1]) / 2, (b[2] + b[3]) / 2, (b[4] + b[5]) / 2]
    ext = [b[1] - b[0], b[3] - b[2], b[5] - b[4]]
    diag = math.sqrt(_sum(e * e for e in ext)) or 1.0
    v = the_view()
    tmp = []
    for side, wb, wc in keep:
        db = pvs.Show(wb, v)
        db.ColorArrayName = ["POINTS", ""]  # plain grey
        db.AmbientColor = db.DiffuseColor = [0.86, 0.86, 0.86]
        db.SetRepresentationType("Surface With Edges" if wb.GetDataInformation().GetNumberOfCells() < 3000 else "Surface")
        db.EdgeColor = [0.6, 0.6, 0.6]
        dc = pvs.Show(wc, v)
        dc.SetRepresentationType("Wireframe")
        pvs.ColorBy(dc, ("CELLS", "damage"))
        dc.LookupTable = lut
        lut.AutomaticRescaleRangeMode = "Never"
        lut.RescaleTransferFunction(lo, hi)
        dc.LineWidth = 4.0
        dc.SetScalarBarVisibility(v, True)
        bar = pvs.GetScalarBar(lut, v)
        bar.Title = "damage of the cohesive elements (0..1)"
        bar.ComponentTitle = ""
        bar.TitleColor = [0, 0, 0]
        bar.LabelColor = [0, 0, 0]
        bar.TitleFontSize = 11
        bar.LabelFontSize = 10
        bar.WindowLocation = "Any Location"
        bar.Orientation = "Horizontal"
        bar.Position = [0.2, 0.03]
        bar.ScalarBarLength = 0.6
        bar.RangeLabelFormat = "%-#6.3g"
        txt = pvs.Text(Text="\n".join([SIDE_NAME[side]] + title_lines))
        td = pvs.Show(txt, v)
        td.Color = [0, 0, 0]
        td.FontSize = 11
        td.WindowLocation = "Upper Left Corner"
        v.CameraParallelProjection = 1
        v.CameraFocalPoint = c
        v.CameraPosition = [c[0], c[1], c[2] + diag * 3 + 1.0]
        v.CameraViewUp = [0, 1, 0]
        v.CameraParallelScale = 0.68 * _max(ext[0], ext[1], 1e-30)
        pvs.Render(v)
        f = f"{png}.{side}.tmp.png"
        pvs.SaveScreenshot(f, v, ImageResolution=[500, 500])
        tmp.append(f)
        dc.SetScalarBarVisibility(v, False)
        pvs.Hide(wb, v)
        pvs.Hide(wc, v)
        pvs.Delete(txt)
    if len(tmp) == 2:
        out["pixel_comparison"] = compose(tmp[0], tmp[1], png)
    else:
        os.replace(tmp[0], png)
    for f in tmp:
        if os.path.exists(f):
            os.remove(f)
    out.update({"range": [lo, hi], "assoc": "CELLS", "unused_points_left_out": {}, "sides_without_the_array": [], "cut_in_half": False})
    for side, wb, wc in keep:
        drop(wc)
        drop(wb)
    return out


def main():
    t_start = time.time()
    res = {"example": args.example, "paraview_version": None, "sides": {}}
    import paraview

    res["paraview_version"] = paraview.__version__
    root = {"port": args.port, "upstream": None if args.oracle == "-" else args.oracle}
    loaded = {}
    for side, r in root.items():
        if r is None:
            continue
        loaded[side] = load_everything(r, side)
        loaded[side]["array_summary"] = summarize_arrays(loaded[side])
        res["sides"][side] = {
            "n_files": loaded[side]["n_files"],
            "load_seconds": loaded[side]["load_seconds"],
            "errors": [f for f, rec in list(loaded[side]["files"].items()) + list(loaded[side]["pvd"].items()) if "error" in rec],
            "files_with_messages": [f for f, rec in list(loaded[side]["files"].items()) + list(loaded[side]["pvd"].items()) if rec.get("messages")],
            "array_summary": loaded[side]["array_summary"],
            "pvd": {k: {kk: vv for kk, vv in v.items() if kk not in ("steps",)} for k, v in loaded[side]["pvd"].items()},
            "pvd_array_summary": {},
        }
        for k, v in loaded[side]["pvd"].items():
            merged = {}
            for st in v.get("steps", []):
                for key, a in st["arrays"].items():
                    m = merged.setdefault(key, {"ncomp": a["ncomp"], "components": [list(c) for c in a["components"]]})
                    for i, c in enumerate(a["components"]):
                        m["components"][i][0] = _min(m["components"][i][0], c[0])
                        m["components"][i][1] = _max(m["components"][i][1], c[1])
            res["sides"][side]["pvd_array_summary"][k] = merged
        # compact per-file table (kept for the data file)
        res["sides"][side]["files"] = {
            f: {k: v for k, v in rec.items() if k in ("sha256", "points", "cells", "messages", "error", "piece_of_pvtu", "arrays", "bounds")}
            for f, rec in loaded[side]["files"].items()
        }
    if "upstream" in loaded:
        res["comparison"] = compare_sides(loaded["port"], loaded["upstream"], args.port, args.oracle)
    res["vtk_messages_loading"] = message_count()  # before any rendering: what ParaView said about the files
    res["vtk_messages_total"] = message_count()

    renders = []
    if not args.no_render:
        try:
            # the port's series is the reference; when the port wrote no ParaView
            # file (scalability_test) the upstream series is rendered alone
            base = "port" if loaded["port"]["files"] else "upstream"
            res["render_base_side"] = base
            best, ser = pick_series(loaded[base]["files"], loaded[base]["found"])
            res["primary_series"] = None
            if best:
                key, lst = best
                res["primary_series"] = {"dir": key[0], "stem": key[1], "n": len(lst), "steps": [s for s, _ in lst]}
                idx = {"first": 0, "middle": len(lst) // 2, "last": len(lst) - 1}
                names = {}
                for nm in ("first", "middle", "last"):  # a later name wins when indices coincide
                    if nm in args.steps.split(","):
                        names[idx[nm]] = nm
                chosen = sorted(names.items())
                pv_p = args.port
                pv_o = args.oracle if root["upstream"] else None
                # series of the other side: same relative names
                arrays0 = loaded[base]["files"][lst[-1][1]]["arrays"]
                bb0 = loaded[base]["files"][lst[-1][1]]["bounds"]
                three_d = bb0[5] - bb0[4] > 1e-12 * _max(1e-300, bb0[1] - bb0[0])
                wanted = [v for v in args.views.split(",") if v] or [
                    k for k, ok in (("disp", "point/displacement" in arrays0), ("stress", "cell/stress" in arrays0 or "point/stress" in arrays0), ("damage", "cell/damage" in arrays0 or "point/damage" in arrays0), ("temperature", "point/temperature" in arrays0)) if ok
                ]
                # warp scale: 10 % of the model diagonal over the largest displacement of the shown steps
                maxu, diag = 0.0, 0.0
                for i, _ in chosen:
                    for sd in loaded:
                        rel = lst[i][1]
                        rec = loaded[sd]["files"].get(rel)
                        if rec and "point/displacement" in rec["arrays"]:
                            a = rec["arrays"]["point/displacement"]
                            maxu = _max(maxu, a.get("magnitude", [0, 0])[1])
                        if rec:
                            bb = rec["bounds"]
                            diag = _max(diag, math.sqrt((bb[1] - bb[0]) ** 2 + (bb[3] - bb[2]) ** 2 + (bb[5] - bb[4]) ** 2))
                scale = (0.1 * diag / maxu) if maxu > 0 and diag > 0 else 1.0
                res["warp"] = {"scale_factor": scale, "max_displacement_shown": maxu, "model_diagonal": diag}
                nser = len(lst)
                for kind in wanted:
                    for i, nm in chosen:
                        step, rel = lst[i]
                        path_p = os.path.join(pv_p, rel) if os.path.exists(os.path.join(pv_p, rel)) else None
                        path_o = os.path.join(pv_o, rel) if pv_o and os.path.exists(os.path.join(pv_o, rel)) else None
                        tstr = ""
                        pvdrec = loaded[base]["pvd"].get(os.path.join(key[0], key[1] + ".pvd"))
                        if pvdrec and pvdrec.get("times") and len(pvdrec["times"]) == nser:
                            tstr = f", t = {pvdrec['times'][i]:.6g} s"
                        tl = [
                            f"{args.example}: {LABEL[kind][2].split(' [')[0]}",
                            f"dump {step} ({nm}, {i + 1} of {nser}){tstr}",
                        ]
                        if kind != "temperature":
                            tl.append(f"shape warped x{scale:.4g}")
                        png = os.path.join(args.out, f"{args.example}-{kind}-{nm}.png")
                        try:
                            info = render_pair(png, kind, path_p, path_o, tl, scale, three_d)
                        except Exception as e:
                            info = {"error": f"{type(e).__name__}: {e}", "trace": traceback.format_exc()[-1500:]}
                        info.update({"kind": kind, "step_name": nm, "dump": step, "index": i, "of": nser, "file": rel, "upstream_file_present": path_o is not None, "port_file_present": path_p is not None})
                        renders.append(info)
                coh = cohesive_series(loaded[base]["files"], ser, key)
                if coh:
                    ckey, clst = coh
                    res["cohesive_series"] = {"dir": ckey[0], "stem": ckey[1], "n": len(clst)}
                    cmap = dict(clst)
                    for i, nm in chosen:
                        step, rel = lst[i]
                        if step not in cmap:
                            continue
                        crel = cmap[step]
                        paths = [[os.path.join(d_, r_) if d_ and os.path.exists(os.path.join(d_, r_)) else None for d_ in (pv_p, pv_o)] for r_ in (rel, crel)]
                        tl = [f"{args.example}: cohesive elements", f"dump {step} ({nm}, {i + 1} of {nser}), shape warped x{scale:.4g}"]
                        png = os.path.join(args.out, f"{args.example}-cohesive-{nm}.png")
                        try:
                            info = render_cohesive(png, paths[0], paths[1], tl, scale, three_d)
                        except Exception as e:
                            info = {"error": f"{type(e).__name__}: {e}", "trace": traceback.format_exc()[-1500:]}
                        info.update({"kind": "cohesive", "step_name": nm, "dump": step, "index": i, "of": nser, "file": crel})
                        renders.append(info)
        except Exception as e:
            res["render_error"] = f"{type(e).__name__}: {e}"
            res["render_trace"] = traceback.format_exc()[-2000:]
    res["renders"] = renders
    res["vtk_messages_total"] = message_count()
    res["seconds"] = _round(time.time() - t_start, 1)
    with open(os.path.join(args.out, "result.json"), "w") as f:
        json.dump(res, f, indent=1, default=str)
    plog(json.dumps({k: res[k] for k in ("example", "vtk_messages_total", "seconds")}))


main()
