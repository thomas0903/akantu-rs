# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the raw side of the
# ParaView visual check (scripts/paraview/README.md).
# For every ParaView file that exists in both runs of an example and whose
# bytes differ, reads the two VTU files directly (XML + base64, as written:
# no ParaView in between) and says, array by array, HOW they differ:
#
#   same                 decoded payloads identical (the bytes differ elsewhere)
#   ud110-layout         upstream holds c*c values per tuple (each c-component
#                        tuple written as a zero-filled c x c square whose
#                        top-left m x m block is the matrix, m*m = c: IOHelper's
#                        ParaviewHelper::pushData for matrices); the port's c
#                        values equal that block bitwise
#   ud110-layout+ud111   the same, except the out-of-plane shear entries of a
#                        padded 2-D stress (block positions (0,2), (1,2),
#                        (2,0), (2,1)): upstream's are never written (the
#                        uninitialised Matrix of PadderGeneric::pad), the
#                        port's are zero
#   ud111                both in upstream's layout (the port's compat layout),
#                        equal except those four entries
#   short                upstream's array holds fewer values than the piece
#                        needs (points or cells x components); the prefix it
#                        holds is compared
#   long                 upstream's array holds more values than the piece needs
#   values               same layout, different values (largest difference)
#   mesh                 the two pieces have different numbers of points or
#                        cells (the runs produced different meshes, e.g. other
#                        cohesive insertions)
#   one-side             the array exists in one file only
#   ud114                upstream's base64 stream of an empty array is invalid
#                        (IOHelper's writer for payloads under 2 bytes), the
#                        port's is valid
#
# and records the base64 length header of every upstream array that is not
# the length of its payload (IOHelper's header counts its own 4 bytes,
# UD-114). Pure Python (no NumPy), run with `python3 -I`:
#
#   python3 -I scripts/paraview/vtu_diff.py <port-run-dir> <upstream-run-dir> <out.tsv>
#
# Writes one row per differing array (and one per file whose arrays are all
# the same) and prints a summary per (array, class) on standard output.
import base64
import hashlib
import os
import re
import struct
import sys

WIDTH = {"Float64": ("d", 8), "Float32": ("f", 4), "Int64": ("q", 8), "UInt64": ("Q", 8),
         "Int32": ("i", 4), "UInt32": ("I", 4), "Int16": ("h", 2), "UInt16": ("H", 2),
         "Int8": ("b", 1), "UInt8": ("B", 1)}
UD111_POS = ((0, 2), (1, 2), (2, 0), (2, 1))


def sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def files_below(root):
    out = []
    for dp, _dn, fn in os.walk(root):
        for f in fn:
            if f.endswith((".vtu", ".pvtu", ".pvd")):
                out.append(os.path.relpath(os.path.join(dp, f), root))
    return sorted(out)


def attr(tag, name, default=None):
    m = re.search(r'\b%s="([^"]*)"' % re.escape(name), tag)
    return m.group(1) if m else default


def parse_vtu(path):
    """{(section, name): record} and the piece's point / cell counts."""
    doc = open(path, encoding="latin-1").read()
    m = re.search(r'NumberOfPoints=\s*"\s*(\d+)"\s*NumberOfCells=\s*"\s*(\d+)"', doc)
    npts, ncells = (int(m.group(1)), int(m.group(2))) if m else (-1, -1)
    arrays = {}
    pos = 0
    while True:
        i = doc.find("<DataArray", pos)
        if i < 0:
            break
        j = doc.index(">", i)
        k = doc.index("</DataArray>", j)
        tag, body, pos = doc[i:j], doc[j + 1:k], k
        before = doc[:i]
        section = max(("Points", "Cells", "PointData", "CellData"), key=lambda t: before.rfind("<%s>" % t))
        name, ty = attr(tag, "Name", ""), attr(tag, "type")
        comps = int(attr(tag, "NumberOfComponents", "1"))
        rec = {"type": ty, "ncomp": comps, "header": None, "error": None}
        if attr(tag, "format", "ascii") == "ascii":
            toks = body.split()
            conv = float if ty.startswith("Float") else int
            rec["values"] = [conv(t) for t in toks]
            rec["raw"] = " ".join(toks).encode()
        else:
            code, w = WIDTH[ty]
            try:
                raw = base64.b64decode("".join(body.split()), validate=True)
            except Exception as e:  # recorded (UD-114: invalid streams of tiny payloads)
                rec["error"] = "base64: %s" % e
                rec["values"], rec["raw"] = [], b""
            else:
                rec["header"] = struct.unpack("<i", raw[:4])[0] if len(raw) >= 4 else None
                payload = raw[4:]
                n = len(payload) // w
                rec["raw"] = payload
                rec["values"] = list(struct.unpack("<%d%s" % (n, code), payload[: n * w]))
        rec["entities"] = npts if section in ("Points", "PointData") else ncells
        rec["section"] = section
        arrays[(section, name)] = rec
    return arrays, npts, ncells


def fmt(x):
    return "%.4g" % x


def classify(p, u):
    """(class, detail) of an array present in both files."""
    if u["error"] and not p["error"] and not p["raw"]:
        return "ud114", "upstream's base64 stream of an empty array is invalid (%s); the port's is valid and empty" % u["error"]
    if p["raw"] == u["raw"]:
        return "same", ""
    if p["entities"] != u["entities"]:
        return "mesh", "the pieces differ: %d (port) and %d (upstream) %s" % (
            p["entities"], u["entities"], "points" if p["section"] in ("Points", "PointData") else "cells")
    c, n = p["ncomp"], p["entities"]
    vp, vu = p["values"], u["values"]
    m = int(round(c ** 0.5))
    square = m >= 2 and m * m == c
    if square and len(vu) == n * c * c and len(vp) in (n * c, n * c * c):
        compat = len(vp) == n * c * c
        rest_nonzero = 0
        mism_ud111 = mism_other = 0
        port_ud111_nonzero = 0
        for e in range(n):
            blk = vu[e * c * c:(e + 1) * c * c]
            for i in range(c):
                for j in range(c):
                    if i >= m or j >= m:
                        if blk[i * c + j] != 0:
                            rest_nonzero += 1
            for i in range(m):
                for j in range(m):
                    a = blk[i * c + j]
                    b = vp[e * c * c + i * c + j] if compat else vp[e * c + i * m + j]
                    same = (a == b) or (a != a and b != b)
                    if (i, j) in UD111_POS and m == 3:
                        if b != 0:
                            port_ud111_nonzero += 1
                        if not same:
                            mism_ud111 += 1
                    elif not same:
                        mism_other += 1
        detail = "upstream %d values for %d tuples of %d (zero-filled %dx%d squares%s); " % (
            len(vu), n, c, c, c, "" if rest_nonzero == 0 else ", %d non-zero entries outside the block" % rest_nonzero)
        if mism_other == 0 and mism_ud111 == 0:
            return ("ud111" if compat else "ud110-layout"), detail + "block bitwise equal"
        if mism_other == 0:
            return ("ud111" if compat else "ud110-layout+ud111"), detail + (
                "block equal except %d out-of-plane shear entries (port: %s)" % (
                    mism_ud111, "zero" if port_ud111_nonzero == 0 else "%d non-zero" % port_ud111_nonzero))
        return "values", detail + "%d block entries differ outside the out-of-plane shear" % mism_other
    need = n * c
    if len(vu) < need and len(vp) == need:
        k = len(vu)
        prefix_equal = vp[:k] == vu
        return "short", "upstream %d of %d values (header %s bytes); its %d values %s the port's first %d; port's last %d: %s" % (
            len(vu), need, u["header"], k, "equal" if prefix_equal else "differ from", k, need - k,
            " ".join(fmt(x) for x in vp[k:k + 8]))
    if len(vu) > need and len(vp) == need:
        return "long", "upstream %d values, the piece needs %d (%d entities x %d)" % (len(vu), need, n, c)
    if len(vp) != len(vu):
        return "values", "lengths %d (port) vs %d (upstream)" % (len(vp), len(vu))
    nd, mx, ref = 0, 0.0, 0.0
    for a, b in zip(vp, vu):
        if a != b and not (a != a and b != b):
            nd += 1
            try:
                mx = max(mx, abs(a - b))
            except OverflowError:
                mx = float("inf")
        if b == b:
            ref = max(ref, abs(b))
    return "values", "%d of %d values differ, largest difference %s where upstream reaches %s" % (nd, len(vu), fmt(mx), fmt(ref))


def main():
    port, up, out = sys.argv[1:4]
    fp, fu = set(files_below(port)), set(files_below(up))
    rows = []
    summary = {}
    headers = {}
    n_diff = 0
    for rel in sorted(fp & fu):
        a, b = os.path.join(port, rel), os.path.join(up, rel)
        if sha(a) == sha(b):
            continue
        n_diff += 1
        if not rel.endswith(".vtu"):
            rows.append((rel, "-", "-", "file", "bytes differ (not a .vtu piece)"))
            summary.setdefault(("(file)", rel.rsplit(".", 1)[-1]), []).append(rel)
            continue
        ap, _, _ = parse_vtu(a)
        au, _, _ = parse_vtu(b)
        all_same = True
        for key in sorted(set(ap) | set(au)):
            sec, name = key
            if key in au:
                u = au[key]
                if u["header"] is not None and u["header"] != len(u["raw"]):
                    headers.setdefault(u["header"] - len(u["raw"]), set()).add(name)
            if key not in ap or key not in au:
                cls, det = "one-side", "only in the %s file" % ("port" if key in ap else "upstream")
            else:
                cls, det = classify(ap[key], au[key])
            if cls != "same":
                all_same = False
                rows.append((rel, name, sec, cls, det))
                summary.setdefault((name, cls), []).append(rel)
        if all_same:
            rows.append((rel, "-", "-", "same", "every decoded array identical; only the encoding differs"))
            summary.setdefault(("(all arrays)", "same"), []).append(rel)
    with open(out, "w") as f:
        f.write("file\tarray\tsection\tclass\tdetail\n")
        for r in rows:
            f.write("\t".join(r) + "\n")
    print("files on both sides: %d, bytes differ: %d, only port: %d, only upstream: %d" % (
        len(fp & fu), n_diff, len(fp - fu), len(fu - fp)))
    for (name, cls), files in sorted(summary.items()):
        print("  %-28s %-20s %5d files (first %s, last %s)" % (name, cls, len(files), files[0], files[-1]))
    for extra, names in sorted(headers.items()):
        print("  base64 length header = payload + %d bytes in upstream's arrays (%s)" % (extra, ", ".join(sorted(names)[:6])))


if __name__ == "__main__":
    main()
