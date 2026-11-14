// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Diagnostics of the
// port's driver of Akantu's JOSS scalability test (performance-testing-cohesive
// @ 483bace8, cohesive_extrinsic.cc); no upstream counterpart. The insertion
// criterion re-evaluated by `facet_margins` follows Akantu's
// material_cohesive_linear.cc (`checkInsertion`, `computeEffectiveNorm`), the
// branch flags of `cohesive_quads` its material_cohesive_linear_inline_impl.hh
// (`computeTractionOnQuad`).

//! Diagnostics of the cohesive driver: **input-gated, off
//! by default**; none of them changes the computation of a run that does not
//! set them (the static import replaces the static solution, by design).
//!
//! User-section parameters (all optional):
//!
//! * `rust_diag_static_out = DIR`: after the static solve, every rank writes
//!   the displacement of its owned nodes (local or master) by global node id
//!   to `DIR/static_u.r<rank>.txt` (exact: the bits of every component in
//!   hexadecimal).
//! * `rust_diag_static_in = DIR`: after the static solve, the displacement
//!   of every node of the rank (ghost and slave nodes included) is replaced
//!   by the one read from all `DIR/static_u.r*.txt` files (any rank count),
//!   then the tail of the linear static solve is repeated on the imposed
//!   displacement (upstream `NonLinearSolverLinear::solve` after
//!   `solve(J)`: `corrector`, the internal residual — stresses and internal
//!   forces —, `afterSolveStep(true)`, i.e. the Cauchy stresses of the
//!   finite-deformation laws that the insertion check reads); the
//!   displacement increment of the solve is replaced as well (the static
//!   solve starts from `u = 0`, so its increment is `u`). On the run that
//!   wrote the files this is a recomputation of identical values (checked:
//!   the serial run with its own solution imposed is bitwise equal to the
//!   plain serial run).
//! * `rust_diag_dir = DIR`: decision logs, one file per rank and kind,
//!   from step `rust_diag_from` (default 1) on:
//!   - `facets.r<rank>.csv`: per step, every not-ghost checked facet the
//!     law flagged (`ins`) and every one within the relative band
//!     `rust_diag_near` (default 1e-6) below its strength (`near`), with the
//!     effective stress and the strength (re-evaluated from the model's
//!     facet stresses, normals and tangents as the law does; a facet whose
//!     re-evaluated decision disagrees with the law's flag is logged as
//!     `mismatch`);
//!   - `quads.r<rank>.csv`: per step, every not-ghost cohesive quadrature
//!     point that is new or whose branch flags of the linear law changed
//!     (`pen`: penetration, `d0`: damage ≈ 0, `d1`: damage ≈ 1, with
//!     upstream's tolerance), with the normal opening, `δc,eff`, damage and
//!     `δmax`;
//!   - `nodes.r<rank>.csv`: per insertion step, every owned copy of every
//!     node position that received a new node on the rank (original copies
//!     included), with its side (below), whether it is a new node (global
//!     id ≥ the initial global node count), its number of attached
//!     tetrahedra on the rank, `u`, `v`, the external force and the blocked
//!     flags;
//!   - `cohesive.r<rank>.csv`: per insertion step, every new not-ghost
//!     cohesive element with the side signature of its two facets (wrapping
//!     sum of the side keys of the node copies of facet 1, of facet 2:
//!     which side is which) and whether each facet's bulk neighbour is a
//!     ghost on the rank (a facet on the partition interface);
//!   - `rust_diag_node_steps = "a:b[:s],c,…"`: at those steps (after the
//!     explicit solve) every owned node with `u`, `v`, the external and the
//!     internal force, to `nodes_s<step>.r<rank>.csv`.
//!
//! * `rust_diag_reapply_neumann = true` (driver, not a log): after every
//!   insertion that added nodes on the rank, the external force is zeroed
//!   and every Neumann load applied so far (compression, shear, the shear
//!   increments) is applied again on the current topology, so each copy of
//!   a doubled node carries the load of its own boundary elements; on the
//!   nodes whose boundary elements did not change this repeats the same
//!   additions in the same order (bitwise identical).
//!
//! Keys (identical in every run and on every rank): a node is its reference
//! position (the bits of `X`) and its **side**, the wrapping sum of a 64-bit
//! hash of every attached regular element (local and ghost) — each element
//! identified by the sorted reference positions of its nodes —, so the two
//! copies of a doubled node get different keys and a node keeps its key on
//! every partition. A facet or a cohesive element is the hash of the sorted
//! reference positions of its (first) three nodes.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_cohesive::laws::CohesiveLaw;
use akantu_cohesive::model::CohesiveCallback;
use akantu_cohesive::vecops::{dot_seq, std_max};
use akantu_comm::Communicator;
use akantu_core::math::are_float_equal;
use akantu_core::{ElementKind, ElementType, Error, GhostType, GlobalIdx, Idx, Real, Result};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_solid::Material as _;
use akantu_solver::callback::SolverCallback as _;

const DIM: usize = 3;
/// The cohesive law of the benchmark input (`material cohesive_linear`).
const LAW: &str = "insertion";

/// Diagnostic switches (see the module documentation).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiagConfig {
    /// `rust_diag_static_out`.
    pub static_out: Option<PathBuf>,
    /// `rust_diag_static_in`.
    pub static_in: Option<PathBuf>,
    /// `rust_diag_dir`.
    pub dir: Option<PathBuf>,
    /// `rust_diag_from` (first logged step).
    pub log_from: usize,
    /// `rust_diag_node_steps` (sorted, unique).
    pub node_steps: Vec<usize>,
    /// `rust_diag_near` (relative band below the strength).
    pub near: Real,
    /// `rust_diag_reapply_neumann`: after every insertion that added nodes
    /// on the rank, the external force is rebuilt from the loads applied so
    /// far on the current boundary topology (driver; a candidate fix tried
    /// in the doubled-node experiment, not upstream's problem).
    pub reapply_neumann: bool,
}

impl DiagConfig {
    /// Any diagnostic requested.
    #[must_use]
    pub fn any(&self) -> bool {
        self.static_out.is_some()
            || self.static_in.is_some()
            || self.dir.is_some()
            || self.reapply_neumann
    }
}

/// Parse `rust_diag_node_steps`: comma-separated steps or ranges
/// `a:b[:s]` (inclusive, stride `s`, default 1).
///
/// # Errors
/// Malformed entries, zero stride, `a > b`.
pub fn parse_steps(v: &str) -> Result<Vec<usize>> {
    parse_steps_for("rust_diag_node_steps", v)
}

/// [`parse_steps`] for the input key `key` (named in the error message).
///
/// # Errors
/// A malformed list.
pub fn parse_steps_for(key: &str, v: &str) -> Result<Vec<usize>> {
    let bad = |what: &str| {
        Error::invalid(format!(
            "{key}: {what} in \"{v}\" (expected steps or ranges a:b[:s], comma-separated)"
        ))
    };
    let num = |s: &str| s.trim().parse::<usize>().map_err(|_| bad("not a step"));
    let mut out = Vec::new();
    for part in v.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let f: Vec<&str> = part.split(':').collect();
        match f.len() {
            1 => out.push(num(f[0])?),
            2 | 3 => {
                let (a, b) = (num(f[0])?, num(f[1])?);
                let s = if f.len() == 3 { num(f[2])? } else { 1 };
                if s == 0 || a > b {
                    return Err(bad("empty range"));
                }
                out.extend((a..=b).step_by(s));
            }
            _ => return Err(bad("too many ':'")),
        }
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

// ---- keys ---------------------------------------------------------------------

/// SplitMix64 finaliser (a bijection on `u64`).
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Order-dependent hash of a word sequence.
fn hash_words(words: impl IntoIterator<Item = u64>) -> u64 {
    let mut h: u64 = 0x9e37_79b9_7f4a_7c15;
    for w in words {
        h = mix(h ^ mix(w));
    }
    h
}

fn pos_bits(x: &[Real], n: usize) -> [u64; 3] {
    [
        x[n * DIM].to_bits(),
        x[n * DIM + 1].to_bits(),
        x[n * DIM + 2].to_bits(),
    ]
}

/// Hash of the sorted reference positions of `nodes`.
fn geometry_key(x: &[Real], nodes: &[Idx]) -> Result<u64> {
    let mut p: Vec<[u64; 3]> = nodes
        .iter()
        .map(|&n| akantu_core::try_to_usize(n).map(|n| pos_bits(x, n)))
        .collect::<Result<_>>()?;
    p.sort_unstable();
    Ok(hash_words(p.into_iter().flatten()))
}

fn hex3(v: &[Real]) -> String {
    format!(
        "{:016x},{:016x},{:016x}",
        v[0].to_bits(),
        v[1].to_bits(),
        v[2].to_bits()
    )
}

fn io_err(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.display().to_string(),
        source,
    }
}

fn create(path: &Path) -> Result<BufWriter<File>> {
    Ok(BufWriter::new(
        File::create(path).map_err(|e| io_err(path, e))?,
    ))
}

/// Side key of every node of `m` (the wrapping sum of the hashes of its
/// attached regular elements, local and ghost; see the module
/// documentation) and the bits of its reference position. For tests and
/// one-off checks (the decision logs cache the element hashes).
///
/// # Errors
/// Mesh errors.
pub fn node_keys(m: &Mesh) -> Result<Vec<([u64; 3], u64)>> {
    let x = m.nodes();
    let x = x.as_slice();
    let nb = m.nb_nodes();
    let mut side = vec![0_u64; nb];
    for g in GhostType::ALL {
        for ty in regular_types(m, g) {
            let conn = m.connectivity(ty, g)?;
            let npe = conn.nb_component();
            for nodes in conn.as_slice().chunks_exact(npe) {
                let h = geometry_key(x, nodes)?;
                for &n in nodes {
                    let n = akantu_core::try_to_usize(n)?;
                    side[n] = side[n].wrapping_add(h);
                }
            }
        }
    }
    Ok((0..nb).map(|n| (pos_bits(x, n), side[n])).collect())
}

// ---- E1: static solution export / import ---------------------------------------

/// Write the displacement of this rank's owned nodes by global id
/// (`rust_diag_static_out`).
///
/// # Errors
/// I/O, mesh or model errors.
pub fn write_static(
    dir: &Path,
    comm: &Communicator,
    shared: &SharedMesh,
    model: &mut SolidMechanicsModelCohesive,
) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    let m = shared.read()?;
    let u = model.smm.displacement()?;
    let u = u.as_slice();
    let mut s = format!(
        "# akantu-rs cohesive driver diagnostics: static solution: rank {} of {}, {} global nodes; \
         global_id ux uy uz (f64 bits)\n",
        comm.rank(),
        comm.size(),
        m.nb_global_nodes()
    );
    let mut rows: Vec<(GlobalIdx, usize)> = Vec::new();
    for n in 0..m.nb_nodes() {
        let ni = Idx::try_from(n).map_err(|_| Error::invalid("node index overflow"))?;
        if m.is_local_or_master_node(ni)? {
            rows.push((m.node_global_id(ni), n));
        }
    }
    rows.sort_unstable();
    for (gid, n) in rows {
        let _ = writeln!(
            s,
            "{gid} {:016x} {:016x} {:016x}",
            u[n * DIM].to_bits(),
            u[n * DIM + 1].to_bits(),
            u[n * DIM + 2].to_bits()
        );
    }
    let path = dir.join(format!("static_u.r{}.txt", comm.rank()));
    std::fs::write(&path, s).map_err(|e| io_err(&path, e))
}

/// Read every `static_u.r*.txt` of `dir` (any rank count).
fn read_static(dir: &Path) -> Result<HashMap<GlobalIdx, [u64; 3]>> {
    let mut map = HashMap::new();
    let mut files = 0;
    for entry in std::fs::read_dir(dir).map_err(|e| io_err(dir, e))? {
        let path = entry.map_err(|e| io_err(dir, e))?.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        if !(name.starts_with("static_u.r") && name.ends_with(".txt")) {
            continue;
        }
        files += 1;
        let text = std::fs::read_to_string(&path).map_err(|e| io_err(&path, e))?;
        for (i, line) in text.lines().enumerate() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let bad = || Error::invalid(format!("{}:{}: malformed line", path.display(), i + 1));
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() != 4 {
                return Err(bad());
            }
            let gid: GlobalIdx = f[0].parse().map_err(|_| bad())?;
            let mut v = [0_u64; 3];
            for d in 0..3 {
                v[d] = u64::from_str_radix(f[d + 1], 16).map_err(|_| bad())?;
            }
            if map.insert(gid, v).is_some_and(|old| old != v) {
                return Err(Error::invalid(format!(
                    "{}: node {gid} listed twice with different values",
                    dir.display()
                )));
            }
        }
    }
    if files == 0 {
        return Err(Error::invalid(format!(
            "rust_diag_static_in: no static_u.r*.txt in {}",
            dir.display()
        )));
    }
    Ok(map)
}

/// Impose the static solution read from `dir` (`rust_diag_static_in`) and
/// repeat the tail of the linear static solve on it. Collective.
///
/// # Errors
/// Missing nodes, I/O, model errors.
pub fn impose_static(
    dir: &Path,
    shared: &SharedMesh,
    model: &mut SolidMechanicsModelCohesive,
) -> Result<usize> {
    let map = read_static(dir)?;
    let values: Vec<Real> = {
        let m = shared.read()?;
        if map.len() != m.nb_global_nodes() {
            return Err(Error::invalid(format!(
                "rust_diag_static_in: {} nodes in {}, the mesh has {} global nodes",
                map.len(),
                dir.display(),
                m.nb_global_nodes()
            )));
        }
        let mut v = Vec::with_capacity(m.nb_nodes() * DIM);
        for n in 0..m.nb_nodes() {
            let ni = Idx::try_from(n).map_err(|_| Error::invalid("node index overflow"))?;
            let gid = m.node_global_id(ni);
            let b = map.get(&gid).ok_or_else(|| {
                Error::invalid(format!("rust_diag_static_in: node {gid} missing"))
            })?;
            v.extend(b.iter().map(|&x| Real::from_bits(x)));
        }
        v
    };
    model
        .smm
        .displacement_mut()?
        .as_mut_slice()
        .copy_from_slice(&values);
    // the static solve starts from u = 0: its increment is u
    if let Ok(inc) = model.smm.nodal_field_shared_mut("increment") {
        let mut inc = inc.write()?;
        if inc.as_slice().len() == values.len() {
            inc.as_mut_slice().copy_from_slice(&values);
        }
    }
    // the tail of upstream's NonLinearSolverLinear::solve after solve(J)
    let SolidMechanicsModelCohesive { smm, ext, .. } = model;
    let sync = ext.parallel.as_ref().map(|p| &p.cohesive);
    let physics = &mut smm.physics;
    smm.solver
        .dof_manager
        .collective("c1exp:impose_static", |d| {
            let mut cb = CohesiveCallback::with_synchronizer(physics, sync);
            d.step("corrector", |d| cb.corrector(d))?;
            d.step("assemble_residual:internal", |d| {
                cb.assemble_residual_part(d, "internal")
            })?;
            d.step("after_solve_step", |d| cb.after_solve_step(d, true))
        })?;
    Ok(values.len() / DIM)
}

// ---- E2: decision logs ------------------------------------------------------------

/// Branch flags of a cohesive quadrature point.
const PEN: u8 = 1;
const D0: u8 = 2;
const D1: u8 = 4;

/// The decision logger of one rank.
#[derive(Debug)]
pub struct DecisionLog {
    cfg: DiagConfig,
    dir: PathBuf,
    rank: usize,
    /// Initial global node count: a node with a larger or equal global id
    /// is a doubled copy.
    n0_global: GlobalIdx,
    /// Hash of every regular 3-D element (not ghost, ghost), by index.
    elem_hash: [Vec<u64>; 2],
    /// Branch flags per (cohesive element key, quadrature point).
    flags: HashMap<(u64, usize), u8>,
    facets: BufWriter<File>,
    quads: BufWriter<File>,
    nodes: BufWriter<File>,
    cohesive: BufWriter<File>,
    nb_nodes_before: usize,
    nb_cohesive_before: usize,
    /// Facets the re-evaluated criterion disagreed with (total).
    pub mismatches: usize,
}

fn ghost_slot(g: GhostType) -> usize {
    usize::from(g == GhostType::Ghost)
}

fn regular_types(m: &Mesh, g: GhostType) -> Vec<ElementType> {
    m.element_types(akantu_core::TypeFilter {
        spatial_dimension: Some(DIM),
        ghost_type: g,
        kind: Some(ElementKind::Regular),
    })
    .collect()
}

impl DecisionLog {
    /// Open the log files of this rank and hash the regular elements.
    ///
    /// # Errors
    /// I/O or mesh errors; more than one regular element type.
    pub fn new(
        cfg: &DiagConfig,
        dir: &Path,
        comm: &Communicator,
        shared: &SharedMesh,
    ) -> Result<Self> {
        std::fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
        let rank = usize::try_from(comm.rank()).map_err(|_| Error::invalid("negative rank"))?;
        let m = shared.read()?;
        let xg = m.nodes();
        let x = xg.as_slice();
        let mut elem_hash: [Vec<u64>; 2] = Default::default();
        for g in GhostType::ALL {
            let types = regular_types(&m, g);
            if types.len() > 1 {
                return Err(Error::Unsupported(
                    "cohesive driver diagnostics: one regular element type expected".into(),
                ));
            }
            for ty in types {
                let conn = m.connectivity(ty, g)?;
                let npe = conn.nb_component();
                let conn = conn.as_slice();
                let mut h = Vec::with_capacity(conn.len() / npe);
                for e in conn.chunks_exact(npe) {
                    h.push(geometry_key(x, e)?);
                }
                elem_hash[ghost_slot(g)] = h;
            }
        }
        let n0_global = GlobalIdx::try_from(m.nb_global_nodes())
            .map_err(|_| Error::invalid("node count overflow"))?;
        drop(xg);
        drop(m);
        let open = |kind: &str, header: &str| -> Result<BufWriter<File>> {
            let mut f = create(&dir.join(format!("{kind}.r{rank}.csv")))?;
            writeln!(f, "{header}").map_err(|e| io_err(dir, e))?;
            Ok(f)
        };
        Ok(Self {
            cfg: cfg.clone(),
            dir: dir.to_path_buf(),
            rank,
            n0_global,
            elem_hash,
            flags: HashMap::new(),
            facets: open(
                "facets",
                "step,kind,facet,bx,by,bz,stress,limit,stress_bits,limit_bits",
            )?,
            quads: open(
                "quads",
                "step,elem,q,new,pen,d0,d1,bx,by,bz,normal_opening,delta_c,damage,delta_max,\
                 normal_opening_bits,delta_c_bits,damage_bits,delta_max_bits",
            )?,
            nodes: open(
                "nodes",
                "step,x_bits,y_bits,z_bits,side,is_new,nelem,x,y,z,ux,uy,uz,vx,vy,vz,fx,fy,fz,\
                 bx,by,bz,fx_dec,fy_dec,fz_dec",
            )?,
            cohesive: open("cohesive", "step,elem,bx,by,bz,sig1,sig2,ghost1,ghost2")?,
            nb_nodes_before: 0,
            nb_cohesive_before: 0,
            mismatches: 0,
        })
    }

    fn logging(&self, step: usize) -> bool {
        step >= self.cfg.log_from
    }

    /// Side key of every node of the rank (wrapping sum of the hashes of the
    /// attached regular elements, local and ghost) and their counts.
    fn sides(&self, m: &Mesh) -> Result<(Vec<u64>, Vec<u32>)> {
        let nb = m.nb_nodes();
        let mut side = vec![0_u64; nb];
        let mut count = vec![0_u32; nb];
        for g in GhostType::ALL {
            for ty in regular_types(m, g) {
                let conn = m.connectivity(ty, g)?;
                let npe = conn.nb_component();
                let hashes = &self.elem_hash[ghost_slot(g)];
                for (e, nodes) in conn.as_slice().chunks_exact(npe).enumerate() {
                    let h = *hashes.get(e).ok_or_else(|| {
                        Error::invalid("cohesive driver diagnostics: regular elements added")
                    })?;
                    for &n in nodes {
                        let n = akantu_core::try_to_usize(n)?;
                        side[n] = side[n].wrapping_add(h);
                        count[n] += 1;
                    }
                }
            }
        }
        Ok((side, count))
    }

    /// Before `check_cohesive_stress`.
    ///
    /// # Errors
    /// Mesh lock errors.
    pub fn before_check(&mut self, shared: &SharedMesh) -> Result<()> {
        let m = shared.read()?;
        self.nb_nodes_before = m.nb_nodes();
        self.nb_cohesive_before = m.nb_element(ElementType::Cohesive3d6, GhostType::NotGhost);
        Ok(())
    }

    /// The new not-ghost cohesive elements of the step: side signatures of
    /// their two facets and whether each facet's bulk neighbour is a ghost.
    fn new_cohesive(&mut self, step: usize, m: &Mesh, side: &[u64]) -> Result<()> {
        let ty = ElementType::Cohesive3d6;
        let g = GhostType::NotGhost;
        let nb = m.nb_element(ty, g);
        if nb <= self.nb_cohesive_before {
            return Ok(());
        }
        let fm = m
            .mesh_facets()
            .ok_or_else(|| Error::invalid("no facet mesh"))?;
        let e2s = fm
            .data()
            .element_lists(akantu_mesh::utils::ELEMENT_TO_SUBELEMENT)?;
        let s2e = fm
            .data()
            .element_map(akantu_mesh::utils::SUBELEMENT_TO_ELEMENT)?;
        let x = m.nodes();
        let x = x.as_slice();
        let conn = m.connectivity(ty, g)?;
        let npe = conn.nb_component();
        let conn = conn.as_slice();
        let mut out = String::new();
        for c in self.nb_cohesive_before..nb {
            let nodes = &conn[c * npe..(c + 1) * npe];
            let half = npe / 2;
            let key = geometry_key(x, &nodes[..3])?;
            let sig = |part: &[Idx]| -> Result<u64> {
                let mut h = 0_u64;
                for &n in part {
                    h = h.wrapping_add(side[akantu_core::try_to_usize(n)?]);
                }
                Ok(h)
            };
            let (sig1, sig2) = (sig(&nodes[..half])?, sig(&nodes[half..])?);
            let el = akantu_core::Element::new(
                ty,
                Idx::try_from(c).map_err(|_| Error::invalid("element index overflow"))?,
                g,
            );
            let mut ghosts = [2_u8; 2];
            for (k, facet) in s2e.element(el)?.iter().take(2).enumerate() {
                if facet.is_null() {
                    continue;
                }
                if let Some(bulk) = e2s
                    .element(*facet)?
                    .first()
                    .and_then(|row| row.iter().find(|e| e.kind() == ElementKind::Regular))
                {
                    ghosts[k] = u8::from(bulk.ghost_type() == GhostType::Ghost);
                }
            }
            let mut b = [0.0; 3];
            for &n in &nodes[..3] {
                let n = akantu_core::try_to_usize(n)?;
                for d in 0..DIM {
                    b[d] += x[n * DIM + d] / 3.0;
                }
            }
            let _ = writeln!(
                out,
                "{step},{key:016x},{:.10},{:.10},{:.10},{sig1:016x},{sig2:016x},{},{}",
                b[0], b[1], b[2], ghosts[0], ghosts[1]
            );
        }
        self.cohesive
            .write_all(out.as_bytes())
            .map_err(|e| io_err(&self.dir, e))
    }

    /// Between `check_cohesive_stress_prepare` and `_finish`: the effective
    /// stress of every not-ghost checked facet of the law, re-evaluated as
    /// the law does, against the law's flags.
    ///
    /// # Errors
    /// Model or mesh errors.
    #[allow(clippy::too_many_lines)] // one pass mirroring the law's check
    pub fn facet_margins(
        &mut self,
        step: usize,
        shared: &SharedMesh,
        model: &SolidMechanicsModelCohesive,
    ) -> Result<()> {
        if !self.logging(step) {
            return Ok(());
        }
        let g = GhostType::NotGhost;
        let mat = model.cohesive_material(LAW)?;
        let CohesiveLaw::Linear(lin) = &mat.law else {
            return Err(Error::Unsupported(
                "cohesive driver diagnostics: the cohesive law must be cohesive_linear".into(),
            ));
        };
        if lin.max_quad_stress_insertion {
            return Err(Error::Unsupported(
                "cohesive driver diagnostics: max_quad_stress_insertion".into(),
            ));
        }
        let beta2_inv = if are_float_equal(lin.beta, 0.0, lin.tol) {
            0.0
        } else {
            1. / lin.beta / lin.beta
        };
        let core = &mat.core;
        let m = shared.read()?;
        let fm = m
            .mesh_facets()
            .ok_or_else(|| Error::invalid("no facet mesh"))?;
        let x = m.nodes();
        let x = x.as_slice();
        let types: Vec<ElementType> = fm
            .element_types(akantu_core::TypeFilter {
                spatial_dimension: Some(DIM - 1),
                ghost_type: g,
                kind: Some(ElementKind::Regular),
            })
            .collect();
        let near = self.cfg.near;
        let mut out = String::new();
        for ty in types {
            let check = model.ext.inserter.check_facets().get(ty, g)?.as_slice();
            let flagged = model.ext.inserter.insertion_facets().get(ty, g)?.as_slice();
            let fs = model.ext.facet_stress.get(ty, g)?;
            let fs = fs.as_slice();
            let tangents = model.ext.tangents.get(ty, g)?;
            let tangents = tangents.as_slice();
            let normals = model.ext.facet_fe.normals_on_integration_points(ty, g)?;
            let normals = normals.as_slice();
            // the law's count (the normals keep their size at insertion, the
            // facet mesh grows)
            let nq = akantu_cohesive::material::nb_quad_facet(ty)?;
            let filter: &[Idx] = match core.facet_filter.get(ty, g) {
                Ok(a) => a.as_slice(),
                Err(_) => &[],
            };
            let limits = core.sigma_c.get(ty, g)?;
            let limits = limits.as_slice();
            let zipped = core.compat.sigma_c_zipped_per_quad;
            let conn = fm.connectivity(ty, g)?;
            let npe = conn.nb_component();
            let conn = conn.as_slice();
            let ntc = DIM * (DIM - 1);
            let nb_pairs = if zipped {
                filter.len().min(limits.len())
            } else {
                filter.len()
            };
            let mut stress_check = vec![0.0; nq];
            let mut nt = [0.0; 3];
            for (k, &facet) in filter.iter().enumerate().take(nb_pairs) {
                let facet = akantu_core::try_to_usize(facet)?;
                if !check.get(facet).copied().unwrap_or(false) {
                    continue;
                }
                for (q, sc) in stress_check.iter_mut().enumerate() {
                    let cq = facet * nq + q;
                    let s = &fs[cq * 2 * DIM * DIM..(cq + 1) * 2 * DIM * DIM];
                    *sc = effective_norm(
                        &s[..DIM * DIM],
                        &s[DIM * DIM..],
                        &normals[cq * DIM..(cq + 1) * DIM],
                        &tangents[cq * ntc..(cq + 1) * ntc],
                        beta2_inv,
                        &mut nt,
                    );
                }
                let mut stress = stress_check[0];
                for v in &stress_check[1..] {
                    stress += v;
                }
                stress /= nq as Real;
                let limit = if zipped {
                    limits[k]
                } else {
                    let lq = &limits[k * nq..(k + 1) * nq];
                    let mut s = lq[0];
                    for v in &lq[1..] {
                        s += v;
                    }
                    s / nq as Real
                };
                let is_flagged = flagged.get(facet).copied().unwrap_or(false);
                let decision = stress > limit;
                let kind = if decision != is_flagged {
                    self.mismatches += 1;
                    "mismatch"
                } else if decision {
                    "ins"
                } else if stress > limit * (1.0 - near) {
                    "near"
                } else {
                    continue;
                };
                let nodes = &conn[facet * npe..(facet + 1) * npe];
                let key = geometry_key(x, &nodes[..3])?;
                let mut b = [0.0; 3];
                for &n in &nodes[..3] {
                    let n = akantu_core::try_to_usize(n)?;
                    for d in 0..DIM {
                        b[d] += x[n * DIM + d] / 3.0;
                    }
                }
                let _ = writeln!(
                    out,
                    "{step},{kind},{key:016x},{:.10},{:.10},{:.10},{:.17e},{:.17e},{:016x},{:016x}",
                    b[0],
                    b[1],
                    b[2],
                    stress,
                    limit,
                    stress.to_bits(),
                    limit.to_bits()
                );
            }
        }
        self.facets
            .write_all(out.as_bytes())
            .map_err(|e| io_err(&self.dir, e))
    }

    /// After the insertion of the step: every owned copy of every position
    /// that received a new node on this rank.
    ///
    /// # Errors
    /// Model or mesh errors.
    pub fn new_nodes(
        &mut self,
        step: usize,
        shared: &SharedMesh,
        model: &mut SolidMechanicsModelCohesive,
    ) -> Result<()> {
        if !self.logging(step) {
            return Ok(());
        }
        let m = shared.read()?;
        let nb = m.nb_nodes();
        let grew =
            m.nb_element(ElementType::Cohesive3d6, GhostType::NotGhost) > self.nb_cohesive_before;
        if nb <= self.nb_nodes_before && !grew {
            return Ok(());
        }
        let (side, count) = self.sides(&m)?;
        self.new_cohesive(step, &m, &side)?;
        if nb <= self.nb_nodes_before {
            return Ok(());
        }
        let x = m.nodes();
        let x = x.as_slice();
        let positions: HashSet<[u64; 3]> =
            (self.nb_nodes_before..nb).map(|n| pos_bits(x, n)).collect();
        let u = model.smm.displacement()?.as_slice().to_vec();
        let v = model.smm.velocity()?.as_slice().to_vec();
        let f = model.smm.external_force()?.as_slice().to_vec();
        let blocked = model.smm.blocked_dofs()?.as_slice().to_vec();
        let mut out = String::new();
        for n in 0..nb {
            let p = pos_bits(x, n);
            if !positions.contains(&p) {
                continue;
            }
            let ni = Idx::try_from(n).map_err(|_| Error::invalid("node index overflow"))?;
            if !m.is_local_or_master_node(ni)? {
                continue;
            }
            let is_new = u8::from(m.node_global_id(ni) >= self.n0_global);
            let r = n * DIM..(n + 1) * DIM;
            let _ = writeln!(
                out,
                "{step},{:016x},{:016x},{:016x},{:016x},{is_new},{},{:.10},{:.10},{:.10},{},{},{},{},{},{},{:.6e},{:.6e},{:.6e}",
                p[0],
                p[1],
                p[2],
                side[n],
                count[n],
                x[n * DIM],
                x[n * DIM + 1],
                x[n * DIM + 2],
                hex3(&u[r.clone()]),
                hex3(&v[r.clone()]),
                hex3(&f[r.clone()]),
                u8::from(blocked[n * DIM]),
                u8::from(blocked[n * DIM + 1]),
                u8::from(blocked[n * DIM + 2]),
                f[n * DIM],
                f[n * DIM + 1],
                f[n * DIM + 2],
            );
        }
        self.nodes
            .write_all(out.as_bytes())
            .map_err(|e| io_err(&self.dir, e))
    }

    /// After the explicit solve of the step: the branch flags of every
    /// not-ghost cohesive quadrature point (logged when new or changed), then
    /// the node dump if the step is listed.
    ///
    /// # Errors
    /// Model or mesh errors.
    pub fn after_solve(
        &mut self,
        step: usize,
        shared: &SharedMesh,
        model: &mut SolidMechanicsModelCohesive,
    ) -> Result<()> {
        if self.logging(step) {
            self.cohesive_quads(step, shared, model)?;
        }
        if self.cfg.node_steps.binary_search(&step).is_ok() {
            self.node_dump(step, shared, model)?;
        }
        Ok(())
    }

    fn cohesive_quads(
        &mut self,
        step: usize,
        shared: &SharedMesh,
        model: &SolidMechanicsModelCohesive,
    ) -> Result<()> {
        let g = GhostType::NotGhost;
        let ty = ElementType::Cohesive3d6;
        let mat = model.cohesive_material(LAW)?;
        let CohesiveLaw::Linear(lin) = &mat.law else {
            return Err(Error::Unsupported(
                "cohesive driver diagnostics: the cohesive law must be cohesive_linear".into(),
            ));
        };
        let Ok(filter) = mat.base().element_filter.get(ty, g) else {
            return Ok(());
        };
        let filter = filter.as_slice();
        if filter.is_empty() {
            return Ok(());
        }
        let core = &mat.core;
        let opening = core.opening.get(ty, g)?;
        let contact = core.contact_opening.get(ty, g)?;
        let normals = core.normals.get(ty, g)?;
        let damage = core.damage.get(ty, g)?;
        let delta_max = core.delta_max.get(ty, g)?;
        let delta_c = lin.delta_c_eff.get(ty, g)?;
        let (opening, contact, normals) =
            (opening.as_slice(), contact.as_slice(), normals.as_slice());
        let (damage, delta_max, delta_c) =
            (damage.as_slice(), delta_max.as_slice(), delta_c.as_slice());
        let nq = damage.len() / filter.len();
        let m = shared.read()?;
        let x = m.nodes();
        let x = x.as_slice();
        let conn = m.connectivity(ty, g)?;
        let npe = conn.nb_component();
        let conn = conn.as_slice();
        let tol = lin.tol;
        let mut out = String::new();
        for (k, &el) in filter.iter().enumerate() {
            let el = akantu_core::try_to_usize(el)?;
            let nodes = &conn[el * npe..(el + 1) * npe];
            let key = geometry_key(x, &nodes[..3])?;
            for q in 0..nq {
                let i = k * nq + q;
                let r = i * DIM..(i + 1) * DIM;
                // the stored opening lost its normal part on penetration
                // (moved to the contact opening): the full opening is the sum
                let mut full = [0.0; 3];
                for d in 0..DIM {
                    full[d] = opening[r.start + d] + contact[r.start + d];
                }
                let nd = dot_seq(&full, &normals[r.clone()], DIM);
                let pen = contact[r.clone()].iter().any(|&c| c != 0.0);
                let mut flags = 0;
                if pen {
                    flags |= PEN;
                }
                if are_float_equal(damage[i], 0.0, tol) {
                    flags |= D0;
                }
                if are_float_equal(damage[i], 1.0, tol) {
                    flags |= D1;
                }
                let prev = self.flags.insert((key, q), flags);
                if prev == Some(flags) {
                    continue;
                }
                let mut b = [0.0; 3];
                for &n in &nodes[..3] {
                    let n = akantu_core::try_to_usize(n)?;
                    for d in 0..DIM {
                        b[d] += x[n * DIM + d] / 3.0;
                    }
                }
                let _ = writeln!(
                    out,
                    "{step},{key:016x},{q},{},{},{},{},{:.10},{:.10},{:.10},{:.17e},{:.17e},{:.17e},{:.17e},{:016x},{:016x},{:016x},{:016x}",
                    u8::from(prev.is_none()),
                    u8::from(flags & PEN != 0),
                    u8::from(flags & D0 != 0),
                    u8::from(flags & D1 != 0),
                    b[0],
                    b[1],
                    b[2],
                    nd,
                    delta_c[i],
                    damage[i],
                    delta_max[i],
                    nd.to_bits(),
                    delta_c[i].to_bits(),
                    damage[i].to_bits(),
                    delta_max[i].to_bits()
                );
            }
        }
        self.quads
            .write_all(out.as_bytes())
            .map_err(|e| io_err(&self.dir, e))
    }

    fn node_dump(
        &mut self,
        step: usize,
        shared: &SharedMesh,
        model: &mut SolidMechanicsModelCohesive,
    ) -> Result<()> {
        let m = shared.read()?;
        let x = m.nodes();
        let x = x.as_slice();
        let (side, count) = self.sides(&m)?;
        let u = model.smm.displacement()?.as_slice().to_vec();
        let v = model.smm.velocity()?.as_slice().to_vec();
        let fe = model.smm.external_force()?.as_slice().to_vec();
        let fi = model.smm.internal_force()?.as_slice().to_vec();
        let path = self.dir.join(format!("nodes_s{step}.r{}.csv", self.rank));
        let mut f = create(&path)?;
        let mut out = String::from(
            "x_bits,y_bits,z_bits,side,is_new,nelem,x,y,z,ux,uy,uz,vx,vy,vz,fex,fey,fez,fix,fiy,fiz\n",
        );
        for n in 0..m.nb_nodes() {
            let ni = Idx::try_from(n).map_err(|_| Error::invalid("node index overflow"))?;
            if !m.is_local_or_master_node(ni)? {
                continue;
            }
            let p = pos_bits(x, n);
            let r = n * DIM..(n + 1) * DIM;
            let _ = writeln!(
                out,
                "{:016x},{:016x},{:016x},{:016x},{},{},{:.10},{:.10},{:.10},{},{},{},{}",
                p[0],
                p[1],
                p[2],
                side[n],
                u8::from(m.node_global_id(ni) >= self.n0_global),
                count[n],
                x[n * DIM],
                x[n * DIM + 1],
                x[n * DIM + 2],
                hex3(&u[r.clone()]),
                hex3(&v[r.clone()]),
                hex3(&fe[r.clone()]),
                hex3(&fi[r.clone()]),
            );
        }
        f.write_all(out.as_bytes()).map_err(|e| io_err(&path, e))
    }

    /// Flush the logs.
    ///
    /// # Errors
    /// I/O errors.
    pub fn finish(&mut self) -> Result<()> {
        for f in [
            &mut self.facets,
            &mut self.quads,
            &mut self.nodes,
            &mut self.cohesive,
        ] {
            f.flush().map_err(|e| io_err(&self.dir, e))?;
        }
        Ok(())
    }
}

/// `computeEffectiveNorm(stress, normal, tangent, normal_traction)` of
/// `MaterialCohesiveLinear` in 3-D, with the arithmetic order of the port's
/// law (`crates/akantu-cohesive/src/laws/linear.rs`, `effective_norm`).
fn effective_norm(
    s1: &[Real],
    s2: &[Real],
    normal: &[Real],
    tangent: &[Real],
    beta2_inv: Real,
    normal_traction: &mut [Real; 3],
) -> Real {
    for i in 0..DIM {
        let coeff = |k: usize| (s1[i + DIM * k] + s2[i + DIM * k]) / 2. * normal[k];
        let mut r = coeff(0);
        for k in 1..DIM {
            r += coeff(k);
        }
        normal_traction[i] = r;
    }
    let mut normal_contrib = dot_seq(normal_traction, normal, DIM);
    let mut tangent_contrib = 0.0;
    for v in 0..2 {
        let t = dot_seq(normal_traction, &tangent[3 * v..3 * v + 3], DIM);
        tangent_contrib += t * t;
    }
    tangent_contrib = tangent_contrib.sqrt();
    normal_contrib = std_max(0.0, normal_contrib);
    (normal_contrib * normal_contrib + tangent_contrib * tangent_contrib * beta2_inv).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_steps_parse() {
        assert_eq!(
            parse_steps("5,1:3,10:20:5").unwrap(),
            vec![1, 2, 3, 5, 10, 15, 20]
        );
        assert!(parse_steps("3:1").is_err());
        assert!(parse_steps("1:5:0").is_err());
        assert!(parse_steps("x").is_err());
    }

    #[test]
    fn geometry_key_ignores_node_order() {
        let x = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let a = geometry_key(&x, &[0, 1, 2]).unwrap();
        assert_eq!(a, geometry_key(&x, &[2, 0, 1]).unwrap());
        assert_ne!(a, geometry_key(&x, &[0, 1, 3]).unwrap());
    }
}
