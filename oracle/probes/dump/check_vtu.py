#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent reader
# check of ParaView output: every.vtu/.pvtu/.pvd below the given
# directories is parsed with xml.etree, base64 arrays are decoded with the
# standard library's strict decoder and NumPy, and the sizes VTK relies on
# are verified. Used on the Rust dumpers' output and, for comparison, on
# the oracle's (fixtures/dump), see check.sh.
#
# Usage: check_vtu.py [--expect-invalid NAME]... [--skip NAME]... DIR...
# Exit status 0 iff every file is valid, except those whose path contains
# one of the --expect-invalid names (.vtu files there must be invalid);
# paths containing a --skip name are ignored.

import argparse
import base64
import binascii
import os
import sys
import xml.etree.ElementTree as ET

import numpy as np

DTYPES = {
    "Float64": "<f8",
    "Int32": "<i4",
    "UInt32": "<u4",
    "UInt8": "u1",
}
# VTK cell type -> number of points (the types IOHelper/akantu-rs write)
CELL_POINTS = {1: 1, 3: 2, 5: 3, 9: 4, 10: 4, 12: 8, 13: 6, 21: 3, 22: 6, 23: 8,
               24: 10, 25: 20, 26: 15, 31: 12}
VARIABLE = {2, 7}  # poly-vertex, polygon


class Invalid(Exception):
    pass


def data(array, count_hint=None):
    """Values of a DataArray (numpy array)."""
    fmt = array.get("format")
    dtype = DTYPES[array.get("type")]
    text = (array.text or "").strip()
    if fmt == "binary":
        try:
            raw = base64.b64decode(text, validate=True)
        except binascii.Error as e:
            raise Invalid(f"array {array.get('Name')}: invalid base64 ({e})")
        if len(raw) < 4:
            raise Invalid(f"array {array.get('Name')}: no header")
        header = int.from_bytes(raw[:4], "little")
        payload = raw[4:]
        # IOHelper's header counts itself (payload + 4)
        if header != len(payload) + 4:
            raise Invalid(f"array {array.get('Name')}: header {header} for {len(payload)} bytes")
        itemsize = np.dtype(dtype).itemsize
        if len(payload) % itemsize:
            raise Invalid(f"array {array.get('Name')}: {len(payload)} bytes for {dtype}")
        return np.frombuffer(payload, dtype=dtype)
    if fmt == "ascii":
        return np.array(text.split(), dtype=float if dtype == "<f8" else np.int64)
    raise Invalid(f"unknown format {fmt}")


def check_vtu(path):
    root = ET.parse(path).getroot()
    if root.get("type") != "UnstructuredGrid":
        raise Invalid("not an UnstructuredGrid")
    piece = root.find("UnstructuredGrid/Piece")
    npts = int(piece.get("NumberOfPoints"))
    ncells = int(piece.get("NumberOfCells"))
    pts = piece.find("Points/DataArray")
    if int(pts.get("NumberOfComponents")) != 3 or data(pts).size != 3 * npts:
        raise Invalid("positions size")
    cells = {a.get("Name"): a for a in piece.findall("Cells/DataArray")}
    conn = data(cells["connectivity"])
    offsets = data(cells["offsets"])
    types = data(cells["types"])
    if offsets.size != ncells or types.size != ncells:
        raise Invalid("offsets/types size")
    if ncells:
        if np.any(np.diff(np.concatenate([[0], offsets])) <= 0):
            raise Invalid("offsets not increasing")
        if offsets[-1] != conn.size:
            raise Invalid("connectivity size != last offset")
        if conn.size and (conn.min() < 0 or conn.max() >= npts):
            raise Invalid("connectivity out of range")
        sizes = np.diff(np.concatenate([[0], offsets]))
        for t, s in zip(types, sizes):
            t = int(t)
            if t in VARIABLE:
                continue
            if CELL_POINTS.get(t) != s:
                raise Invalid(f"cell type {t} with {s} points")
    for section, n in (("PointData", npts), ("CellData", ncells)):
        for a in piece.findall(f"{section}/DataArray"):
            comps = int(a.get("NumberOfComponents", "1"))
            size = data(a).size
            if size != comps * n:
                raise Invalid(f"{section} {a.get('Name')}: {size} values for {comps} x {n}")
    return {
        (s, a.get("Name")): (a.get("type"), a.get("NumberOfComponents"))
        for s in ("PointData", "CellData")
        for a in piece.findall(f"{s}/DataArray")
    }


def check_pvtu(path):
    root = ET.parse(path).getroot()
    if root.get("type") != "PUnstructuredGrid":
        raise Invalid("not a PUnstructuredGrid")
    grid = root.find("PUnstructuredGrid")
    declared = {
        (s[1:], a.get("Name")): (a.get("type"), a.get("NumberOfComponents"))
        for s in ("PPointData", "PCellData")
        for a in grid.findall(f"{s}/PDataArray")
    }
    pieces = [p.get("Source") for p in grid.findall("Piece")]
    if not pieces:
        raise Invalid("no pieces")
    for src in pieces:
        piece = os.path.join(os.path.dirname(path), src)
        if not os.path.exists(piece):
            raise Invalid(f"missing piece {src}")
        arrays = check_vtu(piece)
        if arrays != declared:
            raise Invalid(f"piece {src} arrays {arrays} != declared {declared}")


def check_pvd(path):
    root = ET.parse(path).getroot()
    if root.get("type") != "Collection":
        raise Invalid("not a Collection")
    for ds in root.findall("Collection/DataSet"):
        float(ds.get("timestep"))
        if not os.path.exists(os.path.join(os.path.dirname(path), ds.get("file"))):
            raise Invalid(f"missing {ds.get('file')}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--expect-invalid", action="append", default=[])
    ap.add_argument("--skip", action="append", default=[])
    ap.add_argument("dirs", nargs="+")
    args = ap.parse_args()
    ok = True
    counts = {"valid": 0, "invalid (expected)": 0}
    for d in args.dirs:
        for dirpath, _, files in sorted(os.walk(d)):
            for f in sorted(files):
                path = os.path.join(dirpath, f)
                kind = os.path.splitext(f)[1]
                checker = {".vtu": check_vtu, ".pvtu": check_pvtu, ".pvd": check_pvd}.get(kind)
                if checker is None:
                    continue
                if any(n in path for n in args.skip):
                    continue
                expect_bad = any(n in path for n in args.expect_invalid)
                try:
                    checker(path)
                    if expect_bad and kind == ".vtu":
                        print(f"UNEXPECTEDLY VALID {path}")
                        ok = False
                    else:
                        counts["valid"] += 1
                except (Invalid, ET.ParseError, KeyError, ValueError) as e:
                    if expect_bad:
                        counts["invalid (expected)"] += 1
                        print(f"invalid (expected): {path}: {e}")
                    else:
                        ok = False
                        print(f"INVALID {path}: {e}")
    print(f"checked: {counts}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
