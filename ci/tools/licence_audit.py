#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu; licence counterpart of the
# upstream dependency checks in packages/*.cmake, parity rows CI-026,
# CI-029): a cargo-deny-style licence audit with the D-042 LGPL/GPL
# separation check, using only cargo (metadata, tree) and the Python
# standard library (Python >= 3.11 for tomllib).
#
#   licence_audit.py [--repo DIR] [--policy FILE] [--cargo CARGO] [--offline]
#                    [--report FILE]
#
# Checks (each violation is a failure; the policy is ci/licence-policy.toml):
#   T1  every third-party package in the all-features, all-targets resolve
#       has an SPDX licence expression satisfiable with the allow-list (or a
#       reviewed entry); legacy "/" is OR;
#   W1  every workspace member declares its licence: the default
#       (LGPL-3.0-or-later), "LGPL-3.0-or-later AND MPL-2.0" for the listed
#       MPL members, "GPL-3.0-or-later" only for the listed GPL members;
#   G1  D-042: no non-GPL member reaches a GPL member through its default
#       features (`cargo tree -e normal,build,dev --target all`);
#   G2  D-042: with all features, a GPL member enters a non-GPL member's
#       closure only through features listed in [workspace.gpl_features],
#       each alone, none of them default ("a build with them is
#       GPL-3.0-or-later as a whole" is reported);
#   H1  every .rs file anywhere under a member's directory (tracked or
#       untracked-not-ignored: src, tests, examples, benches, build.rs, and
#       also generated/ or vendored directories pulled in with include!)
#       starts with "// SPDX-License-Identifier: <id>" for an id accepted
#       for the member's licence class (GPL members: GPL only);
#   H3  every C/C++ source or header under a member's directory (.c .h .cc
#       .cpp .hpp) has an SPDX line in its first three lines, and ANY other
#       file of the member that carries an SPDX line in its first ten lines
#       carries an id accepted for the member's class (a GPL-headed shim or
#       data file inside an LGPL crate fails, R7);
#   G4  a non-GPL member does not include!/include_str!/include_bytes!/
#       #[path] a file that lives in a GPL member or carries a GPL SPDX id
#       (R8);
#   G3  every feature the policy allows to bring GPL code in ([workspace.
#       gpl_features]) really does so: the detection must stay live, as in
#       crates/akantu-geometry/tests/license_separation.rs (which requires
#       `cgal`/`igfem` to reach the GPL crate; the two guards must not
#       drift: change both together);
#   H2  every tracked tooling file matching [headers].tooling_globs has an
#       SPDX-License-Identifier line in its first three lines.
# The JSON report (schema akantu-rs-licence-audit/1) goes to --report or
# stdout; a human summary goes to stderr. Exit 0 = no violation, 1 =
# violations, 2 = the audit could not run.
"""Licence audit of the akantu-rs workspace (D-042 separation included)."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python < 3.11
    tomllib = None

SCHEMA = "akantu-rs-licence-audit/1"


# --------------------------------------------------------------------------
# SPDX expressions
# --------------------------------------------------------------------------
class SpdxError(ValueError):
    pass


def tokenize(expr: str) -> list[str]:
    expr = expr.replace("/", " OR ")
    toks = re.findall(r"\(|\)|[^\s()]+", expr)
    if not toks:
        raise SpdxError("empty licence expression")
    return toks


def parse(expr: str):
    """Parse to nested tuples: ('or', [..]), ('and', [..]), ('id', 'X'),
    ('with', 'X', 'EXC')."""
    toks = tokenize(expr)
    pos = 0

    def peek():
        return toks[pos] if pos < len(toks) else None

    def take():
        nonlocal pos
        t = peek()
        pos += 1
        return t

    def primary():
        t = take()
        if t is None:
            raise SpdxError(f"unexpected end of {expr!r}")
        if t == "(":
            node = or_expr()
            if take() != ")":
                raise SpdxError(f"missing ')' in {expr!r}")
            return node
        if t in (")", "AND", "OR", "WITH"):
            raise SpdxError(f"unexpected {t!r} in {expr!r}")
        ident = t[:-1] + "-or-later" if t.endswith("+") else t
        if peek() == "WITH":
            take()
            exc = take()
            if exc is None or exc in ("(", ")", "AND", "OR", "WITH"):
                raise SpdxError(f"bad exception in {expr!r}")
            return ("with", ident, exc)
        return ("id", ident)

    def and_expr():
        items = [primary()]
        while peek() == "AND":
            take()
            items.append(primary())
        return items[0] if len(items) == 1 else ("and", items)

    def or_expr():
        items = [and_expr()]
        while peek() == "OR":
            take()
            items.append(and_expr())
        return items[0] if len(items) == 1 else ("or", items)

    node = or_expr()
    if peek() is not None:
        raise SpdxError(f"trailing {peek()!r} in {expr!r}")
    return node


def satisfiable(node, allow: set[str]) -> bool:
    kind = node[0]
    if kind == "id":
        return node[1] in allow
    if kind == "with":
        # An exception only adds permissions: allowed if the exact
        # "X WITH E" or the bare licence is allowed.
        return f"{node[1]} WITH {node[2]}" in allow or node[1] in allow
    if kind == "and":
        return all(satisfiable(n, allow) for n in node[1])
    return any(satisfiable(n, allow) for n in node[1])


def ids_of(node) -> set[str]:
    if node[0] == "id":
        return {node[1]}
    if node[0] == "with":
        return {node[1]}
    out: set[str] = set()
    for n in node[1]:
        out |= ids_of(n)
    return out


# --------------------------------------------------------------------------
# cargo
# --------------------------------------------------------------------------
class Cargo:
    def __init__(self, cargo: str, repo: Path, offline: bool):
        self.cargo = cargo
        self.repo = repo
        self.flags = ["--locked"] + (["--offline"] if offline else [])
        self.calls = 0

    def run(self, args: list[str]) -> str:
        self.calls += 1
        cmd = [self.cargo, *args, *self.flags]
        r = subprocess.run(cmd, cwd=self.repo, capture_output=True, text=True)
        if r.returncode != 0:
            raise RuntimeError(f"{' '.join(cmd)} failed ({r.returncode}):\n{r.stderr[-4000:]}")
        return r.stdout

    def metadata(self, extra: list[str]) -> dict:
        return json.loads(self.run(["metadata", "--format-version", "1", *extra]))

    def closure(self, member: str, features: list[str]) -> set[str]:
        """Package names in the dependency closure of `member` (itself
        included) for the given feature arguments."""
        out = self.run(["tree", "-p", member, "-e", "normal,build,dev", "--target", "all",
                        "--prefix", "none", "--format", "{p}", *features])
        names = set()
        for line in out.splitlines():
            line = line.strip()
            if line and not line.startswith("["):
                names.add(line.split()[0])
        return names


# --------------------------------------------------------------------------
# files
# --------------------------------------------------------------------------
SPDX_RE = re.compile(r"SPDX-License-Identifier:\s*([^\s*]+(?:\s+(?:AND|OR|WITH)\s+[^\s*]+)*)")


def member_files(repo: Path, root: Path) -> list[Path]:
    """Every file under a member directory: tracked plus untracked-not-ignored
    in a git checkout, else a walk that skips target/ and .git/."""
    try:
        rel = root.relative_to(repo)
        r = subprocess.run(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", str(rel)],
                           cwd=repo, capture_output=True)
        if r.returncode == 0 and r.stdout:
            return sorted({repo / p for p in r.stdout.decode().split("\0") if p and (repo / p).is_file()})
    except (OSError, ValueError):
        pass
    files: list[Path] = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(x for x in dirnames if x not in ("target", ".git"))
        files.extend(Path(dirpath) / fn for fn in sorted(filenames))
    return files


C_SUFFIXES = (".c", ".h", ".cc", ".cpp", ".hpp")
INCLUDE_RE = re.compile(r'\b(?:include|include_str|include_bytes)\s*!\s*\(\s*"([^"]+)"\s*\)|#\s*\[\s*path\s*=\s*"([^"]+)"\s*\]')


def spdx_ids(path: Path, nlines: int) -> list[str] | None:
    """SPDX ids in the first `nlines` lines (first 4 KiB); None for a binary file."""
    try:
        with open(path, "rb") as f:
            head = f.read(4096)
    except OSError:
        return []
    if b"\0" in head:
        return None
    lines = head.decode("utf-8", errors="replace").splitlines()[:nlines]
    out = []
    for line in lines:
        m = SPDX_RE.search(line)
        if m:
            out.append(m.group(1))
    return out


def first_lines(path: Path, n: int) -> list[str]:
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return [f.readline().rstrip("\n") for _ in range(n)]
    except OSError:
        return []


def tracked(repo: Path, globs: list[str]) -> list[Path]:
    try:
        r = subprocess.run(["git", "ls-files", "-z", "--", *globs], cwd=repo, capture_output=True)
        if r.returncode == 0:
            return [repo / p for p in r.stdout.decode().split("\0") if p]
    except OSError:
        pass
    found: set[Path] = set()  # no git: filesystem globbing (`**` for `*/`)
    for g in globs:
        found.update(repo.glob(g.replace("/*.", "/**/*.")))
    return sorted(p for p in found if "target" not in p.relative_to(repo).parts)


# --------------------------------------------------------------------------
# audit
# --------------------------------------------------------------------------
def audit(repo: Path, policy: dict, cargo: Cargo) -> dict:
    failures: list[str] = []
    warnings: list[str] = []
    tp = policy.get("third_party", {})
    allow = set(tp.get("allow", []))
    deny = set(tp.get("deny", []))
    reviewed = tp.get("reviewed", {})
    ws = policy.get("workspace", {})
    default_lic = ws.get("default", "LGPL-3.0-or-later")
    mpl_members = set(ws.get("mpl_members", []))
    gpl_members_policy = set(ws.get("gpl_members", []))
    gpl_features = {k: set(v) for k, v in ws.get("gpl_features", {}).items()}
    headers = policy.get("headers", {})

    meta = cargo.metadata(["--all-features"])
    member_ids = set(meta["workspace_members"])
    resolved = {n["id"] for n in meta["resolve"]["nodes"]}
    pkgs = {p["id"]: p for p in meta["packages"]}
    members = sorted((pkgs[i] for i in member_ids), key=lambda p: p["name"])

    # T1 third-party licences
    third = []
    for pid in sorted(resolved - member_ids, key=lambda i: (pkgs[i]["name"], pkgs[i]["version"])):
        p = pkgs[pid]
        name, ver, lic = p["name"], p["version"], p.get("license")
        rec = {"name": name, "version": ver, "license": lic, "ok": True, "reason": None}
        key = f"{name} {ver}"
        if key in reviewed:
            rec["reason"] = f"reviewed: {reviewed[key]}"
        elif not lic:
            rec["ok"] = False
            rec["reason"] = f"no SPDX licence expression (license-file {p.get('license_file')!r}): needs a [third_party.reviewed] entry"
        else:
            try:
                node = parse(lic)
                if not satisfiable(node, allow):
                    rec["ok"] = False
                    bad = sorted(ids_of(node) - allow)
                    rec["reason"] = f"not satisfiable with the allow-list (outside it: {', '.join(bad)})"
                    if ids_of(node) & deny:
                        rec["reason"] += "; contains a denied licence"
            except SpdxError as e:
                rec["ok"] = False
                rec["reason"] = f"unparsable licence expression: {e}"
        if not rec["ok"]:
            failures.append(f"T1 {name} {ver}: {rec['reason']}")
        third.append(rec)

    # W1 member licences; class per member
    gpl_members = set()
    mrecs = []
    for p in members:
        name, lic = p["name"], p.get("license")
        if name in gpl_members_policy:
            want, cls = {"GPL-3.0-or-later"}, "gpl"
        elif name in mpl_members:
            want, cls = {f"{default_lic} AND MPL-2.0", default_lic}, "mpl"
        else:
            want, cls = {default_lic}, "lgpl"
        rec = {"name": name, "license": lic, "class": cls, "ok": True, "problems": []}
        if lic not in want:
            rec["problems"].append(f"W1 licence {lic!r}, expected {' or '.join(sorted(repr(w) for w in want))}")
        if lic and "GPL" in lic.replace("LGPL", ""):
            gpl_members.add(name)
            if name not in gpl_members_policy:
                rec["problems"].append("W1 GPL licence on a member not listed in [workspace].gpl_members (D-042)")
        mrecs.append(rec)
    by_name = {r["name"]: r for r in mrecs}

    # G1/G2 dependency graph separation (only meaningful if GPL members exist)
    gpl_builds = []
    for p in members:
        name = p["name"]
        rec = by_name[name]
        rec["default_closure_gpl"] = []
        rec["all_features_closure_gpl"] = []
        rec["gpl_enabling_features"] = []
        if not gpl_members or rec["class"] == "gpl":
            continue
        base = sorted(cargo.closure(name, ["--no-default-features"]) & gpl_members)
        dflt = sorted(cargo.closure(name, []) & gpl_members)
        rec["default_closure_gpl"] = dflt
        if base:
            rec["problems"].append(
                f"G1 GPL member(s) {', '.join(base)} reached even with --no-default-features (D-042: LGPL crates "
                f"never depend on them unconditionally; `cargo tree -p {name} -i {base[0]} -e normal,build,dev`)")
        elif dflt:
            rec["problems"].append(
                f"G1 default features pull GPL member(s) {', '.join(dflt)} (D-042: OFF by default; "
                f"`cargo tree -p {name} -i {dflt[0]} -e normal,build,dev`)")
        allf = sorted(cargo.closure(name, ["--all-features"]) & gpl_members)
        rec["all_features_closure_gpl"] = allf
        if not allf or base:
            continue
        feats = p.get("features", {})
        defaults = set(feats.get("default", []))
        enabling = []
        for f in sorted(x for x in feats if x != "default"):
            if cargo.closure(name, ["--no-default-features", "--features", f]) & gpl_members:
                enabling.append(f)
        rec["gpl_enabling_features"] = enabling
        allowed = gpl_features.get(name, set())
        if not enabling:
            rec["problems"].append("G2 a GPL member enters only through a feature combination: review (D-042)")
        for f in enabling:
            if f not in allowed:
                rec["problems"].append(f"G2 feature `{f}` brings in GPL code but is not listed in [workspace.gpl_features] (D-042)")
            if f in defaults:
                rec["problems"].append(f"G2 GPL-enabling feature `{f}` is a default feature (D-042: OFF by default)")
            gpl_builds.append(f"{name} --features {f}")
    # G3 every allowed GPL feature must really bring GPL code in (live detection)
    for name, allowed in sorted(gpl_features.items()):
        if name not in by_name:
            failures.append(f"G3 policy lists gpl_features for unknown member {name}")
        elif gpl_members and by_name[name]["class"] != "gpl":
            for f in sorted(allowed - set(by_name[name]["gpl_enabling_features"])):
                by_name[name]["problems"].append(
                    f"G3 policy allows feature `{f}` to bring GPL code in but it does not: the detection is dead "
                    "(D-042; crates/akantu-geometry/tests/license_separation.rs requires it to be live)")

    # H1/H3/G4 member file headers and includes
    root_of = {p["name"]: Path(p["manifest_path"]).parent for p in members}
    gpl_roots = [root_of[n].resolve() for n in sorted(gpl_members | (gpl_members_policy & set(root_of)))]
    nfiles = 0
    nall = 0

    def rel(f: Path) -> str:
        return str(f.relative_to(repo)) if f.is_relative_to(repo) else str(f)

    for rec in mrecs:
        accepted = set(headers.get(rec["class"], []))
        bad = []
        bad3 = []
        bad4 = []
        for f in member_files(repo, root_of[rec["name"]]):
            nall += 1
            if f.suffix == ".rs":
                nfiles += 1
                line = first_lines(f, 1)
                m = SPDX_RE.search(line[0]) if line and line[0].startswith("//") else None
                if not m or m.group(1) not in accepted:
                    bad.append(f"{rel(f)}: {m.group(1) if m else 'none'}")
                if rec["class"] != "gpl":
                    try:
                        text = f.read_text(errors="replace")
                    except OSError:
                        text = ""
                    for mm in INCLUDE_RE.finditer(text):
                        target = (f.parent / (mm.group(1) or mm.group(2))).resolve()
                        if any(target.is_relative_to(g) for g in gpl_roots):
                            bad4.append(f"{rel(f)} includes {mm.group(1) or mm.group(2)} (inside a GPL member)")
                        elif target.is_file():
                            ids = spdx_ids(target, 10) or []
                            if any(i not in accepted for i in ids):
                                bad4.append(f"{rel(f)} includes {mm.group(1) or mm.group(2)} ({', '.join(ids)})")
                continue
            ids = spdx_ids(f, 3 if f.suffix in C_SUFFIXES else 10)
            if ids is None:
                continue
            if f.suffix in C_SUFFIXES and not ids:
                bad3.append(f"{rel(f)}: no SPDX line in the first 3 lines")
            for i in ids:
                if i not in accepted:
                    bad3.append(f"{rel(f)}: {i}")
        rec["header_violations"] = bad + bad3
        if bad:
            rec["problems"].append(f"H1 {len(bad)} .rs file(s) without an accepted SPDX first line ({', '.join(sorted(accepted))}): "
                                   + "; ".join(bad[:10]) + (" ..." if len(bad) > 10 else ""))
        if bad3:
            rec["problems"].append(f"H3 {len(bad3)} non-.rs file(s) with a missing or unaccepted SPDX id ({', '.join(sorted(accepted))}): "
                                   + "; ".join(bad3[:10]) + (" ..." if len(bad3) > 10 else ""))
        if bad4:
            rec["problems"].append(f"G4 {len(bad4)} include of GPL code into a non-GPL member (D-042): "
                                   + "; ".join(bad4[:10]) + (" ..." if len(bad4) > 10 else ""))
    # H2 tooling files
    tool_bad = []
    tool_files = tracked(repo, headers.get("tooling_globs", []))
    for f in tool_files:
        if not any("SPDX-License-Identifier:" in x for x in first_lines(f, 3)):
            tool_bad.append(str(f.relative_to(repo)))
    for b in tool_bad:
        failures.append(f"H2 {b}: no SPDX-License-Identifier line in the first 3 lines")

    for rec in mrecs:
        rec["ok"] = not rec["problems"]
        failures.extend(f"{rec['name']}: {pr}" for pr in rec["problems"])

    return {
        "schema": SCHEMA,
        "verdict": "FAIL" if failures else "PASS",
        "failures": failures,
        "warnings": warnings,
        "counts": {
            "third_party_packages": len(third),
            "third_party_violations": sum(1 for t in third if not t["ok"]),
            "members": len(mrecs),
            "gpl_members": sorted(gpl_members),
            "member_rs_files_checked": nfiles,
            "member_files_checked": nall,
            "tooling_files_checked": len(tool_files),
            "cargo_invocations": cargo.calls,
        },
        "gpl_builds": gpl_builds,
        "native": policy.get("native", {}),
        "members": mrecs,
        "third_party": third,
    }


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--repo", default=".")
    ap.add_argument("--policy", default=None)
    ap.add_argument("--cargo", default=os.environ.get("CARGO", "cargo"))
    ap.add_argument("--offline", action="store_true")
    ap.add_argument("--report", default=None)
    a = ap.parse_args(argv[1:])
    if tomllib is None:
        print("licence_audit.py needs Python >= 3.11 (tomllib)", file=sys.stderr)
        return 2
    repo = Path(a.repo).resolve()
    policy_path = Path(a.policy) if a.policy else repo / "ci" / "licence-policy.toml"
    try:
        raw = policy_path.read_bytes()
        policy = tomllib.loads(raw.decode())
    except (OSError, tomllib.TOMLDecodeError) as e:
        print(f"cannot read policy {policy_path}: {e}", file=sys.stderr)
        return 2
    cargo = Cargo(a.cargo, repo, a.offline)
    try:
        report = audit(repo, policy, cargo)
    except (RuntimeError, OSError, KeyError, json.JSONDecodeError) as e:
        print(f"audit could not run: {e}", file=sys.stderr)
        return 2
    report["policy"] = {"path": str(policy_path), "sha256": hashlib.sha256(raw).hexdigest()}
    try:
        report["cargo_version"] = subprocess.run([a.cargo, "--version"], capture_output=True,
                                                 text=True).stdout.strip()
    except OSError:
        report["cargo_version"] = None
    text = json.dumps(report, indent=2) + "\n"
    if a.report:
        Path(a.report).write_text(text)
    else:
        sys.stdout.write(text)
    c = report["counts"]
    print(f"licence audit: {c['third_party_packages']} third-party packages "
          f"({c['third_party_violations']} violations), {c['members']} members, "
          f"GPL members {c['gpl_members'] or 'none'}, {c['member_rs_files_checked']} member .rs files "
          f"({c['member_files_checked']} member files), "
          f"{c['tooling_files_checked']} tooling files, {c['cargo_invocations']} cargo calls", file=sys.stderr)
    for g in report["gpl_builds"]:
        print(f"  GPL-3.0-or-later build as a whole: {g}", file=sys.stderr)
    for w in report["warnings"]:
        print(f"  warning: {w}", file=sys.stderr)
    for f in report["failures"]:
        print(f"  FAIL {f}", file=sys.stderr)
    print(f"licence audit: {report['verdict']}", file=sys.stderr)
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
