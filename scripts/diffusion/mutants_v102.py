#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); project tooling, no
# C++ counterpart. Mutation campaign of the diffusion model:
# M10's pattern follows the guard-scoped predictor
# (one block deeper, D-056/D-071; same mutation), a baseline run (M00,
# no mutation) comes first, and the kill set includes the test
# binaries (review_v102_regressions, model_diffusion_v102_fixtures).
# Apply one mutant, run the akantu-diffusion test binaries, record the
# failing tests, restore the file (git checkout).
# Usage: mutants_v102.py <repo clone> <out.json> [M01,M02,...]
import subprocess, sys, os, re, json

REPO = sys.argv[1]
OUT = sys.argv[2]
D = "crates/akantu-diffusion/src/"
MUTANTS = [
    ("M01", D + "model/assembly.rs", "internal flow assembled with +1 (sign)",
     "            ghost,\n            -1.0,\n            Some(&filter),", "            ghost,\n            1.0,\n            Some(&filter),"),
    ("M02", D + "model/assembly.rs", "empty per-type filter returns (UD-276 behaviour by default)",
     "                return Ok(());\n            }\n            continue;", "                return Ok(());\n            }\n            return Ok(());"),
    ("M03", D + "model/assembly.rs", "lumped capacity without the ghost elements",
     "        for ghost in GhostType::ALL {\n            for ty in regular_types(&mesh, dim, ghost) {",
     "        for ghost in [GhostType::NotGhost] {\n            for ty in regular_types(&mesh, dim, ghost) {"),
    ("M04", D + "heat_diffusion.rs", "conductivity transposed in updateInternalParameters",
     "*c.at_mut(i, j) = self.conductivity.at(i, j);", "*c.at_mut(i, j) = self.conductivity.at(j, i);"),
    ("M05", D + "heat_diffusion.rs", "conductivity_variation ignored",
     "                    *ci = k + s;", "                    *ci = k;"),
    ("M06", D + "heat_diffusion.rs", "D grad u with the transposed diffusivity",
     "c[i + k * d] * bt[k]", "c[k + i * d] * bt[k]"),
    ("M07", D + "heat_diffusion.rs", "stable time step without the factor 2",
     "Ok(2. * h * h / 4. * self.density", "Ok(h * h / 4. * self.density"),
    ("M08", D + "heat_diffusion.rs", "thermal energy integrand without the capacity",
     ".map(|&ti| self.capacity * self.density * ti)", ".map(|&ti| self.density * ti)"),
    ("M09", D + "model/energy.rs", "stable time step over every element kind by default",
     "kind: if self.upstream_compat.stable_time_step_includes_all_kinds {", "kind: if true {"),
    ("M10", "crates/akantu-solver/src/integration_scheme.rs", "UD-287 fix: rate reset before the predictor update",
     "                    *u += (1.0 - alpha) * delta_t * *u_dot;\n                    if reset_rate {\n                        *u_dot = 0.0;\n                    }",
     "                    if reset_rate {\n                        *u_dot = 0.0;\n                    }\n                    *u += (1.0 - alpha) * delta_t * *u_dot;"),
    ("M11", "crates/akantu-solver/src/integration_scheme.rs", "UD-287 fix removed (upstream predictor by default)",
     "let reset_rate = alpha != 0.0", "let reset_rate = false && alpha != 0.0"),
    ("M12", D + "model.rs", "lumped capacity gated by the consistent flag by default (UD-281)",
     ".lumped_capacity_gated_by_consistent_flag\n        {", ".lumped_capacity_gated_by_consistent_flag\n            || true\n        {"),
    ("M13", D + "model/parallel.rs", "_diffusion element exchange: received values dropped",
     "buffer.unpack_slice(t)?;", "let mut tmp = t.to_vec();\n                    buffer.unpack_slice(&mut tmp)?;"),
    ("M14", D + "model/energy.rs", "thermal energy by node counts slave nodes",
     ".is_local_or_master()", ".is_local_or_master() || true"),
    ("M15", D + "model/assembly.rs", "capacity density rho c ignored (1)",
     "    mat.fill(rho);", "    let _ = rho;\n    mat.fill(1.0);"),
    ("M16", D + "law.rs", "gradient cache treats release -1 as cached",
     "        let rel = self.grad_u_release(ty, ghost);\n        if rel != -1 && rel == ctx.release {",
     "        let rel = self.grad_u_release(ty, ghost);\n        if rel == ctx.release {"),
]
TESTS = ["model_diffusion_fixtures", "diffusion_independent_checks", "diffusion_parallel",
         "patch_test_linear_heat_transfer", "review_v102_regressions",
         "model_diffusion_v102_fixtures"]
# M00: baseline (the unmutated tree must pass every listed test)
MUTANTS = [("M00", D + "lib.rs", "baseline (no mutation)", "#![forbid(unsafe_code)]",
            "#![forbid(unsafe_code)]")] + MUTANTS
env = dict(os.environ)
env.update(CARGO_TARGET_DIR=os.path.join(REPO, "target"), CARGO_BUILD_JOBS="4", CARGO_INCREMENTAL="0",
           CARGO_PROFILE_DEV_DEBUG="line-tables-only", CARGO_PROFILE_TEST_DEBUG="line-tables-only")
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
        cmd = ["cargo", "test", "-p", "akantu-diffusion", "--no-fail-fast"]
        for t in TESTS:
            cmd += ["--test", t]
        r = subprocess.run(cmd, cwd=REPO, env=env, capture_output=True, text=True, timeout=3000)
        out = r.stdout + r.stderr
        failed = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED", out, re.M)))
        compile_err = "error[E" in out or "error: could not compile" in out
        passed = len(re.findall(r"^test \S+ \.\.\. ok", out, re.M))
        res = {"id": mid, "file": path, "mutation": desc, "rc": r.returncode,
               "killed_by": failed, "compile_error": compile_err, "passed": passed}
    finally:
        subprocess.run(["git", "checkout", "--", path], cwd=REPO)
    results.append(res)
    print(mid, "killed" if res["killed_by"] or res["compile_error"] else "SURVIVED", res["killed_by"][:6], flush=True)
    json.dump(results, open(OUT, "w"), indent=1)
json.dump(results, open(OUT, "w"), indent=1)
