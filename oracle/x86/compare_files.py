#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: cross-platform fixture comparison (D-043).
#
# Value-level comparison of two trees of oracle outputs that are NOT
# manifest-based fixture cases (those are compared with
# `cargo run -p akantu-validation --bin fixture-diff`): text fixtures with
# IEEE-754 bit patterns (dense, gemm, complex division), JSON fixtures
# (random, parser dumps), dumper outputs (VTU/PVTU with base64 arrays, CSV,
# text). Every file present in either tree is reported:
#
#   identical          byte-identical
#   metadata_only      only text tokens differ, and only on comment/header
#                      lines ('#'-prefixed) or in JSON "meta" members --
#                      e.g. "aarch64" vs "x86_64" in a provenance header
#   numeric_diff       same token structure; at least one numeric token
#                      (16-hex-digit bit pattern, decimal number, or decoded
#                      binary array element) differs; ULP and relative
#                      differences are reported
#   text_diff          same token structure, a non-metadata text token differs
#   structure_diff     different token count/structure (line diff excerpt)
#   only_a / only_b    file present on one side only
#
# Numeric tokens: a 16-hex-digit word is a binary64 bit pattern; decimal
# literals (incl. nan/inf) are parsed as binary64. ULP distance uses the
# ordered-integer mapping of the bit patterns (+0 and -0 are 0 ULP apart but
# counted separately as `signed_zero`), NaNs compare equal only bitwise.
#
# Usage:
#   compare_files.py <root_a> <root_b> [--include GLOB]... [--exclude GLOB]...
#                    [--json OUT] [--max-examples N]
#   compare_files.py --self-test
# Exit status: 0 compared (differences are measurements), 2 usage error.
import argparse
import base64
import difflib
import fnmatch
import json
import math
import os
import re
import struct
import sys
import xml.etree.ElementTree as ET

HEX16 = re.compile(r"(?<![0-9A-Za-z_])[0-9a-f]{16}(?![0-9A-Za-z_])")
TOKEN = re.compile(
    r"(?P<hex>(?<![0-9A-Za-z_.])[0-9a-f]{16}(?![0-9A-Za-z_.]))"
    r"|(?P<num>(?<![0-9A-Za-z_.])[-+]?(?:(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?|nan|inf|infinity)"
    r"(?![0-9A-Za-z_.]))"
    r"|(?P<word>[^\s\"',:;|\[\]{}()=<>]+)",
    re.IGNORECASE,
)


def bits_of(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def ordered(b):
    return -(b & 0x7FFFFFFFFFFFFFFF) if b >> 63 else b


def ulp_distance(ba, bb):
    return abs(ordered(ba) - ordered(bb))


def f_of(bits):
    return struct.unpack("<d", struct.pack("<Q", bits))[0]


class Stats:
    def __init__(self):
        self.numeric_tokens = 0
        self.numeric_diff = 0
        self.signed_zero = 0
        self.nan_mismatch = 0
        self.max_ulp = 0
        self.max_abs = 0.0
        self.max_rel = 0.0
        self.ulp_hist = {}
        self.examples = []
        self.text_diff = 0
        self.meta_text_diff = 0
        self.text_examples = []

    def num(self, ba, bb, where, max_examples):
        self.numeric_tokens += 1
        if ba == bb:
            return
        a, b = f_of(ba), f_of(bb)
        if math.isnan(a) or math.isnan(b):
            self.numeric_diff += 1
            self.nan_mismatch += 1
            u = None
        elif a == b:  # +0 / -0
            self.signed_zero += 1
            return
        else:
            self.numeric_diff += 1
            u = ulp_distance(ba, bb)
            self.max_ulp = max(self.max_ulp, u)
            d = abs(a - b)
            if math.isfinite(d):
                self.max_abs = max(self.max_abs, d)
                m = max(abs(a), abs(b))
                if m > 0:
                    self.max_rel = max(self.max_rel, d / m)
            else:
                self.max_abs = math.inf
            k = str(u) if u is not None and u <= 4 else (">4" if u is not None else "nan")
            self.ulp_hist[k] = self.ulp_hist.get(k, 0) + 1
        if len(self.examples) < max_examples:
            self.examples.append({"where": where, "a": f"{ba:016x}", "b": f"{bb:016x}",
                                  "a_value": repr(a), "b_value": repr(b), "ulp": u})

    def text(self, ta, tb, where, meta, max_examples):
        if meta:
            self.meta_text_diff += 1
        else:
            self.text_diff += 1
        if len(self.text_examples) < max_examples:
            self.text_examples.append({"where": where, "a": ta[:120], "b": tb[:120], "metadata": meta})

    def as_dict(self):
        d = {"numeric_tokens": self.numeric_tokens, "numeric_diff": self.numeric_diff,
             "signed_zero": self.signed_zero, "nan_mismatch": self.nan_mismatch,
             "max_ulp": self.max_ulp, "max_abs": self.max_abs, "max_rel": self.max_rel,
             "ulp_hist": self.ulp_hist, "text_diff": self.text_diff,
             "meta_text_diff": self.meta_text_diff}
        if self.examples:
            d["examples"] = self.examples
        if self.text_examples:
            d["text_examples"] = self.text_examples
        return d


def tokens_of_line(line):
    out = []
    for m in TOKEN.finditer(line):
        if m.group("hex"):
            out.append(("n", int(m.group("hex"), 16), m.group("hex")))
        elif m.group("num"):
            t = m.group("num")
            try:
                out.append(("n", bits_of(float(t)), t))
            except ValueError:
                out.append(("t", t, t))
        else:
            out.append(("t", m.group("word"), m.group("word")))
    return out


def is_meta_line(line):
    s = line.lstrip()
    return s.startswith("#") or s.startswith("//") or s.startswith("<!--")


def compare_text(ta, tb, st, max_examples):
    la, lb = ta.splitlines(), tb.splitlines()
    if len(la) != len(lb):
        return False
    for i, (xa, xb) in enumerate(zip(la, lb)):
        if xa == xb:
            # still count numeric tokens for the statistics
            st.numeric_tokens += sum(1 for k in tokens_of_line(xa) if k[0] == "n")
            continue
        ka, kb = tokens_of_line(xa), tokens_of_line(xb)
        meta = is_meta_line(xa) and is_meta_line(xb)
        if len(ka) != len(kb):
            if meta:
                st.text(xa, xb, f"line {i + 1}", True, max_examples)
                continue
            return False
        for j, (a, b) in enumerate(zip(ka, kb)):
            if a[0] == "n" and b[0] == "n" and not meta:
                st.num(a[1], b[1], f"line {i + 1} token {j}", max_examples)
            elif a[2] != b[2]:
                st.text(a[2], b[2], f"line {i + 1} token {j}", meta, max_examples)
    return True


def compare_json(va, vb, st, path, max_examples, meta=False):
    if isinstance(va, dict) and isinstance(vb, dict):
        if list(va.keys()) != list(vb.keys()):
            if meta:
                st.text(str(sorted(va)), str(sorted(vb)), path, True, max_examples)
                return True
            return False
        return all(compare_json(va[k], vb[k], st, f"{path}.{k}", max_examples,
                                meta or k in ("meta", "provenance", "build", "created"))
                   for k in va)
    if isinstance(va, list) and isinstance(vb, list):
        if len(va) != len(vb):
            if meta:
                st.text(f"len {len(va)}", f"len {len(vb)}", path, True, max_examples)
                return True
            return False
        return all(compare_json(x, y, st, f"{path}[{i}]", max_examples, meta)
                   for i, (x, y) in enumerate(zip(va, vb)))
    na, nb = num_of_json(va), num_of_json(vb)
    if na is not None and nb is not None and not meta:
        st.num(na, nb, path, max_examples)
        return True
    if va != vb:
        if type(va) is not type(vb) and not meta and not (isinstance(va, str) and isinstance(vb, str)):
            return False
        st.text(str(va), str(vb), path, meta, max_examples)
    return True


def num_of_json(v):
    if isinstance(v, bool):
        return None
    if isinstance(v, (int, float)):
        try:
            return bits_of(float(v))
        except OverflowError:
            return None
    if isinstance(v, str) and HEX16.fullmatch(v):
        return int(v, 16)
    return None


VTK_DT = {"Float64": ("d", 8), "Float32": ("f", 4), "Int32": ("i", 4), "UInt32": ("I", 4),
          "Int64": ("q", 8), "UInt64": ("Q", 8), "UInt8": ("B", 1), "Int8": ("b", 1)}


def vtk_arrays(text):
    root = ET.fromstring(text)
    out = []
    for i, el in enumerate(root.iter("DataArray")):
        name = el.get("Name") or f"#{i}"
        dt = el.get("type")
        fmt = el.get("format", "ascii")
        body = (el.text or "").strip()
        if fmt == "binary" and dt in VTK_DT:
            raw = base64.b64decode(body)
            code, size = VTK_DT[dt]
            hdr = struct.unpack("<I", raw[:4])[0] if len(raw) >= 4 else 0
            payload = raw[4:4 + hdr] if hdr <= len(raw) - 4 else raw[4:]
            n = len(payload) // size
            vals = list(struct.unpack("<" + code * n, payload[:n * size]))
        else:
            vals = body.split()
            if dt and dt.startswith("Float"):
                vals = [float(v) for v in vals]
        out.append((name, dt, vals))
    return root, out


def compare_vtk(ta, tb, st, max_examples):
    ra, aa = vtk_arrays(ta)
    rb, ab = vtk_arrays(tb)
    if [(n, d, len(v)) for n, d, v in aa] != [(n, d, len(v)) for n, d, v in ab]:
        return False
    for (name, dt, va), (_, _, vb) in zip(aa, ab):
        for k, (x, y) in enumerate(zip(va, vb)):
            if dt and dt.startswith("Float"):
                st.num(bits_of(float(x)), bits_of(float(y)), f"{name}[{k}]", max_examples)
            elif x != y:
                st.text(str(x), str(y), f"{name}[{k}]", False, max_examples)
    # attributes / structure outside the arrays
    for el in list(ra.iter("DataArray")) + list(rb.iter("DataArray")):
        el.text = ""
    if ET.tostring(ra) != ET.tostring(rb):
        st.text("xml-structure", "xml-structure", "non-array XML", False, max_examples)
    return True


def compare_pair(pa, pb, max_examples):
    with open(pa, "rb") as f:
        ba = f.read()
    with open(pb, "rb") as f:
        bb = f.read()
    if ba == bb:
        return {"status": "identical", "bytes": len(ba)}
    st = Stats()
    ok = False
    kind = "text"
    try:
        ta, tb = ba.decode("utf-8"), bb.decode("utf-8")
    except UnicodeDecodeError:
        return {"status": "binary_diff", "bytes_a": len(ba), "bytes_b": len(bb)}
    ext = os.path.splitext(pa)[1].lower()
    try:
        if ext in (".vtu", ".pvtu", ".vtk", ".pvd") and "<VTKFile" in ta[:400]:
            kind = "vtk"
            ok = compare_vtk(ta, tb, st, max_examples)
        elif ext in (".json",):
            kind = "json"
            ok = compare_json(json.loads(ta), json.loads(tb), st, "$", max_examples)
        elif ext == ".jsonl":
            kind = "jsonl"
            la, lb = ta.splitlines(), tb.splitlines()
            ok = len(la) == len(lb) and all(
                compare_json(json.loads(x) if x.strip() else None,
                             json.loads(y) if y.strip() else None, st, f"line {i + 1}", max_examples)
                for i, (x, y) in enumerate(zip(la, lb)))
        else:
            ok = compare_text(ta, tb, st, max_examples)
    except (ValueError, ET.ParseError, struct.error) as e:
        kind = f"{kind} (fallback: {type(e).__name__})"
        st = Stats()
        ok = compare_text(ta, tb, st, max_examples)
    if not ok:
        diff = list(difflib.unified_diff(ta.splitlines(), tb.splitlines(), "a", "b", n=0, lineterm=""))
        return {"status": "structure_diff", "kind": kind, "diff_lines": len(diff),
                "excerpt": [d[:200] for d in diff[:12]]}
    d = st.as_dict()
    if st.numeric_diff or st.nan_mismatch:
        status = "numeric_diff"
    elif st.text_diff:
        status = "text_diff"
    elif st.meta_text_diff:
        status = "metadata_only"
    elif st.signed_zero:
        status = "signed_zero_only"
    else:
        status = "identical_tokens"  # e.g. whitespace / number formatting only
    d.update({"status": status, "kind": kind})
    return d


def walk(root):
    out = set()
    for dp, dn, fn in os.walk(root):
        dn.sort()
        for f in fn:
            out.add(os.path.relpath(os.path.join(dp, f), root))
    return out


def selected(rel, inc, exc):
    if inc and not any(fnmatch.fnmatch(rel, g) for g in inc):
        return False
    return not any(fnmatch.fnmatch(rel, g) for g in exc)


def run(a, b, inc, exc, max_examples):
    fa, fb = walk(a), walk(b)
    files = {}
    for rel in sorted(fa | fb):
        if not selected(rel, inc, exc):
            continue
        if rel not in fb:
            files[rel] = {"status": "only_a"}
        elif rel not in fa:
            files[rel] = {"status": "only_b"}
        else:
            files[rel] = compare_pair(os.path.join(a, rel), os.path.join(b, rel), max_examples)
    summary = {}
    for r in files.values():
        summary[r["status"]] = summary.get(r["status"], 0) + 1
    return {"root_a": a, "root_b": b, "summary": summary, "files": files}


def self_test():
    import tempfile
    t = tempfile.mkdtemp()
    a, b = os.path.join(t, "a"), os.path.join(t, "b")
    os.makedirs(a)
    os.makedirs(b)
    one = 0x3FF0000000000000

    def w(root, name, s):
        with open(os.path.join(root, name), "w") as f:
            f.write(s)
    w(a, "same.txt", "x 1.5\n")
    w(b, "same.txt", "x 1.5\n")
    w(a, "hdr.txt", "# arch aarch64\nop | 3ff0000000000000\n")
    w(b, "hdr.txt", "# arch x86_64\nop | 3ff0000000000000\n")
    w(a, "ulp.txt", f"op | {one:016x} {one:016x}\n")
    w(b, "ulp.txt", f"op | {one + 1:016x} {one:016x}\n")
    w(a, "dec.csv", "t,e\n0.1,1e-3\n")
    w(b, "dec.csv", "t,e\n0.1,1.0000000000000002e-3\n")
    w(a, "struct.txt", "a 1\nb 2\n")
    w(b, "struct.txt", "a 1\n")
    w(a, "zero.txt", "v 0.0\n")
    w(b, "zero.txt", "v -0.0\n")
    w(a, "j.json", json.dumps({"meta": {"arch": "aarch64"}, "v": ["3ff0000000000000", 2.0]}))
    w(b, "j.json", json.dumps({"meta": {"arch": "x86_64"}, "v": ["3ff0000000000001", 2.0]}))
    w(a, "jm.json", json.dumps({"meta": {"arch": "aarch64"}, "v": [1.0]}))
    w(b, "jm.json", json.dumps({"meta": {"arch": "x86_64"}, "v": [1.0]}))
    w(a, "txt.txt", "name alpha 1\n")
    w(b, "txt.txt", "name beta 1\n")
    w(a, "nan.txt", "v nan\n")
    w(b, "nan.txt", "v 1\n")
    arr_a = base64.b64encode(struct.pack("<I", 16) + struct.pack("<dd", 1.0, 2.0)).decode()
    arr_b = base64.b64encode(struct.pack("<I", 16) + struct.pack("<dd", 1.0, f_of(bits_of(2.0) + 3))).decode()
    vtu = ('<?xml version="1.0"?><VTKFile type="UnstructuredGrid"><UnstructuredGrid><Piece>'
           '<PointData><DataArray type="Float64" Name="u" format="binary">{}</DataArray>'
           '</PointData></Piece></UnstructuredGrid></VTKFile>')
    w(a, "d.vtu", vtu.format(arr_a))
    w(b, "d.vtu", vtu.format(arr_b))
    w(a, "onlya.txt", "x")
    r = run(a, b, [], [], 5)["files"]
    expect = {"same.txt": "identical", "hdr.txt": "metadata_only", "ulp.txt": "numeric_diff",
              "dec.csv": "numeric_diff", "struct.txt": "structure_diff", "zero.txt": "signed_zero_only",
              "j.json": "numeric_diff", "jm.json": "metadata_only", "txt.txt": "text_diff",
              "nan.txt": "numeric_diff", "d.vtu": "numeric_diff", "onlya.txt": "only_a"}
    bad = {k: (r[k]["status"], v) for k, v in expect.items() if r[k]["status"] != v}
    assert not bad, bad
    assert r["ulp.txt"]["max_ulp"] == 1 and r["ulp.txt"]["numeric_diff"] == 1, r["ulp.txt"]
    assert r["dec.csv"]["max_ulp"] == 1, r["dec.csv"]
    assert r["d.vtu"]["max_ulp"] == 3, r["d.vtu"]
    assert r["j.json"]["max_ulp"] == 1 and r["j.json"]["meta_text_diff"] == 1, r["j.json"]
    assert r["nan.txt"]["nan_mismatch"] == 1
    print(f"self-test OK ({len(expect)} cases)")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("root_a", nargs="?")
    ap.add_argument("root_b", nargs="?")
    ap.add_argument("--include", action="append", default=[])
    ap.add_argument("--exclude", action="append", default=[])
    ap.add_argument("--json", dest="out")
    ap.add_argument("--max-examples", type=int, default=5)
    ap.add_argument("--self-test", action="store_true")
    o = ap.parse_args()
    if o.self_test:
        self_test()
        return 0
    if not o.root_a or not o.root_b or not os.path.isdir(o.root_a) or not os.path.isdir(o.root_b):
        ap.print_usage(sys.stderr)
        return 2
    rep = run(o.root_a, o.root_b, o.include, o.exclude, o.max_examples)
    txt = json.dumps(rep, indent=1, sort_keys=False)
    if o.out:
        with open(o.out, "w") as f:
            f.write(txt + "\n")
    else:
        print(txt)
    print(json.dumps(rep["summary"]), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
