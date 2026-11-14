#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (oracle tooling, no Akantu counterpart).
# Case-granular overlay of a staged x86_64 corpus onto a
# copy of a NEWER tree, for a sandbox-only Rust test run against the staged
# goldens (taken from the review's overlay.py version 2).
#  * manifest cases: a case regenerated on x86_64 replaces the WHOLE case
#    directory of the copy (files added/removed as in the regeneration),
#    unless the newer tree changed any file of that case since the staging
#    clone's base (then the newer case is kept whole and listed) or the case
#    is identical;
#  * other tracked fixture files (crate fixtures, fixtures/dump, ...): file by
#    file, unless changed since the base.
# SHA256SUMS lines of replaced/added/removed files are recomputed. Nothing is
# committed. A file-by-file overlay (the former TESTING recipe "replace
# fixtures/ by the staged trees") mixes newer manifests with old-base data
# (one such run gave 7 artefact failures).
# Usage: overlay-staged.py <base-clone> <regen-clone> <copy> <base-sha> <newer-ref> <report>
#   <base-clone>: a clone holding both <base-sha> (the commit the staging clone
#   was made from) and <newer-ref>; <regen-clone>: the staging clone after
#   oracle/x86/regenerate-corpus.sh; <copy>: the tree to overlay (a checkout of
#   <newer-ref>).
import hashlib, json, shutil, subprocess, sys
from pathlib import Path

if len(sys.argv) != 7:
    sys.exit("usage: overlay-staged.py <base-clone> <regen-clone> <copy> <base-sha> <newer-ref> <report>")

base, regen, dst = (Path(a) for a in sys.argv[1:4])
base_sha, main_ref, report = sys.argv[4], sys.argv[5], Path(sys.argv[6])
roots = ["fixtures"] + [str(p.relative_to(base)) for p in sorted(base.glob("crates/*/tests/fixtures"))]
git = lambda *a: subprocess.run(["git", "-C", str(base), *a], capture_output=True, check=True).stdout.decode()
tracked = [f for f in git("ls-files", "-z", "--", *roots).split("\0") if f]
changed_on_main = set(git("diff", "--name-only", "-z", base_sha, main_ref, "--", *roots).split("\0"))
case_dirs = sorted({str(Path(f).parent) for f in tracked if Path(f).name == "manifest.json"})
in_case = lambda f: next((c for c in case_dirs if f.startswith(c + "/")), None)
log, touched = [], set()
for c in case_dirs:
    rc = regen / c
    if not (rc / "manifest.json").exists():
        log.append(f"ABSENT-CASE {c}"); continue
    if json.loads((rc / "manifest.json").read_text()).get("build", {}).get("arch") != "x86_64":
        continue
    files_b = {f for f in tracked if f.startswith(c + "/")}
    files_r = {str(p.relative_to(regen)) for p in rc.rglob("*") if p.is_file()}
    if any(f in changed_on_main for f in files_b):
        log.append(f"KEPT-MAIN-CASE {c}"); continue
    if files_b == files_r and all((base / f).read_bytes() == (regen / f).read_bytes() for f in files_b):
        continue
    shutil.rmtree(dst / c)
    shutil.copytree(rc, dst / c)
    touched |= files_b | files_r
    log.append(f"CASE {c} ({len(files_r - files_b)} added, {len(files_b - files_r)} removed)")
for f in tracked:
    if Path(f).name == "SHA256SUMS" or in_case(f):
        continue
    b, r = base / f, regen / f
    if not r.exists():
        log.append(f"ABSENT {f}"); continue
    if b.read_bytes() == r.read_bytes():
        continue
    if f in changed_on_main:
        log.append(f"KEPT-MAIN {f}"); continue
    (dst / f).write_bytes(r.read_bytes()); touched.add(f)
    log.append(f"FILE {f}")
# SHA256SUMS: drop lines of removed files, recompute touched ones, add new ones.
fixed = 0
for sums in sorted(dst.glob("**/SHA256SUMS")):
    if ".git" in sums.parts:
        continue
    d = sums.parent; drel = str(d.relative_to(dst))
    entries = {}
    for line in sums.read_text().splitlines():
        h, _, p = line.partition("  ")
        if p:
            entries[p] = h
    for t in touched:
        if t.startswith(drel + "/") or drel == ".":
            p = t[len(drel) + 1:] if drel != "." else t
            if p in entries or (dst / t).exists():
                if (dst / t).exists():
                    entries[p] = hashlib.sha256((dst / t).read_bytes()).hexdigest()
                else:
                    entries.pop(p, None)
                fixed += 1
    sums.write_text("".join(f"{h}  {p}\n" for p, h in sorted(entries.items())))
cases = sum(l.startswith("CASE ") for l in log); kept = sum(l.startswith("KEPT-MAIN-CASE") for l in log)
files = sum(l.startswith("FILE ") for l in log); kf = sum(l.startswith("KEPT-MAIN ") for l in log)
report.write_text(f"replaced {cases} cases and {files} other files; kept main's version of {kept} cases and {kf} files changed on main "
                  f"since {base_sha[:8]}; {fixed} SHA256SUMS entries updated\n" + "\n".join(log) + "\n")
print(report.read_text().splitlines()[0])
