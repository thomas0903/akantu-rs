#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: compare a staged regeneration of
# fixtures/model/diffusion (e.g. on the x86_64 oracle, D-043) with the
# committed corpus, value by value. Pure standard library (no numpy).
#
# Usage: compare_platforms.py <committed fixtures root> <staged root> [--out FILE]
#   both roots contain model/diffusion/<case>/...
# Every file of a case other than manifest.json is compared byte for byte;
# .npy files that differ are decoded and compared value by value (count,
# max |a-b|, max relative difference, max ulp distance for f64).
# manifest.json is compared after dropping the provenance keys (generator,
# reference, build, created) and the per-quantity sha256 (implied by the
# payload comparison). Exit code 0 iff every case is identical.
import argparse
import ast
import json
import math
import struct
import sys
from pathlib import Path

PROVENANCE_KEYS = ("generator", "reference", "build", "created")
DTYPES = {"<f8": ("d", 8), "<i8": ("q", 8), "|u1": ("B", 1), "|b1": ("?", 1), "<u8": ("Q", 8), "<i4": ("i", 4)}


def read_npy(path):
    raw = path.read_bytes()
    if raw[:6] != b"\x93NUMPY":
        raise ValueError(f"{path}: not an npy file")
    major = raw[6]
    if major == 1:
        hlen = struct.unpack("<H", raw[8:10])[0]
        start = 10
    else:
        hlen = struct.unpack("<I", raw[8:12])[0]
        start = 12
    header = ast.literal_eval(raw[start:start + hlen].decode("latin1"))
    if header.get("fortran_order"):
        raise ValueError(f"{path}: fortran order not supported")
    code, size = DTYPES[header["descr"]]
    n = 1
    for s in header["shape"]:
        n *= s
    body = raw[start + hlen:]
    if len(body) != n * size:
        raise ValueError(f"{path}: {len(body)} payload bytes for {n} x {size}")
    fmt = "<" + code * n if code not in "B?" else code * n
    return header["descr"], tuple(header["shape"]), struct.unpack(fmt, body)


def ulp_distance(a, b):
    ia = struct.unpack("<q", struct.pack("<d", a))[0]
    ib = struct.unpack("<q", struct.pack("<d", b))[0]
    if ia < 0:
        ia = -(1 << 63) - ia
    if ib < 0:
        ib = -(1 << 63) - ib
    return abs(ia - ib)


def compare_npy(pa, pb):
    da, sa, va = read_npy(pa)
    db, sb, vb = read_npy(pb)
    if (da, sa) != (db, sb):
        return f"dtype/shape {da}{list(sa)} vs {db}{list(sb)}"
    ndiff = 0
    max_abs = 0.0
    max_rel = 0.0
    max_ulp = 0
    first = None
    for i, (x, y) in enumerate(zip(va, vb)):
        if da == "<f8":
            same = struct.pack("<d", x) == struct.pack("<d", y)
        else:
            same = x == y
        if same:
            continue
        ndiff += 1
        if first is None:
            first = (i, x, y)
        if da == "<f8":
            if math.isfinite(x) and math.isfinite(y):
                d = abs(x - y)
                max_abs = max(max_abs, d)
                scale = max(abs(x), abs(y))
                if scale > 0:
                    max_rel = max(max_rel, d / scale)
                max_ulp = max(max_ulp, ulp_distance(x, y))
            else:
                max_abs = math.inf
    msg = f"{ndiff}/{len(va)} values differ"
    if da == "<f8" and ndiff:
        msg += f", max |d| {max_abs:.3e}, max rel {max_rel:.3e}, max ulp {max_ulp}"
    if first is not None:
        msg += f", first at flat index {first[0]}: {first[1]!r} vs {first[2]!r}"
    return msg


def strip_manifest(m):
    m = {k: v for k, v in m.items() if k not in PROVENANCE_KEYS}
    for q in m.get("quantities", []):
        q.pop("sha256", None)
    return m


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("committed")
    ap.add_argument("staged")
    ap.add_argument("--out")
    args = ap.parse_args()
    ca = Path(args.committed) / "model" / "diffusion"
    cb = Path(args.staged) / "model" / "diffusion"
    cases_a = sorted(p.name for p in ca.iterdir() if (p / "manifest.json").is_file())
    cases_b = sorted(p.name for p in cb.iterdir() if (p / "manifest.json").is_file())
    lines = []
    bad = 0
    files_total = 0
    values_note = []
    for case in sorted(set(cases_a) | set(cases_b)):
        if case not in cases_a or case not in cases_b:
            lines.append(f"{case}: MISSING in {'committed' if case not in cases_a else 'staged'}")
            bad += 1
            continue
        a = ca / case
        b = cb / case
        fa = sorted(str(p.relative_to(a)) for p in a.rglob("*") if p.is_file())
        fb = sorted(str(p.relative_to(b)) for p in b.rglob("*") if p.is_file())
        problems = []
        for rel in sorted(set(fa) ^ set(fb)):
            problems.append(f"file {rel} only in {'committed' if rel in fa else 'staged'}")
        for rel in sorted(set(fa) & set(fb)):
            files_total += 1
            pa, pb = a / rel, b / rel
            if rel == "manifest.json":
                ma = strip_manifest(json.loads(pa.read_text()))
                mb = strip_manifest(json.loads(pb.read_text()))
                if ma != mb:
                    keys = sorted(k for k in set(ma) | set(mb) if ma.get(k) != mb.get(k))
                    problems.append(f"manifest differs outside provenance: keys {keys}")
                continue
            if pa.read_bytes() == pb.read_bytes():
                continue
            if rel.endswith(".npy"):
                problems.append(f"{rel}: {compare_npy(pa, pb)}")
            else:
                problems.append(f"{rel}: bytes differ")
        if problems:
            bad += 1
            lines.append(f"{case}: DIFFERENT")
            lines.extend(f"  {p}" for p in problems)
        else:
            lines.append(f"{case}: identical ({len(fa)} files)")
    summary = (f"SUMMARY cases={len(set(cases_a) | set(cases_b))} different={bad} "
               f"files_compared={files_total}")
    lines.append(summary)
    text = "\n".join(lines) + "\n"
    if args.out:
        Path(args.out).write_text(text)
    sys.stdout.write(text)
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
