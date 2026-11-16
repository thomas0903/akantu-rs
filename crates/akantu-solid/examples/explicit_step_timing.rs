// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Timing harness of
// the explicit solid-mechanics step; the problem follows
// examples/c++/solid_mechanics_model/explicit/explicit_dynamic.cc (elastic
// material, explicit lumped mass, Δt = 0.8 Δt_crit, sine–Gaussian pulse).

//! Explicit-step timing harness (`docs/BENCHMARKS.md` "Explicit
//! step", `docs/TESTING.md`).
//!
//! ```text
//! cargo run --release -p akantu-solid --example explicit_step_timing -- \
//!     --mesh cube.msh [--dim 3] [--fd] [--steps 20] [--reps 5] [--warmup 5] \
//!     [--phases step,fint,stress,grad,btd,integ,asm] [--digest-steps N] [--label L]
//! ```
//!
//! Reads a Gmsh mesh, builds a `SolidMechanicsModel` with one elastic
//! material (`rho = 2500`, `E = 70e9`, `nu = 0.22`, optionally
//! `finite_deformation = true` as in the cohesive scalability benchmark),
//! `explicit_lumped_mass`, `Δt = 0.8 · stable time step`, and the
//! sine–Gaussian displacement pulse of the upstream explicit example along
//! `x`. After `--warmup` steps, every repetition times each requested phase
//! over `--steps` calls, phases interleaved within the repetition:
//!
//! | phase | what is timed (the model's own code path unless noted) |
//! |---|---|
//! | `step` | `solve_step` (whole explicit step) |
//! | `fint` | `assemble_internal_forces` (stresses + `−∫Bᵀσ` assembly) |
//! | `stress` | every law's `compute_all_stresses` (gradient + constitutive law) |
//! | `grad` | `FeEngine::gradient_on_integration_points` of the displacement (output reused) |
//! | `btd` | `FeEngine::compute_btd` of the stresses (small strain; output reused) |
//! | `integ` | `FeEngine::integrate` of `Bᵀσ` (small strain; output reused) |
//! | `asm` | `DofManager::assemble_elemental_array_local_array` (output reused) |
//!
//! Output: one JSON object per line (`"kind": "meta" | "time" | "digest"`);
//! `ns_el_step` = seconds / (elements × steps) × 1e9. `--digest-steps N`
//! runs `N` steps from the initial state (no timing) and prints an FNV-1a
//! digest of the bits of `u`, `v`, `a` and `f_int`, used to check that two
//! builds give bitwise identical states.
//!
//! The execution policy is the process default (D-123): `AKANTU_NUM_THREADS`,
//! `AKANTU_PRECISION`, `AKANTU_SIMD`; the meta line reports it.
//!
//! The phases `grad`, `btd`, `integ`, `asm` call the public kernels with the
//! arguments `assemble_internal_forces` uses but reuse their outputs (the
//! model path allocates them per call): they isolate kernel costs.

use std::path::PathBuf;
use std::time::Instant;

use akantu_core::array::ArrayViewMut;
use akantu_core::array_shared::ArrayRef;
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{Array, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_fe::FeEngine;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{LawContext, LawPhase, ModelServices};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::DofManager;

struct Args {
    mesh: PathBuf,
    dim: usize,
    fd: bool,
    steps: usize,
    reps: usize,
    warmup: usize,
    phases: Vec<String>,
    digest_steps: Option<usize>,
    label: String,
}

fn usage() -> Error {
    Error::invalid(
        "usage: explicit_step_timing --mesh FILE [--dim D] [--fd] [--steps S] [--reps R] \
         [--warmup W] [--phases step,fint,stress,grad,btd,integ,asm] [--digest-steps N] [--label L]",
    )
}

fn parse_args() -> Result<Args> {
    let mut a = Args {
        mesh: PathBuf::new(),
        dim: 3,
        fd: false,
        steps: 20,
        reps: 5,
        warmup: 5,
        phases: ["step", "fint", "stress", "grad", "btd", "integ", "asm"]
            .map(String::from)
            .to_vec(),
        digest_steps: None,
        label: String::new(),
    };
    let mut it = std::env::args().skip(1);
    let num =
        |v: Option<String>| -> Result<usize> { v.ok_or_else(usage)?.parse().map_err(|_| usage()) };
    while let Some(k) = it.next() {
        match k.as_str() {
            "--mesh" => a.mesh = PathBuf::from(it.next().ok_or_else(usage)?),
            "--dim" => a.dim = num(it.next())?,
            "--fd" => a.fd = true,
            "--steps" => a.steps = num(it.next())?,
            "--reps" => a.reps = num(it.next())?,
            "--warmup" => a.warmup = num(it.next())?,
            "--phases" => {
                a.phases = it
                    .next()
                    .ok_or_else(usage)?
                    .split(',')
                    .map(String::from)
                    .collect();
            }
            "--digest-steps" => a.digest_steps = Some(num(it.next())?),
            "--label" => a.label = it.next().ok_or_else(usage)?,
            _ => return Err(usage()),
        }
    }
    if a.mesh.as_os_str().is_empty() {
        return Err(usage());
    }
    Ok(a)
}

/// Read-only services for timing `compute_all_stresses` outside the model
/// (the model's own services type is crate-private): the mesh, the FE
/// engines and the displacement.
struct Services<'a> {
    mesh: &'a Mesh,
    fems: &'a akantu_model::model::FeEngineRegistry,
    displacement: &'a Array<Real>,
    dim: usize,
}

impl ModelServices for Services<'_> {
    // as the model's own services: the law loops run on the model's
    // executor (D-123)
    fn exec(&self) -> &akantu_core::exec::Exec {
        self.fems.exec()
    }
    fn spatial_dimension(&self) -> usize {
        self.dim
    }
    fn mesh(&self) -> &Mesh {
        self.mesh
    }
    fn time_step(&self) -> Real {
        0.0
    }
    fn nb_integration_points(&self, ty: ElementType, ghost: GhostType) -> usize {
        self.fems
            .get("")
            .and_then(|fe| fe.nb_integration_points(ty, ghost))
            .unwrap_or(0)
    }
    fn integration_point_coordinates(
        &self,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
        out: &mut Array<Real>,
    ) -> Result<()> {
        self.fems.get("")?.interpolate_on_integration_points(
            self.mesh,
            self.mesh.nodes().view(),
            out,
            self.mesh.spatial_dimension(),
            ty,
            ghost,
            Some(filter),
        )
    }
    fn nodal_field(&self, name: &str) -> Result<ArrayRef<'_, Real>> {
        match name {
            "displacement" => Ok(self.displacement.into()),
            _ => Err(Error::not_found("nodal field", name)),
        }
    }
    fn fe_engine(&self, name: &str) -> Result<&FeEngine> {
        self.fems.get(name)
    }
    // as the model's own services: the gradient straight into the stored
    // block
    fn gradient_on_integration_points_view(
        &self,
        field: &str,
        out: ArrayViewMut<'_, Real>,
        nb_component: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
    ) -> Result<()> {
        let u = self.nodal_field(field)?;
        self.fe_engine("")?.gradient_on_integration_points_view(
            self.mesh,
            u.view(),
            out,
            nb_component,
            ty,
            ghost,
            Some(filter),
        )
    }
}

fn material_dat(fd: bool) -> String {
    format!(
        "material elastic [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n  \
         finite_deformation = {fd}\n]\n"
    )
}

fn build_model(a: &Args) -> Result<SolidMechanicsModel> {
    let mut mesh = Mesh::new(a.dim, "mesh")?;
    akantu_io::read_mesh(&mut mesh, &a.mesh, MeshIoType::Gmsh)?;
    let parser = Parser::parse_str(&material_dat(a.fd), "material.dat")?;
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )?;
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))?;
    let dt = m.stable_time_step()? * 0.8;
    m.set_time_step(dt, None)?;
    // sine–Gaussian pulse along x (upstream explicit example), centred in
    // the mesh's x range, amplitude 1e-3 of the length
    let x: Vec<Real> = {
        let mesh = m.physics.base.mesh.read()?;
        mesh.nodes().as_slice().to_vec()
    };
    let d = a.dim;
    let (lo, hi) = x.chunks_exact(d).fold((Real::MAX, Real::MIN), |(l, h), p| {
        (l.min(p[0]), h.max(p[0]))
    });
    let len = (hi - lo).max(Real::MIN_POSITIVE);
    let mid = 0.5 * (lo + hi);
    let pulse = 0.25 * len;
    let k = 0.1 * 2. * std::f64::consts::PI * 3. / pulse;
    let amp = 1e-3 * len;
    for (u, p) in m
        .displacement_mut()?
        .as_mut_slice()
        .chunks_exact_mut(d)
        .zip(x.chunks_exact(d))
    {
        let xx = (p[0] - mid) / len * 4.;
        u[0] = amp * (k * xx).sin() * (-(k * xx) * (k * xx) / (pulse * pulse)).exp();
    }
    Ok(m)
}

fn fnv(h: &mut u64, v: &[Real]) {
    for x in v {
        for b in x.to_bits().to_le_bytes() {
            *h ^= u64::from(b);
            *h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

/// Element types of the model dimension owned by laws, with their filters.
fn law_blocks(m: &SolidMechanicsModel, dim: usize) -> Result<Vec<(usize, ElementType, Vec<Idx>)>> {
    let mut out = Vec::new();
    for (i, law) in m.physics.materials.laws().iter().enumerate() {
        for ty in law.base().filter_types(GhostType::NotGhost) {
            if ty.spatial_dimension() != dim {
                continue;
            }
            let f = law
                .base()
                .element_filter
                .get(ty, GhostType::NotGhost)?
                .as_slice()
                .to_vec();
            if !f.is_empty() {
                out.push((i, ty, f));
            }
        }
    }
    Ok(out)
}

/// Scratch outputs of the kernel phases, reused across calls.
#[derive(Default)]
struct Scratch {
    grad: Vec<Array<Real>>,
    btd: Vec<Array<Real>>,
    integ: Vec<Array<Real>>,
}

fn run_phase(
    m: &mut SolidMechanicsModel,
    phase: &str,
    blocks: &[(usize, ElementType, Vec<Idx>)],
    dim: usize,
    fd: bool,
    scratch: &mut Scratch,
) -> Result<bool> {
    match phase {
        "step" => m.solve_step(None)?,
        "fint" => m.assemble_internal_forces()?,
        "stress" => {
            let SolidMechanicsModel {
                physics, solver, ..
            } = m;
            let mesh_h = physics.base.mesh.clone();
            let mesh = mesh_h.read()?;
            let u = solver.dof_manager.dofs("displacement")?;
            let services = Services {
                mesh: &mesh,
                fems: &physics.base.fems,
                displacement: &u,
                dim,
            };
            let ctx = LawContext {
                phase: LawPhase::Stress,
                services: &services,
            };
            for law in physics.materials.laws_mut() {
                law.compute_all_stresses(&ctx, GhostType::NotGhost)?;
            }
        }
        "grad" => {
            let mesh_h = m.physics.base.mesh.clone();
            let mesh = mesh_h.read()?;
            let fe = m.physics.base.fems.get("")?;
            let u = m.solver.dof_manager.dofs("displacement")?;
            scratch
                .grad
                .resize_with(blocks.len(), || Array::new(dim * dim));
            for ((_, ty, f), out) in blocks.iter().zip(&mut scratch.grad) {
                fe.gradient_on_integration_points(
                    &mesh,
                    u.view(),
                    out,
                    dim,
                    *ty,
                    GhostType::NotGhost,
                    Some(f),
                )?;
            }
        }
        "btd" | "integ" | "asm" if fd => return Ok(false),
        "btd" => {
            let fe = m.physics.base.fems.get("")?;
            scratch.btd.resize_with(blocks.len(), || Array::new(1));
            for ((i, ty, f), out) in blocks.iter().zip(&mut scratch.btd) {
                let law = m.physics.materials.law(*i)?;
                let stress = law.base().stress.get(*ty, GhostType::NotGhost)?;
                fe.compute_btd(stress.view(), out, *ty, GhostType::NotGhost, Some(f))?;
            }
        }
        "integ" => {
            if scratch.btd.len() != blocks.len() {
                run_phase(m, "btd", blocks, dim, fd, scratch)?;
            }
            let fe = m.physics.base.fems.get("")?;
            scratch.integ.resize_with(blocks.len(), || Array::new(1));
            for (((_, ty, f), btd), out) in blocks.iter().zip(&scratch.btd).zip(&mut scratch.integ)
            {
                let n = btd.nb_component();
                fe.integrate(btd.view(), out, n, *ty, GhostType::NotGhost, Some(f))?;
            }
        }
        "asm" => {
            if scratch.integ.len() != blocks.len() {
                run_phase(m, "integ", blocks, dim, fd, scratch)?;
            }
            let mesh_h = m.physics.base.mesh.clone();
            let mesh = mesh_h.read()?;
            let n = mesh.nb_nodes();
            let mut f_int = Array::try_zeros(n, dim)?;
            for ((_, ty, f), r_e) in blocks.iter().zip(&scratch.integ) {
                DofManager::assemble_elemental_array_local_array(
                    &mesh,
                    r_e.view(),
                    &mut f_int,
                    *ty,
                    GhostType::NotGhost,
                    -1.0,
                    Some(f),
                )?;
            }
            std::hint::black_box(&f_int);
        }
        _ => return Err(Error::invalid(format!("unknown phase {phase}"))),
    }
    Ok(true)
}

fn main() -> Result<()> {
    let a = parse_args()?;
    let t0 = Instant::now();
    let mut m = build_model(&a)?;
    let init_s = t0.elapsed().as_secs_f64();
    let dim = a.dim;
    let blocks = law_blocks(&m, dim)?;
    let nb_el: usize = blocks.iter().map(|(_, _, f)| f.len()).sum();
    let types: Vec<String> = blocks
        .iter()
        .map(|(_, t, _)| t.name().to_string())
        .collect();
    let nq: Vec<usize> = blocks
        .iter()
        .map(|(_, t, _)| {
            m.physics
                .base
                .fems
                .get("")
                .and_then(|fe| fe.nb_integration_points(*t, GhostType::NotGhost))
                .unwrap_or(0)
        })
        .collect();
    let nb_nodes = m.physics.base.mesh.read()?.nb_nodes();
    println!(
        "{{\"kind\":\"meta\",\"label\":{:?},\"mesh\":{:?},\"types\":{:?},\"nq\":{:?},\
         \"nb_elements\":{nb_el},\"nb_nodes\":{nb_nodes},\"fd\":{},\"init_s\":{init_s:.6},\
         \"time_step\":{:e},\"threads\":{},\"precision\":{:?},\"simd\":{:?}}}",
        a.label,
        a.mesh.display().to_string(),
        types,
        nq,
        a.fd,
        m.time_step(None)?,
        m.exec_policy().threads(),
        m.exec_policy().precision().name(),
        m.exec_policy().simd().name()
    );
    if let Some(n) = a.digest_steps {
        for _ in 0..n {
            m.solve_step(None)?;
        }
        let mut h = 0xcbf2_9ce4_8422_2325_u64;
        fnv(&mut h, m.displacement()?.as_slice());
        fnv(&mut h, m.velocity()?.as_slice());
        fnv(&mut h, m.acceleration()?.as_slice());
        fnv(&mut h, m.internal_force()?.as_slice());
        let u_max = m
            .displacement()?
            .as_slice()
            .iter()
            .fold(0.0_f64, |acc, x| acc.max(x.abs()));
        println!(
            "{{\"kind\":\"digest\",\"label\":{:?},\"steps\":{n},\"fnv1a\":\"{h:016x}\",\"u_max\":{u_max:e}}}",
            a.label
        );
        return Ok(());
    }
    for _ in 0..a.warmup {
        m.solve_step(None)?;
    }
    let mut scratch = Scratch::default();
    for rep in 0..a.reps {
        for phase in &a.phases {
            // one untimed call: first-touch of reused outputs
            if !run_phase(&mut m, phase, &blocks, dim, a.fd, &mut scratch)? {
                continue;
            }
            let t = Instant::now();
            for _ in 0..a.steps {
                run_phase(&mut m, phase, &blocks, dim, a.fd, &mut scratch)?;
            }
            let s = t.elapsed().as_secs_f64();
            let per = s / (nb_el as f64 * a.steps as f64) * 1e9;
            println!(
                "{{\"kind\":\"time\",\"label\":{:?},\"phase\":{phase:?},\"rep\":{rep},\
                 \"steps\":{},\"seconds\":{s:.6},\"ns_el_step\":{per:.3}}}",
                a.label, a.steps
            );
        }
    }
    Ok(())
}
