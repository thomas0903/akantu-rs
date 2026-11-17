// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test support for the
// material tests: a one-element model reproducing what the material probe
// (oracle/probes/cpp/material_probe.cc) runs on (one simplex, one
// integration point), with the energy integration of upstream
// src/fe_engine/{element_class_tmpl.hh (computeJacobian: J = dnds·Xᵀ,
// det), integrator_gauss_inline_impl.hh (J·w, per-element f·J, Math::reduce),
// gauss_integration.cc (1-point weights)}.

#![allow(dead_code)] // each test binary uses a subset

use akantu_core::eigen_gebp::{Extent, ProductShape, product_into};
use akantu_core::tensor::MatrixX;
use akantu_core::{Array, Element, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_model::ConstitutiveLawsHandler;
use akantu_model::constitutive_law::{LawContext, LawPhase, ModelServices};
use akantu_parser::{AnonymousIds, Parser, ParserType};
use akantu_solid::Material;
use akantu_solid::material::{LawOptions, MaterialFactory, register_builtin_materials};

pub const NG: GhostType = GhostType::NotGhost;

/// The linear simplex of a dimension (the probe's element).
#[must_use]
pub fn simplex(dim: usize) -> ElementType {
    match dim {
        1 => ElementType::Segment2,
        2 => ElementType::Triangle3,
        _ => ElementType::Tetrahedron4,
    }
}

/// `dN/ds` of the simplex (upstream `computeDNDS`), row-major
/// `nat × nnodes`.
fn dnds(dim: usize) -> (usize, usize, Vec<Real>) {
    match dim {
        1 => (1, 2, vec![-0.5, 0.5]),
        2 => (2, 3, vec![-1., 1., 0., -1., 0., 1.]),
        _ => (
            3,
            4,
            vec![-1., 0., 0., 1., -1., 1., 0., 0., -1., 0., 1., 0.],
        ),
    }
}

/// 1-point Gauss weight (upstream `gauss_integration.cc`).
fn weight(dim: usize) -> Real {
    match dim {
        1 => 2.,
        2 => 1. / 2.,
        _ => 1. / 6.,
    }
}

/// Model services of a mesh of linear simplices with one integration point
/// per element.
pub struct SimplexModel {
    pub mesh: Mesh,
    pub displacement: Array<Real>,
    /// `|J|·w` per element (upstream precomputed jacobians).
    pub jw: Vec<Real>,
    /// Gradient returned at every quadrature point by
    /// `gradient_on_integration_points` (column-major `dim × dim`), when set.
    pub gradient: Option<Vec<Real>>,
    /// `true`: the laws' point loops get the serial executor whatever the
    /// process default (tests whose counts assume serial services);
    /// `false` (default): [`process_exec`].
    pub serial: bool,
}

/// The executor of the process default policy (`AKANTU_NUM_THREADS`,
/// `AKANTU_EXEC_GRAIN`, ...), built once: the law-level fixture tests run
/// the laws that declare independent points on that many threads.
pub fn process_exec() -> &'static akantu_core::exec::Exec {
    static EXEC: std::sync::LazyLock<akantu_core::exec::Exec> = std::sync::LazyLock::new(|| {
        let policy = akantu_core::exec::ExecPolicy::process_default()
            .expect("a valid AKANTU_* execution policy in the environment");
        akantu_core::exec::Exec::new(policy).expect("the threads of the process default policy")
    });
    &EXEC
}

impl SimplexModel {
    /// Model on the given nodes (`dim` coordinates each) and simplices.
    pub fn new(dim: usize, nodes: &[Vec<Real>], elements: &[Vec<Idx>]) -> Self {
        let mut mesh = Mesh::new(dim, "mesh").unwrap();
        let ty = simplex(dim);
        {
            let mut a = MeshAccessor::new(&mut mesh);
            for x in nodes {
                a.push_node(&x[..dim]).unwrap();
            }
            let mut c = a.connectivity_mut(ty, NG);
            for e in elements {
                c.push_tuple(e);
            }
        }
        let jw = elements
            .iter()
            .map(|e| {
                let x: Vec<&[Real]> = e.iter().map(|&n| &nodes[n as usize][..dim]).collect();
                jacobian(dim, &x) * weight(dim)
            })
            .collect();
        Self {
            displacement: Array::zeros(nodes.len(), dim),
            mesh,
            jw,
            gradient: None,
            serial: false,
        }
    }

    /// Model from a Gmsh 2.2 ASCII file with one simplex type.
    pub fn from_msh(dim: usize, path: &std::path::Path) -> Self {
        let text = std::fs::read_to_string(path).unwrap();
        let mut lines = text.lines();
        let mut nodes = Vec::new();
        let mut elements = Vec::new();
        while let Some(l) = lines.next() {
            match l.trim() {
                "$Nodes" => {
                    let n: usize = lines.next().unwrap().trim().parse().unwrap();
                    for _ in 0..n {
                        let v: Vec<Real> = lines
                            .next()
                            .unwrap()
                            .split_whitespace()
                            .skip(1)
                            .map(|t| t.parse().unwrap())
                            .collect();
                        nodes.push(v);
                    }
                }
                "$Elements" => {
                    let n: usize = lines.next().unwrap().trim().parse().unwrap();
                    for _ in 0..n {
                        let v: Vec<i64> = lines
                            .next()
                            .unwrap()
                            .split_whitespace()
                            .map(|t| t.parse().unwrap())
                            .collect();
                        let ntags = v[2] as usize;
                        let conn: Vec<Idx> =
                            v[3 + ntags..].iter().map(|&n| (n - 1) as Idx).collect();
                        elements.push(conn);
                    }
                }
                _ => {}
            }
        }
        Self::new(dim, &nodes, &elements)
    }

    /// Phase context on this model.
    pub fn ctx(&self, phase: LawPhase) -> LawContext<'_> {
        LawContext {
            phase,
            services: self,
        }
    }
}

/// Upstream `ElementClass::computeJacobian` for a linear simplex:
/// `J = dnds · Xᵀ` (fixed `dnds`, dynamic node coordinates: coefficient-based
/// product over the nodes), `det(J)` by the dynamic (LU) path.
fn jacobian(dim: usize, x: &[&[Real]]) -> Real {
    let (nat, nn, d) = dnds(dim);
    let mut j = vec![0.0; nat * dim];
    product_into(
        &ProductShape::new(
            Extent::Fixed(nat),
            Extent::Fixed(nn),
            Extent::Dynamic(nn),
            Extent::Dynamic(dim),
        )
        .rhs_transposed(),
        &|i, k| d[i * nn + k],
        &|k, c| x[k][c],
        &mut j,
    )
    .unwrap();
    MatrixX::from_col_major(nat, dim, j).unwrap().det().unwrap()
}

impl ModelServices for SimplexModel {
    /// The process default policy's executor (serial with `serial`).
    fn exec(&self) -> &akantu_core::exec::Exec {
        if self.serial {
            akantu_core::exec::Exec::serial_ref()
        } else {
            process_exec()
        }
    }
    fn spatial_dimension(&self) -> usize {
        self.mesh.spatial_dimension()
    }
    fn mesh(&self) -> &Mesh {
        &self.mesh
    }
    fn time_step(&self) -> Real {
        0.0
    }
    fn nb_integration_points(&self, _: ElementType, _: GhostType) -> usize {
        1
    }
    fn integration_point_coordinates(
        &self,
        ty: ElementType,
        g: GhostType,
        filter: &[Idx],
        out: &mut Array<Real>,
    ) -> Result<()> {
        *out = Array::new(self.mesh.spatial_dimension());
        for &e in filter {
            out.push_tuple(&self.mesh.barycenter(Element::new(ty, e, g))?);
        }
        Ok(())
    }
    fn nodal_field(&self, name: &str) -> Result<akantu_core::array_shared::ArrayRef<'_, Real>> {
        match name {
            "displacement" => Ok((&self.displacement).into()),
            _ => Err(Error::not_found("nodal field", name)),
        }
    }
    fn gradient_on_integration_points(
        &self,
        field: &str,
        out: &mut Array<Real>,
        nb_component: usize,
        _ty: ElementType,
        _ghost: GhostType,
        filter: &[Idx],
    ) -> Result<()> {
        let _ = self.nodal_field(field)?;
        let g = self
            .gradient
            .as_ref()
            .ok_or_else(|| Error::not_found("FE engine", "prescribed gradient"))?;
        let d = self.mesh.spatial_dimension();
        *out = Array::new(nb_component * d);
        for _ in filter {
            out.push_tuple(g);
        }
        Ok(())
    }
    /// Upstream `IntegratorGauss::integrate(f, type, ghost, filter)` with
    /// one quadrature point: per element `0 + f·(|J|w)` (GEMV fallback of a
    /// `1 × 1` result), then `Math::reduce`.
    fn integrate_scalar(
        &self,
        f: &[Real],
        _ty: ElementType,
        _ghost: GhostType,
        filter: &[Idx],
    ) -> Result<Real> {
        if f.is_empty() {
            return Ok(0.0);
        }
        let mut per_element: Vec<Real> = filter
            .iter()
            .zip(f)
            .map(|(&e, &v)| {
                let mut acc = 0.0;
                acc += v * self.jw[e as usize];
                acc
            })
            .collect();
        Ok(akantu_core::math::reduce(&mut per_element))
    }
}

/// Factory with the built-in materials.
#[must_use]
pub fn factory() -> MaterialFactory {
    let mut f = MaterialFactory::default();
    register_builtin_materials(&mut f).unwrap();
    f
}

/// Upstream `SolidMechanicsModel::initFull` restricted to the materials:
/// instantiate from the `.dat` text (root sections), assign every element
/// (default selector), `initMaterial`. Laws keep their default (corrected)
/// [`LawOptions`].
pub fn materials_from_dat(
    model: &SimplexModel,
    dat: &[u8],
) -> Result<ConstitutiveLawsHandler<dyn Material>> {
    materials_from_dat_with(model, dat, None)
}

/// [`materials_from_dat`] with the upstream-defect switches of every law set
/// to `options` after parsing and **before** `initMaterial` (the switches
/// that shape the stiffness or copy parameters into internals are applied by
/// the updates of `initMaterial`; see [`LawOptions`]).
pub fn materials_from_dat_with(
    model: &SimplexModel,
    dat: &[u8],
    options: Option<LawOptions>,
) -> Result<ConstitutiveLawsHandler<dyn Material>> {
    let p = Parser::parse_bytes(dat, "material.dat", AnonymousIds::Local(&mut 0))?;
    let mut h = ConstitutiveLawsHandler::<dyn Material>::new(
        "solid_mechanics_model",
        &model.mesh,
        model.mesh.spatial_dimension(),
        ParserType::Material,
    )?;
    let f = factory();
    h.instantiate_laws(&f, &p.root, model, false)?;
    h.assign_laws_to_elements(&model.mesh, None)?;
    if let Some(o) = options {
        for law in h.laws_mut() {
            if let Some(slot) = law.options_mut() {
                *slot = o;
            }
        }
    }
    let ctx = model.ctx(LawPhase::Init);
    for law in h.laws_mut() {
        law.init_material(&ctx)?;
    }
    Ok(h)
}

/// [`materials_from_dat_with`] for the single material section of `dat`
/// registered through the factory with the allocator option `option`, as
/// the probe's `--factory-option` does (upstream
/// `registerNewMaterial(name, type, option)`, `parseSection`,
/// `initConstitutiveLaws`): options an input file cannot spell, e.g.
/// `anisotropic_damage` `mazars-drucker-prager` (section options are
/// identifiers).
pub fn materials_from_dat_with_factory_option(
    model: &SimplexModel,
    dat: &[u8],
    options: Option<LawOptions>,
    option: &str,
) -> Result<ConstitutiveLawsHandler<dyn Material>> {
    let p = Parser::parse_bytes(dat, "material.dat", AnonymousIds::Local(&mut 0))?;
    let mut h = ConstitutiveLawsHandler::<dyn Material>::new(
        "solid_mechanics_model",
        &model.mesh,
        model.mesh.spatial_dimension(),
        ParserType::Material,
    )?;
    let f = factory();
    let sections: Vec<_> = p.root.sub_sections_of(ParserType::Material).collect();
    let [section] = sections.as_slice() else {
        return Err(Error::invalid(
            "--factory-option needs exactly one material section",
        ));
    };
    let name = section
        .parameter("name")
        .ok_or_else(|| Error::invalid("material section without a name"))?
        .value
        .to_string_lossy()
        .into_owned();
    let index = h.register_new_law(&f, &name, &section.name, option, model)?;
    h.law_mut(index)?.parse_section(section, false)?;
    h.assign_laws_to_elements(&model.mesh, None)?;
    if let Some(o) = options {
        for law in h.laws_mut() {
            if let Some(slot) = law.options_mut() {
                *slot = o;
            }
        }
    }
    let ctx = model.ctx(LawPhase::Init);
    for law in h.laws_mut() {
        law.init_material(&ctx)?;
    }
    Ok(h)
}

/// `M(r, c)` of column-major tuples, flattened row-major (fixture layout
/// `[step, row, col]`).
#[must_use]
pub fn col_major_to_rows(tuple: &[Real], n: usize) -> Vec<Real> {
    let mut v = Vec::with_capacity(n * n);
    for r in 0..n {
        for c in 0..n {
            v.push(tuple[r + c * n]);
        }
    }
    v
}

/// Bit-identity count of two arrays (exact comparison of decision D-046:
/// default NaNs of either sign are identical, other NaNs by bits).
#[must_use]
pub fn bitwise_equal_count(a: &[Real], b: &[Real]) -> usize {
    a.iter()
        .zip(b)
        .filter(|&(&x, &y)| akantu_validation::exact_eq(x, y))
        .count()
}
