#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): mutants of the
# threaded point loops of the constitutive laws and the heat model. Each
# makes a point depend on its predecessor (or its position) within its part,
# or drops the neo-Hookean pre-check; the named thread-invariance test must
# fail ("CAUGHT"). The anchors are the source text of commit 896d0f46 (the
# J2 and visco-plastic loops were rewritten with `enumerate` afterwards; the
# extra-materials mutants use the later text). Run from the checkout root:
#   python3 -I mutants.py [NAME ...]
# with a target directory of its own (CARGO_TARGET_DIR): a target shared
# with another checkout links the mutated libraries into its binaries.
import os, re, subprocess, sys
P = "  let mut prev = 0.0_f64;\n"
M = "crates/akantu-solid/src/materials/"
S = ("akantu-solid", "exec_threads")
MUT = [
 ("anisotropic", M+"anisotropic.rs", [
   ("    for (g, s) in grad_u.chunks_exact(n).zip(sigma.chunks_exact_mut(n)) {", P+"    for (g, s) in grad_u.chunks_exact(n).zip(sigma.chunks_exact_mut(n)) {"),
   ("        core.stress_on_quad(g, s)?;\n", "        core.stress_on_quad(g, s)?;\n s[0][0] += 1e-9 * prev; prev = s[0][0];\n")], S, "orthotropic_law"),
 ("maxwell stress", M+"maxwell.rs", [
   ("                for ((((g, pg), sv), &th), o) in parts {", P+"                for ((((g, pg), sv), &th), o) in parts {"),
   ("                    o.copy_from_slice(s.as_flattened());\n", "                    o.copy_from_slice(s.as_flattened());\n o[0] += 1e-9 * prev; prev = o[0];\n")], S, "maxwell_law"),
 ("maxwell branch update", M+"maxwell.rs", [
   ("                for (((g, pg), svs), evs) in parts {", P+"                for (((g, pg), svs), evs) in parts {"),
   ("                        evs,\n                    );\n", "                        evs,\n                    );\n svs[0] += 1e-9 * prev; prev = svs[0];\n")], S, "maxwell_law"),
 ("maxwell work", M+"maxwell.rs", [
   ("for ((((gu, pgu), s), ps), w) in parts {", P+"for ((((gu, pgu), s), ps), w) in parts {"),
   ("*w += dd * dt;\n", "*w += dd * dt;\n *w += 1e-9 * prev; prev = *w;\n")], S, "maxwell_law"),
 ("maxwell dissipated energy", M+"maxwell.rs", [
   ("for ((d, w), e) in d", P+"for ((d, w), e) in d"),
   ("*d = *w - *e;\n", "*d = *w - *e;\n *d += 1e-9 * prev; prev = *d;\n")], S, "maxwell_law"),
 ("marigo", M+"marigo.rs", [
   ("                for ((((g, s), (&th, &dt)), (&yd, &pd)), (d, y)) in points {", P+"                for ((((g, s), (&th, &dt)), (&yd, &pd)), (d, y)) in points {"),
   ("                    *y = yq;\n", "                    *y = yq + 1e-9 * prev; prev = *y;\n")], S, "marigo_law"),
 ("mazars", M+"mazars.rs", [
   ("                ehat.as_mut_slice()[k] = e;\n", "                ehat.as_mut_slice()[k] = e + if k > 0 { 1e-9 * ehat.as_mut_slice()[k - 1] } else { 0.0 };\n")], S, "mazars_law_is"),
 ("neohookean stress", M+"neohookean.rs", [
   ("            for ((g, s), &x) in g.chunks_exact(n).zip(s.chunks_exact_mut(n)).zip(c33) {", P+"            for ((g, s), &x) in g.chunks_exact(n).zip(s.chunks_exact_mut(n)).zip(c33) {"),
   ("                *s = neohookean_stress(lambda, mu, g, x)?;\n", "                *s = neohookean_stress(lambda, mu, g, x)?;\n s[0][0] += 1e-9 * prev; prev = s[0][0];\n")], S, "neohookean_law"),
 ("neohookean error path without the pre-check", M+"neohookean.rs", [
   ("    first.into_inner()\n", "    let _ = first.into_inner();\n    nq\n")], S, "neohookean_error_path"),
 ("plastic", M+"plastic.rs", [
   ("for (k, r_iso) in iso.iter_mut().enumerate() {", P+"for (k, r_iso) in iso.iter_mut().enumerate() {"),
   ("inel[r].copy_from_slice(e.as_flattened());\n                }\n                Ok(())", "inel[r].copy_from_slice(e.as_flattened());\n *r_iso += 1e-9 * prev; prev = *r_iso;\n                }\n                Ok(())")], S, "plastic_law"),
 ("plastic commits although a part failed", M+"plastic.rs", [
   ("let mut green_out = Vec::new();\n        {", "let mut green_out = Vec::new();\n let mut part_result = Ok(());\n        {"),
   ("exec.try_for_each((ins, outs), 512, |_, (i, o)| part(i, o, None))?;", "part_result = exec.try_for_each((ins, outs), 512, |_, (i, o)| part(i, o, None));"),
   (".commit_overwrite(ty, ghost, scratch.get_mut(2))?;\n        Ok(())", ".commit_overwrite(ty, ghost, scratch.get_mut(2))?;\n        part_result")], S, "plastic_plane_stress_error_path"),
 ("plastic work", M+"plastic.rs", [
   ("            for ((((s, ps), e), pe_), (pe, wp)) in", P+"            for ((((s, ps), e), pe_), (pe, wp)) in"),
   ("                *pe += *wp;\n", "                *pe += *wp;\n *pe += 1e-9 * prev; prev = *pe;\n")], S, "plastic_law"),
 ("damage dissipated energy", M+"damage.rs", [
   ("                    .enumerate()\n                {\n                    energy_on_quad::<D>(", "                    .enumerate()\n                {\n *is += 1e-9 * (k as f64);\n                    energy_on_quad::<D>(")], S, "marigo_law_is"),
 ("sls", M+"sls_deviatoric.rs", [
   ("                for (((((((g, d0), h0), &e0), so), dout), hout), e_q) in points {", P+"                for (((((((g, d0), h0), &e0), so), dout), hout), e_q) in points {"),
   ("                    *e_q = e;\n", "                    *e_q = e + 1e-9 * prev; prev = *e_q;\n")], S, "sls_law"),
 ("sls stress", M+"sls_deviatoric.rs", [
   ("                for (((((((g, d0), h0), &e0), so), dout), hout), e_q) in points {", P+"                for (((((((g, d0), h0), &e0), so), dout), hout), e_q) in points {"),
   ("                    *e_q = e;\n", "                    *e_q = e;\n so[0] += 1e-9 * prev; prev = so[0];\n")], S, "sls_law"),
 ("anisotropic damage", M+"anisotropic_damage.rs", [
   ("                for ((((((g, dn), &tp), so), dout), elout), (tout, eout)) in points {", P+"                for ((((((g, dn), &tp), so), dout), elout), (tout, eout)) in points {"),
   ("                    *eout = e;\n", "                    *eout = e + 1e-9 * prev; prev = *eout;\n")], S, "anisotropic_damage_laws"),
 ("drucker-prager", M+"drucker_prager.rs", [
   ("        inel_out[r].copy_from_slice(e.as_flattened());\n", "        inel_out[r.clone()].copy_from_slice(e.as_flattened());\n if q > 0 { inel_out[r.start] += 1e-9 * inel_out[r.start - n]; }\n")], S, "drucker_prager_law"),
 ("phasefield law", M+"phasefield.rs", [
   ("            for ((g, (&d, &th)), o) in points {", P+"            for ((g, (&d, &th)), o) in points {"),
   ("                        s[j][i] = g_d * plus[j][i] + minus[j][i];\n                    }\n                }\n", "                        s[j][i] = g_d * plus[j][i] + minus[j][i];\n                    }\n                }\n s[0][0] += 1e-9 * prev; prev = s[0][0];\n")], S, "phasefield_law"),
 ("visco_plastic", "crates/akantu-extra-materials/src/viscoplastic.rs", [
   ("                    *iso_k = r_new;\n", "                    *iso_k = r_new + if k > 0 { 1e-9 * (k as f64) } else { 0.0 };\n")], ("akantu-extra-materials", "exec_threads"), "visco_plastic"),
 ("damage_linear", "crates/akantu-extra-materials/src/damage_linear.rs", [
   ("                        *k = knew;\n", "                        *k = knew;\n *dam += 1e-9 * prev; prev = *dam;\n"),
   ("                    for ((g, s), (dam, k)) in points {\n", P+"                    for ((g, s), (dam, k)) in points {\n")], ("akantu-extra-materials", "exec_threads"), "damage_linear"),
 ("ve_stiffness_prop", "crates/akantu-extra-materials/src/stiffness_proportional.rs", [
   ("                    as_matrix_mut::<DIM, DIM>(sv)?.clone_from(&sigma_visc);\n", "                    as_matrix_mut::<DIM, DIM>(sv)?.clone_from(&sigma_visc);\n sv[0] += 1e-9 * prev; prev = sv[0];\n"),
   ("                for (((s, v), sv), se) in s\n", P+"                for (((s, v), sv), se) in s\n")], ("akantu-extra-materials", "exec_threads"), "ve_stiffness_prop"),
 ("heat D grad u", "crates/akantu-diffusion/src/heat_diffusion.rs", [
   ("    let c = &c[..D * D];\n", "    let c = &c[..D * D];\n    let mut prev = 0.0_f64;\n"),
   ("            *ki = eigen_order::product_coeff(D, true, &|k| c[i + k * D] * bt[k]);\n        }\n    }\n}\n\n/// `D∇u` of every point for a run-time", "            *ki = eigen_order::product_coeff(D, true, &|k| c[i + k * D] * bt[k]);\n        }\n k_bt[0] += 1e-9 * prev; prev = k_bt[0];\n    }\n}\n\n/// `D∇u` of every point for a run-time")], ("akantu-diffusion", "exec_threads_explicit"), ""),
 ("heat diffusivity", "crates/akantu-diffusion/src/heat_diffusion.rs", [
   ("                for (c, &tt) in b.as_mut_slice().chunks_exact_mut(d * d).zip(tq.as_slice()) {\n", P+"                for (c, &tt) in b.as_mut_slice().chunks_exact_mut(d * d).zip(tq.as_slice()) {\n"),
   ("                    let s = var * (tt - t_ref);\n", "                    let s = var * (tt - t_ref) + 1e-9 * prev; prev = s;\n")], ("akantu-diffusion", "exec_threads_explicit"), ""),
 ("heat threaded scatter scale", "crates/akantu-diffusion/src/model/assembly.rs", [
   (".scatter_add(exec, r_e.as_slice(), nb_dof, -1.0, f_int.as_mut_slice())", ".scatter_add(exec, r_e.as_slice(), nb_dof, -1.000_000_1, f_int.as_mut_slice())")], ("akantu-diffusion", "exec_threads_explicit"), ""),
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
        r = subprocess.run(["cargo", "test", "--release", "--locked", "-p", pkg, "--test", test, filt,
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
        elif r.returncode != 0 and "differs from the one-thread run" in out:
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
