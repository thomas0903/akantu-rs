// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): helpers of the extra-materials test ports
// (extra_packages/extra-materials/test/*).

//! Shared helpers of the extra-materials tests.

#![allow(dead_code)] // each test binary uses a subset

use std::path::{Path, PathBuf};

use akantu_core::enums::AnalysisMethod;
use akantu_extra_materials::{
    ExtraLawOptions, IterativeDamage, as_iterative_damage_mut, register_extra_materials_with,
};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{LawContext, LawPhase};
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solid::{Material, SolidMechanicsModel};

/// `tests/data/<rel>`.
pub fn data(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(rel)
}

/// Read a Gmsh mesh.
pub fn read_mesh(path: &Path, dim: usize) -> Mesh {
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    akantu_io::read_mesh(&mut mesh, path, akantu_core::enums::MeshIoType::Gmsh).unwrap();
    mesh
}

/// A solid model on `mesh` with the input text `dat` and the extra
/// materials registered.
pub fn model_with(mesh: Mesh, dat: &str) -> SolidMechanicsModel {
    model_with_options(mesh, dat, ExtraLawOptions::default())
}

/// [`model_with`] with the extra materials registered with `options`
/// (in effect when the input is parsed).
pub fn model_with_options(mesh: Mesh, dat: &str, options: ExtraLawOptions) -> SolidMechanicsModel {
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    register_extra_materials_with(&mut model.physics.factory, options).unwrap();
    model
}

/// `initFull(_static)`.
pub fn init_static(model: &mut SolidMechanicsModel) {
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
}

/// Run `f` on the iterative-damage interface of material `index`.
pub fn with_iterative<R>(
    model: &mut SolidMechanicsModel,
    index: usize,
    f: impl FnOnce(&mut dyn IterativeDamage, &LawContext<'_>) -> akantu_core::Result<R>,
) -> R {
    model
        .with_material(index, LawPhase::AfterSolveStep, |law, ctx| {
            let it = as_iterative_damage_mut(law).expect("an iterative damage law");
            f(it, ctx)
        })
        .unwrap()
}

/// Run `f` on material `index` with its phase context.
pub fn with_law<R>(
    model: &mut SolidMechanicsModel,
    index: usize,
    f: impl FnOnce(&mut dyn Material, &LawContext<'_>) -> akantu_core::Result<R>,
) -> R {
    model.with_material(index, LawPhase::Energy, f).unwrap()
}

/// Set the Newton–Raphson parameters of the default solver.
pub fn set_newton(
    model: &mut SolidMechanicsModel,
    max_iterations: i64,
    threshold: f64,
    criterion: &str,
) {
    let nls = model.solver.non_linear_solver_mut(None).unwrap();
    nls.set_param("max_iterations", ParamValue::Int(max_iterations))
        .unwrap();
    nls.set_param("threshold", ParamValue::Real(threshold))
        .unwrap();
    nls.set_param("convergence_type", ParamValue::Enum(criterion.into()))
        .unwrap();
}

/// C `printf("%.<p>g", x)` (the default `std::ostream` floating format
/// with `std::setprecision(p)`), for finite `x`.
pub fn format_g(x: f64, p: usize) -> String {
    if x == 0. {
        return if x.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }
    let sci = format!("{:.*e}", p - 1, x);
    let (mantissa, exp) = sci.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let strip = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    if exp < -4 || exp >= p as i32 {
        let m = strip(mantissa);
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{m}e{sign}{:02}", exp.abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        strip(&format!("{x:.decimals$}"))
    }
}
