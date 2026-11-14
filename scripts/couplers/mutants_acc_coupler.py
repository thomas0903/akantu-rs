#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); project tooling, no
# C++ counterpart. Mutation runs on the coupled interplay of
# the solid / phase-field coupler (INT-004): the material's stress split
# against the law's driving-energy split. Apply one mutant, run the
# coupler's phase-field replays (coupler_fixtures, the pf tests: every case
# in its own unwinding scope, so every killing case is named), record the
# failing tests and cases, restore the file (git checkout + touch: a restored
# file must not keep an older mtime, or cargo keeps the mutated build).
# M00 is the baseline (no mutation): every listed test must pass.
# Usage: mutants_acc_coupler.py <repo clone> <out.json> [M00,M01,...]
#        (release profile, crate-scoped: cargo test -p akantu-couplers)
import json
import os
import re
import subprocess
import sys

REPO = sys.argv[1]
OUT = sys.argv[2]
MUTANTS = [
    ("M00", "crates/akantu-couplers/src/lib.rs", "baseline (no mutation)",
     "#![forbid(unsafe_code)]", "#![forbid(unsafe_code)]"),
    # The solid `phasefield` material degrades the part of the stress its
    # split leaves undegraded (sigma = g(d) (sigma+ + sigma-) instead of
    # g(d) sigma+ + sigma-): with the volumetric-deviatoric split the
    # volumetric compression is degraded although the law's driving energy
    # never lets it drive the damage.
    ("M01", "crates/akantu-solid/src/materials/phasefield.rs",
     "material: compressive volumetric stress degraded (sigma = g(d)(sigma+ + sigma-))",
     "s[j][i] = g_d * plus[j][i] + minus[j][i];",
     "s[j][i] = g_d * (plus[j][i] + minus[j][i]);"),
    # The AT1 (`linear`) law drives its damage with the no-split energy
    # whatever its split option, while the material keeps its own split.
    ("M02", "crates/akantu-phase-field/src/laws.rs",
     "AT1 law: driving energy without its split (EnergySplitKind::NoSplit)",
     "        self.base.set_init();\n        self.split = Some(EnergySplit::new(\n            self.split_kind,",
     "        self.base.set_init();\n        self.split = Some(EnergySplit::new(\n            EnergySplitKind::NoSplit,"),
]
TESTS = ["coupler_solid_phase_field_fixtures",
         "coupler_solid_phase_field_corrected_single_law_identical",
         "coupler_solid_phase_field_default_mode_differences"]
env = dict(os.environ)
env.update(CARGO_TARGET_DIR=os.path.join(REPO, "target"), CARGO_BUILD_JOBS="4",
           CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="line-tables-only",
           CARGO_PROFILE_TEST_DEBUG="line-tables-only")
only = set(sys.argv[3].split(",")) if len(sys.argv) > 3 else None
results = []
for mid, path, desc, old, new in MUTANTS:
    if only and mid not in only:
        continue
    p = os.path.join(REPO, path)
    src = open(p).read()
    if src.count(old) != 1:
        results.append({"id": mid, "error": "pattern count %d" % src.count(old)})
        print(mid, "PATTERN", src.count(old), flush=True)
        continue
    open(p, "w").write(src.replace(old, new))
    try:
        cmd = ["cargo", "test", "-p", "akantu-couplers", "--release", "--test",
               "coupler_fixtures", "--no-fail-fast", "--", "coupler_solid_phase_field",
               "--test-threads=3"]
        r = subprocess.run(cmd, cwd=REPO, env=env, capture_output=True, text=True,
                           timeout=3000)
        out = r.stdout + r.stderr
        failed = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED", out, re.M)))
        passed = sorted(set(re.findall(r"^test (\S+) \.\.\. ok", out, re.M)))
        cases = sorted(set(re.findall(r"^FAILED CASE (\S+):", out, re.M)))
        first = [l for l in out.splitlines() if l.startswith("FAILED CASE")][:20]
        build_error = "error[" in out and not passed and not failed
        killed = bool(failed) and not build_error
        rec = {"id": mid, "file": path, "description": desc, "exit": r.returncode,
               "failed_tests": failed, "passed_tests": passed, "killing_cases": cases,
               "first_messages": first, "build_error": build_error,
               "killed": killed if mid != "M00" else None}
        if mid == "M00":
            rec["baseline_ok"] = r.returncode == 0 and len(passed) == len(TESTS)
        results.append(rec)
        print(mid, "exit", r.returncode, "failed", failed, "cases", cases, flush=True)
    finally:
        subprocess.run(["git", "checkout", "--", path], cwd=REPO, check=True)
        os.utime(p, None)
json.dump(results, open(OUT, "w"), indent=1)
ok = all(r.get("baseline_ok", True) and (r.get("killed") in (None, True)) for r in results)
print("ALL KILLED, BASELINE OK" if ok else "NOT ALL KILLED OR BASELINE FAILED")
sys.exit(0 if ok else 1)
