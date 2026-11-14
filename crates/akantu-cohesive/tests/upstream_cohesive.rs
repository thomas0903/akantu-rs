// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_cohesive/{test_cohesive.cc, test_cohesive_fixture.hh, material_0.dat,
// material_0_finite_def.dat, material_1.dat, data/cohesive_*.geo}
// (gtest suite test_solid_mechanics_model_cohesive).
//! Port of `TestSMMCFixture.{ExtrinsicModeI, ExtrinsicModeIFiniteDef,
//! ExtrinsicModeII, IntrinsicModeI, IntrinsicModeII}` (TEST-115) for every
//! element-type triple of upstream's `coh_types` (9; `ExtrinsicModeIFiniteDef`
//! skips 1-D as upstream): 44 cases, each a test of its own, run by the port
//! with **upstream's assertions** — `checkInsertion` (number of cohesive
//! elements = size of the `insertion` group) and `checkDissipated`
//! (`|surface·G_c − getEnergy("dissipated")| ≤ 0.5`). Meshes: the frozen
//! Gmsh meshes of the upstream `add_mesh` targets (D-009); material files
//! verbatim (`common::MATERIAL_0/1`, `MATERIAL_0_FINITE_DEF` below);
//! `Δt = 0.01·getStableTimeStep()`; the loading of `testModeI` /
//! `testModeII` (initial strain `(1 − 1e-5)·ε`, increments `2e-2·ε` / `5e-3·ε`
//! through `StrainIncrement` on `loading` and `fixed`, `sides` flagged in
//! y (and z) for mode II, `checkCohesiveStress` before each extrinsic step).
//!
//! The same runs are replayed bitwise against the oracle by
//! `cohesive_fixtures.rs::fixture_cases_bitwise` (whose 3-D cases are
//! skipped in unoptimised builds); this file checks upstream's criteria on
//! the port's own runs. Cost: the ten 3-D cases
//! (`tetrahedron_4`, `tetrahedron_10` of the five tests) take 31 s of the
//! binary's 35 s in a debug build (M4 Pro, 4 threads; 37 s on the x86_64
//! gate sandbox), so they are `#[ignore]`d in unoptimised builds
//! (reported as ignored, not as passed) and run in the release tier
//! (`cargo test --release -p akantu-cohesive`; in an unoptimised build
//! `-- --ignored`), like `akantu-bench`'s replays; the 34 1-D / 2-D cases
//! run in every build.
//!
//! The suite's `PARALLEL` / `PARALLEL_LEVEL 1 2` registration:
//! every case also runs at **2 ranks** of the local world
//! (module `two_ranks`, `Fixture::create_on`: mesh read on rank 0 and
//! distributed, upstream's all-reduced surface, group size and cohesive
//! count, the global dissipated energy) with the same two assertions on
//! every rank; the 3-D cases again in the release tier only. The four 1-D
//! cases return at once there, as upstream's (`getNbProc() > 1 and dim ==
//! 1`): upstream refuses a distributed 1-D facet mesh and so does the port
//! (UD-1141, category C; `two_ranks_1d_facet_mesh_refused`).
//! The 2-D cases at 2 ranks are also compared with the oracle's
//! 2-rank runs (`test_cohesive_parallel.rs`).

mod common;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::{Element, ElementType, GhostType, Real};
use common::{Fixture, FlagOnly, MATERIAL_0, MATERIAL_1, real_param};

/// `material_0_finite_def.dat` (verbatim, `finite_deformation = 1`).
const MATERIAL_0_FINITE_DEF: &str = "model solid_mechanics_model_cohesive [
  cohesive_inserter [
    cohesive_surfaces = [insertion]
  ]

  material elastic [
    name = body
    rho = 1e3   # density
    E   = 1e9   # young's modulus
    nu  = 0.2 # poisson's ratio
    finite_deformation = 1
  ]

  material cohesive_linear [
    name = insertion
    beta = 1
    G_c = 10
    sigma_c = 1e6
  ]
]
";

#[derive(Clone, Copy, Debug)]
enum Test {
    ExtrinsicModeI,
    ExtrinsicModeIFiniteDef,
    ExtrinsicModeII,
    IntrinsicModeI,
    IntrinsicModeII,
}

/// Upstream `testModeII()` up to the steps; returns the strain.
fn mode_ii_strain(f: &mut Fixture) -> Vec<Real> {
    let body = f.model.smm.material_index("body").unwrap();
    let speed = {
        let law = f.model.smm.material(body).unwrap();
        // "the slowest speed if exists"
        law.shear_wave_speed(Element::NULL)
            .or_else(|_| law.push_wave_speed(Element::NULL))
            .unwrap()
    };
    let dir = if f.dim == 1 { 0 } else { 1 };
    let length = {
        let m = f.mesh.read().unwrap();
        m.upper_bounds()[dir] - m.lower_bounds()[dir]
    };
    let dt = f.model.smm.time_step(None).unwrap();
    f.nb_steps = (2. * length / 2. / speed / dt) as usize;
    if f.dim > 1 {
        f.model
            .smm
            .apply_dirichlet_bc(&FlagOnly(1), "sides")
            .unwrap();
    }
    if f.dim > 2 {
        f.model
            .smm
            .apply_dirichlet_bc(&FlagOnly(2), "sides")
            .unwrap();
    }
    let sigma_c = real_param(&f.model, "insertion", "sigma_c");
    let beta = real_param(&f.model, "insertion", "beta");
    let e = real_param(&f.model, "body", "E");
    let nu = real_param(&f.model, "body", "nu");
    // column-major: (0, 1) is entry `d`
    let mut strain: Vec<Real> = match f.dim {
        1 => vec![1.],
        2 => vec![0., 0., 1. + nu, 0.],
        _ => {
            let mut s = vec![0.; 9];
            s[3] = 1. + nu;
            s
        }
    };
    for s in &mut strain {
        *s *= 2. * beta * beta * sigma_c / e;
    }
    strain
}

fn material_of(test: Test) -> (&'static str, bool) {
    match test {
        Test::ExtrinsicModeI | Test::ExtrinsicModeII => (MATERIAL_0, true),
        Test::ExtrinsicModeIFiniteDef => (MATERIAL_0_FINITE_DEF, true),
        Test::IntrinsicModeI | Test::IntrinsicModeII => (MATERIAL_1, false),
    }
}

fn run(test: Test, mesh: &str, dim: usize, cohesive_type: ElementType) {
    let (dat, extrinsic) = material_of(test);
    let mut f = Fixture::create(mesh, dim, cohesive_type, dat, extrinsic);
    check(test, mesh, &mut f, Fixture::nb_cohesive, |_, _, _| {});
}

/// `PARALLEL_LEVEL 2`: the case at 2 ranks of the local world, upstream's
/// assertions on every rank (the counts and the energy are global). The
/// 1-D cases return at once as upstream's (`test_cohesive.cc:28-31, 66-69,
/// 85-88, 105-108`: `if (this->mesh->getCommunicator().getNbProc() > 1 and
/// this->dim == 1) { SUCCEED(); return; }`), which skips them because
/// upstream refuses a distributed 1-D facet mesh (UD-1141; the port's
/// refusal is asserted by `two_ranks_1d_facet_mesh_refused`).
fn run_two_ranks(test: Test, mesh: &str, dim: usize, cohesive_type: ElementType) {
    if dim == 1 {
        eprintln!(
            "{test:?} {mesh} at 2 ranks: SUCCEED() without running, as upstream \
             (getNbProc() > 1 and dim == 1; UD-1141)"
        );
        return;
    }
    let (dat, extrinsic) = material_of(test);
    let case = oracle_case_name(test, mesh);
    let out = akantu_comm::run_local(2, std::time::Duration::from_secs(300), |c| {
        let mut f = Fixture::create_on(&c, mesh, dim, cohesive_type, dat, extrinsic);
        let mut steps = Vec::new();
        let n = check(
            test,
            mesh,
            &mut f,
            Fixture::nb_cohesive_global,
            |_, model, inserted| {
                steps.push(step_record(model, cohesive_type, inserted));
            },
        );
        let compared = compare_with_oracle(&case, &c, &f, &steps)?;
        let compared = compared + compare_boundary_with_oracle(&case, &c, &f)?;
        Ok::<_, String>((n, compared))
    });
    for (r, o) in out.into_iter().enumerate() {
        let (n, compared) = o.unwrap_or_else(|e| panic!("{test:?} {mesh} 2 ranks, rank {r}: {e}"));
        if r == 0 {
            eprintln!(
                "{test:?} {mesh} at 2 ranks: {n} cohesive elements in all; oracle \
                 parallel_test_cohesive/{case}/np2: {compared} values bitwise on rank 0"
            );
        }
    }
}

/// Oracle case of `test` on `mesh` (`oracle/probes/parallel_test_cohesive`):
/// `<test>_<types>`.
fn oracle_case_name(test: Test, mesh: &str) -> String {
    let prefix = match test {
        Test::ExtrinsicModeI => "extrinsic_mode_i",
        Test::ExtrinsicModeIFiniteDef => "extrinsic_mode_i_finite_def",
        Test::ExtrinsicModeII => "extrinsic_mode_ii",
        Test::IntrinsicModeI => "intrinsic_mode_i",
        Test::IntrinsicModeII => "intrinsic_mode_ii",
    };
    // "_cohesive_2d_4_triangle_3.msh" -> "triangle_3"
    let types = mesh
        .trim_end_matches(".msh")
        .splitn(5, '_')
        .nth(4)
        .unwrap_or_default();
    format!("{prefix}_{types}")
}

/// What the probe records per step on each rank.
struct StepRecord {
    inserted: usize,
    nb_cohesive: usize,
    /// kinetic, potential, dissipated, reversible, cohesive contact (global)
    energies: [Real; 5],
}

const ENERGIES: [(&str, &str); 5] = [
    ("kinetic", "kinetic"),
    ("potential", "potential"),
    ("dissipated", "dissipated"),
    ("reversible", "reversible"),
    ("cohesive contact", "cohesive_contact"),
];

fn step_record(
    model: &mut SolidMechanicsModelCohesive,
    cohesive_type: ElementType,
    inserted: usize,
) -> StepRecord {
    let nb_cohesive = model
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nb_element(cohesive_type, GhostType::NotGhost);
    let mut energies = [0.; 5];
    for (e, (id, _)) in energies.iter_mut().zip(ENERGIES) {
        *e = model.energy(id).unwrap();
    }
    StepRecord {
        inserted,
        nb_cohesive,
        energies,
    }
}

fn bits_equal(what: &str, got: &[Real], want: &[f64]) -> Result<usize, String> {
    if got.len() != want.len() {
        return Err(format!(
            "{what}: {} values, oracle {}",
            got.len(),
            want.len()
        ));
    }
    if let Some((i, (a, b))) = got
        .iter()
        .zip(want)
        .enumerate()
        .find(|(_, (a, b))| a.to_bits() != b.to_bits())
    {
        return Err(format!("{what}[{i}]: {a:e} vs oracle {b:e}"));
    }
    Ok(got.len())
}

/// The 2-rank run of this rank against the oracle's
/// 2-rank run with the same partition (`fixtures/parallel_test_cohesive/
/// <case>/np2/rank<r>`, oracle/probes/parallel_test_cohesive): the RCB
/// partition (rank 0), the time steps and surface, per step the inserted
/// count, the local cohesive count and the five global energies, after the
/// run every connectivity in node global ids, the node global ids and the
/// displacement and velocity of every node (pure ghosts included) by global
/// id, all exact / bitwise. Returns the number of values compared. The
/// pure-ghost nodes are the regression of UD-1250 (the
/// element groups' node lists, hence the BC writes, follow upstream's
/// rebuild instant; in `extrinsic_mode_{i,ii}_tetrahedron_4` two pure-ghost
/// nodes per rank held 35 / 60 loading increments instead of upstream's
/// 39 / 83 before the fix).
fn compare_with_oracle(
    case: &str,
    comm: &akantu_comm::Communicator,
    f: &Fixture,
    steps: &[StepRecord],
) -> Result<usize, String> {
    use akantu_validation::Fixture as Golden;
    let id = format!(
        "parallel_test_cohesive/{case}/np{}/rank{}",
        comm.size(),
        comm.rank()
    );
    let g = Golden::open_by_id(&id).map_err(|e| format!("{id}: {e}"))?;
    g.require_golden().map_err(|e| format!("{id}: {e}"))?;
    g.verify_checksums().map_err(|e| format!("{id}: {e}"))?;
    let f64s = |n: &str| {
        g.load_f64(n)
            .map(|x| x.data)
            .map_err(|e| format!("{id}: {n}: {e}"))
    };
    let i64s = |n: &str| {
        g.load_i64(n)
            .map(|x| x.data)
            .map_err(|e| format!("{id}: {n}: {e}"))
    };
    let param = |n: &str| g.manifest().parameters[n].clone();
    let mut n = 0;
    if comm.rank() == 0 {
        let want = i64s("partition/rcb")?;
        let got: Vec<i64> = f.partition.iter().map(|&p| i64::from(p)).collect();
        if got != want {
            return Err(format!("{id}: partition differs from the oracle's"));
        }
        n += got.len();
    }
    let dt = f.model.smm.time_step(None).map_err(|e| e.to_string())?;
    n += bits_equal("time_step", &[f.stable_time_step, dt], &f64s("time_step")?)?;
    n += bits_equal("surface", &[f.surface], &f64s("surface")?)?;
    for (key, got) in [
        ("nb_steps", f.nb_steps),
        ("group_size", f.group_size),
        ("nb_cohesive_global", f.nb_cohesive_global()),
    ] {
        if param(key).as_u64() != Some(got as u64) {
            return Err(format!("{id}: {key} {got} vs oracle {}", param(key)));
        }
        n += 1;
    }
    let ints = |v: Vec<usize>| v.into_iter().map(|x| x as i64).collect::<Vec<i64>>();
    for (name, got) in [
        (
            "steps/inserted",
            ints(steps.iter().map(|s| s.inserted).collect()),
        ),
        (
            "steps/nb_cohesive",
            ints(steps.iter().map(|s| s.nb_cohesive).collect()),
        ),
    ] {
        let want = i64s(name)?;
        if got != want {
            let i = got.iter().zip(&want).position(|(a, b)| a != b);
            return Err(format!(
                "{id}: {name} differs (lengths {} / {}, first at {i:?})",
                got.len(),
                want.len()
            ));
        }
        n += got.len();
    }
    for (k, (_, key)) in ENERGIES.iter().enumerate() {
        let got: Vec<Real> = steps.iter().map(|s| s.energies[k]).collect();
        n += bits_equal(
            &format!("{id}: energy {key}"),
            &got,
            &f64s(&format!("steps/energy/{key}"))?,
        )?;
    }
    let mesh = f.mesh.read().map_err(|e| e.to_string())?;
    let dim = f.dim;
    // connectivities in node global ids, every block of the mesh dimension
    // and of the cohesive kind (not ghost, ghost)
    let blocks: Vec<String> = param("mesh_blocks")
        .as_array()
        .ok_or_else(|| format!("{id}: mesh_blocks"))?
        .iter()
        .map(|b| b.as_str().unwrap_or_default().to_owned())
        .collect();
    let mut port_blocks = Vec::new();
    for ghost in GhostType::ALL {
        let types: Vec<ElementType> = mesh
            .element_types(akantu_core::TypeFilter::new().ghost(ghost))
            .collect();
        for ty in types {
            if ty.spatial_dimension() != dim && ty.kind() != akantu_core::ElementKind::Cohesive {
                continue;
            }
            let gname = if ghost == GhostType::Ghost {
                "ghost"
            } else {
                "not_ghost"
            };
            let key = format!("{gname}/{ty}");
            let conn = mesh.connectivity(ty, ghost).map_err(|e| e.to_string())?;
            let got: Vec<i64> = conn
                .as_slice()
                .iter()
                .map(|&x| mesh.node_global_id(x))
                .collect();
            if !blocks.contains(&key) {
                if got.is_empty() {
                    continue;
                }
                return Err(format!("{id}: block {key} exists in the port only"));
            }
            let want = i64s(&format!("mesh/{key}/connectivity_gids"))?;
            if got != want {
                return Err(format!(
                    "{id}: connectivity of {key} differs (in node global ids)"
                ));
            }
            n += got.len();
            port_blocks.push(key);
        }
    }
    for b in &blocks {
        if !port_blocks.contains(b) && !i64s(&format!("mesh/{b}/connectivity_gids"))?.is_empty() {
            return Err(format!("{id}: block {b} exists in the oracle only"));
        }
    }
    // nodes by global id: the same set, displacement and velocity bitwise
    let want_gids = i64s("nodes/global_id")?;
    let got_gids: Vec<i64> = (0..mesh.nb_nodes())
        .map(|i| mesh.node_global_id(i as akantu_core::Idx))
        .collect();
    let mut a = got_gids.clone();
    let mut b = want_gids.clone();
    a.sort_unstable();
    b.sort_unstable();
    if a != b {
        return Err(format!("{id}: node global ids differ from the oracle's"));
    }
    drop(mesh);
    let at: std::collections::HashMap<i64, usize> =
        want_gids.iter().enumerate().map(|(i, &g)| (g, i)).collect();
    for (name, got) in [
        (
            "displacement",
            f.model
                .smm
                .displacement()
                .map_err(|e| e.to_string())?
                .as_slice()
                .to_vec(),
        ),
        (
            "velocity",
            f.model
                .smm
                .velocity()
                .map_err(|e| e.to_string())?
                .as_slice()
                .to_vec(),
        ),
    ] {
        let want = f64s(name)?;
        let mut mapped = vec![0.; want.len()];
        for (i, &g) in got_gids.iter().enumerate() {
            let j = at[&g];
            mapped[j * dim..(j + 1) * dim].copy_from_slice(&got[i * dim..(i + 1) * dim]);
        }
        let mut differing = Vec::new();
        for (j, &g) in want_gids.iter().enumerate() {
            let (a, b) = (
                &mapped[j * dim..(j + 1) * dim],
                &want[j * dim..(j + 1) * dim],
            );
            match bits_equal(&format!("{id}: {name} of node gid {g}"), a, b) {
                Ok(k) => n += k,
                Err(e) => differing.push(e),
            }
        }
        if !differing.is_empty() {
            return Err(format!(
                "{id}: {name} (by node global id) differs at {} node(s): {}",
                differing.len(),
                differing.join("; ")
            ));
        }
    }
    Ok(n)
}

/// The boundary record of the 2-rank
/// oracle run (`fixtures/parallel_test_cohesive_boundary/<case>/np2/
/// rank<r>`, probe version 2, `PTC_RECORD=boundary`), which version 1 did
/// not record: after the run, the connectivities of the boundary blocks
/// (the regular types of dimension `dim − 1`: the `loading`, `fixed` and
/// `sides` facets, whose doubled nodes the insertion rewrites;
/// `sync_boundary_connectivity`) in node global ids, and every element
/// group's node list (node global ids, in the group's order: the list
/// UD-1250 was about) and element count per block — exactly.
fn compare_boundary_with_oracle(
    case: &str,
    comm: &akantu_comm::Communicator,
    f: &Fixture,
) -> Result<usize, String> {
    use akantu_validation::Fixture as Golden;
    let id = format!(
        "parallel_test_cohesive_boundary/{case}/np{}/rank{}",
        comm.size(),
        comm.rank()
    );
    let g = Golden::open_by_id(&id).map_err(|e| format!("{id}: {e}"))?;
    g.require_golden().map_err(|e| format!("{id}: {e}"))?;
    g.verify_checksums().map_err(|e| format!("{id}: {e}"))?;
    let i64s = |n: &str| {
        g.load_i64(n)
            .map(|x| x.data)
            .map_err(|e| format!("{id}: {n}: {e}"))
    };
    let strings = |v: &serde_json::Value| -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .map(|b| b.as_str().unwrap_or_default().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    let params = &g.manifest().parameters;
    let mesh = f.mesh.read().map_err(|e| e.to_string())?;
    let dim = f.dim;
    let gname = |ghost: GhostType| {
        if ghost == GhostType::Ghost {
            "ghost"
        } else {
            "not_ghost"
        }
    };
    let mut n = 0;
    // boundary blocks
    let blocks = strings(&params["boundary_blocks"]);
    let mut port_blocks = Vec::new();
    for ghost in GhostType::ALL {
        let types: Vec<ElementType> = mesh
            .element_types(akantu_core::TypeFilter::new().ghost(ghost))
            .filter(|t| {
                t.spatial_dimension() + 1 == dim && t.kind() == akantu_core::ElementKind::Regular
            })
            .collect();
        for ty in types {
            let key = format!("{}/{ty}", gname(ghost));
            let conn = mesh.connectivity(ty, ghost).map_err(|e| e.to_string())?;
            let got: Vec<i64> = conn
                .as_slice()
                .iter()
                .map(|&x| mesh.node_global_id(x))
                .collect();
            if !blocks.contains(&key) {
                if got.is_empty() {
                    continue;
                }
                return Err(format!(
                    "{id}: boundary block {key} exists in the port only"
                ));
            }
            let want = i64s(&format!("boundary/{key}/connectivity_gids"))?;
            if got != want {
                return Err(format!(
                    "{id}: boundary block {key} differs from the oracle (node global ids)"
                ));
            }
            n += got.len();
            port_blocks.push(key);
        }
    }
    for b in &blocks {
        if !port_blocks.contains(b) && !i64s(&format!("boundary/{b}/connectivity_gids"))?.is_empty()
        {
            return Err(format!(
                "{id}: boundary block {b} exists in the oracle only"
            ));
        }
    }
    // element groups: node lists and element counts
    for name in strings(&params["groups"]) {
        let group = mesh
            .groups()
            .element_group(&name)
            .map_err(|e| format!("{id}: group {name}: {e}"))?;
        let got: Vec<i64> = group
            .nodes()
            .as_slice()
            .iter()
            .map(|&x| mesh.node_global_id(x))
            .collect();
        let want = i64s(&format!("groups/{name}/node_gids"))?;
        if got != want {
            let mut a = got.clone();
            let mut b = want.clone();
            a.sort_unstable();
            b.sort_unstable();
            return Err(format!(
                "{id}: node list of group {name} differs from the oracle ({} vs {} nodes; as \
                 sets {})",
                got.len(),
                want.len(),
                if a == b {
                    "equal: the order differs"
                } else {
                    "different"
                }
            ));
        }
        n += got.len();
        let gblocks = strings(&params["groups_blocks"][name.as_str()]);
        let counts = i64s(&format!("groups/{name}/nb_elements"))?;
        if gblocks.len() != counts.len() {
            return Err(format!("{id}: group {name}: malformed record"));
        }
        for (key, &want) in gblocks.iter().zip(&counts) {
            let (gh, ty) = key
                .split_once('/')
                .ok_or_else(|| format!("{id}: group block {key}"))?;
            let ghost = if gh == "ghost" {
                GhostType::Ghost
            } else {
                GhostType::NotGhost
            };
            let ty: ElementType = ty
                .parse()
                .map_err(|e| format!("{id}: group {name}: type {ty}: {e}"))?;
            let got = group.elements_of(ty, ghost).len() as i64;
            if got != want {
                return Err(format!(
                    "{id}: group {name} holds {got} elements of {key}, the oracle {want}"
                ));
            }
            n += 1;
        }
        // no block of the port's group is missing from the record
        for ghost in GhostType::ALL {
            for ty in group.group().element_types(ghost) {
                let key = format!("{}/{ty}", gname(ghost));
                if !gblocks.contains(&key) && !group.elements_of(ty, ghost).is_empty() {
                    return Err(format!("{id}: group {name}: block {key} in the port only"));
                }
            }
        }
    }
    Ok(n)
}

/// The loading of `testModeI` / `testModeII`, then `checkInsertion` (count
/// from `count`) and `checkDissipated(G_c)`; returns the count. `observe`
/// sees every step after its `solveStep` (step, model, inserted count).
fn check(
    test: Test,
    mesh: &str,
    f: &mut Fixture,
    count: impl Fn(&Fixture) -> usize,
    observe: impl FnMut(usize, &mut SolidMechanicsModelCohesive, usize),
) -> usize {
    let (strain, init_factor, inc_factor) = match test {
        Test::ExtrinsicModeI | Test::ExtrinsicModeIFiniteDef | Test::IntrinsicModeI => {
            (f.mode_i_strain(), 1. - 1e-5, 2e-2)
        }
        Test::ExtrinsicModeII | Test::IntrinsicModeII => (mode_ii_strain(f), 1. - 1e-5, 5e-3),
    };
    let init: Vec<Real> = strain.iter().map(|s| s * init_factor).collect();
    f.set_initial_condition(&init);
    let inc: Vec<Real> = strain.iter().map(|s| s * inc_factor).collect();
    f.steps(&inc, observe);
    // checkInsertion
    let nb_cohesive = count(f);
    assert_eq!(
        nb_cohesive, f.group_size,
        "{test:?} {mesh}: cohesive elements"
    );
    // checkDissipated(G_c)
    let g_c = real_param(&f.model, "insertion", "G_c");
    let edis = f.model.energy("dissipated").unwrap();
    eprintln!(
        "{test:?} {mesh}: {} steps, {nb_cohesive} cohesive elements, surface·G_c {} vs dissipated {edis}",
        f.nb_steps,
        f.surface * g_c
    );
    assert!(
        (f.surface * g_c - edis).abs() <= 5e-1,
        "{test:?} {mesh}: dissipated {edis} vs surface·G_c {}",
        f.surface * g_c
    );
    nb_cohesive
}

macro_rules! cases {
    ($run:ident; $($name:ident: $test:ident, $mesh:literal, $dim:literal, $ct:ident;)*) => {
        $(
            #[test]
            fn $name() {
                $run(Test::$test, $mesh, $dim, ElementType::$ct);
            }
        )*
    };
}

/// The 3-D cases: release tier only (module documentation, cost).
macro_rules! release_cases {
    ($run:ident; $($name:ident: $test:ident, $mesh:literal, $dim:literal, $ct:ident;)*) => {
        $(
            #[test]
            #[cfg_attr(
                debug_assertions,
                ignore = "release tier: cargo test --release -p akantu-cohesive"
            )]
            fn $name() {
                $run(Test::$test, $mesh, $dim, ElementType::$ct);
            }
        )*
    };
}

macro_rules! types_2d_3d {
    ($run:ident; $test:ident; $($extra:tt)*) => {
        cases! {
            $run;
            $($extra)*
            triangle_3: $test, "_cohesive_2d_4_triangle_3.msh", 2, Cohesive2d4;
            quadrangle_4: $test, "_cohesive_2d_4_quadrangle_4.msh", 2, Cohesive2d4;
            triangle_3_quadrangle_4: $test, "_cohesive_2d_4_triangle_3_quadrangle_4.msh", 2, Cohesive2d4;
            triangle_6: $test, "_cohesive_2d_6_triangle_6.msh", 2, Cohesive2d6;
            quadrangle_8: $test, "_cohesive_2d_6_quadrangle_8.msh", 2, Cohesive2d6;
            triangle_6_quadrangle_8: $test, "_cohesive_2d_6_triangle_6_quadrangle_8.msh", 2, Cohesive2d6;
        }
        release_cases! {
            $run;
            tetrahedron_4: $test, "_cohesive_3d_6_tetrahedron_4.msh", 3, Cohesive3d6;
            tetrahedron_10: $test, "_cohesive_3d_12_tetrahedron_10.msh", 3, Cohesive3d12;
        }
    };
}

macro_rules! suite {
    ($run:ident) => {
        mod extrinsic_mode_i {
            use super::*;
            types_2d_3d!($run; ExtrinsicModeI; segment_2: ExtrinsicModeI, "_cohesive_1d_2_segment_2.msh", 1, Cohesive1d2;);
        }
        mod extrinsic_mode_i_finite_def {
            use super::*;
            types_2d_3d!($run; ExtrinsicModeIFiniteDef;);
        }
        mod extrinsic_mode_ii {
            use super::*;
            types_2d_3d!($run; ExtrinsicModeII; segment_2: ExtrinsicModeII, "_cohesive_1d_2_segment_2.msh", 1, Cohesive1d2;);
        }
        mod intrinsic_mode_i {
            use super::*;
            types_2d_3d!($run; IntrinsicModeI; segment_2: IntrinsicModeI, "_cohesive_1d_2_segment_2.msh", 1, Cohesive1d2;);
        }
        mod intrinsic_mode_ii {
            use super::*;
            types_2d_3d!($run; IntrinsicModeII; segment_2: IntrinsicModeII, "_cohesive_1d_2_segment_2.msh", 1, Cohesive1d2;);
        }
    };
}

suite!(run);

/// `PARALLEL_LEVEL 2`: every case at 2 ranks (the 1-D
/// ones return at once, as upstream's).
mod two_ranks {
    use super::*;
    suite!(run_two_ranks);
}

/// UD-1250: upstream rebuilds the element groups'
/// node lists of a distributed mesh only when an insertion adds an owned node
/// somewhere (`updateGlobalIDs(…) > 0` → `CohesiveNewNodesEvent` →
/// `ElementGroup::onNodesAdded`). `extrinsic_mode_i_tetrahedron_4` at 2 ranks:
/// the first insertions (steps 34 and 36) double no node, the one of step 38
/// adds one node per rank. The pure-ghost nodes that the distribution put
/// into `loading` (rank 0: gids 44, 45) and `fixed` (rank 1: gids 65, 66)
/// stay members through steps 34–37 and leave at step 38, as upstream's
/// (`fixtures/parallel_test_cohesive/extrinsic_mode_i_tetrahedron_4`: 39
/// loading increments). Before the fix they left at step 34.
#[test]
fn two_ranks_ud1250_group_membership_follows_owned_node_insertions() {
    let out = akantu_comm::run_local(2, std::time::Duration::from_secs(300), |c| {
        let mut f = Fixture::create_on(
            &c,
            "_cohesive_3d_6_tetrahedron_4.msh",
            3,
            ElementType::Cohesive3d6,
            MATERIAL_0,
            true,
        );
        let strain = f.mode_i_strain();
        let init: Vec<Real> = strain.iter().map(|s| s * (1. - 1e-5)).collect();
        f.set_initial_condition(&init);
        let inc: Vec<Real> = strain.iter().map(|s| s * 2e-2).collect();
        f.nb_steps = 40;
        let (group, gids): (&str, [i64; 2]) = if c.rank() == 0 {
            ("loading", [44, 45])
        } else {
            ("fixed", [65, 66])
        };
        // per step: (inserted, nodes on this rank, members among `gids`,
        // members that are pure ghosts)
        let mut trace = Vec::new();
        f.steps(&inc, |_, model, inserted| {
            let m = model.smm.physics.base.mesh.read().unwrap();
            let members: Vec<akantu_core::Idx> = m
                .groups()
                .element_group(group)
                .map(|g| g.nodes().as_slice().to_vec())
                .unwrap_or_default();
            let held: Vec<(i64, bool)> = members
                .iter()
                .map(|&n| (m.node_global_id(n), m.node_flag(n).is_pure_ghost()))
                .filter(|(g, _)| gids.contains(g))
                .collect();
            trace.push((
                inserted,
                m.nb_nodes(),
                held.len(),
                held.iter().filter(|(_, pg)| *pg).count(),
            ));
        });
        let fail = |what: String| Err(format!("rank {}: {what}; trace {trace:?}", c.rank()));
        for (s, &(inserted, _, held, pure)) in trace.iter().enumerate() {
            let want = if s < 38 { 2 } else { 0 };
            if held != want || pure != want {
                return fail(format!(
                    "step {s}: {held} of gids {gids:?} in `{group}` ({pure} pure ghosts), \
                     expected {want}"
                ));
            }
            if (s == 34 && inserted == 0) || (s < 34 && inserted != 0) {
                return fail(format!("step {s}: {inserted} inserted"));
            }
        }
        // the insertions before step 38 add no node on this rank; step 38 does
        if trace[37].1 != trace[0].1 || trace[38].1 <= trace[37].1 {
            return fail("node counts".into());
        }
        Ok::<(), String>(())
    });
    for (r, o) in out.into_iter().enumerate() {
        o.unwrap_or_else(|e| panic!("rank {r}: {e}"));
    }
}

/// UD-1141: the 1-D mesh of the suite distributed over 2
/// ranks of the local world: the facet mesh and the intrinsic and extrinsic
/// cohesive models are refused on both ranks with the same error, the mesh
/// and the communicator stay usable; on one rank they are built
/// ([`common::distributed_1d_refusal`]; the serial 1-D cases above are
/// unaffected).
#[test]
fn two_ranks_1d_facet_mesh_refused() {
    for np in [1, 2] {
        let out = akantu_comm::run_local(np, std::time::Duration::from_secs(120), |c| {
            common::distributed_1d_refusal(&c)
        });
        for (r, o) in out.into_iter().enumerate() {
            let summary = o.unwrap_or_else(|e| panic!("{np} rank(s), rank {r}: {e}"));
            if r == 0 {
                eprintln!("{summary}");
            }
        }
    }
}
