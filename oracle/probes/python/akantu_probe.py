# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: Python writer of the golden fixture format
# (docs/NUMERICAL_VALIDATION.md); mirrors
# oracle/probes/include/akantu_probe/fixture_writer.hh.
"""Fixture writer for Python probes (numpy only).

Example::

    from akantu_probe import FixtureWriter
    w = FixtureWriter(root, "model/static/plate_2d", "linear static plate",
                      probe="model_probe", probe_version="1")
    w.add("displacement", u, ["node", "component"], "nodal",
          components=["x", "y"], tolerance_class="linear_solve")
    w.finish()

Provenance comes from the same environment variables as the C++ writer
(AKANTU_PROBE_BUILD_INFO, AKANTU_PROBE_REFERENCE_SHA, ...).
"""

from __future__ import annotations

import hashlib
import io
import json
import os
import platform
import re
import shutil
from pathlib import Path

import numpy as np

REFERENCE_SHA = "35a851780d0b9471d258ed53a003ae98aaa95ea0"

_DTYPES = {
    np.dtype("<f8"): "f64",
    np.dtype("<i8"): "i64",
    np.dtype("<i4"): "i32",
    np.dtype("<u8"): "u64",
    np.dtype("<u4"): "u32",
    np.dtype("|u1"): "u8",
    np.dtype("|b1"): "bool",
}


def sanitize(name: str) -> str:
    """Portable file stem ('/' -> '__'), same rule as the Rust/C++ writers."""
    out = name.replace("/", "__")
    out = re.sub(r"[^A-Za-z0-9_.+-]", "_", out)
    return "_" if out in ("", ".", "..") else out


def npy_bytes(array: np.ndarray) -> bytes:
    """Encode as .npy format 1.0, little-endian, C order."""
    a = np.asarray(array)
    if a.dtype.kind == "f":
        a = a.astype("<f8", copy=False)
    elif a.dtype.kind == "b":
        a = a.astype("|b1", copy=False)
    elif a.dtype.kind in "iu":
        a = a.astype(a.dtype.newbyteorder("<"), copy=False)
    if a.dtype not in _DTYPES:
        raise ValueError(f"unsupported dtype {a.dtype}")
    if not a.flags.c_contiguous:
        # (np.ascontiguousarray would promote 0-d arrays to 1-d.)
        a = a.copy(order="C")
    buf = io.BytesIO()
    np.lib.format.write_array(buf, a, version=(1, 0), allow_pickle=False)
    return buf.getvalue()


def _env(name: str, default: str = "") -> str:
    v = os.environ.get(name, "")
    return v if v else default


def build_info() -> dict:
    """Build description: AKANTU_PROBE_BUILD_INFO file, else best effort."""
    path = _env("AKANTU_PROBE_BUILD_INFO")
    if path:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    return {
        "config": _env("AKANTU_PROBE_CONFIG", "unknown"),
        "compiler": "unknown (python probe without AKANTU_PROBE_BUILD_INFO)",
        "cxx_flags": "unknown",
        "build_type": "unknown",
        "arch": platform.machine(),
        "os": platform.platform(),
        "libraries": {"numpy": np.__version__},
    }


class FixtureWriter:
    """Incrementally writes one fixture case directory."""

    def __init__(self, root, case_id: str, description: str, probe: str,
                 probe_version: str):
        segs = case_id.split("/")
        if len(segs) < 2 or any(s in ("", ".", "..") for s in segs):
            raise ValueError(f"invalid case id {case_id!r}")
        self.case_id = case_id
        self.dir = Path(root).joinpath(*segs)
        # Regeneration replaces generated files, never tolerances.json.
        for sub in ("data", "inputs"):
            if (self.dir / sub).exists():
                shutil.rmtree(self.dir / sub)
        self.dir.mkdir(parents=True, exist_ok=True)
        ref = {"sha": _env("AKANTU_PROBE_REFERENCE_SHA", REFERENCE_SHA)}
        if _env("AKANTU_PROBE_REFERENCE_DESCRIBE"):
            ref["describe"] = _env("AKANTU_PROBE_REFERENCE_DESCRIBE")
        patches = [p for p in _env("AKANTU_PROBE_PATCHES").split(",") if p]
        if patches:
            ref["patches"] = patches
        self.manifest = {
            "format_version": 1,
            "case_id": case_id,
            "description": description,
            "status": "golden",
            "generator": {
                "probe": probe,
                "probe_version": probe_version,
                "command": _env("AKANTU_PROBE_COMMAND"),
            },
            "reference": ref,
            "build": build_info(),
        }
        if _env("AKANTU_PROBE_CREATED"):
            self.manifest["created"] = _env("AKANTU_PROBE_CREATED")
        self.parameters: dict = {}
        self.inputs: list = []
        self.quantities: list = []
        self.tags: list = []
        self.extra: dict = {}
        self._bytes = 0

    # -- inputs ------------------------------------------------------------
    def add_input_file(self, role: str, src, source: str = "") -> None:
        src = Path(src)
        data = src.read_bytes()
        rel = f"inputs/{src.name}"
        (self.dir / "inputs").mkdir(parents=True, exist_ok=True)
        (self.dir / rel).write_bytes(data)
        entry = {"role": role, "path": rel,
                 "sha256": hashlib.sha256(data).hexdigest()}
        if source:
            entry["source"] = source
        self.inputs.append(entry)

    def add_input_reference(self, role: str, source: str, file) -> None:
        data = Path(file).read_bytes()
        self.inputs.append({"role": role, "source": source,
                            "sha256": hashlib.sha256(data).hexdigest()})

    # -- quantities --------------------------------------------------------
    def add(self, name: str, array, axes, layout: str, **attrs) -> dict:
        """Write `array` as data/<name>.npy and record it.

        attrs: description, element_type, ghost_type, components, ordering
        (list of dicts), ragged (dict), units, tolerance_class.
        """
        if any(q["name"] == name for q in self.quantities):
            raise ValueError(f"duplicate quantity {name}")
        a = np.asarray(array)
        if a.ndim != len(axes):
            raise ValueError(f"{name}: {len(axes)} axes for a {a.ndim}-d array")
        data = npy_bytes(a)
        rel = f"data/{sanitize(name)}.npy"
        (self.dir / "data").mkdir(parents=True, exist_ok=True)
        (self.dir / rel).write_bytes(data)
        self._bytes += len(data)
        dtype = np.lib.format.read_array(io.BytesIO(data)).dtype
        q = {
            "name": name,
            "file": rel,
            "dtype": _DTYPES[dtype],
            "shape": [int(d) for d in a.shape],
            "axes": list(axes),
            "layout": layout,
            "sha256": hashlib.sha256(data).hexdigest(),
        }
        for k, v in attrs.items():
            if v is not None:
                q[k] = v
        self.quantities.append(q)
        return q

    # -- misc --------------------------------------------------------------
    def set_steps(self, recorded, dt=None, time_quantity=None) -> None:
        s = {"recorded": [int(r) for r in recorded]}
        if dt is not None:
            s["dt"] = float(dt)
        if time_quantity:
            s["time_quantity"] = time_quantity
        self.extra["steps"] = s

    def set_parallel(self, ranks: int, rank=None, partitioner=None,
                     partition_quantity=None) -> None:
        p = {"ranks": int(ranks)}
        if rank is not None:
            p["rank"] = int(rank)
        if partitioner:
            p["partitioner"] = partitioner
        if partition_quantity:
            p["partition_quantity"] = partition_quantity
        self.extra["parallel"] = p

    def expect_error(self, kind: str = "", message: str = "",
                     exit_code: int = 0) -> None:
        e = {"status": "error", "exit_code": exit_code}
        if kind:
            e["error_kind"] = kind
        if message:
            e["message_contains"] = message
        self.extra["expect"] = e

    def finish(self) -> Path:
        m = dict(self.manifest)
        if self.inputs:
            m["inputs"] = self.inputs
        if self.parameters:
            m["parameters"] = self.parameters
        if "parallel" in self.extra:
            m["parallel"] = self.extra["parallel"]
        m["storage"] = {"mode": "committed", "bytes": self._bytes}
        if "steps" in self.extra:
            m["steps"] = self.extra["steps"]
        m["quantities"] = self.quantities
        if "expect" in self.extra:
            m["expect"] = self.extra["expect"]
        if self.tags:
            m["tags"] = self.tags
        if "notes" in self.extra:
            m["notes"] = self.extra["notes"]
        text = json.dumps(m, indent=2, allow_nan=False) + "\n"
        (self.dir / "manifest.json").write_text(text, encoding="utf-8")
        return self.dir
