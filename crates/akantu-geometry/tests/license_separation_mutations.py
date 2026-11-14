#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Mutation campaign for the D-042 licence-separation guard
# (tests/license_separation.rs). No upstream
# counterpart.
"""Apply each bypass form to the real workspace manifests, run the guard,
record which of its tests fail, and restore the manifests.

Usage (from anywhere inside the repository, clean manifests required):

    python3 -I crates/akantu-geometry/tests/license_separation_mutations.py

Every mutation must make at least one guard test fail ("killed"); the
controls must leave every test passing. Exit status 0 only if both hold.
The manifests and Cargo.lock are restored with `git checkout` after every
case, also on error.
"""

import os
import re
import subprocess
import sys

ROOT = subprocess.run(
    ["git", "rev-parse", "--show-toplevel"], check=True, capture_output=True, text=True
).stdout.strip()
UMB = "crates/akantu/Cargo.toml"
EMB = "crates/akantu-embedded/Cargo.toml"
IGF = "crates/akantu-igfem/Cargo.toml"
XMAT = "crates/akantu-extra-materials/Cargo.toml"
TOUCHED = [UMB, EMB, IGF, XMAT, "Cargo.lock"]


def read(p):
    with open(os.path.join(ROOT, p)) as f:
        return f.read()


def write(p, s):
    with open(os.path.join(ROOT, p), "w") as f:
        f.write(s)


def after_header(path, header, lines):
    """Insert `lines` right after the table header `header`."""
    s = read(path)
    if header in s:
        s = s.replace(header + "\n", header + "\n" + lines + "\n", 1)
    else:
        # Insert a new table before [lints] (or at the end).
        block = header + "\n" + lines + "\n\n"
        s = s.replace("[lints]", block + "[lints]", 1) if "[lints]" in s else s + "\n" + block
    write(path, s)


def append(path, text):
    write(path, read(path).rstrip("\n") + "\n\n" + text + "\n")


def sub(path, old, new):
    s = read(path)
    if old not in s:
        raise SystemExit(f"{path}: pattern not found: {old!r}")
    write(path, s.replace(old, new, 1))


UMB_DEFAULT = 'default = ["solid", "cohesive", "contact", "diffusion", "phase-field", "couplers", "structural", "embedded"'
UMB_IGFEM = 'igfem = ["solid", "cgal", "dep:akantu-igfem", "akantu-igfem/cgal"]'
CGAL_OPT = 'akantu-geometry-cgal = { workspace = true, optional = true }'

MUTATIONS = [
    # --- M1-M5 ------------------------------------------
    ("M1 inline `akantu-geometry-cgal.workspace = true` in akantu-embedded",
     lambda: after_header(EMB, "[dependencies]", "akantu-geometry-cgal.workspace = true")),
    ("M2 table form `[dependencies.akantu-geometry-cgal]` in akantu-embedded",
     lambda: append(EMB, "[dependencies.akantu-geometry-cgal]\nworkspace = true")),
    ("M3 renamed `geocgal = { package = \"akantu-geometry-cgal\", path = … }` in akantu-embedded",
     lambda: after_header(EMB, "[dependencies]",
                          'geocgal = { package = "akantu-geometry-cgal", path = "../akantu-geometry-cgal" }')),
    ("M4 `cgal` in the umbrella's default",
     lambda: sub(UMB, UMB_DEFAULT, UMB_DEFAULT + ', "cgal"')),
    ("M5 `embedded = [..., \"cgal\"]` in the umbrella",
     lambda: sub(UMB, 'embedded = ["solid", "dep:akantu-embedded"]',
                 'embedded = ["solid", "dep:akantu-embedded", "cgal"]')),
    # --- additions ---------------------------------------------
    ("GEOMFIX-1 `[target.'cfg(windows)'.dev-dependencies]` GPL entry in akantu-embedded",
     lambda: append(EMB, "[target.'cfg(windows)'.dev-dependencies]\nakantu-geometry-cgal.workspace = true")),
    ("GEOMFIX-2 umbrella `igfem` without `cgal` (still enabling akantu-igfem/cgal)",
     lambda: sub(UMB, UMB_IGFEM, 'igfem = ["solid", "dep:akantu-igfem", "akantu-igfem/cgal"]')),
    ("GEOMFIX-3 `igfem` in the umbrella's default (= IG-01 form 3 through the umbrella)",
     lambda: sub(UMB, UMB_DEFAULT, UMB_DEFAULT + ', "igfem"')),
    ("GEOMFIX-4 akantu-embedded `cgal` feature in its own default",
     lambda: (after_header(EMB, "[dependencies]", CGAL_OPT),
              after_header(EMB, "[features]", 'default = ["cgal"]\ncgal = ["dep:akantu-geometry-cgal"]'))),
    ("IG01-0 umbrella `igfem` no longer enabling akantu-igfem/cgal (generator dropped)",
     lambda: sub(UMB, UMB_IGFEM, 'igfem = ["solid", "cgal", "dep:akantu-igfem"]')),
    # --- form 1: a listed feature turned on by default ----
    ("IG01-1 akantu-igfem `default = [\"cgal\"]`",
     lambda: sub(IGF, 'cgal = ["dep:akantu-geometry-cgal"]',
                 'default = ["cgal"]\ncgal = ["dep:akantu-geometry-cgal"]')),
    # --- form 2: a second, unlisted optional path ---------------------------
    ("IG01-2a akantu-embedded: optional GPL dependency behind unlisted feature `aabb`",
     lambda: (after_header(EMB, "[dependencies]", CGAL_OPT),
              after_header(EMB, "[features]", 'aabb = ["dep:akantu-geometry-cgal"]'))),
    ("IG01-2b akantu-igfem: unlisted feature `gel = [\"cgal\"]`",
     lambda: sub(IGF, 'cgal = ["dep:akantu-geometry-cgal"]',
                 'cgal = ["dep:akantu-geometry-cgal"]\ngel = ["cgal"]')),
    ("IG01-2c akantu-igfem: second (renamed) optional GPL dependency behind `spheres`",
     lambda: (sub(IGF, 'cgal = ["dep:akantu-geometry-cgal"]',
                  'cgal = ["dep:akantu-geometry-cgal"]\nspheres = ["dep:cgal2"]'),
              after_header(IGF, "[dependencies]",
                           'cgal2 = { package = "akantu-geometry-cgal", path = "../akantu-geometry-cgal", optional = true }'))),
    ("IG01-2d akantu-igfem: optional GPL dependency as an implicit feature (no `dep:`)",
     lambda: after_header(IGF, "[dependencies]",
                          'cgal3 = { package = "akantu-geometry-cgal", path = "../akantu-geometry-cgal", optional = true }')),
    ("IG01-2e umbrella: unlisted feature `gel` forwarding to akantu-igfem/cgal",
     lambda: sub(UMB, UMB_IGFEM, UMB_IGFEM + '\ngel = ["dep:akantu-igfem", "akantu-igfem/cgal"]')),
    # --- form 3: a listed feature enabled transitively by another crate ------
    ("IG01-3a akantu-extra-materials default feature enabling akantu-igfem/cgal",
     lambda: (after_header(XMAT, "[dependencies]", "akantu-igfem = { workspace = true, optional = true }"),
              after_header(XMAT, "[features]",
                           'default = ["igfem-gel"]\nigfem-gel = ["dep:akantu-igfem", "akantu-igfem/cgal"]'))),
    ("IG01-3b akantu-extra-materials: `akantu-igfem = { workspace = true, features = [\"cgal\"] }`",
     lambda: after_header(XMAT, "[dependencies]", 'akantu-igfem = { workspace = true, features = ["cgal"] }')),
    ("IG01-3c akantu-extra-materials: optional akantu-igfem with `features = [\"cgal\"]` behind default `gel`",
     lambda: (after_header(XMAT, "[dependencies]",
                           'akantu-igfem = { workspace = true, optional = true, features = ["cgal"] }'),
              after_header(XMAT, "[features]", 'default = ["gel"]\ngel = ["dep:akantu-igfem"]'))),
]

CONTROLS = [
    ("CONTROL unchanged manifests", lambda: None),
    ("CONTROL akantu-igfem gains an LGPL feature `extra = []`",
     lambda: sub(IGF, 'cgal = ["dep:akantu-geometry-cgal"]', 'cgal = ["dep:akantu-geometry-cgal"]\nextra = []')),
    ("CONTROL akantu-extra-materials gains an optional akantu-igfem without `cgal`",
     lambda: (after_header(XMAT, "[dependencies]", "akantu-igfem = { workspace = true, optional = true }"),
              after_header(XMAT, "[features]", 'igfem = ["dep:akantu-igfem"]'))),
]


def restore():
    subprocess.run(["git", "checkout", "--"] + TOUCHED, cwd=ROOT, check=True)


def run_guard():
    p = subprocess.run(
        ["cargo", "test", "-q", "-p", "akantu-geometry", "--test", "license_separation"],
        cwd=ROOT, capture_output=True, text=True,
    )
    out = p.stdout + p.stderr
    failed = sorted(set(re.findall(r"^---- (\S+) stdout ----", out, re.M)))
    passed = re.search(r"test result: \w+\. (\d+) passed; (\d+) failed", out)
    if passed is None:
        return None, out
    first = []
    for name in failed:
        m = re.search(r"^---- " + re.escape(name) + r" stdout ----\n(.*?)(?:\n\n|\Z)", out, re.M | re.S)
        if m:
            lines = [l for l in m.group(1).splitlines() if l.strip() and not l.startswith("thread ")]
            first.append(f"{name}: {lines[0][:160] if lines else ''}"
                         + (f" | {lines[1][:200]}" if len(lines) > 1 else ""))
    return failed, "\n      ".join(first)


def main():
    dirty = subprocess.run(["git", "status", "--porcelain", "--"] + TOUCHED, cwd=ROOT,
                           capture_output=True, text=True).stdout.strip()
    if dirty:
        raise SystemExit(f"refusing to run: modified files\n{dirty}")
    ok = True
    try:
        for kind, cases in (("mutation", MUTATIONS), ("control", CONTROLS)):
            for name, apply in cases:
                apply()
                failed, detail = run_guard()
                restore()
                if failed is None:
                    print(f"ERROR   {name}: the guard did not run\n{detail[-2000:]}")
                    ok = False
                    continue
                if kind == "mutation":
                    good = bool(failed)
                    verdict = "KILLED " if good else "SURVIVED"
                else:
                    good = not failed
                    verdict = "PASS   " if good else "FAILED "
                ok &= good
                print(f"{verdict} {name}")
                if failed:
                    print(f"      failing: {', '.join(failed)}")
                    print(f"      {detail}")
                sys.stdout.flush()
    finally:
        restore()
    print("ALL MUTATIONS KILLED, CONTROLS PASS" if ok else "CAMPAIGN FAILED")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
