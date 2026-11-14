// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs tolerance evidence (no upstream counterpart): the
// conditioning and backward errors behind the `parallel/smm_static_cube_n3`
// displacement rule of fixtures/parallel/tolerances.json
// ("commit the computation that produced cond2 = 4.08e2
// and eta"). The scenario is the oracle probe's
// `scenario_static` (oracle/probes/parallel/parallel_probe.cc), solved by the
// port as `par_oracle::static_matches` does.

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_smm;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{AnalysisMethod, MatrixType, SparseSolverType};
use akantu_core::{NodeFlag, Real};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::Fixture;
use par_smm::{Part, ok};

/// The tolerance of the `model/displacement` rule under test.
const RULE_REL: Real = 1e-12;

// ------------------------------------------- double-double residuals
// Error-free transformations without FMA (no `mul_add`, AGENTS, "Rules for numerical code"):
// Knuth's TwoSum and Dekker's TwoProduct with Veltkamp splitting, so the
// residual f − K u is exact to ~u² relative (its rounding would otherwise
// be of the size of the backward errors measured, ~4e-17).

fn two_sum(a: Real, b: Real) -> (Real, Real) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

fn split(a: Real) -> (Real, Real) {
    let c = 134_217_729.0 * a; // 2^27 + 1
    let hi = c - (c - a);
    (hi, a - hi)
}

fn two_prod(a: Real, b: Real) -> (Real, Real) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    (p, ((ah * bh - p) + ah * bl + al * bh) + al * bl)
}

/// `(hi, lo)` accumulator.
#[derive(Clone, Copy, Default)]
struct Dd(Real, Real);

impl Dd {
    fn add(self, x: Real, xe: Real) -> Self {
        let (s, e) = two_sum(self.0, x);
        let (hi, lo) = two_sum(s, e + self.1 + xe);
        Dd(hi, lo)
    }
    fn value(self) -> Real {
        self.0 + self.1
    }
}

/// `f − K u` on the free DOFs, in double-double.
fn residual(k: &[Real], n: usize, f: &[Real], u: &[Real]) -> Vec<Real> {
    (0..n)
        .map(|i| {
            let mut acc = Dd(f[i], 0.0);
            for j in 0..n {
                let (p, e) = two_prod(k[i * n + j], u[j]);
                acc = acc.add(-p, -e);
            }
            acc.value()
        })
        .collect()
}

fn norm2(v: &[Real]) -> Real {
    v.iter().map(|x| x * x).sum::<Real>().sqrt()
}

fn matvec(k: &[Real], n: usize, x: &[Real]) -> Vec<Real> {
    (0..n)
        .map(|i| (0..n).map(|j| k[i * n + j] * x[j]).sum())
        .collect()
}

/// Dense Cholesky factor `L` (row-major, lower) of the SPD matrix `k`.
fn cholesky(k: &[Real], n: usize) -> Vec<Real> {
    let mut l = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = k[i * n + j];
            for p in 0..j {
                s -= l[i * n + p] * l[j * n + p];
            }
            if i == j {
                assert!(s > 0.0, "K_ff not positive definite at pivot {i}: {s:e}");
                l[i * n + i] = s.sqrt();
            } else {
                l[i * n + j] = s / l[j * n + j];
            }
        }
    }
    l
}

fn cholesky_solve(l: &[Real], n: usize, b: &[Real]) -> Vec<Real> {
    let mut y = b.to_vec();
    for i in 0..n {
        for p in 0..i {
            y[i] -= l[i * n + p] * y[p];
        }
        y[i] /= l[i * n + i];
    }
    for i in (0..n).rev() {
        for p in i + 1..n {
            y[i] -= l[p * n + i] * y[p];
        }
        y[i] /= l[i * n + i];
    }
    y
}

/// Largest eigenvalue of the SPD `k` by power iteration (`inverse`:
/// `1 / λ_min` by inverse iteration), iterated until the Rayleigh quotient
/// is stationary to 1e-13 relative.
fn extreme_eigenvalue(k: &[Real], n: usize, inverse: bool) -> Real {
    let l = inverse.then(|| cholesky(k, n));
    // deterministic start with components along every eigenvector
    let mut x: Vec<Real> = (0..n).map(|i| 1.0 + (i % 7) as Real * 0.1).collect();
    let nx = norm2(&x);
    x.iter_mut().for_each(|v| *v /= nx);
    let mut lambda = 0.0;
    for it in 0..200_000 {
        let y = match &l {
            Some(l) => cholesky_solve(l, n, &x),
            None => matvec(k, n, &x),
        };
        let rq: Real = x.iter().zip(&y).map(|(a, b)| a * b).sum();
        let ny = norm2(&y);
        x = y.iter().map(|v| v / ny).collect();
        if it > 10 && (rq - lambda).abs() <= 1e-13 * rq.abs() {
            return rq;
        }
        lambda = rq;
    }
    panic!("eigenvalue iteration did not converge (last {lambda:e})");
}

/// Oracle displacement of `np` ranks in global node order (owned values;
/// coordinates checked against `coords`).
fn oracle_displacement(np: usize, coords: &[Real]) -> Vec<Real> {
    let mut u = vec![Real::NAN; coords.len()];
    for r in 0..np {
        let fx =
            Fixture::open_by_id(&format!("parallel/smm_static_cube_n3/np{np}/rank{r}")).unwrap();
        let d = fx.load_f64("model/displacement").unwrap().data;
        let x = fx.load_f64("nodes/coordinates").unwrap().data;
        let gid = fx.load_i64("nodes/global_ids").unwrap().data;
        let flags = fx.load_i64("nodes/flags").unwrap().data;
        for (local, &g) in gid.iter().enumerate() {
            let g = usize::try_from(g).unwrap();
            for c in 0..3 {
                assert_eq!(
                    x[3 * local + c].to_bits(),
                    coords[3 * g + c].to_bits(),
                    "np{np} rank {r}: node {local} (global {g}) coordinates"
                );
            }
            if NodeFlag(u8::try_from(flags[local]).unwrap()).is_local_or_master() {
                u[3 * g..3 * g + 3].copy_from_slice(&d[3 * local..3 * local + 3]);
            }
        }
    }
    assert!(
        u.iter().all(|v| v.is_finite()),
        "np{np}: every node owned once"
    );
    u
}

/// The a-priori bound cited by the static rule,
/// recomputed. `K` is assembled by the port at 1 rank (the oracle's
/// element-level assembly is bitwise equal, `smm_static_cube_n3_matches_the_oracle`)
/// and restricted to the 144 free DOFs; cond₂ = λ_max / λ_min (power and
/// inverse iteration); normwise backward errors
/// η = ‖f − K u‖₂ / (‖K‖₂ ‖u‖₂ + ‖f‖₂) (Rigal–Gaches, Higham Thm 7.1) of
/// the port's faer solution and of the oracle's MUMPS solutions at 1 and 2
/// ranks. Each solution is within 2 κ η of the exact one (Higham Thm 7.2,
/// κη ≪ 1), so two solutions differ by at most 2 κ (η₁ + η₂) in the 2-norm
/// and by `sqrt(n_free)` times that in the max-norm relative measure of
/// the rule: the bound must stay below the rule's 1e-12.
#[test]
fn pa2_04_static_cube_conditioning_supports_the_tolerance() {
    let out = run_local(1, Duration::from_secs(120), |comm: Communicator| {
        let m = par_smm::distributed_cube(&comm, 3, &Part::Rcb)?;
        let shared = SharedMesh::new(m);
        let parser = ok(Parser::parse_str(par_smm::DAT, "material.dat"), "parser")?;
        let mut model = ok(
            SolidMechanicsModel::with_parser(shared.clone(), None, "smm", parser),
            "model",
        )?;
        let mut options = ModelOptions::new(AnalysisMethod::Static);
        options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
        ok(model.init_full(&options), "init")?;
        let (coords, nb) = {
            let mm = ok(shared.read(), "read")?;
            (mm.nodes().as_slice().to_vec(), mm.nb_nodes())
        };
        // the probe's boundary conditions (par_oracle::static_matches)
        {
            let mut b_g = ok(model.blocked_dofs_mut(), "blocked")?;
            let b = b_g.as_mut_slice();
            for n in 0..nb {
                if coords[3 * n + 2] == 0.0 {
                    b[3 * n..3 * n + 3].fill(true);
                }
            }
        }
        {
            let mut f_g = ok(model.external_force_mut(), "f")?;
            let f = f_g.as_mut_slice();
            for n in 0..nb {
                if coords[3 * n + 2] == 1.0 {
                    f[3 * n] = 1e-3 * (1.0 + coords[3 * n + 1]);
                    f[3 * n + 2] = -2e-3;
                }
            }
        }
        ok(model.solve_step(None), "solve")?;
        let u = ok(model.displacement(), "u")?.as_slice().to_vec();
        let f = ok(model.external_force(), "f")?.as_slice().to_vec();
        let blocked = ok(model.blocked_dofs(), "blocked")?.as_slice().to_vec();
        let k = ok(model.dof_manager().matrix("K"), "K")?;
        let symmetric = k.matrix_type() == MatrixType::Symmetric;
        let (irn, jcn, a) = k.coo();
        let n = u.len();
        let mut dense = vec![0.0; n * n];
        for ((&i, &j), &v) in irn.iter().zip(jcn).zip(a) {
            let (i, j) = (usize::try_from(i).unwrap(), usize::try_from(j).unwrap());
            dense[i * n + j] += v;
            if symmetric && i != j {
                dense[j * n + i] += v;
            }
        }
        Ok::<_, String>((coords, u, f, blocked, dense))
    });
    let (coords, u_port, f, blocked, k) = out.into_iter().next().unwrap().unwrap();
    let n = u_port.len();
    assert_eq!(n, 192, "3 DOFs × 64 nodes");
    let free: Vec<usize> = (0..n).filter(|&i| !blocked[i]).collect();
    let nf = free.len();
    assert_eq!(nf, 144, "free DOFs (z = 0 clamped)");
    let mut kff = vec![0.0; nf * nf];
    for (a, &i) in free.iter().enumerate() {
        for (b, &j) in free.iter().enumerate() {
            kff[a * nf + b] = k[i * n + j];
        }
    }
    for a in 0..nf {
        for b in 0..a {
            assert_eq!(
                kff[a * nf + b].to_bits(),
                kff[b * nf + a].to_bits(),
                "K symmetric"
            );
        }
    }
    let ff: Vec<Real> = free.iter().map(|&i| f[i]).collect();
    let restrict = |u: &[Real]| -> Vec<Real> { free.iter().map(|&i| u[i]).collect() };

    let lambda_max = extreme_eigenvalue(&kff, nf, false);
    let lambda_min = 1.0 / extreme_eigenvalue(&kff, nf, true);
    let cond2 = lambda_max / lambda_min;
    let eta = |u: &[Real]| -> Real {
        let uf = restrict(u);
        norm2(&residual(&kff, nf, &ff, &uf)) / (lambda_max * norm2(&uf) + norm2(&ff))
    };
    let solutions = [
        ("port faer (1 rank)", u_port.clone()),
        ("oracle MUMPS np1", oracle_displacement(1, &coords)),
        ("oracle MUMPS np2", oracle_displacement(2, &coords)),
    ];
    let etas: Vec<Real> = solutions.iter().map(|(_, u)| eta(u)).collect();
    println!(
        "INFO K_ff ({nf} free DOFs): lambda_max {lambda_max:.6e}, lambda_min {lambda_min:.6e}, cond2 {cond2:.6e}"
    );
    for ((what, _), e) in solutions.iter().zip(&etas) {
        println!("INFO backward error {what}: {e:.3e}");
    }
    // the values the rule's justification cites (rounded there); cond2 and
    // the eigenvalues are deterministic in K. The backward errors depend on
    // the solvers' kernels (faer's are platform- and version-dependent):
    // they are reported above, not asserted to their third digit;
    // the bound below uses the measured values.
    assert!(
        (lambda_max - 2.81).abs() < 5e-3,
        "lambda_max {lambda_max:e}"
    );
    assert!(
        (lambda_min - 6.90e-3).abs() < 5e-5,
        "lambda_min {lambda_min:e}"
    );
    assert!((cond2 - 408.0).abs() < 0.5, "cond2 {cond2:e}");
    // the a-priori bound for every pair the rule compares (np1 and np2
    // oracle solutions against the port's)
    let worst_pair = etas[1].max(etas[2]) + etas[0];
    let bound = (nf as Real).sqrt() * 2.0 * cond2 * worst_pair;
    println!("INFO a-priori bound sqrt({nf}) * 2 * cond2 * (eta_port + eta_oracle) = {bound:.3e}");
    assert!(
        bound < RULE_REL,
        "bound {bound:e} does not support the rule {RULE_REL:e}"
    );
    // and the measured differences sit below the bound
    for (what, u) in &solutions[1..] {
        let mag = u.iter().fold(0.0 as Real, |m, x| m.max(x.abs()));
        let diff = u_port
            .iter()
            .zip(u)
            .fold(0.0 as Real, |m, (x, y)| m.max((x - y).abs()));
        println!(
            "INFO measured max rel difference port vs {what}: {:.3e}",
            diff / mag
        );
        assert!(
            diff / mag <= bound,
            "{what}: {:e} > bound {bound:e}",
            diff / mag
        );
    }
}
