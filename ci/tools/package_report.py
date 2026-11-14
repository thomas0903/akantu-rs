#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu; counterpart of upstream's
# packaging/release jobs, parity rows CI-021, CI-022, CI-029): judge the
# `cargo package --list` dry runs of every workspace member. Nothing is
# packaged into a registry and nothing is published.
#
#   package_report.py <metadata.json> <lists-dir> <repo> [--json FILE]
#
# <lists-dir>/<crate>.list holds the output of `cargo package --list -p
# <crate>`. For every member the report gives the file count, the
# uncompressed size and a gzip estimate of the .crate (tar of the listed
# files that exist in the crate directory; cargo's generated files —
# Cargo.toml, .cargo_vcs_info.json, Cargo.lock — are counted from their
# sources), and publication-readiness findings:
#   R1 publish = false            R2 licence texts missing: any COPYING/LICENSE*
#                                 file in general, COPYING + COPYING.lesser for
#                                 an LGPL crate, COPYING for a GPL crate
#   R3 no description             R4 no readme
#   R5 path dependency on a member without `version` (cannot publish)
#   R6 a file > 1 MiB, or a .crate estimate > 10 MiB (crates.io limit)
#   R7 MPL-2.0 files present but LICENSES/MPL-2.0.txt not packaged
#   R8 tests read files outside the package (../../fixtures etc., or the
#      workspace fixtures through akantu-validation, whose fixtures_root() is
#      CARGO_MANIFEST_DIR-relative): the packaged tests cannot run from the
#      .crate (informational)
# Hard failures (exit 1): a missing list (cargo package --list failed) and,
# for members that do NOT set publish = false, any of R2–R7: a crate opted
# into publication must be publishable. For publish = false members the
# findings are reported only (this is a dry run of what would be packaged).
"""Publication-readiness report of the cargo package dry runs."""

from __future__ import annotations

import gzip
import io
import json
import re
import sys
import tarfile
from pathlib import Path

LICENCE_NAMES = re.compile(r"^(LICEN[CS]E|COPYING|NOTICE)", re.IGNORECASE)
ANCESTOR_RE = re.compile(r"\.ancestors\(\)\s*\.nth\(\s*\d+\s*\)")  # with CARGO_MANIFEST_DIR in the same file


def licence_texts_ok(names: list[str], licence: str) -> tuple[bool, str]:
    """(ok, what is needed) for the licence texts packaged with a crate."""
    low = [n.lower() for n in names]
    texts = [n for n in low if n.startswith(("copying", "license", "licence"))]
    lesser = any("lesser" in n or "lgpl" in n for n in texts)
    plain = any("lesser" not in n and "lgpl" not in n for n in texts)
    if "LGPL" in licence:
        return plain and lesser, "COPYING (GPL text) and COPYING.lesser"
    if "GPL" in licence:
        return plain, "COPYING (GPL text)"
    return bool(texts), "a LICENSE/COPYING text"


OUTSIDE_RE = re.compile(r"\.\./\.\./(fixtures|oracle|docs|scripts)|CARGO_MANIFEST_DIR\"\)\s*\)?\s*\.join\(\"\.\./\.\.")


def gz_estimate(root: Path, files: list[str]) -> tuple[int, int, list[tuple[str, int]]]:
    raw = 0
    big: list[tuple[str, int]] = []
    buf = io.BytesIO()
    with gzip.GzipFile(fileobj=buf, mode="wb", mtime=0) as gz, tarfile.open(fileobj=gz, mode="w") as tar:
        for rel in files:
            src = root / ("Cargo.toml" if rel == "Cargo.toml.orig" else rel)
            if rel in (".cargo_vcs_info.json", "Cargo.lock") or not src.is_file():
                continue
            size = src.stat().st_size
            raw += size
            if size > 1 << 20:
                big.append((rel, size))
            tar.add(src, arcname=rel)
    return raw, len(buf.getvalue()), big


def main(argv: list[str]) -> int:
    args = argv[1:]
    out_json = None
    if "--json" in args:
        i = args.index("--json")
        out_json = args[i + 1]
        del args[i:i + 2]
    if len(args) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    meta = json.loads(Path(args[0]).read_text())
    lists = Path(args[1])
    repo = Path(args[2]).resolve()
    ids = set(meta["workspace_members"])
    members = sorted((p for p in meta["packages"] if p["id"] in ids), key=lambda p: p["name"])
    names = {p["name"] for p in members}
    root_prefix = Path(meta.get("workspace_root", str(repo)))
    failures: list[str] = []
    rows = []
    for p in members:
        name = p["name"]
        root = repo / Path(p["manifest_path"]).parent.relative_to(root_prefix)
        lst = lists / f"{name}.list"
        rec: dict = {"name": name, "publish": p.get("publish") != [], "findings": []}
        if not lst.exists() or lst.stat().st_size == 0:
            failures.append(f"{name}: no `cargo package --list` output")
            rec["error"] = "cargo package --list failed"
            rows.append(rec)
            continue
        files = [x for x in lst.read_text().splitlines() if x]
        raw, gz, big = gz_estimate(root, files)
        rec.update({"files": len(files), "bytes": raw, "crate_gz_estimate": gz})
        f = rec["findings"]
        if p.get("publish") == []:
            f.append("R1 publish = false")
        names_in_pkg = [Path(x).name for x in files if LICENCE_NAMES.match(Path(x).name)]
        if p.get("license_file"):
            names_in_pkg.append(Path(p["license_file"]).name)
        ok_texts, need = licence_texts_ok(names_in_pkg, p.get("license") or "")
        if not ok_texts:
            f.append(f"R2 licence text missing from the package: needs {need} for `{p.get('license')}` "
                     "(add the texts to the crate directory or set `license-file`)")
        if not p.get("description"):
            f.append("R3 no description")
        if not p.get("readme"):
            f.append("R4 no readme")
        for d in p.get("dependencies", []):
            if d["name"] in names and d.get("path") and d.get("req") in (None, "*", "") and d.get("kind") != "dev":
                f.append(f"R5 path dependency on {d['name']} without a version requirement")
        for rel, size in big:
            f.append(f"R6 {rel} is {size} bytes (> 1 MiB)")
        if gz > 10 << 20:
            f.append(f"R6 .crate estimate {gz} bytes exceeds the crates.io limit (10 MiB)")
        mpl = [x for x in files if x.endswith(".rs") and "SPDX-License-Identifier: MPL-2.0" in
               (root / x).read_text(errors="replace")[:200]] if (root / "src").is_dir() else []
        if mpl and not any(x.endswith("MPL-2.0.txt") for x in files):
            f.append(f"R7 {len(mpl)} MPL-2.0 file(s) but LICENSES/MPL-2.0.txt is not packaged")
        outside = sorted({x for x in files if x.startswith("tests/") and x.endswith(".rs")
                          and OUTSIDE_RE.search((root / x).read_text(errors="replace"))})
        if outside:
            f.append(f"R8 {len(outside)} test file(s) read data outside the package (e.g. {outside[0]}): "
                     "packaged tests cannot run from the .crate")
        via_validation = sorted({x for x in files if x.startswith("tests/") and x.endswith(".rs")
                                 and "akantu_validation" in (root / x).read_text(errors="replace")})
        if via_validation:
            f.append(f"R8 {len(via_validation)} test file(s) read the workspace fixtures through akantu-validation "
                     f"(e.g. {via_validation[0]}): fixtures_root() is CARGO_MANIFEST_DIR-relative, packaged tests cannot run from the .crate")
        own_root = sorted({x for x in files if x.startswith("src/") and x.endswith(".rs")
                           and "CARGO_MANIFEST_DIR" in (root / x).read_text(errors="replace")
                           and ANCESTOR_RE.search((root / x).read_text(errors="replace"))})
        if own_root:
            f.append(f"R8 {own_root[0]} locates the workspace root from CARGO_MANIFEST_DIR (ancestors().nth(..)): "
                     "the crate cannot work from a .crate")
        if rec["publish"]:
            hard = [x for x in f if x[:2] in ("R2", "R3", "R4", "R5", "R6", "R7")]
            failures.extend(f"{name}: {x}" for x in hard)
        rows.append(rec)
    report = {
        "schema": "akantu-rs-package-report/1",
        "verdict": "FAIL" if failures else "PASS",
        "failures": failures,
        "publishable_members": [r["name"] for r in rows if r.get("publish")],
        "members": rows,
    }
    text = json.dumps(report, indent=2) + "\n"
    if out_json:
        Path(out_json).write_text(text)
    else:
        sys.stdout.write(text)
    for r in rows:
        if "error" in r:
            print(f"{r['name']:<24} ERROR {r['error']}", file=sys.stderr)
            continue
        codes = sorted({x[:2] for x in r["findings"]})
        print(f"{r['name']:<24} files={r['files']:<4} bytes={r['bytes']:<9} gz~{r['crate_gz_estimate']:<8} "
              f"publish={'yes' if r['publish'] else 'no '} findings={','.join(codes) or '-'}", file=sys.stderr)
    for x in failures:
        print(f"FAIL {x}", file=sys.stderr)
    print(f"package dry run: {report['verdict']} ({len(rows)} members, "
          f"{len(report['publishable_members'])} opted into publication)", file=sys.stderr)
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
