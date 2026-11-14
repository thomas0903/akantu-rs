// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Scenarios of
// node removal (SMM-015, UD-209): under a live
// SolidMechanicsModel (upstream solid_mechanics_model.cc:752-783,
// dof_manager.cc:901-902, mesh_utils.cc purifyMesh, mesh.cc eraseElements),
// shared by the local-world tests (`node_removal.rs`) and the real-MPI
// binary (`mpi_smm.rs`).

//! Criterion (no upstream test exercises a removal under a live model
//! outside IGFEM): **a removal between two solves equals a fresh model
//! built on the mesh after the removal, with the state transferred**. The
//! live model `A` runs, the mesh is changed through `A.mutate_mesh`, `A`
//! runs on; the fresh model `B` is built on a mesh that went through the
//! same change without consumers, receives `A`'s state right after the
//! change (every array of the DOF family — values, velocity, acceleration,
//! blocked flags, previous values, increment — and the external force) and
//! runs the same steps. The DOF numbering of `A` must equal `B`'s, and
//! every nodal field must match **bitwise**: the renumbering reproduces the
//! fresh registration exactly and the element order (hence every
//! assembly order) is the same in both models
//! (`docs/NUMERICAL_VALIDATION.md`).

// shared by several test binaries, each of which uses a subset
#![allow(dead_code)]

use akantu_comm::Communicator;
use akantu_core::enums::{AnalysisMethod, SparseSolverType};
use akantu_core::{Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::distributed_removal::DistributedRemovalHook;
use akantu_mesh::{DistributedHook, Mesh, MeshAccessor, MeshPart, MeshTransaction, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_solid::model::DOF_ID;

pub type Outcome = std::result::Result<(), String>;

fn ok<T>(r: akantu_core::Result<T>, what: &str) -> std::result::Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

/// SPMD discipline of the distributed scenarios: a check of the
/// scenario whose outcome may differ between ranks is **agreed before the
/// next collective call** — every rank fails there, the failing ones with
/// their own message — instead of returning on the failing ranks only,
/// which leaves the others waiting in the next collective until the
/// deadline and breaks the communicator for every later scenario of the
/// binary. (The library's own failures are agreed by the library.)
pub fn agreed(comm: &Communicator, local: Outcome) -> Outcome {
    let mut c = [u64::from(local.is_err())];
    ok(
        comm.all_reduce(&mut c, akantu_comm::ReduceOp::Sum),
        "agree a check of the scenario",
    )?;
    match local {
        Err(e) => Err(e),
        Ok(()) if c[0] > 0 => Err(format!("a check failed on {} other rank(s)", c[0])),
        Ok(()) => Ok(()),
    }
}

pub const DAT: &str = "material elastic [\n  name = steel\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n";

/// The integration of a scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// Linear static (faer).
    Static,
    /// Explicit central differences, lumped mass.
    Explicit,
    /// Implicit trapezoidal rule, consistent mass (faer).
    ImplicitDynamic,
}

impl Method {
    fn options(self) -> ModelOptions {
        let mut o = ModelOptions::new(match self {
            Method::Static => AnalysisMethod::Static,
            Method::Explicit => AnalysisMethod::ExplicitLumpedMass,
            Method::ImplicitDynamic => AnalysisMethod::ImplicitDynamic,
        });
        if self != Method::Explicit {
            o.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
        }
        o
    }
}

/// Model on `shared` with [`DAT`].
pub fn model_on(shared: &SharedMesh) -> std::result::Result<SolidMechanicsModel, String> {
    let parser = ok(Parser::parse_str(DAT, "material.dat"), "parser")?;
    ok(
        SolidMechanicsModel::with_parser(shared.clone(), None, "smm", parser),
        "model",
    )
}

// ------------------------------------------------------------- the meshes

/// Structured `[0, nx] × [0, ny]` triangle mesh whose nodes are created
/// **top row first** (so the purify numbering, by first appearance in the
/// connectivity, is a non-trivial permutation) with an unused node after
/// every fourth node when `with_unused` (the purify numbering removes them).
pub fn scrambled_square(nx: usize, ny: usize, with_unused: bool) -> Mesh {
    let mut mesh = Mesh::new(2, "square").unwrap();
    let mut index = vec![0 as Idx; (nx + 1) * (ny + 1)];
    {
        let mut a = MeshAccessor::new(&mut mesh);
        let mut count = 0usize;
        for j in (0..=ny).rev() {
            for i in 0..=nx {
                index[j * (nx + 1) + i] = a.push_node(&[i as Real, j as Real]).unwrap();
                count += 1;
                if with_unused && count.is_multiple_of(4) {
                    // an unused node inside the domain (no bounding-box
                    // change)
                    a.push_node(&[i as Real * 0.5 + 0.25, 0.5]).unwrap();
                }
            }
        }
        let id = |i: usize, j: usize| index[j * (nx + 1) + i];
        let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for j in 0..ny {
            for i in 0..nx {
                c.push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                c.push_tuple(&[id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
    }
    mesh.compute_bounding_box().unwrap();
    mesh
}

// ----------------------------------------------------------- the transfer

/// The nodal state of a model: every array of the DOF family (values,
/// derivatives, previous values, increment), the external force and the
/// blocked flags, by name.
#[derive(Clone, Debug, PartialEq)]
pub struct NodalState {
    dim: usize,
    arrays: Vec<(String, Vec<Real>)>,
    blocked: Vec<bool>,
}

/// The nodal state of `m`.
pub fn snapshot(m: &SolidMechanicsModel) -> std::result::Result<NodalState, String> {
    let f = ok(m.solver.dof_manager.family(DOF_ID), "family")?;
    let read = |a: &akantu_core::array_shared::SharedArray<Real>| -> std::result::Result<Vec<Real>, String> {
        Ok(ok(a.read(), "read")?.as_slice().to_vec())
    };
    let mut arrays = vec![("displacement".to_owned(), read(&f.dofs)?)];
    for (o, d) in &f.derivatives {
        arrays.push((format!("derivative {o}"), read(d)?));
    }
    if let Some(p) = &f.previous {
        arrays.push(("previous".into(), read(p)?));
    }
    if let Some(i) = &f.increment {
        arrays.push(("increment".into(), read(i)?));
    }
    arrays.push((
        "external_force".into(),
        ok(m.external_force(), "f_ext")?.as_slice().to_vec(),
    ));
    let blocked = ok(m.blocked_dofs(), "blocked")?.as_slice().to_vec();
    Ok(NodalState {
        dim: m.spatial_dimension(),
        arrays,
        blocked,
    })
}

/// The node numbering of a committed removal (`MeshPart::Main`); the
/// identity when nothing was removed.
pub fn removal_numbering(committed: &akantu_mesh::CommittedMutation, nb_nodes: usize) -> Vec<Idx> {
    committed
        .events
        .iter()
        .find_map(|(p, e)| match e {
            akantu_mesh::MeshEvent::NodesRemoved { new_numbering, .. } if *p == MeshPart::Main => {
                Some(new_numbering.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| (0..nb_nodes as Idx).collect())
}

/// `state` moved node by node with `numbering` (upstream
/// `removeNodesFromArray`, computed here independently of the port's
/// compaction), `nb_nodes` rows: rows past the kept ones (nodes appended
/// after the removal) are zero and free.
pub fn moved(state: &NodalState, numbering: &[Idx], nb_nodes: usize) -> NodalState {
    let d = state.dim;
    let mv = |v: &[Real]| {
        let mut out = vec![0.0; nb_nodes * d];
        for (i, &n) in numbering.iter().enumerate() {
            if let Ok(n) = usize::try_from(n) {
                out[d * n..d * (n + 1)].copy_from_slice(&v[d * i..d * (i + 1)]);
            }
        }
        out
    };
    let mut blocked = vec![false; nb_nodes * d];
    for (i, &n) in numbering.iter().enumerate() {
        if let Ok(n) = usize::try_from(n) {
            blocked[d * n..d * (n + 1)].copy_from_slice(&state.blocked[d * i..d * (i + 1)]);
        }
    }
    NodalState {
        dim: d,
        arrays: state
            .arrays
            .iter()
            .map(|(name, v)| (name.clone(), mv(v)))
            .collect(),
        blocked,
    }
}

/// Write `state` into `b` (the transfer of the state to the fresh model).
pub fn set_state(b: &mut SolidMechanicsModel, state: &NodalState) -> Outcome {
    for (name, v) in &state.arrays {
        if name == "external_force" {
            let mut f = ok(b.external_force_mut(), "f_ext")?;
            if f.as_slice().len() != v.len() {
                return Err(format!(
                    "{name}: {} vs {} values",
                    f.as_slice().len(),
                    v.len()
                ));
            }
            f.as_mut_slice().copy_from_slice(v);
            continue;
        }
        let fam = ok(b.solver.dof_manager.family_mut(DOF_ID), "family")?;
        let dst = match name.as_str() {
            "displacement" => Some(&mut fam.dofs),
            "previous" => fam.previous.as_mut(),
            "increment" => fam.increment.as_mut(),
            other => other
                .strip_prefix("derivative ")
                .and_then(|o| o.parse::<usize>().ok())
                .and_then(|o| fam.derivatives.get_mut(&o)),
        };
        let dst = dst.ok_or_else(|| format!("{name}: not allocated in the fresh model"))?;
        let mut g = ok(dst.write(), name)?;
        if g.as_slice().len() != v.len() {
            return Err(format!(
                "{name}: {} vs {} values",
                g.as_slice().len(),
                v.len()
            ));
        }
        g.as_mut_slice().copy_from_slice(v);
    }
    let mut bl = ok(b.blocked_dofs_mut(), "blocked")?;
    if bl.as_slice().len() != state.blocked.len() {
        return Err("blocked flags: lengths differ".into());
    }
    bl.as_mut_slice().copy_from_slice(&state.blocked);
    Ok(())
}

/// `m`'s nodal state is exactly `want`.
pub fn check_state(m: &SolidMechanicsModel, want: &NodalState, what: &str) -> Outcome {
    let got = snapshot(m)?;
    for ((name, g), (_, w)) in got.arrays.iter().zip(&want.arrays) {
        if bits(g) != bits(w) {
            return Err(format!("{what}: {name} did not follow its nodes"));
        }
    }
    if got.arrays.len() != want.arrays.len() {
        return Err(format!("{what}: other arrays"));
    }
    if got.blocked != want.blocked {
        return Err(format!("{what}: blocked flags did not follow their nodes"));
    }
    Ok(())
}

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// `A` and `B` agree: DOF numbering equal, nodal fields bitwise equal.
pub fn compare(a: &mut SolidMechanicsModel, b: &mut SolidMechanicsModel, what: &str) -> Outcome {
    compare_fields(a, b, what, true)
}

/// [`compare`]; `after_a_step: false` right after the transfer: `B` has
/// not assembled its internal force yet, and its current position is the
/// cache of its initialisation (writing the displacement bumps no release,
/// as upstream; the removal bumps `A`'s, as upstream's `onNodesRemoved`),
/// so both are compared only after a step.
pub fn compare_fields(
    a: &mut SolidMechanicsModel,
    b: &mut SolidMechanicsModel,
    what: &str,
    after_a_step: bool,
) -> Outcome {
    {
        let (da, db) = (&a.solver.dof_manager, &b.solver.dof_manager);
        if da.layout() != db.layout() {
            return Err(format!("{what}: equation layouts differ"));
        }
        if ok(da.local_equation_numbers(DOF_ID), "eqs")?
            != ok(db.local_equation_numbers(DOF_ID), "eqs")?
        {
            return Err(format!("{what}: local equation numbers differ"));
        }
    }
    let xa = ok(a.current_position(), "x")?.as_slice().to_vec();
    let xb = ok(b.current_position(), "x")?.as_slice().to_vec();
    let get = |m: &SolidMechanicsModel| -> std::result::Result<[Vec<Real>; 4], String> {
        Ok([
            ok(m.displacement(), "u")?.as_slice().to_vec(),
            m.velocity()
                .map(|v| v.as_slice().to_vec())
                .unwrap_or_default(),
            m.acceleration()
                .map(|v| v.as_slice().to_vec())
                .unwrap_or_default(),
            ok(m.internal_force(), "f_int")?.as_slice().to_vec(),
        ])
    };
    let [ua, va, aa, fa] = get(a)?;
    let [ub, vb, ab, fb] = get(b)?;
    let fields = [
        ("displacement", ua, ub),
        ("velocity", va, vb),
        ("acceleration", aa, ab),
        ("internal_force", fa, fb),
        ("current_position", xa, xb),
    ];
    if after_a_step && fields[0].1.iter().all(|&u| u == 0.0) && !fields[0].1.is_empty() {
        return Err(format!("{what}: vacuous comparison (zero displacement)"));
    }
    for (name, x, y) in fields {
        if (name == "internal_force" || name == "current_position") && !after_a_step {
            continue;
        }
        if x.len() != y.len() {
            return Err(format!("{what}: {name}: {} vs {} values", x.len(), y.len()));
        }
        if bits(&x) != bits(&y) {
            let (k, (p, q)) = x
                .iter()
                .zip(&y)
                .enumerate()
                .find(|(_, (p, q))| p.to_bits() != q.to_bits())
                .unwrap_or((0, (&0.0, &0.0)));
            let max = x
                .iter()
                .zip(&y)
                .map(|(p, q)| (p - q).abs())
                .fold(0.0, Real::max);
            return Err(format!(
                "{what}: {name} differs (first at {k}: {p:e} vs {q:e}; max |Δ| {max:e})"
            ));
        }
    }
    Ok(())
}

// --------------------------------------------------- set-up of the models

/// Blocked: nodes at `x = 0` (2D) / `z = 0` (3D) and every node of
/// `extra`; then the initial state (dynamic: a smooth bending mode) or the
/// load (static).
fn set_up(model: &mut SolidMechanicsModel, method: Method, extra: &[usize]) -> Outcome {
    let (coords, nb, dim) = {
        let m = ok(model.physics.base.mesh.read(), "read")?;
        (
            m.nodes().as_slice().to_vec(),
            m.nb_nodes(),
            m.spatial_dimension(),
        )
    };
    let axis = if dim == 2 { 0 } else { 2 };
    {
        let mut b = ok(model.blocked_dofs_mut(), "blocked")?;
        let b = b.as_mut_slice();
        for n in 0..nb {
            if coords[dim * n + axis] == 0.0 || extra.contains(&n) {
                b[dim * n..dim * (n + 1)].fill(true);
            }
        }
    }
    let far = coords
        .chunks(dim)
        .map(|x| x[axis])
        .fold(Real::MIN, Real::max);
    match method {
        Method::Static => load(model, 1.0)?,
        Method::Explicit | Method::ImplicitDynamic => {
            let mut u = ok(model.displacement_mut(), "u")?;
            let u = u.as_mut_slice();
            for n in 0..nb {
                if extra.contains(&n) {
                    continue;
                }
                let x = &coords[dim * n..dim * (n + 1)];
                let s = x[axis] / far;
                u[dim * n] = 1e-3 * s * s * (1.0 + 0.3 * x[1]);
                u[dim * n + 1] = 5e-4 * s * (0.5 - 0.1 * x[0]);
            }
        }
    }
    Ok(())
}

/// External force `scale × (1e-3 (1 + y), −2e-3)` (2D; 3D: also along z)
/// at the nodes of the far face; zero elsewhere.
pub fn load(model: &mut SolidMechanicsModel, scale: Real) -> Outcome {
    let (coords, nb, dim) = {
        let m = ok(model.physics.base.mesh.read(), "read")?;
        (
            m.nodes().as_slice().to_vec(),
            m.nb_nodes(),
            m.spatial_dimension(),
        )
    };
    let axis = if dim == 2 { 0 } else { 2 };
    let far = coords
        .chunks(dim)
        .map(|x| x[axis])
        .fold(Real::MIN, Real::max);
    let mut f = ok(model.external_force_mut(), "f_ext")?;
    let f = f.as_mut_slice();
    f.fill(0.0);
    for n in 0..nb {
        let x = &coords[dim * n..dim * (n + 1)];
        if x[axis] == far {
            f[dim * n] = scale * 1e-3 * (1.0 + x[1]);
            f[dim * n + dim - 1] = -scale * 2e-3;
        }
    }
    Ok(())
}

fn time_step(model: &mut SolidMechanicsModel, method: Method) -> std::result::Result<Real, String> {
    let dt = ok(model.stable_time_step(), "stable time step")?;
    Ok(match method {
        Method::Static => 0.0,
        Method::Explicit => 0.8 * dt,
        Method::ImplicitDynamic => 4.0 * dt,
    })
}

// ------------------------------------------------------------ scenarios

/// The change of a serial scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// The square has unused nodes (blocked in `A`); `purify` removes them
    /// and permutes the others. Explicit only: a static or implicit `K`
    /// has no entry, hence no diagonal, at an unused node (upstream's
    /// `applyBoundary` modifies existing entries only), so it is singular.
    PurifyUnused,
    /// The last square (two triangles, the last elements of the mesh) is
    /// erased and `purify` removes its orphaned corner node and permutes the
    /// others, in one mutation. The erased elements are the last ones, so
    /// no element moves and every law filter keeps the fresh model's order.
    EraseCornerAndPurify,
}

/// Serial: the scrambled square (`purify` is a non-identity renumbering)
/// with `change` between the solves. Static: one solve, the load doubled
/// after the removal, one more solve. Dynamic: `steps` before and after.
pub fn serial_equivalence(method: Method, change: Change, steps: usize) -> Outcome {
    let (nx, ny) = (8, 3);
    let with_unused = change == Change::PurifyUnused;
    let mesh = scrambled_square(nx, ny, with_unused);
    let unused: Vec<usize> = ok(
        akantu_mesh::utils::purify_numbering(&mesh),
        "purify numbering",
    )?
    .iter()
    .enumerate()
    .filter(|&(_, &n)| n < 0)
    .map(|(i, _)| i)
    .collect();
    let last_square = |m: &Mesh| -> Vec<Element> {
        let ne = m.nb_element(ElementType::Triangle3, GhostType::NotGhost);
        (ne - 2..ne)
            .map(|e| Element::new(ElementType::Triangle3, e as Idx, GhostType::NotGhost))
            .collect()
    };
    let apply = |tx: &mut MeshTransaction<'_>| -> akantu_core::Result<()> {
        if change == Change::EraseCornerAndPurify {
            let els = last_square(tx.mesh());
            tx.erase_elements(MeshPart::Main, &els)?;
        }
        tx.purify(MeshPart::Main)
    };
    let shared_a = SharedMesh::new(mesh);
    let mut a = model_on(&shared_a)?;
    ok(a.init_full(&method.options()), "init A")?;
    set_up(&mut a, method, &unused)?;
    let dt = time_step(&mut a, method)?;
    if method != Method::Static {
        ok(a.set_time_step(dt, None), "dt A")?;
    }
    let before = if method == Method::Static { 1 } else { steps };
    for s in 0..before {
        ok(a.solve_step(None), &format!("A step {s}"))?;
    }
    let nodes_before = ok(shared_a.read(), "read")?.nb_nodes();
    let pre = snapshot(&a)?;
    let committed = ok(a.mutate_mesh(apply, None), "removal")?;
    let removed = committed.events.iter().any(|(p, e)| {
        *p == MeshPart::Main
            && matches!(e, akantu_mesh::MeshEvent::NodesRemoved { nodes, new_numbering, .. }
                if !nodes.is_empty()
                    && new_numbering.iter().enumerate().any(|(i, &n)| n >= 0 && n as usize != i))
    });
    let nodes_after = ok(shared_a.read(), "read")?.nb_nodes();
    if !removed || nodes_after >= nodes_before {
        return Err("the scenario needs removed nodes and a permutation of the others".into());
    }
    let state = moved(
        &pre,
        &removal_numbering(&committed, nodes_before),
        nodes_after,
    );
    check_state(&a, &state, "right after the removal")?;
    if method == Method::Static {
        load(&mut a, 2.0)?;
    }
    // B: the same mesh through the same change without consumers
    let shared_b = SharedMesh::new(scrambled_square(nx, ny, with_unused));
    ok(shared_b.mutate(&mut [], apply, None), "change of B")?;
    let mut b = model_on(&shared_b)?;
    ok(b.init_full(&method.options()), "init B")?;
    if method != Method::Static {
        ok(b.set_time_step(dt, None), "dt B")?;
    }
    set_state(&mut b, &state)?;
    if method == Method::Static {
        load(&mut b, 2.0)?;
    }
    compare_fields(&mut a, &mut b, "right after the removal", false)?;
    let after = if method == Method::Static { 1 } else { steps };
    for s in 0..after {
        ok(a.solve_step(None), &format!("A step {}", before + s))?;
        ok(b.solve_step(None), &format!("B step {s}"))?;
        compare(
            &mut a,
            &mut b,
            &format!("{method:?} step {s} after the removal"),
        )?;
    }
    Ok(())
}

/// Element parts of the cube for `size` ranks: by cell position, the six
/// tetrahedra of the **last** cell (corner `(1, 1, 1)`) dealt round-robin
/// to the ranks, so that its nodes are shared by the first `min(size, 6)`
/// ranks ([`corner_holders`]; every rank up to 6 ranks).
pub fn corner_parts(n: usize, size: usize) -> Vec<Idx> {
    let mut parts = Vec::with_capacity(6 * n * n * n);
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let last = i == n - 1 && j == n - 1 && k == n - 1;
                for t in 0..6 {
                    let p = if last {
                        t % size
                    } else {
                        let (px, py) = ((2 * i) / n, (2 * j) / n);
                        match size {
                            1 => 0,
                            2 => px,
                            _ => (px + 2 * py) % size,
                        }
                    };
                    parts.push(p as Idx);
                }
            }
        }
    }
    parts
}

/// Ranks that hold the corner node `(n, n, n)` under [`corner_parts`]: the
/// corner cell's six tetrahedra are dealt round-robin, so `min(size, 6)`.
/// From 7 ranks on the other ranks hold no corner element, and
/// since the other cells use four parts, ranks 4 and up hold only corner
/// tetrahedra, ranks 6 and up nothing at all (empty ranks, UD-813): the
/// scenarios check what each rank must see, holder or not.
pub fn corner_holders(size: usize) -> usize {
    size.min(6)
}

/// This rank's mesh holds the corner node `(n, n, n)` (the last node of
/// the structured numbering).
pub fn holds_corner(mesh: &Mesh, n: usize) -> bool {
    let g = ((n + 1) * (n + 1) * (n + 1) - 1) as i64;
    (0..mesh.nb_nodes()).any(|l| mesh.node_global_id(l as Idx) == g)
}

/// After the corner cell was erased and its orphaned nodes removed: a rank
/// holding the corner node lost at least one node, any other rank none.
fn removal_as_expected(holder: bool, before: usize, after: usize, what: &str) -> Outcome {
    if holder && after >= before {
        return Err(format!(
            "{what}: no node removed ({before} → {after}) on a rank holding the corner node"
        ));
    }
    if !holder && after != before {
        return Err(format!(
            "{what}: {before} → {after} nodes on a rank without the corner node"
        ));
    }
    Ok(())
}

/// The elements of the last cell of the cube (barycentre beyond
/// `(n − 1)/n` in every direction), not-ghost and ghost.
pub fn corner_elements(mesh: &Mesh, n: usize) -> Vec<Element> {
    let lim = (n as Real - 1.0) / n as Real;
    let mut out = Vec::new();
    for ghost in GhostType::ALL {
        let ty = ElementType::Tetrahedron4;
        for e in 0..mesh.nb_element(ty, ghost) {
            let el = Element::new(ty, e as Idx, ghost);
            if let Ok(b) = mesh.barycenter(el)
                && b.iter().all(|&x| x > lim)
            {
                out.push(el);
            }
        }
    }
    out
}

/// The change of the distributed scenario: the corner cell erased on every
/// rank holding it (local and ghost copies), then every node no longer
/// used removed (`purify`: the corner node, shared by every rank holding
/// a corner tetrahedron, among them).
pub fn erase_corner(tx: &mut MeshTransaction<'_>, n: usize) -> akantu_core::Result<()> {
    let els = corner_elements(tx.mesh(), n);
    tx.erase_elements(MeshPart::Main, &els)?;
    tx.purify(MeshPart::Main)
}

/// Distributed (any number of ranks): the cube distributed with
/// [`corner_parts`]; between the solves the corner cell is erased and its
/// orphaned nodes removed — the corner node is a shared node of the
/// [`corner_holders`] ranks (every rank up to 6 ranks), so the removal is on
/// a rank boundary (the node synchronizer's schemes and the global
/// equations follow, `DistributedRemovalHook`); from 7 ranks on the other
/// ranks hold no corner element and lose no node. `B`'s mesh goes
/// through the same mutation without consumers. Per rank: numbering and
/// nodal fields bitwise equal to the fresh model.
pub fn rank_boundary_equivalence(comm: Communicator, method: Method, steps: usize) -> Outcome {
    let n = 2;
    let part = crate::par_smm::Part::Prescribed(corner_parts(n, comm.size()));
    let mesh_a = crate::par_smm::distributed_cube(&comm, n, &part)?;
    let holder = holds_corner(&mesh_a, n);
    let holders = {
        let mut c = [u64::from(holder)];
        ok(
            comm.all_reduce(&mut c, akantu_comm::ReduceOp::Sum),
            "count holders",
        )?;
        c[0]
    };
    if holders != corner_holders(comm.size()) as u64 {
        return Err(format!(
            "the corner node is held by {holders} of {} ranks (expected {})",
            comm.size(),
            corner_holders(comm.size())
        ));
    }
    let shared_a = SharedMesh::new(mesh_a);
    let mut a = model_on(&shared_a)?;
    ok(a.init_full(&method.options()), "init A")?;
    set_up(&mut a, method, &[])?;
    let dt = time_step(&mut a, method)?;
    if method != Method::Static {
        ok(a.set_time_step(dt, None), "dt A")?;
    }
    let before = if method == Method::Static { 1 } else { steps };
    for s in 0..before {
        ok(a.solve_step(None), &format!("A step {s}"))?;
    }
    let nodes_before = ok(shared_a.read(), "read")?.nb_nodes();
    let pre = snapshot(&a)?;
    let mut hook = DistributedRemovalHook::new();
    let committed = ok(
        a.mutate_mesh(
            |tx| erase_corner(tx, n),
            Some(&mut hook as &mut dyn DistributedHook),
        ),
        "erase corner (A)",
    )?;
    let nodes_after = ok(shared_a.read(), "read")?.nb_nodes();
    agreed(
        &comm,
        removal_as_expected(
            holder,
            nodes_before,
            nodes_after,
            &format!("rank {}", comm.rank()),
        ),
    )?;
    let state = moved(
        &pre,
        &removal_numbering(&committed, nodes_before),
        nodes_after,
    );
    agreed(
        &comm,
        check_state(
            &a,
            &state,
            &format!("rank {}: right after the removal", comm.rank()),
        ),
    )?;
    // The renumbered synchronizers, checked
    // independently of the hook (geometry and global ids exchanged)
    agreed(
        &comm,
        check_synchronizers(
            &*ok(shared_a.read(), "read")?,
            &format!("rank {}: after the removal", comm.rank()),
        ),
    )?;
    if method == Method::Static {
        load(&mut a, 2.0)?;
    }
    let shared_b = SharedMesh::new(crate::par_smm::distributed_cube(&comm, n, &part)?);
    let mut hook_b = DistributedRemovalHook::new();
    ok(
        shared_b.mutate(
            &mut [],
            |tx| erase_corner(tx, n),
            Some(&mut hook_b as &mut dyn DistributedHook),
        ),
        "erase corner (B)",
    )?;
    let mut b = model_on(&shared_b)?;
    ok(b.init_full(&method.options()), "init B")?;
    if method != Method::Static {
        ok(b.set_time_step(dt, None), "dt B")?;
    }
    agreed(&comm, set_state(&mut b, &state))?;
    if method == Method::Static {
        load(&mut b, 2.0)?;
    }
    let after = if method == Method::Static { 1 } else { steps };
    for s in 0..after {
        ok(a.solve_step(None), &format!("A step {}", before + s))?;
        ok(b.solve_step(None), &format!("B step {s}"))?;
        agreed(
            &comm,
            compare(
                &mut a,
                &mut b,
                &format!("rank {} {method:?} step {s} after the removal", comm.rank()),
            ),
        )?;
    }
    Ok(())
}

// -------------------------------------------- the IGFEM-shaped sequence

/// An IGFEM-**shaped** update in one mutation, in the order the composition
/// rules accept (`docs/ARCHITECTURE.md`, "Event rules"):
/// the elements using the old node erased (one call), the orphaned node
/// removed with a renumbering of the others (`purify`), a new node appended
/// (`NodesAdded`) and new elements using it (`ElementsAdded`). Here the new
/// node and elements reproduce the old ones, so the final mesh is the
/// initial one up to the node numbering.
///
/// This validates the DOF path of a **removal followed by an addition**; it
/// is **not** upstream's order: upstream's interface
/// move adds the intersection nodes first
/// (`computeMeshQueryListIntersectionPoint`,
/// `mesh_igfem_spherical_growing_gel_tmpl.hh:62-108`), sends the element
/// events with the old → new element map (`:380-391`; upstream
/// `onElementsAdded` reads the old elements), removes the old elements, and
/// only then `removeAdditionalNodes` (`:113-173`) removes the old enriched
/// nodes and renumbers the added ones. A port of that sequence is two
/// mutations (rule 3 of the composition ADR).
pub fn igfem_shaped_update(tx: &mut MeshTransaction<'_>) -> akantu_core::Result<()> {
    use akantu_mesh::MeshEvent;
    let ty = ElementType::Triangle3;
    let ne = tx.mesh().nb_element(ty, GhostType::NotGhost);
    let last: Vec<Element> = (ne - 2..ne)
        .map(|e| Element::new(ty, e as Idx, GhostType::NotGhost))
        .collect();
    let conn: Vec<[Idx; 3]> = {
        let c = tx.mesh().connectivity(ty, GhostType::NotGhost)?;
        last.iter()
            .map(|e| {
                let t = c.tuple(e.idx());
                [t[0], t[1], t[2]]
            })
            .collect()
    };
    let corner = {
        // the node of both triangles absent from every other element: the
        // corner (nx, ny)
        let shared: Vec<Idx> = conn[0]
            .iter()
            .copied()
            .filter(|n| conn[1].contains(n))
            .collect();
        let c = tx.mesh().connectivity(ty, GhostType::NotGhost)?;
        *shared
            .iter()
            .find(|&&n| (0..ne - 2).all(|e| !c.tuple(e).contains(&n)))
            .ok_or_else(|| akantu_core::Error::invalid("no corner node"))?
    };
    let x = tx.mesh().nodes().tuple(corner as usize).to_vec();
    tx.erase_elements(MeshPart::Main, &last)?;
    let numbering = akantu_mesh::utils::purify_numbering(tx.mesh())?;
    tx.remove_nodes(MeshPart::Main, &numbering)?;
    let new = tx.push_node(MeshPart::Main, &x)?;
    tx.record(
        MeshPart::Main,
        MeshEvent::NodesAdded {
            nodes: vec![new],
            old_nodes: None,
            origin: "igfem-shaped".into(),
            extension: None,
        },
    );
    let mut added = Vec::new();
    for t in conn {
        let t: Vec<Idx> = t
            .iter()
            .map(|&n| {
                if n == corner {
                    new
                } else {
                    numbering[n as usize]
                }
            })
            .collect();
        added.push(tx.push_element(MeshPart::Main, ty, GhostType::NotGhost, &t)?);
    }
    tx.record(
        MeshPart::Main,
        MeshEvent::ElementsAdded {
            elements: added,
            constitutive_law: None,
            origin: "igfem-shaped".into(),
            extension: None,
        },
    );
    Ok(())
}

/// The IGFEM-shaped update between explicit steps of a live model equals a
/// fresh model on the final mesh with the state transferred (the new node
/// starts at rest in both), bitwise.
pub fn igfem_shaped_equivalence(steps: usize) -> Outcome {
    let (nx, ny) = (8, 3);
    let method = Method::Explicit;
    let shared_a = SharedMesh::new(scrambled_square(nx, ny, false));
    let mut a = model_on(&shared_a)?;
    ok(a.init_full(&method.options()), "init A")?;
    set_up(&mut a, method, &[])?;
    let dt = time_step(&mut a, method)?;
    ok(a.set_time_step(dt, None), "dt A")?;
    for s in 0..steps {
        ok(a.solve_step(None), &format!("A step {s}"))?;
    }
    let nodes_before = ok(shared_a.read(), "read")?.nb_nodes();
    let pre = snapshot(&a)?;
    let committed = ok(a.mutate_mesh(igfem_shaped_update, None), "update")?;
    let kinds: Vec<&str> = committed
        .events
        .iter()
        .filter(|(p, _)| *p == MeshPart::Main)
        .map(|(_, e)| match e {
            akantu_mesh::MeshEvent::ElementsRemoved { .. } => "elements removed",
            akantu_mesh::MeshEvent::NodesRemoved { .. } => "nodes removed",
            akantu_mesh::MeshEvent::NodesAdded { .. } => "nodes added",
            akantu_mesh::MeshEvent::ElementsAdded { .. } => "elements added",
            _ => "other",
        })
        .collect();
    if kinds
        != [
            "elements removed",
            "nodes removed",
            "nodes added",
            "elements added",
        ]
    {
        return Err(format!("unexpected event sequence {kinds:?}"));
    }
    if ok(shared_a.read(), "read")?.nb_nodes() != nodes_before {
        return Err("the update changes the node count".into());
    }
    // kept nodes moved, the appended node at rest and free
    let state = moved(
        &pre,
        &removal_numbering(&committed, nodes_before),
        nodes_before,
    );
    check_state(&a, &state, "right after the update")?;
    let shared_b = SharedMesh::new(scrambled_square(nx, ny, false));
    ok(
        shared_b.mutate(&mut [], igfem_shaped_update, None),
        "update B",
    )?;
    let mut b = model_on(&shared_b)?;
    ok(b.init_full(&method.options()), "init B")?;
    ok(b.set_time_step(dt, None), "dt B")?;
    set_state(&mut b, &state)?;
    compare_fields(&mut a, &mut b, "right after the update", false)?;
    for s in 0..steps {
        ok(a.solve_step(None), &format!("A step {}", steps + s))?;
        ok(b.solve_step(None), &format!("B step {s}"))?;
        compare(&mut a, &mut b, &format!("step {s} after the update"))?;
    }
    Ok(())
}

// ------------------------------------- rollback of a rank-specific refusal

/// The solid model's event parts, refusing node removal in the capability
/// check when `refuse` (a consumer that cannot follow the removal on one
/// rank only).
struct RefusingParts<'a> {
    inner: akantu_solid::model::SmmEventParts<'a>,
    refuse: bool,
}

impl akantu_mesh::MeshEventHandler for RefusingParts<'_> {
    fn event_priority(&self) -> akantu_core::enums::EventPriority {
        self.inner.event_priority()
    }
    fn event_priorities(&self) -> Vec<akantu_core::enums::EventPriority> {
        self.inner.event_priorities()
    }
    fn on_mesh_event(
        &mut self,
        mesh: &Mesh,
        event: &akantu_mesh::MeshEvent,
    ) -> akantu_core::Result<()> {
        self.inner.on_mesh_event(mesh, event)
    }
    fn check_mesh_event(
        &self,
        mesh: &Mesh,
        event: &akantu_mesh::MeshEvent,
    ) -> akantu_core::Result<()> {
        if self.refuse && matches!(event, akantu_mesh::MeshEvent::NodesRemoved { .. }) {
            return Err(akantu_core::Error::NotImplemented(
                "test consumer: node removal refused on this rank",
            ));
        }
        self.inner.check_mesh_event(mesh, event)
    }
    fn on_mesh_event_at(
        &mut self,
        mesh: &Mesh,
        event: &akantu_mesh::MeshEvent,
        level: akantu_core::enums::EventPriority,
    ) -> akantu_core::Result<()> {
        self.inner.on_mesh_event_at(mesh, event, level)
    }
    fn on_mesh_event_phased(
        &mut self,
        mesh: &Mesh,
        event: &akantu_mesh::MeshEvent,
        level: akantu_core::enums::EventPriority,
        phases: &mut akantu_comm::phase::Phases,
    ) -> akantu_core::Result<()> {
        self.inner.on_mesh_event_phased(mesh, event, level, phases)
    }
}

/// The rank-boundary removal of [`rank_boundary_equivalence`] refused by a
/// consumer on the **last** rank only: the mutation is rolled back on every
/// rank (recoverable: the refusing rank returns its error, the others a
/// recoverable communication error), the mesh (nodes, revision, node
/// schemes) and the model (numbering, nodal arrays) are unchanged, and the
/// domain is usable: the same removal then commits and the model runs on.
/// Any rank count: from 7 ranks on the last rank holds no element, and its
/// refusal still rolls the mutation back everywhere (the event is checked
/// on every rank since its global count is not zero).
pub fn rollback_when_one_rank_refuses(comm: Communicator) -> Outcome {
    let n = 2;
    let method = Method::Explicit;
    let part = crate::par_smm::Part::Prescribed(corner_parts(n, comm.size()));
    let shared = SharedMesh::new(crate::par_smm::distributed_cube(&comm, n, &part)?);
    let holder = holds_corner(&*ok(shared.read(), "read")?, n);
    let mut a = model_on(&shared)?;
    ok(a.init_full(&method.options()), "init")?;
    set_up(&mut a, method, &[])?;
    let dt = time_step(&mut a, method)?;
    ok(a.set_time_step(dt, None), "dt")?;
    for s in 0..2 {
        ok(a.solve_step(None), &format!("step {s}"))?;
    }
    /// Node count, revision, numbering and node schemes, `u` and `v` bits.
    type State = (usize, u64, String, Vec<u64>, Vec<u64>);
    let state =
        |a: &SolidMechanicsModel, shared: &SharedMesh| -> std::result::Result<State, String> {
            let m = ok(shared.read(), "read")?;
            Ok((
                m.nb_nodes(),
                ok(shared.revision(), "revision")?,
                format!(
                    "{:?} {:?}",
                    a.solver.dof_manager.layout(),
                    m.synchronizers()
                        .node
                        .as_ref()
                        .map(|s| (&s.communications.send, &s.communications.recv))
                ),
                bits(ok(a.displacement(), "u")?.as_slice()),
                bits(ok(a.velocity(), "v")?.as_slice()),
            ))
        };
    let before = state(&a, &shared)?;
    let refuse = usize::try_from(comm.rank()).is_ok_and(|r| r + 1 == comm.size());
    let r = {
        let (token, parts) = a.split_for_mutation();
        let mut handler = RefusingParts {
            inner: parts,
            refuse,
        };
        let mut hook = DistributedRemovalHook::new();
        shared.mutate(
            &mut [(token, &mut handler)],
            |tx| erase_corner(tx, n),
            Some(&mut hook),
        )
    };
    let refused = match &r {
        Ok(_) => Err("the refused removal committed".into()),
        Err(e) if e.fatal_kind().is_some() => Err(format!("the refusal was fatal: {e}")),
        Err(akantu_core::Error::NotImplemented(_)) if refuse => Ok(()),
        Err(akantu_core::Error::Communication(_)) if !refuse => Ok(()),
        Err(e) => Err(format!("unexpected error class: {e:?}")),
    };
    let unchanged = refused.and_then(|()| {
        if state(&a, &shared)? == before {
            Ok(())
        } else {
            Err("the refused removal changed the mesh or the model".into())
        }
    });
    agreed(&comm, unchanged)?;
    let mut hook = DistributedRemovalHook::new();
    ok(
        a.mutate_mesh(
            |tx| erase_corner(tx, n),
            Some(&mut hook as &mut dyn DistributedHook),
        ),
        "the same removal afterwards",
    )?;
    let after = ok(shared.read(), "read")?.nb_nodes();
    agreed(
        &comm,
        removal_as_expected(
            holder,
            before.0,
            after,
            &format!("rank {}: the same removal afterwards", comm.rank()),
        ),
    )?;
    for s in 0..2 {
        ok(a.solve_step(None), &format!("step {s} after the removal"))?;
    }
    if !ok(a.displacement(), "u")?
        .as_slice()
        .iter()
        .all(|u| u.is_finite())
    {
        return Err("non-finite displacement after the removal".into());
    }
    Ok(())
}

/// UD-1072: every rank erases only its **local** corner elements (their
/// ghost copies are kept elsewhere): the owners' send schemes would name
/// erased elements (upstream writes `-1` into them); the hook refuses and
/// the mutation rolls back on every rank (1 rank: no ghost, it commits).
pub fn inconsistent_element_erasure_is_refused(comm: Communicator) -> Outcome {
    let n = 2;
    let part = crate::par_smm::Part::Prescribed(corner_parts(n, comm.size()));
    let shared = SharedMesh::new(crate::par_smm::distributed_cube(&comm, n, &part)?);
    let mut a = model_on(&shared)?;
    ok(a.init_full(&Method::Explicit.options()), "init")?;
    let revision = ok(shared.revision(), "revision")?;
    let nodes = ok(shared.read(), "read")?.nb_nodes();
    let mut hook = DistributedRemovalHook::new();
    let r = a.mutate_mesh(
        |tx| {
            let local: Vec<Element> = corner_elements(tx.mesh(), n)
                .into_iter()
                .filter(|e| e.ghost_type() == GhostType::NotGhost)
                .collect();
            tx.erase_elements(MeshPart::Main, &local)?;
            tx.purify(MeshPart::Main)
        },
        Some(&mut hook as &mut dyn DistributedHook),
    );
    if comm.size() == 1 {
        return ok(r, "one rank: no ghost copy").map(|_| ());
    }
    match &r {
        Ok(_) => return Err("the inconsistent erasure committed".into()),
        Err(e) if e.fatal_kind().is_some() => return Err(format!("fatal: {e}")),
        Err(e)
            if !format!("{e}").contains("distributed element removal")
                && !format!("{e}").contains("another rank") =>
        {
            return Err(format!("unexpected refusal: {e}"));
        }
        Err(_) => {}
    }
    if ok(shared.revision(), "revision")? != revision
        || ok(shared.read(), "read")?.nb_nodes() != nodes
    {
        return Err("the refused erasure changed the mesh".into());
    }
    Ok(())
}

// ---------------------------------- the synchronizers after a removal

/// Element barycentres exchanged through the element synchronizer: the
/// owner packs the barycentre of each element of its send scheme, the
/// receiver compares it bit for bit with the barycentre of the ghost
/// element its receive scheme names.
struct BarycentreCheck<'a> {
    mesh: &'a Mesh,
    unpacked: usize,
    mismatch: Option<String>,
}

impl akantu_comm::DataAccessor<Element> for BarycentreCheck<'_> {
    fn nb_data(&self, els: &[Element], _: akantu_core::enums::SynchronizationTag) -> usize {
        els.len() * self.mesh.spatial_dimension() * std::mem::size_of::<Real>()
    }
    fn pack_data(
        &self,
        b: &mut akantu_comm::buffer::CommunicationBuffer,
        els: &[Element],
        _: akantu_core::enums::SynchronizationTag,
    ) -> akantu_core::Result<()> {
        for &e in els {
            b.pack_slice(&self.mesh.barycenter(e)?);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        b: &mut akantu_comm::buffer::CommunicationBuffer,
        els: &[Element],
        _: akantu_core::enums::SynchronizationTag,
    ) -> akantu_core::Result<()> {
        let mut v = vec![0.0; self.mesh.spatial_dimension()];
        for &e in els {
            b.unpack_slice(&mut v)?;
            let own = self.mesh.barycenter(e)?;
            if bits(&own) != bits(&v) && self.mismatch.is_none() {
                self.mismatch = Some(format!("{e}: received {v:?}, own {own:?}"));
            }
            self.unpacked += 1;
        }
        Ok(())
    }
}

/// Node coordinates and global ids exchanged through the node
/// synchronizer (master → slave), compared bit for bit with the slave's.
struct NodeCheck<'a> {
    mesh: &'a Mesh,
    unpacked: usize,
    mismatch: Option<String>,
}

impl akantu_comm::DataAccessor<Idx> for NodeCheck<'_> {
    fn nb_data(&self, nodes: &[Idx], _: akantu_core::enums::SynchronizationTag) -> usize {
        nodes.len() * (self.mesh.spatial_dimension() * std::mem::size_of::<Real>() + 8)
    }
    fn pack_data(
        &self,
        b: &mut akantu_comm::buffer::CommunicationBuffer,
        nodes: &[Idx],
        _: akantu_core::enums::SynchronizationTag,
    ) -> akantu_core::Result<()> {
        for &n in nodes {
            let u = usize::try_from(n).map_err(|_| akantu_core::Error::invalid("node"))?;
            b.pack_slice(self.mesh.nodes().tuple(u));
            b.pack(&self.mesh.node_global_id(n));
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        b: &mut akantu_comm::buffer::CommunicationBuffer,
        nodes: &[Idx],
        _: akantu_core::enums::SynchronizationTag,
    ) -> akantu_core::Result<()> {
        let mut x = vec![0.0; self.mesh.spatial_dimension()];
        for &n in nodes {
            b.unpack_slice(&mut x)?;
            let g: i64 = b.unpack()?;
            let u = usize::try_from(n).map_err(|_| akantu_core::Error::invalid("node"))?;
            let own = self.mesh.nodes().tuple(u).to_vec();
            if (bits(&own) != bits(&x) || self.mesh.node_global_id(n) != g)
                && self.mismatch.is_none()
            {
                self.mismatch = Some(format!(
                    "node {n}: received {x:?} (global {g}), own {own:?} (global {})",
                    self.mesh.node_global_id(n)
                ));
            }
            self.unpacked += 1;
        }
        Ok(())
    }
}

/// After a removal on a distributed mesh, the element
/// and node synchronizers renumbered by `DistributedRemovalHook` are
/// checked independently of the hook: every ghost element receives its
/// owner's barycentre and every slave node its master's coordinates and
/// global id, bit for bit, and every receive entry is served (schemes
/// matched in size and order on both sides; an entry naming another
/// element or node, or out of range, fails).
pub fn check_synchronizers(mesh: &Mesh, what: &str) -> Outcome {
    let tag = akantu_core::enums::SynchronizationTag::Test;
    // the element check's verdict is returned after the node exchange, which
    // every rank enters (no rank-local return between two exchanges)
    let mut element_verdict = Ok(());
    if let Some(sync) = mesh.synchronizers().element.clone() {
        let expected: usize = sync.communications.recv.values().map(Vec::len).sum();
        let mut acc = BarycentreCheck {
            mesh,
            unpacked: 0,
            mismatch: None,
        };
        ok(sync.synchronize(&mut acc, tag), "element synchronize")?;
        if let Some(m) = acc.mismatch {
            element_verdict = Err(format!("{what}: element synchronizer: {m}"));
        } else if acc.unpacked != expected {
            element_verdict = Err(format!(
                "{what}: element synchronizer: {} of {expected} ghosts received",
                acc.unpacked
            ));
        }
    }
    if let Some(sync) = mesh.synchronizers().node.clone() {
        let expected: usize = sync.communications.recv.values().map(Vec::len).sum();
        let mut acc = NodeCheck {
            mesh,
            unpacked: 0,
            mismatch: None,
        };
        ok(sync.synchronize(&mut acc, tag), "node synchronize")?;
        element_verdict?;
        if let Some(m) = acc.mismatch {
            return Err(format!("{what}: node synchronizer: {m}"));
        }
        if acc.unpacked != expected {
            return Err(format!(
                "{what}: node synchronizer: {} of {expected} slaves received",
                acc.unpacked
            ));
        }
        return Ok(());
    }
    element_verdict
}

// ------------------------- element events composed in one mutation

/// A distributed hook that accepts every event and changes nothing: it
/// lets a mutation of a distributed mesh with an element addition reach
/// the capability stage (3b) in [`added_then_erased_is_refused`] (the
/// refusal there rolls the mutation back, so the synchronizers this hook
/// leaves alone are never used).
struct AcceptingHook;

impl DistributedHook for AcceptingHook {
    fn prepare(
        &mut self,
        _: &Mesh,
        _: &[(MeshPart, akantu_mesh::MeshEvent)],
    ) -> akantu_core::Result<()> {
        Ok(())
    }
    fn apply(
        &mut self,
        _: &mut MeshTransaction<'_>,
        _: &[(MeshPart, akantu_mesh::MeshEvent)],
        _: &mut akantu_comm::phase::Phases,
    ) -> akantu_core::Result<()> {
        Ok(())
    }
}

/// The refusal of a composed mutation on `comm`: recoverable on every rank
/// — the composition rule's `Unsupported` on a rank where
/// the composition is local, a recoverable communication error ("another
/// rank") elsewhere — and at least one rank refusing it itself.
fn agreed_refusal(
    comm: &Communicator,
    r: &akantu_core::Result<akantu_mesh::CommittedMutation>,
) -> Outcome {
    let own = match r {
        Ok(_) => Err("the composed mutation committed".to_owned()),
        Err(e) if e.fatal_kind().is_some() => Err(format!("fatal: {e}")),
        Err(e @ akantu_core::Error::Unsupported(_))
            if format!("{e}").contains("of the same mutation names") =>
        {
            Ok(1u64)
        }
        Err(e @ akantu_core::Error::Communication(_))
            if format!("{e}").contains("another rank") =>
        {
            Ok(0)
        }
        Err(e) => Err(format!("unexpected refusal: {e:?}")),
    };
    let own = {
        let class = own.as_ref().map(|_| ()).map_err(Clone::clone);
        agreed(comm, class)?;
        own?
    };
    let mut c = [own];
    ok(
        comm.all_reduce(&mut c, akantu_comm::ReduceOp::Sum),
        "count refusing ranks",
    )?;
    if c[0] == 0 {
        return Err("no rank refused the composition itself".into());
    }
    Ok(())
}

/// Mesh revision, node count, element counts per block, the model's
/// numbering and displacement bits.
fn mesh_model_state(
    a: &SolidMechanicsModel,
    shared: &SharedMesh,
) -> std::result::Result<String, String> {
    let m = ok(shared.read(), "read")?;
    let counts: Vec<usize> = GhostType::ALL
        .into_iter()
        .map(|g| m.nb_element(ElementType::Tetrahedron4, g))
        .collect();
    Ok(format!(
        "{} {} {counts:?} {:?} {:?}",
        ok(shared.revision(), "revision")?,
        m.nb_nodes(),
        a.solver.dof_manager.layout(),
        bits(ok(a.displacement(), "u")?.as_slice())
    ))
}

/// After a refused composition the domain is usable: the corner erased in
/// one call (and its orphaned nodes removed) commits, the synchronizers
/// pass [`check_synchronizers`] and the model runs.
fn usable_afterwards(
    comm: &Communicator,
    a: &mut SolidMechanicsModel,
    shared: &SharedMesh,
    n: usize,
) -> Outcome {
    let (nodes, holder) = {
        let m = ok(shared.read(), "read")?;
        (m.nb_nodes(), holds_corner(&m, n))
    };
    let mut hook = DistributedRemovalHook::new();
    ok(
        a.mutate_mesh(
            |tx| erase_corner(tx, n),
            Some(&mut hook as &mut dyn DistributedHook),
        ),
        "the corner in one call afterwards",
    )?;
    let after = ok(shared.read(), "read")?.nb_nodes();
    agreed(
        comm,
        removal_as_expected(
            holder,
            nodes,
            after,
            &format!("rank {}: afterwards", comm.rank()),
        ),
    )?;
    agreed(
        comm,
        check_synchronizers(&*ok(shared.read(), "read")?, "afterwards"),
    )?;
    for s in 0..2 {
        ok(a.solve_step(None), &format!("step {s} afterwards"))?;
    }
    if !ok(a.displacement(), "u")?
        .as_slice()
        .iter()
        .all(|u| u.is_finite())
    {
        return Err("non-finite displacement afterwards".into());
    }
    Ok(())
}

/// The explicit model on the corner-dealt cube, two steps run.
fn corner_model(
    comm: &Communicator,
    n: usize,
) -> std::result::Result<(SharedMesh, SolidMechanicsModel), String> {
    let part = crate::par_smm::Part::Prescribed(corner_parts(n, comm.size()));
    let shared = SharedMesh::new(crate::par_smm::distributed_cube(comm, n, &part)?);
    let mut a = model_on(&shared)?;
    let method = Method::Explicit;
    ok(a.init_full(&method.options()), "init")?;
    set_up(&mut a, method, &[])?;
    let dt = time_step(&mut a, method)?;
    ok(a.set_time_step(dt, None), "dt")?;
    for s in 0..2 {
        ok(a.solve_step(None), &format!("step {s}"))?;
    }
    Ok((shared, a))
}

/// The bits of an element's barycentre (identical on every rank holding
/// the element: same coordinates, same computation).
fn barycentre_key(m: &Mesh, e: Element) -> akantu_core::Result<Vec<u64>> {
    Ok(bits(&m.barycenter(e)?))
}

/// On any number of ranks: the corner cell erased in
/// **two** calls of one mutation — every corner tetrahedron but one (with
/// its copies on every rank), then the remaining one (the corner element
/// of largest barycentre, the same on every rank holding the corner, since
/// each of them holds the six): the hook follows both erasures, and on at least one rank both
/// calls renumber the same block (the remaining tetrahedron's owner holds
/// other corner tetrahedra, or a rank holds it as a ghost with other
/// ghosts). The composition is refused in the capability stage on that
/// rank, agreed (recoverable) on every rank, rolled back — mesh and model
/// unchanged — and the domain stays usable. From 7 ranks on the ranks
/// without the corner node hold none of the six and erase nothing (empty
/// events).
pub fn two_erasures_of_a_block_are_refused(comm: Communicator) -> Outcome {
    let n = 2;
    let (shared, mut a) = corner_model(&comm, n)?;
    let (held, holder) = {
        let m = ok(shared.read(), "read")?;
        (corner_elements(&m, n).len(), holds_corner(&m, n))
    };
    let want = if holder { 6 } else { 0 };
    agreed(
        &comm,
        if held == want {
            Ok(())
        } else {
            Err(format!(
                "rank {} holds {held} of the 6 corner tetrahedra (expected {want})",
                comm.rank()
            ))
        },
    )?;
    let before = mesh_model_state(&a, &shared)?;
    let mut hook = DistributedRemovalHook::new();
    let r = a.mutate_mesh(
        |tx| {
            let corner = corner_elements(tx.mesh(), n);
            let mut keys = Vec::with_capacity(corner.len());
            for &e in &corner {
                keys.push(barycentre_key(tx.mesh(), e)?);
            }
            let last = keys.iter().max().cloned();
            let rest: Vec<Element> = corner
                .iter()
                .zip(&keys)
                .filter(|(_, k)| Some(*k) != last.as_ref())
                .map(|(e, _)| *e)
                .collect();
            tx.erase_elements(MeshPart::Main, &rest)?;
            // the remaining corner element, in the numbering after the
            // first erasure
            let remaining = corner_elements(tx.mesh(), n);
            tx.erase_elements(MeshPart::Main, &remaining)
        },
        Some(&mut hook as &mut dyn DistributedHook),
    );
    agreed_refusal(&comm, &r)?;
    let unchanged = if mesh_model_state(&a, &shared)? == before {
        Ok(())
    } else {
        Err("the refused mutation changed the mesh or the model".into())
    };
    agreed(&comm, unchanged)?;
    usable_afterwards(&comm, &mut a, &shared, n)
}

/// On any number of ranks: on rank 0 a tetrahedron
/// is added and erased again in one mutation (the other ranks record the
/// same event kinds, empty). `DistributedRemovalHook` refuses additions, so
/// an accepting hook ([`AcceptingHook`]) lets the mutation reach the
/// capability stage: the composition is refused there on rank 0, agreed on
/// every rank (the others return a recoverable communication error),
/// rolled back, and the domain stays usable.
pub fn added_then_erased_is_refused(comm: Communicator) -> Outcome {
    use akantu_mesh::MeshEvent;
    let n = 2;
    let (shared, mut a) = corner_model(&comm, n)?;
    let before = mesh_model_state(&a, &shared)?;
    let composer = comm.rank() == 0;
    let mut hook = AcceptingHook;
    let r = a.mutate_mesh(
        |tx| {
            let ty = ElementType::Tetrahedron4;
            let mut added = Vec::new();
            if composer {
                let conn = tx
                    .mesh()
                    .connectivity(ty, GhostType::NotGhost)?
                    .tuple(0)
                    .to_vec();
                added.push(tx.push_element(MeshPart::Main, ty, GhostType::NotGhost, &conn)?);
            }
            tx.record(
                MeshPart::Main,
                MeshEvent::ElementsAdded {
                    elements: added.clone(),
                    constitutive_law: None,
                    origin: "v134".into(),
                    extension: None,
                },
            );
            tx.erase_elements(MeshPart::Main, &added)
        },
        Some(&mut hook as &mut dyn DistributedHook),
    );
    agreed_refusal(&comm, &r)?;
    let unchanged = if mesh_model_state(&a, &shared)? == before {
        Ok(())
    } else {
        Err("the refused mutation changed the mesh or the model".into())
    };
    agreed(&comm, unchanged)?;
    usable_afterwards(&comm, &mut a, &shared, n)
}

/// With elements that **move**: on the corner-dealt cube
/// the first cell (corner `(0, 0, 0)`, its tetrahedra the first elements of
/// their owner's block and ghosts of the neighbouring ranks) is erased on
/// every rank holding it and its orphaned node removed (`purify`), through
/// `DistributedRemovalHook` without consumers. The holes are filled with
/// the last elements (on at least one rank a kept element changes its
/// index), and [`check_synchronizers`] must pass on the result: the
/// element schemes were filtered **and** renumbered.
pub fn synchronizers_after_a_middle_erasure(comm: Communicator) -> Outcome {
    let n = 2;
    let part = crate::par_smm::Part::Prescribed(corner_parts(n, comm.size()));
    let shared = SharedMesh::new(crate::par_smm::distributed_cube(&comm, n, &part)?);
    let lim = 1.0 / n as Real;
    let mut hook = DistributedRemovalHook::new();
    let committed = ok(
        shared.mutate(
            &mut [],
            |tx| {
                let mut first = Vec::new();
                for ghost in GhostType::ALL {
                    let ty = ElementType::Tetrahedron4;
                    for e in 0..tx.mesh().nb_element(ty, ghost) {
                        let el = Element::new(ty, e as Idx, ghost);
                        if tx.mesh().barycenter(el)?.iter().all(|&x| x < lim) {
                            first.push(el);
                        }
                    }
                }
                tx.erase_elements(MeshPart::Main, &first)?;
                tx.purify(MeshPart::Main)
            },
            Some(&mut hook as &mut dyn DistributedHook),
        ),
        "erase the first cell",
    )?;
    let moved = committed.events.iter().any(|(p, e)| match e {
        akantu_mesh::MeshEvent::ElementsRemoved { new_numbering, .. } if *p == MeshPart::Main => {
            new_numbering.as_map().iter_all().any(|(_, _, num)| {
                num.as_slice()
                    .iter()
                    .enumerate()
                    .any(|(i, &k)| k >= 0 && k as usize != i)
            })
        }
        _ => false,
    });
    let mut c = [u64::from(moved)];
    ok(
        comm.all_reduce(&mut c, akantu_comm::ReduceOp::Sum),
        "count ranks with moved elements",
    )?;
    if c[0] == 0 {
        return Err("no kept element moved on any rank (the scenario renumbers nothing)".into());
    }
    check_synchronizers(
        &*ok(shared.read(), "read")?,
        &format!("rank {}: after the middle erasure", comm.rank()),
    )
}
