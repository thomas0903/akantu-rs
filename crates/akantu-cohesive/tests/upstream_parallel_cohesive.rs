// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ports with the same
// intent of the parallel cohesive tests of
// test/test_model/test_solid_mechanics_model/test_cohesive/
// test_parallel_cohesive/ (unregistered upstream: the directory is commented
// out of test_cohesive/CMakeLists.txt and the tests use the pre-v4 API):
//   test_cohesive_facet_stress_synchronizer.cc,
//   test_cohesive_parallel_insertion/test_cohesive_parallel_insertion_along_physical_surfaces.cc,
//   test_cohesive_parallel_extrinsic/test_cohesive_parallel_extrinsic_tetrahedron_displacement.cc,
//   test_cohesive_parallel_buildfragments/test_cohesive_parallel_buildfragments.cc (TEST-236),
//   test_cohesive_parallel_insertion/test_cohesive_parallel_intrinsic_implicit_insertion.cc (TEST-242).
// The meshes are the frozen ones of fixtures/meshes; Scotch is replaced by
// the recursive coordinate bisection (any rank count), by the probe's box
// rule for the oracle record facet_stress_tet10 and by the partitions
// recorded in TEST-242's 2017 output files.

mod par_coh;

use std::time::Duration;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_cohesive::selector::MeshDataMaterialCohesiveSelector;
use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{Array, Element, ElementKind, ElementType, GhostType, Idx, Real, TypeFilter};
use akantu_mesh::facet_synchronizer::DistributedFacetsOptions;
use akantu_mesh::partition::{MeshPartitioner, PrescribedPartition, RcbPartitioner};
use akantu_mesh::utils::{ELEMENT_TO_SUBELEMENT, FacetFlipMode, init_mesh_facets_with};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use par_coh::{Outcome, ok};

fn on_ranks(ranks: &[usize], f: impl Fn(Communicator) -> par_coh::Outcome + Sync) {
    for &np in ranks {
        let errors: Vec<String> = run_local(np, par_coh::local_timeout(), &f)
            .into_iter()
            .enumerate()
            .filter_map(|(r, o)| o.err().map(|e| format!("np {np} rank {r}: {e}")))
            .collect();
        assert!(errors.is_empty(), "{}", errors.join("\n"));
    }
}

/// Linearised parts of a read mesh (rank 0).
type PartsFn<'a> = &'a dyn Fn(&Mesh) -> Vec<Idx>;

/// `(barycentre, element-node displacements)` of every not-ghost element.
type ElementDisplacements = Vec<(Vec<Real>, Vec<Real>)>;

fn frozen(rel: &str) -> std::path::PathBuf {
    akantu_validation::fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/test_cohesive")
        .join(rel)
}

/// Read `rel` on rank 0 and distribute it over `comm` (`parts`: prescribed
/// linearised parts on rank 0, else RCB), facet mesh with `flip`.
fn read_distributed(
    comm: &Communicator,
    rel: &str,
    dim: usize,
    parts: Option<PartsFn<'_>>,
    flip: FacetFlipMode,
) -> std::result::Result<SharedMesh, String> {
    let mut m = ok(Mesh::with_communicator(dim, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        ok(
            akantu_io::read_mesh(&mut m, &frozen(rel), MeshIoType::Gmsh),
            "read",
        )?;
    }
    let _ = match parts {
        Some(f) => {
            let p = if comm.rank() == 0 { f(&m) } else { Vec::new() };
            let mut p = PrescribedPartition::new(p);
            ok(
                akantu_mesh::distribution::distribute_with(
                    &mut m,
                    Some(&mut p as &mut dyn MeshPartitioner),
                    0,
                ),
                "distribute",
            )?
        }
        None => {
            let mut p = RcbPartitioner;
            ok(
                akantu_mesh::distribution::distribute_with(
                    &mut m,
                    Some(&mut p as &mut dyn MeshPartitioner),
                    0,
                ),
                "distribute",
            )?
        }
    };
    let _ = ok(
        init_mesh_facets_with(&mut m, DistributedFacetsOptions { flip }),
        "init_mesh_facets",
    )?;
    Ok(SharedMesh::new(m))
}

const FACET_STRESS_DAT: &str = "material elastic [\n  name = steel\n  rho = 1e3\n  E = 1e3\n  nu = 0.001\n]\nmaterial cohesive_linear [\n  name = cohesive\n  beta = 1\n  G_c = 100\n  sigma_c = 100\n]\n";

/// The upstream test's stress field: `σ_ij = (3 min(i,j) + max(i,j)) + 2x +
/// 3y + 4z`, symmetric.
fn stress_function(constant: Real, x: Real, y: Real, z: Real) -> Real {
    constant + 2. * x + 3. * y + 4. * z
}

/// The facet-stress scenario: Tet10 `tetrahedron.msh`, extrinsic model,
/// `Math::setTolerance(1e-11)`, the not-ghost bulk stresses set to the
/// linear field, then `checkCohesiveStress` (no facet reaches `σ_c`).
fn facet_stress_run(
    mesh: SharedMesh,
) -> std::result::Result<(SolidMechanicsModelCohesive, usize), String> {
    let parser = ok(
        Parser::parse_str(FACET_STRESS_DAT, "material.dat"),
        "parser",
    )?;
    let mut model = ok(
        SolidMechanicsModelCohesive::with_parser(
            mesh.clone(),
            None,
            akantu_cohesive::model::DEFAULT_ID,
            parser,
        ),
        "model",
    )?;
    model.set_math_tolerance(1e-11);
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true),
        "init_full",
    )?;
    {
        let m = ok(mesh.read(), "read")?;
        let ty = ElementType::Tetrahedron10;
        let ne = m.nb_element(ty, GhostType::NotGhost);
        let fe = ok(model.smm.physics.base.fems.get(""), "fe")?;
        let nq = ok(fe.nb_integration_points(ty, GhostType::NotGhost), "nq")?;
        let mut quad = Array::new(3);
        ok(
            fe.interpolate_on_integration_points(
                &m,
                m.nodes().view(),
                &mut quad,
                3,
                ty,
                GhostType::NotGhost,
                None,
            ),
            "quad",
        )?;
        let q = quad.as_slice();
        let law = ok(model.smm.physics.materials.law_mut(0), "law")?;
        let filter: Vec<Idx> = law
            .base()
            .element_filter
            .get(ty, GhostType::NotGhost)
            .map(|a| a.as_slice().to_vec())
            .unwrap_or_default();
        let mut stress = ok(
            law.base_mut().stress.get_mut(ty, GhostType::NotGhost),
            "stress",
        )?;
        let s = stress.as_mut_slice();
        if s.len() != filter.len() * nq * 9 || filter.len() != ne {
            return Err("unexpected stress layout".into());
        }
        for (k, &e) in filter.iter().enumerate() {
            for qq in 0..nq {
                let p = &q[(e as usize * nq + qq) * 3..(e as usize * nq + qq + 1) * 3];
                for i in 0..3 {
                    for j in 0..3 {
                        let (a, b) = (i.min(j), i.max(j));
                        s[(k * nq + qq) * 9 + i * 3 + j] =
                            stress_function((a * 3 + b) as Real, p[0], p[1], p[2]);
                    }
                }
            }
        }
    }
    let n = ok(model.check_cohesive_stress(), "checkCohesiveStress")?;
    Ok((model, n))
}

/// `test_cohesive_facet_stress_synchronizer`: on every checked not-ghost
/// facet with a ghost neighbour (the facets whose second side arrives
/// through `_smmc_facets_stress`) both sides of the facet stress equal the linear
/// field at the facet quadrature points (upstream `Math::are_float_equal`,
/// tolerance 1e-11: the elemental interpolation of a linear field from the
/// Tet10 quadrature points is exact up to rounding).
fn facet_stress_synchronizer(comm: Communicator) -> Outcome {
    let mesh = read_distributed(
        &comm,
        "test_parallel_cohesive/test_cohesive_facet_stress_synchronizer/tetrahedron.msh",
        3,
        None,
        FacetFlipMode::RemoteOrder,
    )?;
    let (model, inserted) = facet_stress_run(mesh.clone())?;
    // local verdict first, then one reduction (no early return before the
    // collective)
    let local = check_facet_stresses(&model, &mesh, inserted);
    let mut total = [
        local.as_ref().map_or(0, |&n| n as u64),
        u64::from(local.is_err()),
    ];
    ok(
        comm.all_reduce(&mut total, akantu_comm::ReduceOp::Sum),
        "reduce",
    )?;
    local?;
    if total[1] > 0 {
        return Err("the check failed on another rank".into());
    }
    if comm.size() > 1 && total[0] == 0 {
        return Err("no facet with a ghost neighbour was checked".into());
    }
    Ok(())
}

/// The upstream check on this rank; returns the number of checked facet
/// quadrature points.
fn check_facet_stresses(
    model: &SolidMechanicsModelCohesive,
    mesh: &SharedMesh,
    inserted: usize,
) -> std::result::Result<usize, String> {
    if inserted != 0 {
        return Err(format!(
            "{inserted} cohesive elements inserted (expected none)"
        ));
    }
    let m = ok(mesh.read(), "read")?;
    let fm = m.mesh_facets().ok_or("no facet mesh")?;
    let ft = ElementType::Triangle6;
    let e2s = ok(fm.data().element_lists(ELEMENT_TO_SUBELEMENT), "e2s")?;
    let mut checked = 0usize;
    // upstream loops over the not-ghost facets (`getNbElement(type_facet)`,
    // default `_not_ghost`): the facet owners, which receive the second side
    // through `_smmc_facets_stress`
    for g in [GhostType::NotGhost] {
        let nf = fm.nb_element(ft, g);
        if nf == 0 {
            continue;
        }
        let mut quad = Array::new(3);
        ok(
            model.ext.facet_fe.interpolate_on_integration_points(
                fm,
                fm.nodes().view(),
                &mut quad,
                3,
                ft,
                g,
                None,
            ),
            "facet quad",
        )?;
        let nq = quad.len() / nf;
        let fs = ok(model.ext.facet_stress.get(ft, g), "facet_stress")?;
        let check = ok(model.ext.inserter.check_facets().get(ft, g), "check")?;
        for f in 0..nf {
            let el = ok(Element::try_new(ft, f, g), "el")?;
            let row = &ok(e2s.element(el), "row")?[0];
            let both_local = row
                .iter()
                .take(2)
                .all(|e| e.ghost_type() == GhostType::NotGhost);
            if !check.as_slice()[f] || both_local {
                continue;
            }
            for qq in 0..nq {
                let p = &quad.as_slice()[(f * nq + qq) * 3..(f * nq + qq + 1) * 3];
                for side in 0..2 {
                    for i in 0..3 {
                        for j in 0..3 {
                            let (a, b) = (i.min(j), i.max(j));
                            let want = stress_function((a * 3 + b) as Real, p[0], p[1], p[2]);
                            let have = fs.as_slice()[((f * nq + qq) * 2 + side) * 9 + i * 3 + j];
                            if !akantu_core::math::are_float_equal(have, want, 1e-11) {
                                return Err(format!(
                                    "facet {el} (row {row:?}) point {qq} side {side} ({i},{j}): \
                                     {have} vs {want}"
                                ));
                            }
                        }
                    }
                }
                checked += 1;
            }
        }
    }
    Ok(checked)
}

#[test]
fn test_cohesive_facet_stress_synchronizer() {
    on_ranks(&[1, 2, 3, 4], facet_stress_synchronizer);
}

/// The oracle record `facet_stress_tet10` (probe box rule): topology,
/// synchronizer schemes and the facet stresses of both sides, bitwise.
fn facet_stress_matches_oracle(comm: Communicator) -> Outcome {
    let np = comm.size();
    let fx = par_coh::fixture("facet_stress_tet10", &comm)?;
    let parts = move |m: &Mesh| -> Vec<Idx> {
        let (lo, hi) = (m.lower_bounds().to_vec(), m.upper_bounds().to_vec());
        let c: Vec<Real> = (0..3).map(|i| 0.5 * (lo[i] + hi[i])).collect();
        let mut out = Vec::new();
        let mut b = vec![0.0; 3];
        for ty in m.element_types(TypeFilter {
            spatial_dimension: Some(3),
            ghost_type: GhostType::NotGhost,
            kind: None,
        }) {
            for e in 0..m.nb_element(ty, GhostType::NotGhost) {
                if m.barycenter_into(Element::new(ty, e as Idx, GhostType::NotGhost), &mut b)
                    .is_err()
                {
                    return Vec::new();
                }
                let mut p = Idx::from(b[0] >= c[0]);
                if np == 4 && b[1] >= c[1] {
                    p += 2;
                }
                out.push(if np == 1 { 0 } else { p });
            }
        }
        out
    };
    let mesh = read_distributed(
        &comm,
        "test_parallel_cohesive/test_cohesive_facet_stress_synchronizer/tetrahedron.msh",
        3,
        Some(&parts),
        FacetFlipMode::UpstreamInverse,
    )?;
    let (model, inserted) = facet_stress_run(mesh.clone())?;
    let mut rec = par_coh::oracle::Rec::for_fixture(&fx);
    {
        let m = ok(mesh.read(), "read")?;
        rec.topology("check/", &model, &m, par_coh::oracle::Level::Conn)?;
        let fm = m.mesh_facets().ok_or("no facet mesh")?;
        let mut all = akantu_core::ElementTypeMapArray::<bool>::new("all");
        for g in GhostType::ALL {
            for ty in fm.element_types(TypeFilter {
                spatial_dimension: Some(2),
                ghost_type: g,
                kind: Some(ElementKind::Regular),
            }) {
                ok(all.alloc(fm.nb_element(ty, g), 1, ty, g, true), "alloc")?;
            }
        }
        rec.facet_stress("check/facet_stress/", &model, fm, &all);
    }
    let mut bad = par_coh::oracle::compare(&fx, &rec, "check/");
    if fx.manifest().parameters["inserted"].as_u64() != Some(inserted as u64) {
        bad.push(format!(
            "inserted {inserted} vs oracle {}",
            fx.manifest().parameters["inserted"]
        ));
    }
    if bad.is_empty() {
        Ok(())
    } else {
        let n = bad.len();
        bad.truncate(20);
        Err(format!("{n} difference(s):\n  {}", bad.join("\n  ")))
    }
}

#[test]
fn facet_stress_tet10_matches_the_oracle() {
    on_ranks(&[1, 2, 4], facet_stress_matches_oracle);
}

const INSERTION_INPUT: &str = include_str!("data/insertion_input_file.dat");

/// `test_cohesive_parallel_insertion_along_physical_surfaces`: intrinsic
/// insertion along the six named surfaces of `3d_spherical_inclusion.msh`
/// with one `cohesive_exponential` law each (selected by the facet's
/// physical name); on every rank and ghost type each law holds as many
/// cohesive elements as its surface has facets of that ghost type in the
/// facet mesh's `physical_names`, the not-ghost counts add up over the ranks
/// to the surface's facet count, and the stiffness matrix assembles.
fn insertion_along_physical_surfaces(comm: Communicator) -> Outcome {
    let mesh = read_distributed(
        &comm,
        "test_parallel_cohesive/test_cohesive_parallel_insertion/3d_spherical_inclusion.msh",
        3,
        None,
        FacetFlipMode::RemoteOrder,
    )?;
    let parser = ok(
        Parser::parse_str(INSERTION_INPUT, "input_file.dat"),
        "parser",
    )?;
    let mut model = ok(
        SolidMechanicsModelCohesive::with_parser(
            mesh.clone(),
            None,
            akantu_cohesive::model::DEFAULT_ID,
            parser,
        ),
        "model",
    )?;
    model.set_math_tolerance(1e-15);
    let default = model.default_cohesive_selector();
    model.set_material_selector(Box::new(MeshDataMaterialCohesiveSelector { default }));
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::Static), false),
        "init_full",
    )?;
    let names = ["interface", "coh1", "coh2", "coh3", "coh4", "coh5"];
    let mut local_counts = Vec::new();
    {
        let m = ok(mesh.read(), "read")?;
        let fm = m.mesh_facets().ok_or("no facet mesh")?;
        let pn = ok(
            fm.data().strings(akantu_mesh::utils::PHYSICAL_NAMES),
            "names",
        )?;
        for ty in m.element_types(TypeFilter {
            spatial_dimension: Some(3),
            ghost_type: GhostType::NotGhost,
            kind: Some(ElementKind::Cohesive),
        }) {
            let ft = ty.facet_type(0).ok_or("facet type")?;
            for g in GhostType::ALL {
                for name in names {
                    let facets = pn
                        .get(ft, g)
                        .map(|a| a.as_slice().iter().filter(|n| n.as_str() == name).count())
                        .unwrap_or(0);
                    let mat = ok(model.cohesive_material(name), "law")?;
                    let inserted = akantu_solid::Material::base(mat)
                        .element_filter
                        .get(ty, g)
                        .map_or(0, |f| f.len());
                    if inserted != facets {
                        return Err(format!(
                            "surface {name}: {inserted} inserted elements of type {ty} ({g:?}) out \
                             of {facets} facets"
                        ));
                    }
                    if g == GhostType::NotGhost {
                        local_counts.push(inserted as u64);
                    }
                }
            }
        }
    }
    // the not-ghost elements over the ranks: the serial counts
    let mut total = local_counts.clone();
    ok(
        comm.all_reduce(&mut total, akantu_comm::ReduceOp::Sum),
        "reduce",
    )?;
    if total.iter().sum::<u64>() == 0 {
        return Err("nothing inserted".into());
    }
    ok(
        model.smm.assemble_stiffness_matrix(true),
        "assembleStiffnessMatrix",
    )?;
    Ok(())
}

#[test]
fn test_cohesive_parallel_insertion_along_physical_surfaces() {
    on_ranks(&[1, 2, 3, 4], insertion_along_physical_surfaces);
}

/// The scenario of `test_cohesive_parallel_extrinsic_tetrahedron_displacement`
/// on `tetrahedron.msh` (Tet10), extrinsic: both x faces blocked, `v_x =
/// v_y = x`, every step `u_x, u_y += dt · x` on them, `checkCohesiveStress`,
/// solve; `steps` steps. Returns `(barycentre, element-node displacements)`
/// of every not-ghost Tet10.
fn tet10_displacement_run(
    comm: &Communicator,
    steps: usize,
) -> std::result::Result<ElementDisplacements, String> {
    let dat = "material elastic [\n  name = steel\n  rho = 1e3\n  E = 1e3\n  nu = 0.001\n]\nmaterial cohesive_linear [\n  name = cohesive\n  beta = 1\n  G_c = 100\n  sigma_c = 100\n]\n";
    let mesh = read_distributed(
        comm,
        "test_parallel_cohesive/test_cohesive_parallel_extrinsic/tetrahedron.msh",
        3,
        None,
        FacetFlipMode::RemoteOrder,
    )?;
    let parser = ok(Parser::parse_str(dat, "material.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModelCohesive::with_parser(
            mesh.clone(),
            None,
            akantu_cohesive::model::DEFAULT_ID,
            parser,
        ),
        "model",
    )?;
    model.set_math_tolerance(1e-12);
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true),
        "init_full",
    )?;
    let dt = ok(model.stable_time_step(), "dt")? * 0.1;
    ok(model.set_time_step(dt, None), "set dt")?;
    {
        let (x, n) = {
            let m = ok(mesh.read(), "read")?;
            (m.nodes().as_slice().to_vec(), m.nb_nodes())
        };
        let mut b = ok(model.smm.blocked_dofs_mut(), "blocked")?;
        for i in 0..n {
            if x[3 * i] > 0.99 || x[3 * i] < -0.99 {
                b.as_mut_slice()[3 * i..3 * i + 3].fill(true);
            }
        }
        drop(b);
        let mut v = ok(model.smm.velocity_mut(), "v")?;
        for i in 0..n {
            v.as_mut_slice()[3 * i] = x[3 * i];
            v.as_mut_slice()[3 * i + 1] = x[3 * i];
        }
    }
    for _ in 0..steps {
        let (x, n) = {
            let m = ok(mesh.read(), "read")?;
            (m.nodes().as_slice().to_vec(), m.nb_nodes())
        };
        {
            let mut u = ok(model.smm.displacement_mut(), "u")?;
            for i in 0..n {
                if x[3 * i] > 0.99 || x[3 * i] < -0.99 {
                    u.as_mut_slice()[3 * i] += dt * x[3 * i];
                    u.as_mut_slice()[3 * i + 1] += dt * x[3 * i];
                }
            }
        }
        ok(model.check_cohesive_stress(), "check")?;
        ok(model.solve_step(None), "solve")?;
    }
    let m = ok(mesh.read(), "read")?;
    let ty = ElementType::Tetrahedron10;
    let c = ok(m.connectivity(ty, GhostType::NotGhost), "conn")?;
    let u = ok(model.smm.displacement(), "u")?;
    let mut out = Vec::new();
    let mut b = vec![0.0; 3];
    for (e, row) in c.as_slice().as_chunks::<10>().0.iter().enumerate() {
        ok(
            m.barycenter_into(Element::new(ty, e as Idx, GhostType::NotGhost), &mut b),
            "bary",
        )?;
        let mut d = Vec::with_capacity(30);
        for &nd in row {
            d.extend_from_slice(&u.as_slice()[nd as usize * 3..nd as usize * 3 + 3]);
        }
        out.push((b.clone(), d));
    }
    Ok(out)
}

/// `test_cohesive_parallel_extrinsic_tetrahedron_displacement`: the element
/// node displacements of the distributed run, matched with the serial run's
/// by barycentre, agree within the upstream test's 1e-9 (maximum absolute
/// error) after 500 steps with insertions.
fn tet10_displacement(comm: Communicator) -> Outcome {
    let steps = 500;
    let local = tet10_displacement_run(&comm, steps)?;
    let serial = {
        let solo = Communicator::local_world(1, Duration::from_secs(300))
            .into_iter()
            .next()
            .ok_or("local world")?;
        tet10_displacement_run(&solo, steps)?
    };
    let mut max_err: Real = 0.0;
    for (bary, d) in &local {
        let (_, sd) = serial
            .iter()
            .find(|(b, _)| b.iter().zip(bary).all(|(a, c)| (a - c).abs() < 1e-12))
            .ok_or_else(|| format!("element at {bary:?} not found in the serial run"))?;
        for (a, b) in d.iter().zip(sd) {
            max_err = max_err.max((a - b).abs());
            if (a - b).is_nan() {
                return Err("NaN displacement".into());
            }
        }
    }
    let ok_err = max_err <= 1e-9;
    if !ok_err {
        return Err(format!("max displacement error {max_err:e} > 1e-9"));
    }
    Ok(())
}

#[test]
fn test_cohesive_parallel_extrinsic_tetrahedron_displacement() {
    on_ranks(&[2, 4], tet10_displacement);
}

// ---------------------------------------------------------------------------
// TEST-236: test_cohesive_parallel_buildfragments
// ---------------------------------------------------------------------------

/// Upstream's material file of the test (copied in the parser fixtures).
const BUILDFRAGMENTS_DAT: &str = include_str!(
    "../../akantu-parser/tests/fixtures/test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_buildfragments/material.dat"
);

/// The test's `global_tolerance` (upstream `Math::setTolerance`; the
/// acceptance criteria use `Math::are_float_equal` with it).
const BF_TOLERANCE: Real = 1e-9;
const BF_NB_FRAGMENT: usize = 4;

/// Upstream `Math::are_float_equal` (combined absolute and relative test).
fn are_float_equal(x: Real, y: Real) -> bool {
    let abs_max = x.abs().max(y.abs()).max(1.0);
    (x - y).abs() <= BF_TOLERANCE * abs_max
}

/// Upstream `isInertiaEqual`: relative difference of every principal
/// moment below the tolerance.
fn is_inertia_equal(a: &[Real], b: &[Real]) -> bool {
    a.iter()
        .zip(b)
        .all(|(x, y)| (x - y).abs() / x < BF_TOLERANCE)
}

/// Fragment statistics of one iteration (every rank holds the global
/// arrays).
#[derive(Clone, Debug)]
struct FragStats {
    nb: usize,
    mass: Vec<Real>,
    center: Vec<Real>,
    velocity: Vec<Real>,
    inertia: Vec<Real>,
    nb_elements: Vec<usize>,
}

/// The frozen mesh of the test read on rank 0 (`purifyMesh` there, as
/// upstream), distributed over `comm` with recursive coordinate bisection
/// (instead of Scotch); `None`: the serial (not distributed) mesh.
fn buildfragments_mesh(comm: Option<&Communicator>) -> std::result::Result<SharedMesh, String> {
    let rel = "test_parallel_cohesive/test_cohesive_parallel_buildfragments/mesh.msh";
    let mut m = match comm {
        Some(c) => ok(Mesh::with_communicator(3, "mesh", c.clone()), "mesh")?,
        None => ok(Mesh::new(3, "mesh"), "mesh")?,
    };
    if comm.is_none_or(|c| c.rank() == 0) {
        ok(
            akantu_io::read_mesh(&mut m, &frozen(rel), MeshIoType::Gmsh),
            "read",
        )?;
        let _ = ok(akantu_mesh::utils::purify_mesh(&mut m), "purify")?;
    }
    if comm.is_some() {
        let mut p = RcbPartitioner;
        let _ = ok(
            akantu_mesh::distribution::distribute_with(
                &mut m,
                Some(&mut p as &mut dyn MeshPartitioner),
                0,
            ),
            "distribute",
        )?;
    }
    let _ = ok(
        init_mesh_facets_with(
            &mut m,
            DistributedFacetsOptions {
                flip: FacetFlipMode::RemoteOrder,
            },
        ),
        "init_mesh_facets",
    )?;
    Ok(SharedMesh::new(m))
}

/// Global bounding box `(lower, upper)` of the nodes (all ranks).
fn global_bounds(m: &Mesh) -> std::result::Result<([Real; 3], [Real; 3]), String> {
    let mut lo = [Real::INFINITY; 3];
    let mut hi = [Real::NEG_INFINITY; 3];
    for x in m.nodes().as_slice().as_chunks::<3>().0 {
        for i in 0..3 {
            lo[i] = lo[i].min(x[i]);
            hi[i] = hi[i].max(x[i]);
        }
    }
    let c = m.communicator();
    ok(c.all_reduce(&mut lo, akantu_comm::ReduceOp::Min), "min")?;
    ok(c.all_reduce(&mut hi, akantu_comm::ReduceOp::Max), "max")?;
    Ok((lo, hi))
}

/// The driver of test_cohesive_parallel_buildfragments with the current
/// API (by intent: `initParallel` → `distribute_with`, `updateResidual` →
/// `assemble_internal_forces`; the dumps are not ported; the rotation of
/// the nodes by π/4 between the fragment manager's construction and the
/// stress check (upstream `rotateArray` on `mesh.getNodes()`, undone and
/// redone around every `displaceElements`) is not applied. It does not
/// change what the test checks: upstream's `FragmentManager` freezes its
/// quadrature coordinates at construction (`fragment_manager.cc:57-62`),
/// before the rotation, and the rotation changes only the facet normals of
/// the stress check, in which every check facet exceeds σc = 1 Pa whether
/// the normal is rotated or not (`u_x = 0.1 x`, E = 42 GPa).
/// The port could apply it, through the export path
/// `SharedMesh::with_exports` that Python's writable `Mesh.getNodes()`
/// uses, PY-053). Every acceptance criterion of the upstream test is
/// checked on every rank; returns the statistics of the four iterations.
fn buildfragments_run(mesh: SharedMesh) -> std::result::Result<Vec<FragStats>, String> {
    let parser = ok(
        Parser::parse_str(BUILDFRAGMENTS_DAT, "material.dat"),
        "parser",
    )?;
    let mut model = ok(
        SolidMechanicsModelCohesive::with_parser(
            mesh.clone(),
            None,
            akantu_cohesive::model::DEFAULT_ID,
            parser,
        ),
        "model",
    )?;
    model.set_math_tolerance(BF_TOLERANCE);
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true),
        "init_full",
    )?;
    let (l, h, nb_element) = {
        let m = ok(mesh.read(), "read")?;
        let (lo, hi) = global_bounds(&m)?;
        let mut n = [m
            .element_types(TypeFilter {
                spatial_dimension: Some(3),
                ghost_type: GhostType::NotGhost,
                kind: Some(ElementKind::Regular),
            })
            .map(|t| m.nb_element(t, GhostType::NotGhost) as u64)
            .sum::<u64>()];
        ok(
            m.communicator()
                .all_reduce(&mut n, akantu_comm::ReduceOp::Sum),
            "count",
        )?;
        (hi[0] - lo[0], hi[1] - lo[1], n[0] as usize)
    };
    let rho = {
        let i = ok(model.smm.material_index("bulk"), "bulk")?;
        ok(model.smm.material(i), "bulk")?.rho()
    };
    let theoretical_mass = l * h * h * rho;
    let frag_theo_mass = theoretical_mass / BF_NB_FRAGMENT as Real;
    let nb_element_per_fragment = nb_element / BF_NB_FRAGMENT;
    let big = |fm: &akantu_cohesive::FragmentManager| {
        fm.nb_elements_per_fragment
            .iter()
            .filter(|&&n| n > nb_element_per_fragment)
            .count()
    };
    let mut fm = ok(
        akantu_cohesive::FragmentManager::new(&model),
        "fragment manager",
    )?;
    ok(fm.compute_all_data(&mut model, 1.0), "computeAllData")?;
    // verticalInsertionLimit: only the facets whose nodes share their x
    {
        let mesh_h = model.smm.physics.base.mesh.clone();
        let m = ok(mesh_h.read(), "read")?;
        let fmesh = m.mesh_facets().ok_or("no facet mesh")?;
        let x = fmesh.nodes().as_slice().to_vec();
        let check = model.ext.inserter.check_facets_mut();
        for g in GhostType::ALL {
            for ty in fmesh.element_types(TypeFilter {
                spatial_dimension: Some(2),
                ghost_type: g,
                kind: Some(ElementKind::Regular),
            }) {
                let conn = ok(fmesh.connectivity(ty, g), "conn")?;
                let nn = conn.nb_component();
                let Ok(cf) = check.get_mut(ty, g) else {
                    continue;
                };
                for (f, row) in conn.as_slice().chunks_exact(nn).enumerate() {
                    if !cf.as_slice()[f] {
                        continue;
                    }
                    let first = x[row[0] as usize * 3];
                    if !row
                        .iter()
                        .all(|&n| are_float_equal(first, x[n as usize * 3]))
                    {
                        cf.as_mut_slice()[f] = false;
                    }
                }
            }
        }
    }
    {
        let (x, n) = {
            let m = ok(mesh.read(), "read")?;
            (m.nodes().as_slice().to_vec(), m.nb_nodes())
        };
        let mut u = ok(model.smm.displacement_mut(), "u")?;
        for i in 0..n {
            u.as_mut_slice()[3 * i] = x[3 * i] * 0.1;
        }
        drop(u);
        let mut v = ok(model.smm.velocity_mut(), "v")?;
        for i in 0..n {
            v.as_mut_slice()[3 * i] = x[3 * i];
        }
    }
    ok(model.assemble_internal_forces(), "updateResidual")?;
    let inserted = ok(model.check_cohesive_stress(), "checkCohesiveStress")?;
    if inserted == 0 {
        return Err("nothing inserted".into());
    }
    let el_size = l / BF_NB_FRAGMENT as Real;
    let mut lim = -l / 2.0 + el_size * 0.99;
    let mut small_inertia = [
        frag_theo_mass * (h * h + h * h) / 12.,
        frag_theo_mass * (el_size * el_size + h * h) / 12.,
        frag_theo_mass * (el_size * el_size + h * h) / 12.,
    ];
    small_inertia.sort_by(|a, b| b.total_cmp(a));
    let mut stats = Vec::new();
    for frag in 1..=BF_NB_FRAGMENT {
        ok(fm.compute_all_data(&mut model, 1.0), "computeAllData")?;
        let nb_big = big(&fm);
        let want_big = usize::from(frag < BF_NB_FRAGMENT);
        if nb_big != want_big {
            return Err(format!(
                "iteration {frag}: {nb_big} big fragments, want {want_big}"
            ));
        }
        let nbf = fm.nb_fragment();
        if nbf != frag {
            return Err(format!("iteration {frag}: {nbf} fragments, want {frag}"));
        }
        if frag < BF_NB_FRAGMENT {
            let mut total_mass = 0.0;
            let mut small = 0;
            for f in 0..nbf {
                let inertia = &fm.inertia_moments[3 * f..3 * f + 3];
                if are_float_equal(fm.mass[3 * f], frag_theo_mass) {
                    let bound = l * frag as Real / BF_NB_FRAGMENT as Real - l / 2.0;
                    if fm.mass_center[3 * f] > bound {
                        return Err(format!("iteration {frag}: fragment {f} centre is wrong"));
                    }
                    if !is_inertia_equal(inertia, &small_inertia) {
                        return Err(format!(
                            "iteration {frag}: fragment {f} inertia {inertia:?} vs {small_inertia:?}"
                        ));
                    }
                    small += 1;
                    total_mass += frag_theo_mass;
                } else {
                    let k = (BF_NB_FRAGMENT - frag + 1) as Real;
                    let (bm, bs) = (frag_theo_mass * k, el_size * k);
                    let mut bi = [
                        bm * (h * h + h * h) / 12.,
                        bm * (bs * bs + h * h) / 12.,
                        bm * (bs * bs + h * h) / 12.,
                    ];
                    bi.sort_by(|a, b| b.total_cmp(a));
                    if !is_inertia_equal(inertia, &bi) {
                        return Err(format!(
                            "iteration {frag}: big fragment {f} inertia {inertia:?} vs {bi:?}"
                        ));
                    }
                }
            }
            if small != nbf - 1 {
                return Err(format!("iteration {frag}: {small} small fragments"));
            }
            if !are_float_equal(total_mass, small as Real * frag_theo_mass) {
                return Err(format!("iteration {frag}: wrong small-fragment mass"));
            }
        }
        stats.push(FragStats {
            nb: nbf,
            mass: fm.mass.clone(),
            center: fm.mass_center.clone(),
            velocity: fm.velocity.clone(),
            inertia: fm.inertia_moments.clone(),
            nb_elements: fm.nb_elements_per_fragment.clone(),
        });
        ok(
            akantu_cohesive::par_scenarios::displace_elements(
                &mut model,
                lim,
                true,
                -(el_size * 2.0),
            ),
            "displace",
        )?;
        ok(model.assemble_internal_forces(), "updateResidual")?;
        lim += el_size;
    }
    // centres and velocities of the four fragments (the velocity is v_x = x)
    let last = stats.last().ok_or("no iteration")?;
    let initial = -l / 2.0 + el_size / 2.0;
    for k in 0..BF_NB_FRAGMENT {
        let theo = initial + el_size * k as Real;
        if !(0..BF_NB_FRAGMENT).any(|f| are_float_equal(last.center[3 * f], theo)) {
            return Err(format!("no fragment centre at {theo:e}"));
        }
        if !(0..BF_NB_FRAGMENT).any(|f| are_float_equal(last.velocity[3 * f], theo)) {
            return Err(format!("no fragment velocity {theo:e}"));
        }
    }
    Ok(stats)
}

/// `stats` against the serial run's: per iteration the same number of
/// fragments, element counts, and masses, centres, velocities and inertia
/// moments equal within the test's tolerance (`are_float_equal`, 1e-9),
/// fragments matched by their centre (the global numbering differs).
fn same_fragments(a: &[FragStats], b: &[FragStats]) -> std::result::Result<(), String> {
    if a.len() != b.len() {
        return Err("different number of iterations".into());
    }
    for (k, (x, y)) in a.iter().zip(b).enumerate() {
        if x.nb != y.nb {
            return Err(format!(
                "iteration {}: {} vs {} fragments",
                k + 1,
                x.nb,
                y.nb
            ));
        }
        let order = |s: &FragStats| {
            let mut o: Vec<usize> = (0..s.nb).collect();
            o.sort_by(|&i, &j| s.center[3 * i].total_cmp(&s.center[3 * j]));
            o
        };
        for (&i, &j) in order(x).iter().zip(&order(y)) {
            let pairs = [
                ("mass", &x.mass[3 * i..3 * i + 3], &y.mass[3 * j..3 * j + 3]),
                (
                    "centre",
                    &x.center[3 * i..3 * i + 3],
                    &y.center[3 * j..3 * j + 3],
                ),
                (
                    "velocity",
                    &x.velocity[3 * i..3 * i + 3],
                    &y.velocity[3 * j..3 * j + 3],
                ),
                (
                    "inertia",
                    &x.inertia[3 * i..3 * i + 3],
                    &y.inertia[3 * j..3 * j + 3],
                ),
            ];
            for (what, p, q) in pairs {
                if !p.iter().zip(q).all(|(u, v)| are_float_equal(*u, *v)) {
                    return Err(format!("iteration {}: {what} {p:?} vs serial {q:?}", k + 1));
                }
            }
            if x.nb_elements[i] != y.nb_elements[j] {
                return Err(format!("iteration {}: element counts differ", k + 1));
            }
        }
    }
    Ok(())
}

fn buildfragments(comm: Communicator) -> Outcome {
    let mine = buildfragments_run(buildfragments_mesh(Some(&comm))?)?;
    let serial = buildfragments_run(buildfragments_mesh(None)?)?;
    same_fragments(&mine, &serial)
}

/// TEST-236 (`test_cohesive_parallel_buildfragments`, ported by intent:
/// see [`buildfragments_run`]): the fragments of the bar broken layer by
/// layer at 1 (distributed over one rank), 2 and 4 ranks satisfy every
/// criterion of the upstream test and equal the serial run's within its
/// `global_tolerance` 1e-9.
#[test]
fn test_cohesive_parallel_buildfragments() {
    on_ranks(&[1, 2, 4], buildfragments);
}

// ---------------------------------------------------------------------------
// TEST-242: test_cohesive_parallel_intrinsic_implicit_insertion
// ---------------------------------------------------------------------------

/// Upstream's input file (copied in the parser fixtures).
const IMPLICIT_INSERTION_DAT: &str = include_str!(
    "../../akantu-parser/tests/fixtures/test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/input_file_iii.dat"
);

/// The partitions of the triangles recorded by the test in 2017
/// (`output_dir_verified/output_from_proc_0_out_of_{2,4}.out`, Scotch of
/// that era), and the per-rank counts of those files after the insertion
/// (`output_from_proc_<r>_out_of_<P>.out`): `(not-ghost triangles,
/// not-ghost cohesive, ghost cohesive, not-ghost segments, ghost
/// segments)`; the two `_segment_2` of the mesh get their parts from the
/// triangles' (the 2017 files: `[1, 0]` at 2 ranks, `[2, 0]` at 4).
const RECORDED_PARTS_2: [Idx; 16] = [1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0];
const RECORDED_PARTS_4: [Idx; 16] = [2, 0, 0, 2, 0, 2, 2, 0, 3, 1, 1, 3, 1, 3, 3, 1];
type Counts = (usize, usize, usize, usize, usize);
const RECORDED_COUNTS_2: [Counts; 2] = [(8, 1, 1, 1, 1), (8, 1, 1, 1, 1)];
const RECORDED_COUNTS_4: [Counts; 4] = [
    (4, 1, 1, 1, 1),
    (4, 0, 2, 0, 2),
    (4, 1, 1, 1, 1),
    (4, 0, 2, 0, 2),
];
/// Global nodes after the insertion in every recorded file.
const RECORDED_GLOBAL_NODES: usize = 16;

/// The test's model: `2d_basic_interface.msh` read on rank 0 (distributed
/// with `parts` when given; the reader creates the physical-name groups),
/// upstream's input file; the insertion surfaces `coh_1`, `coh_2` of its pre-v4
/// `mesh parameters [cohesive_surfaces = …]` given through the inserter
/// (upstream `addPhysicalSurface`); static intrinsic `init_full`.
fn implicit_insertion_run(
    comm: Option<&Communicator>,
    parts: Option<&[Idx]>,
) -> std::result::Result<(SharedMesh, SolidMechanicsModelCohesive), String> {
    let path = akantu_validation::fixtures_root().join(
        "mesh_io/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/2d_basic_interface/inputs/2d_basic_interface.msh",
    );
    let mut m = match comm {
        Some(c) => ok(Mesh::with_communicator(2, "mesh", c.clone()), "mesh")?,
        None => ok(Mesh::new(2, "mesh"), "mesh")?,
    };
    if comm.is_none_or(|c| c.rank() == 0) {
        ok(
            akantu_io::read_mesh(&mut m, &path, MeshIoType::Gmsh),
            "read",
        )?;
    }
    if let Some(c) = comm {
        let p = if c.rank() == 0 {
            parts.ok_or("parts")?.to_vec()
        } else {
            Vec::new()
        };
        let mut p = PrescribedPartition::new(p);
        let _ = ok(
            akantu_mesh::distribution::distribute_with(
                &mut m,
                Some(&mut p as &mut dyn MeshPartitioner),
                0,
            ),
            "distribute",
        )?;
    }
    let _ = ok(
        init_mesh_facets_with(
            &mut m,
            DistributedFacetsOptions {
                flip: FacetFlipMode::RemoteOrder,
            },
        ),
        "init_mesh_facets",
    )?;
    // upstream's driver calls `createGroupsFromMeshData("physical_names")`
    // here; the reader already created those groups (the call would clash)
    let mesh = SharedMesh::new(m);
    let parser = ok(
        Parser::parse_str(IMPLICIT_INSERTION_DAT, "input_file_iii.dat"),
        "parser",
    )?;
    let mut model = ok(
        SolidMechanicsModelCohesive::with_parser(
            mesh.clone(),
            None,
            akantu_cohesive::model::DEFAULT_ID,
            parser,
        ),
        "model",
    )?;
    model.ext.inserter.add_physical_surface("coh_1");
    model.ext.inserter.add_physical_surface("coh_2");
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::Static), false),
        "init_full",
    )?;
    Ok((mesh, model))
}

fn count_kind(m: &Mesh, kind: ElementKind, g: GhostType) -> usize {
    m.element_types(TypeFilter {
        spatial_dimension: Some(2),
        ghost_type: g,
        kind: Some(kind),
    })
    .map(|t| m.nb_element(t, g))
    .sum()
}

/// TEST-242 at one rank count: the inserted elements over the ranks are
/// the serial run's (by the coordinates of their facet), the counts of the
/// recorded 2017 partitions hold (not-ghost triangles, not-ghost and ghost
/// cohesive elements, not-ghost and ghost segments per rank, 16 global
/// nodes after the insertion), and
/// the global node ids are consistent (owned ids `0..16` once, one
/// position per id, every node that is not a pure ghost numbered).
fn implicit_insertion(comm: Communicator) -> Outcome {
    let np = comm.size();
    let (parts, counts): (&[Idx], &[Counts]) = match np {
        2 => (&RECORDED_PARTS_2, &RECORDED_COUNTS_2),
        4 => (&RECORDED_PARTS_4, &RECORDED_COUNTS_4),
        _ => return Err(format!("no recorded partition for {np} ranks")),
    };
    let (mesh, _model) = implicit_insertion_run(Some(&comm), Some(parts))?;
    let (smesh, _smodel) = implicit_insertion_run(None, None)?;
    let m = ok(mesh.read(), "read")?;
    let sm = ok(smesh.read(), "read")?;
    let serial_keys = par_coh::cohesive_keys(&sm, GhostType::NotGhost)?;
    if serial_keys.len() != 2 {
        return Err(format!(
            "{} cohesive elements in the serial run, want 2",
            serial_keys.len()
        ));
    }
    let mine = par_coh::cohesive_keys(&m, GhostType::NotGhost)?;
    let width = serial_keys[0].len();
    let all = ok(comm.all_gather_v(&mine.concat()), "gather")?;
    let mut keys: Vec<Vec<f64>> = all
        .concat()
        .chunks_exact(width)
        .map(<[f64]>::to_vec)
        .collect();
    keys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if keys != serial_keys {
        return Err(format!(
            "inserted set {keys:?} differs from the serial {serial_keys:?}"
        ));
    }
    let r = comm.rank() as usize;
    let have = (
        m.nb_element(ElementType::Triangle3, GhostType::NotGhost),
        count_kind(&m, ElementKind::Cohesive, GhostType::NotGhost),
        count_kind(&m, ElementKind::Cohesive, GhostType::Ghost),
        m.nb_element(ElementType::Segment2, GhostType::NotGhost),
        m.nb_element(ElementType::Segment2, GhostType::Ghost),
    );
    if have != counts[r] {
        return Err(format!(
            "rank {r}: (triangles, cohesive, ghost cohesive, segments, ghost segments) = \
             {have:?}, recorded {:?}",
            counts[r]
        ));
    }
    if m.nb_global_nodes() != RECORDED_GLOBAL_NODES || sm.nb_nodes() != RECORDED_GLOBAL_NODES {
        return Err(format!(
            "{} global nodes ({} in the serial run), recorded {RECORDED_GLOBAL_NODES}",
            m.nb_global_nodes(),
            sm.nb_nodes()
        ));
    }
    let mut owned = Vec::new();
    let mut ids_coords = Vec::new();
    for i in 0..m.nb_nodes() {
        let f = m.node_flag(i as Idx);
        if f.is_pure_ghost() {
            continue;
        }
        let g = m.node_global_id(i as Idx);
        if g < 0 {
            return Err(format!("node {i} has no global id"));
        }
        if f.is_local_or_master() {
            owned.push(g);
        }
        ids_coords.push(g as f64);
        ids_coords.extend_from_slice(&m.nodes().as_slice()[2 * i..2 * i + 2]);
    }
    let mut all_owned = ok(comm.all_gather_v(&owned), "gather")?.concat();
    all_owned.sort_unstable();
    if all_owned != (0..RECORDED_GLOBAL_NODES as i64).collect::<Vec<_>>() {
        return Err(format!("owned global ids {all_owned:?}"));
    }
    let all_ic = ok(comm.all_gather_v(&ids_coords), "gather")?.concat();
    let mut by_id: std::collections::BTreeMap<i64, Vec<f64>> = Default::default();
    for row in all_ic.as_chunks::<3>().0 {
        let prev = by_id
            .entry(row[0] as i64)
            .or_insert_with(|| row[1..].to_vec());
        if *prev != row[1..] {
            return Err(format!("global id {} has two positions", row[0]));
        }
    }
    Ok(())
}

/// TEST-242 (`test_cohesive_parallel_intrinsic_implicit_insertion`, ported
/// by intent: the 2017 `.out` files are not compared byte for byte, D-085):
/// the intrinsic insertion along `coh_1` / `coh_2` during `init_full` at 2
/// and 4 ranks with the recorded partitions (see [`implicit_insertion`]).
#[test]
fn test_cohesive_parallel_intrinsic_implicit_insertion() {
    on_ranks(&[2, 4], implicit_insertion);
}
