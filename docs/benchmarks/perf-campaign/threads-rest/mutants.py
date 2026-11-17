#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): mutants of the loops
# threaded to remove the serial remainder of the threaded explicit step
# (solver per-equation loops, non-local average, FE checks, history copies,
# energy densities). Each makes a part's result depend on its position in
# the part, drops a part or reverses an order; the named test must fail
# ("CAUGHT"). Run from the checkout root, with a target directory of its
# own (CARGO_TARGET_DIR):
#   python3 -I mutants.py [--dry] [NAME ...]
import os, re, subprocess, sys
C = "  let mut cnt = 0.0_f64;\n"
S = ("akantu-solid", "exec_threads")
MUT = [
 ("trapezoidal predictor", "crates/akantu-solver/src/integration_scheme.rs", [
   ("for ((u, u_dot), &bld) in u.iter_mut().zip(u_dot).zip(blocked) {", C + "for ((u, u_dot), &bld) in u.iter_mut().zip(u_dot).zip(blocked) {\n cnt += 1.0; *u += 1e-9 * cnt;")],
   ("akantu-diffusion", "exec_threads_explicit"), ""),
 ("trapezoidal corrector", "crates/akantu-solver/src/integration_scheme.rs", [
   ("for (((u, u_dot), &delta), &bld) in u.iter_mut().zip(u_dot).zip(solution).zip(blocked) {", C + "for (((u, u_dot), &delta), &bld) in u.iter_mut().zip(u_dot).zip(solution).zip(blocked) {\n cnt += 1.0; *u_dot += 1e-9 * cnt;")],
   ("akantu-diffusion", "exec_threads_explicit"), ""),
 ("increment", "crates/akantu-solver/src/time_step.rs", [
   ("for ((i, &x), &p) in inc.iter_mut().zip(u).zip(prev) {", C + "for ((i, &x), &p) in inc.iter_mut().zip(u).zip(prev) {\n cnt += 1.0;"),
   ("*i = x - p;", "*i = x - p + 1e-12 * cnt;")], S, "explicit_step_is_bitwise"),
 ("blocked scan drops the last part", "crates/akantu-solver/src/dof_manager/blocked.rs", [
   ("for l in lists {", "for l in lists.into_iter().take(parts - 1) {")],
   ("akantu-solid", "exec_threads_matrices"), ""),
 ("non-local average order", "crates/akantu-model/src/non_local/neighborhood.rs", [
   ("for &code in &table.entries[range] {", "for &code in table.entries[range].iter().rev() {")],
   ("akantu-model", "--lib"), "threaded_weighted"),
 ("energy densities", "crates/akantu-solid/src/materials/damage.rs", [
   ("*w = point(start + first + k)?;", "*w = point(start + first + k)? + 1e-9 * (k as f64);")], S, "marigo_law_is"),
 ("parallel any drops the last part", "crates/akantu-fe/src/engine/fast.rs", [
   ("found.contains(&true)", "found[..parts - 1].contains(&true)")], ("akantu-fe", "--lib"), "threaded_any"),
 ("parallel maximum drops the last part", "crates/akantu-fe/src/engine/fast.rs", [
   ("let m = maxima.iter().fold(0u32, |a, &b| a.max(b));", "let m = maxima[..parts - 1].iter().fold(0u32, |a, &b| a.max(b));")],
   ("akantu-fe", "--lib"), "threaded_maximum"),
 ("per-element connectivity check skips the last element", "crates/akantu-fe/src/engine/fast.rs", [
   ("let e = id_index(ids[k]);", "if k + 1 == ids.len() { return false; } let e = id_index(ids[k]);")],
   ("akantu-fe", "--lib"), "threaded_per_element"),
 ("parallel consecutive test drops the last part", "crates/akantu-fe/src/engine/fast.rs", [
   ("!ok.contains(&false)", "!ok[..parts - 1].contains(&false)")], ("akantu-fe", "--lib"), "threaded_consecutive"),
 ("Mazars kept buffer reused without the copy", "crates/akantu-solid/src/materials/mazars.rs", [
   ("Some(b) if b.len() == len => b,", "Some(b) if b.len() == len => return Array::from_vec(b, a.nb_component()),")], S, "mazars_law_is"),
 ("finite-deformation filter check drops the last part", "crates/akantu-solid/src/model/assembly/finite_deformation.rs", [
   ("maxima.iter().fold(0u32, |a, &b| a.max(b))", "maxima[..parts - 1].iter().fold(0u32, |a, &b| a.max(b))")],
   ("akantu-solid", "--lib"), "threaded_filter_check"),
 ("Mazars kept buffer reused at another size", "crates/akantu-solid/src/materials/mazars.rs", [
   ("Some(b) if b.len() == len => b,", "Some(b) => b,")], S, "mazars_block_growth"),
 ("deferred save copy skips a value per part", "crates/akantu-model/src/internal_field.rs", [
   ("d.as_mut_slice().clone_from_slice(s.as_slice());", "if d.as_mut_slice().len() > 1 { d.as_mut_slice()[1..].clone_from_slice(&s.as_slice()[1..]); }")],
   ("akantu-model", "--lib"), "get_mut_with"),
 ("non-local Mazars damage", "crates/akantu-solid/src/materials/non_local/mazars_non_local.rs", [
   ("let f = 1. - d;", "let f = 1. - d + 1e-9 * (k as f64);")], S, "mazars_non_local"),
 ("scheme store copy skips a value per part", "crates/akantu-solver/src/integration_scheme.rs", [
   ("fn copy_values(exec: &akantu_core::exec::Exec, dst: &mut [Real], src: &[Real]) {\n    let items = (Tuples::new(dst, 1), TuplesRef::new(src, 1));\n    exec.for_each(items, EQUATIONS_PER_PART, |_, (mut d, s)| {\n        d.as_mut_slice().copy_from_slice(s.as_slice());",
    "fn copy_values(exec: &akantu_core::exec::Exec, dst: &mut [Real], src: &[Real]) {\n    let items = (Tuples::new(dst, 1), TuplesRef::new(src, 1));\n    exec.for_each(items, EQUATIONS_PER_PART, |_, (mut d, s)| {\n        if d.as_mut_slice().len() > 1 { d.as_mut_slice()[1..].copy_from_slice(&s.as_slice()[1..]); }")],
   ("akantu-solver", "--lib"), "threaded_copy"),
 ("Mazars block copies skip a value per part", "crates/akantu-solid/src/materials/mazars.rs", [
   ("d.as_mut_slice().copy_from_slice(s.as_slice());", "if d.as_mut_slice().len() > 1 { d.as_mut_slice()[1..].copy_from_slice(&s.as_slice()[1..]); }")], S, "mazars_law_is"),
]
DRY = "--dry" in sys.argv
only = [a for a in sys.argv[1:] if a != "--dry"]
for name, path, reps, (pkg, test), filt in MUT:
    if only and name not in only:
        continue
    src = open(path).read()
    s = src
    ok = True
    for old, new in reps:
        # whitespace runs of the anchor match any whitespace run
        pat = re.compile(r"\s+".join(re.escape(w) for w in old.split()))
        if not pat.search(s):
            print(f"MUTANT {name}: anchor not found: {old[:60]!r}", flush=True); ok = False; break
        if DRY:
            continue
        s = pat.sub(lambda _m, new=new: new, s)
    if not ok:
        continue
    if DRY:
        print(f"MUTANT {name}: anchors found", flush=True)
        continue
    open(path, "w").write(s)
    try:
        # with MUTANT_BASE_DUMP (the serial reference states of the
        # unchanged code, AKANTU_TEST_STATE_DUMP of the solid law tests) a
        # serial state that differs from it also counts as caught
        dump = "mutant-dump.txt"
        if os.path.exists(dump):
            os.remove(dump)
        env = dict(os.environ, AKANTU_TEST_STATE_DUMP=os.path.abspath(dump))
        target = ["--lib"] if test == "--lib" else ["--test", test]
        r = subprocess.run(["cargo", "test", "--release", "--locked", "-p", pkg] + target + [filt,
                            "--", "--test-threads", "1"],
                           capture_output=True, text=True, env=env)
        out = r.stdout + r.stderr
        base = {}
        if os.environ.get("MUTANT_BASE_DUMP") and os.path.exists(os.environ["MUTANT_BASE_DUMP"]):
            for l in open(os.environ["MUTANT_BASE_DUMP"]):
                base[l.split("\t")[0]] = l
        mine = {l.split("\t")[0]: l for l in open(dump)} if os.path.exists(dump) else {}
        moved = [k for k in mine if k in base and base[k] != mine[k]]
        if "error[E" in out or "could not compile" in out:
            verdict = "COMPILE-ERROR"
        elif r.returncode != 0 and ("differs" in out or "assertion" in out or "threads" in out):
            verdict = "CAUGHT"
        elif moved:
            verdict = "CAUGHT (serial state differs from the unchanged code's: " + moved[0] + ")"
        elif r.returncode != 0:
            verdict = "FAILED-OTHER"
        else:
            verdict = "SURVIVED"
        line = next((l for l in out.splitlines() if "differs" in l or "panicked" in l), "")
        print(f"MUTANT {name}: {verdict} :: {line.strip()[:200]}", flush=True)
    finally:
        open(path, "w").write(src)
