# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: turns a `pending_oracle` regression case (persisted by
# akantu_validation::regression from a failing Rust test) into a golden
# fixture by replaying its inputs through the matching probe.
"""Replay a pending regression case through the oracle.

  python3 -I replay_regression.py <probe_bin_dir> <input_root> <output_root> <case_id>

The pending case is read from <input_root>/<case_id> (normally the fixtures
tree), the golden case is written to <output_root>/<case_id> (the staging
directory of the run). The input checksums of the pending case are verified
before the replay, and the golden output must record the same inputs.

Supported generator.probe values:
  material_probe: parameters {"dim": d, "material_dat": "<.dat text>",
                  "internals": [...]} and an input quantity "grad_u"
                  [step, dim, dim] (prescribed displacement gradients).
"""
from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main(argv: list[str]) -> int:
    if len(argv) != 5:
        print(__doc__, file=sys.stderr)
        return 2
    bindir, in_root, out_root, case_id = Path(argv[1]), Path(argv[2]), Path(argv[3]), argv[4]
    if not case_id.startswith("regression/"):
        print(f"{case_id}: regression cases live under regression/", file=sys.stderr)
        return 2
    case = in_root.joinpath(*case_id.split("/"))
    m = json.loads((case / "manifest.json").read_text(encoding="utf-8"))
    if m["case_id"] != case_id:
        print(f"{case_id}: manifest case_id is {m['case_id']}", file=sys.stderr)
        return 1
    if m["status"] != "pending_oracle":
        print(f"{case_id}: status is {m['status']}, nothing to replay", file=sys.stderr)
        return 1
    for q in m["quantities"]:
        if sha256(case / q["file"]) != q["sha256"]:
            print(f"{case_id}: input {q['name']} does not match its checksum", file=sys.stderr)
            return 1
    probe = m["generator"]["probe"]
    p = m.get("parameters") or {}
    with tempfile.TemporaryDirectory() as tmpd:
        tmp = Path(tmpd)
        if probe == "material_probe":
            q = next((q for q in m["quantities"] if q["name"] == "grad_u"), None)
            if q is None or len(q["shape"]) != 3 or q["shape"][1:] != [p["dim"], p["dim"]]:
                print(f"{case_id}: needs an input grad_u [step, dim, dim]", file=sys.stderr)
                return 1
            shutil.copy(case / q["file"], tmp / "grad_u.npy")
            (tmp / "material.dat").write_text(p["material_dat"], encoding="utf-8")
            cmd = [str(bindir / "material_probe"), str(out_root), case_id, str(p["dim"]),
                   str(tmp / "material.dat"), str(tmp / "grad_u.npy"),
                   "--description", m["description"]]
            for i in p.get("internals", []):
                cmd += ["--internal", i]
            expected_inputs = {"grad_u": q["sha256"]}
        else:
            print(f"{case_id}: no replay rule for probe {probe!r}", file=sys.stderr)
            return 1
        r = subprocess.run(cmd, check=False)
        if r.returncode != 0:
            print(f"{case_id}: probe exited with {r.returncode}", file=sys.stderr)
            return 1
    out = json.loads(out_root.joinpath(*case_id.split("/"), "manifest.json")
                     .read_text(encoding="utf-8"))
    if out["status"] != "golden":
        print(f"{case_id}: replay produced status {out['status']}", file=sys.stderr)
        return 1
    got = {i["role"]: i["sha256"] for i in out.get("inputs", [])}
    for role, sha in expected_inputs.items():
        if got.get(role) != sha:
            print(f"{case_id}: replayed input `{role}` differs from the pending case",
                  file=sys.stderr)
            return 1
    print(f"replay: {case_id}: golden, inputs verified ({', '.join(expected_inputs)})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
