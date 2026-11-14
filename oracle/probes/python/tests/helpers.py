# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: helpers of the unit tests of the fixture-generation scripts.
"""Synthetic fixture trees for the script tests (no numpy needed)."""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

PYTHON_DIR = Path(__file__).resolve().parents[1]
REPO = Path(__file__).resolve().parents[4]
if str(PYTHON_DIR) not in sys.path:
    sys.path.insert(0, str(PYTHON_DIR))

SHA = "35a851780d0b9471d258ed53a003ae98aaa95ea0"


def write_case(root: Path, case_id: str, quantities: dict[str, bytes],
               inputs: dict[str, tuple[bytes, str]] | None = None, *, probe: str = "mesh_probe",
               created: str = "2026-11-09T00:00:00Z", storage: dict | None = None,
               write_data: bool = True, extra: dict | None = None) -> Path:
    """Write a manifest (and optionally its data) for `case_id`."""
    d = root.joinpath(*case_id.split("/"))
    d.mkdir(parents=True, exist_ok=True)
    qs = []
    for name, data in quantities.items():
        rel = f"data/{name.replace('/', '__')}.npy"
        if write_data:
            (d / "data").mkdir(exist_ok=True)
            (d / rel).write_bytes(data)
        qs.append({"name": name, "file": rel, "dtype": "f64", "shape": [len(data) // 8],
                   "axes": ["entry"], "layout": "table",
                   "sha256": hashlib.sha256(data).hexdigest()})
    ins = []
    for role, (data, source) in (inputs or {}).items():
        rel = f"inputs/{role}.txt"
        if write_data:
            (d / "inputs").mkdir(exist_ok=True)
            (d / rel).write_bytes(data)
        ins.append({"role": role, "path": rel, "source": source,
                    "sha256": hashlib.sha256(data).hexdigest()})
    m = {"format_version": 1, "case_id": case_id, "description": "test", "status": "golden",
         "generator": {"probe": probe, "probe_version": "1", "command": "test"},
         "reference": {"sha": SHA},
         "build": {"config": "strict", "compiler": "g++", "arch": "aarch64"},
         "created": created, "inputs": ins,
         "storage": storage or {"mode": "committed", "bytes": sum(len(v) for v in quantities.values())},
         "quantities": qs}
    m.update(extra or {})
    (d / "manifest.json").write_text(json.dumps(m, indent=2) + "\n", encoding="utf-8")
    return d


def manifest(root: Path, case_id: str) -> dict:
    return json.loads(root.joinpath(*case_id.split("/"), "manifest.json").read_text())


def msh2(path: Path, tag_counts: list[tuple[int, int]]) -> None:
    """Minimal Gmsh 2.2 file: one element per (gmsh type, number of tags)."""
    lines = ["$MeshFormat", "2.2 0 8", "$EndMeshFormat", "$Elements", str(len(tag_counts))]
    for k, (t, n) in enumerate(tag_counts, start=1):
        lines.append(" ".join([str(k), str(t), str(n)] + ["1"] * n + ["1", "2"]))
    lines.append("$EndElements")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
