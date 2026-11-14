// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh_utils/mesh_partition/
// mesh_partition_scotch.{hh,cc} (MeshPartitionScotch: constructor seeding,
// partitionate with its load callbacks and dblDump files). Feature `scotch`
// (C shim src/scotch_shim.c).

//! Scotch graph partitioning (MUTIL-014, PART-003; `docs/ARCHITECTURE.md`).
//!
//! Upstream `MeshPartitionScotch::partitionate`: the dual graph of the
//! elements of the mesh dimension ([`crate::partition::build_dual_graph`],
//! vertex and edge loads 1 by default), periodically tweaked connectivity,
//! the **default strategy** (`SCOTCH_stratInit` without a strategy string),
//! `SCOTCH_graphPart` into `nb_parts`, then
//! [`crate::partition::fill_partition_information_with`]. Scotch is seeded with
//! the seed upstream passes (`RandomGenerator<Int>::seed()`, given here
//! explicitly) and reset, so that a process computing several partitions
//! behaves like a fresh process for each.
//!
//! **Reproducibility.** Scotch 7 partitions with as many threads as cores
//! by default, and that is not reproducible run to run even with a fixed
//! seed (observed in the oracle: two 4-part partitions of `cube.msh`
//! differed). Set `SCOTCH_PTHREAD_NUMBER=1` (as the oracle fixtures do) for
//! reproducible partitions; [`PartitionMetadata`] records the library
//! version, strategy, seed, reset, `SCOTCH_Num` width, the graph hash and
//! `SCOTCH_PTHREAD_NUMBER`.
//!
//! **Threads.** `SCOTCH_randomSeed`,
//! `SCOTCH_randomReset` and `SCOTCH_graphPart` act on Scotch's
//! process-global default context: two partitions computed concurrently
//! (local-world ranks are threads; library users) would race inside C
//! code and lose reproducibility. Every partition therefore runs under one
//! process-wide lock ([`ScotchPartitioner::partition`]).
//!
//! **Loads (PART-001/PART-003).** Upstream's
//! `Mesh::distribute(_edge_weight_function = …, _vertex_weight_function =
//! …)` hands its callbacks to `partitionate(nb_part, edge_load_func,
//! vertex_load_func)`, which evaluates them in `buildDualGraph` and passes
//! the loads to Scotch (`velotab`, `edlotab`). [`ScotchPartitioner`] uses
//! upstream's defaults (every load 1);
//! [`ScotchPartitioner::with_loads`] gives the callbacks
//! ([`WeightedScotchPartitioner`]). Loads Scotch rejects (e.g. reciprocal
//! arcs with different loads) fail `SCOTCH_graphCheck` in the shim: an
//! error, never undefined behaviour. The same partitioner serves
//! `distribution::distribute_with` and the weighted oracle cases of
//! MUTIL-014. Upstream ignores every Scotch return
//! code (UD-673): here 0 parts are refused before Scotch is called, through
//! every entry point, and every Scotch failure is an error.
//!
//! **Debug files** (upstream: debug level `dblDump`, current directory;
//! here an explicit directory set with
//! [`WeightedScotchPartitioner::with_debug_dump`]): `GraphIniFile.grf`
//! (`SCOTCH_graphSave` of the dual graph), `GeomIniFile.xyz` (dimension,
//! vertex count, then `i x y z ` per element: the mean of its nodes, C++
//! stream formatting), `GraphFile.grf` (after partitioning) and
//! `MapFile.map` (vertex count, then `i    part` per vertex).

#![allow(unsafe_code)] // the Scotch FFI edge (feature `scotch`); every block has a SAFETY comment

use std::ffi::{CStr, CString, c_char, c_int};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

use akantu_core::{Element, Error, GhostType, Idx, Int, Result, TypeFilter};

use crate::mesh::Mesh;
use crate::partition::{
    MeshPartition, MeshPartitioner, PartitionCompat, PartitionMetadata, PartitionerKind,
    build_dual_graph, fill_partition_information_with,
};

/// `SCOTCH_Num` of the supported builds (32-bit, as upstream's `Int`;
/// checked at run time against the library).
pub(crate) type ScotchNum = i32;

unsafe extern "C" {
    fn akantu_scotch_num_size() -> c_int;
    fn akantu_scotch_version(version: *mut c_int, release: *mut c_int, patch: *mut c_int);
    fn akantu_scotch_graph_part(
        vertnbr: ScotchNum,
        verttab: *const ScotchNum,
        velotab: *const ScotchNum,
        edgenbr: ScotchNum,
        edgetab: *const ScotchNum,
        edlotab: *const ScotchNum,
        partnbr: ScotchNum,
        seed: ScotchNum,
        parttab: *mut ScotchNum,
        graph_ini_file: *const c_char,
        graph_file: *const c_char,
    ) -> c_int;
}

/// Serialises every call that touches Scotch's process-global default
/// context (seed, reset, partitioning).
pub(crate) static SCOTCH_CONTEXT: Mutex<()> = Mutex::new(());

/// Version of the linked Scotch library (`SCOTCH_version`).
#[must_use]
pub fn scotch_version() -> String {
    let (mut v, mut r, mut p) = (0, 0, 0);
    // SAFETY: the three out-pointers are valid, distinct local integers;
    // SCOTCH_version only writes them.
    unsafe { akantu_scotch_version(&raw mut v, &raw mut r, &raw mut p) };
    format!("{v}.{r}.{p}")
}

/// Width of the library's `SCOTCH_Num` in bits.
#[must_use]
pub fn scotch_num_bits() -> u32 {
    // SAFETY: a pure function of the shim returning `sizeof(SCOTCH_Num)`.
    let s = unsafe { akantu_scotch_num_size() };
    u32::try_from(s).unwrap_or(0) * 8
}

/// A compact CSR graph handed to Scotch (private copies of the dual
/// graph: Scotch receives pointers derived from unique borrows).
#[derive(Debug)]
pub(crate) struct ScotchCsr {
    /// `vertnbr + 1` start indices.
    pub(crate) xadj: Vec<ScotchNum>,
    /// Adjacency targets (`edgenbr`).
    pub(crate) adjncy: Vec<ScotchNum>,
    /// One load per vertex.
    pub(crate) vertex_loads: Vec<ScotchNum>,
    /// One load per edge.
    pub(crate) edge_loads: Vec<ScotchNum>,
}

/// `SCOTCH_graphBuild`, the debug `SCOTCH_graphSave` of `graph_ini_file`,
/// `SCOTCH_graphCheck`, `SCOTCH_graphPart` and the debug save of
/// `graph_file` (upstream `MeshPartitionScotch::partitionate`) on `csr`:
/// the parts of the `vertnbr` vertices.
///
/// The CSR sizes and start indices are checked here, in release builds,
/// before Scotch reads the arrays. The adjacency
/// targets are not: `SCOTCH_graphCheck` refuses an out-of-range target
/// before `SCOTCH_graphPart`, and neither `SCOTCH_graphBuild` nor the debug
/// `SCOTCH_graphSave` (which run before it, as upstream) index by them
/// (`graph_part_rejects_out_of_range_targets`, also run under valgrind
/// memcheck).
///
/// # Errors
/// Inconsistent CSR sizes or start indices; a count overflowing
/// `SCOTCH_Num`; a Scotch failure (`rc ≠ 0`).
pub(crate) fn graph_part_csr(
    csr: ScotchCsr,
    partnbr: ScotchNum,
    seed: ScotchNum,
    graph_ini_file: Option<&CStr>,
    graph_file: Option<&CStr>,
) -> Result<Vec<ScotchNum>> {
    let ScotchCsr {
        mut xadj,
        mut adjncy,
        vertex_loads: mut vl,
        edge_loads: mut el,
    } = csr;
    let n = vl.len();
    let num = |v: usize, what: &str| -> Result<ScotchNum> {
        ScotchNum::try_from(v)
            .map_err(|_| Error::InvalidArgument(format!("Scotch: {what} {v} overflows SCOTCH_Num")))
    };
    let vertnbr = num(n, "vertex count")?;
    let edgenbr = num(adjncy.len(), "edge count")?;
    // the compact CSR invariants `SCOTCH_graphBuild` and the
    // `SCOTCH_graphSave` of `GraphIniFile.grf` (before
    // `SCOTCH_graphCheck`) rely on: `build_dual_graph` guarantees them,
    // and they are checked here once per call before Scotch reads the
    // arrays: `vertnbr + 1` start indices from 0
    // to `edgenbr`, non-decreasing, one load per vertex and per edge.
    // The adjacency targets are validated by `SCOTCH_graphCheck` before
    // `SCOTCH_graphPart` (`graphSave` prints them without indexing).
    let csr_ok = xadj.len() == n + 1
        && xadj.first().copied() == Some(0)
        && xadj.last().copied() == Some(edgenbr)
        && xadj.windows(2).all(|w| w[0] <= w[1])
        && el.len() == adjncy.len();
    if !csr_ok {
        return Err(Error::Mesh(format!(
            "Scotch: inconsistent dual graph ({} start indices, {} vertex loads, {} edges, \
             {} edge loads, {n} vertices)",
            xadj.len(),
            vl.len(),
            adjncy.len(),
            el.len()
        )));
    }
    let mut parts = vec![0 as ScotchNum; n];
    // An empty edge table still needs a valid pointer for Scotch.
    let mut dummy_edges: [ScotchNum; 1] = [0];
    let mut dummy_loads: [ScotchNum; 1] = [0];
    let edgetab = if adjncy.is_empty() {
        dummy_edges.as_mut_ptr()
    } else {
        adjncy.as_mut_ptr()
    };
    let edlotab = if el.is_empty() {
        dummy_loads.as_mut_ptr()
    } else {
        el.as_mut_ptr()
    };
    // Scotch's default context is process-global: one partition at a
    // time (a poisoned lock only means another thread panicked outside
    // the C call; the context holds no Rust state).
    let context = SCOTCH_CONTEXT
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    // SAFETY: the graph is a compact CSR graph (its sizes and start indices
    // checked above): `xadj` has `vertnbr + 1` non-decreasing entries from
    // `xadj[0] = 0` to `xadj[vertnbr] = edgenbr`, `vl` and `parts` have
    // `vertnbr` entries, `adjncy`/`el` `edgenbr` (or the one-entry dummies
    // when empty), so `SCOTCH_graphBuild` and the `SCOTCH_graphSave` of the
    // debug file (which runs before `SCOTCH_graphCheck`, as upstream) stay
    // in bounds; they do not index by the adjacency targets, which
    // `SCOTCH_graphCheck` validates (`< vertnbr`) before `SCOTCH_graphPart`
    // reads through them (an out-of-range target gives
    // `rc ≠ 0`, memcheck clean). Every pointer comes from a private,
    // uniquely borrowed buffer of this function, alive for the call; the
    // path pointers are null or NUL-terminated strings. Scotch reads the
    // input arrays (const casts in the shim only) and writes `vertnbr`
    // parts. The call holds `SCOTCH_CONTEXT`, so no other thread uses
    // Scotch's global context (seed, reset, partitioning) meanwhile.
    let rc = unsafe {
        akantu_scotch_graph_part(
            vertnbr,
            xadj.as_mut_ptr(),
            vl.as_mut_ptr(),
            edgenbr,
            edgetab,
            edlotab,
            partnbr,
            seed,
            parts.as_mut_ptr(),
            graph_ini_file.map_or(std::ptr::null(), CStr::as_ptr),
            graph_file.map_or(std::ptr::null(), CStr::as_ptr),
        )
    };
    drop(context);
    if rc != 0 {
        return Err(Error::Mesh(format!(
            "Scotch partitioning failed (code {rc})"
        )));
    }
    Ok(parts)
}

/// Upstream `MeshPartitionScotch` with an explicit seed (upstream:
/// `RandomGenerator<Int>::seed()`, itself `AKA_SEED × (rank + 1)` or the
/// clock).
#[derive(Clone, Copy, Debug)]
pub struct ScotchPartitioner {
    /// Seed given to `SCOTCH_randomSeed`.
    pub seed: i64,
    /// Upstream-compatibility switches of the lower-dimensional parts.
    pub compat: PartitionCompat,
}

impl ScotchPartitioner {
    /// Partitioner seeded with `seed`.
    #[must_use]
    pub fn new(seed: i64) -> Self {
        Self {
            seed,
            compat: PartitionCompat::default(),
        }
    }

    /// With upstream-compatibility switches (differential replays).
    #[must_use]
    pub fn with_compat(mut self, compat: PartitionCompat) -> Self {
        self.compat = compat;
        self
    }

    /// Partitioner seeded with the global random seed (upstream default).
    #[must_use]
    pub fn from_global_seed() -> Self {
        Self::new(akantu_core::random::RandomGenerator::<
            akantu_core::random::IdxSlot,
        >::get_seed())
    }

    /// With upstream's load callbacks (`Mesh::distribute(
    /// _edge_weight_function = edge_load, _vertex_weight_function =
    /// vertex_load)` → `MeshPartitionScotch::partitionate(nb_part,
    /// edge_load_func, vertex_load_func)`): `edge_load(e, neighbour)` for
    /// every arc of the dual graph, `vertex_load(e)` for every element of
    /// the mesh dimension ([`build_dual_graph`]).
    #[must_use]
    pub fn with_loads<'a>(
        self,
        edge_load: &'a dyn Fn(Element, Element) -> Int,
        vertex_load: &'a dyn Fn(Element) -> Int,
    ) -> WeightedScotchPartitioner<'a> {
        WeightedScotchPartitioner {
            base: self,
            edge_load,
            vertex_load,
            debug_dump: None,
        }
    }
}

/// [`ScotchPartitioner`] with upstream's edge and vertex load callbacks
/// (see [`ScotchPartitioner::with_loads`]).
pub struct WeightedScotchPartitioner<'a> {
    /// Seed and compatibility switches.
    pub base: ScotchPartitioner,
    edge_load: &'a dyn Fn(Element, Element) -> Int,
    vertex_load: &'a dyn Fn(Element) -> Int,
    /// Directory receiving the `dblDump` files (`None`: none).
    debug_dump: Option<&'a Path>,
}

impl<'a> WeightedScotchPartitioner<'a> {
    /// Write upstream's `dblDump` debug files (`GraphIniFile.grf`,
    /// `GeomIniFile.xyz`, `GraphFile.grf`, `MapFile.map`; module
    /// documentation) into `dir` at every partition (upstream: the current
    /// directory, at debug level `dblDump`). Refused for periodic meshes
    /// (upstream writes them from its tweaked connectivity). Unit loads
    /// (`with_loads(&|_, _| 1, &|_| 1)`) give upstream's default
    /// `partitionate(nb_part)`.
    #[must_use]
    pub fn with_debug_dump(mut self, dir: &'a Path) -> Self {
        self.debug_dump = Some(dir);
        self
    }
}

impl std::fmt::Debug for WeightedScotchPartitioner<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeightedScotchPartitioner")
            .field("base", &self.base)
            .field("debug_dump", &self.debug_dump)
            .finish_non_exhaustive()
    }
}

impl MeshPartitioner for WeightedScotchPartitioner<'_> {
    fn partition(&mut self, mesh: &Mesh, nb_parts: usize) -> Result<MeshPartition> {
        self.base.partition_with_loads(
            mesh,
            nb_parts,
            self.edge_load,
            self.vertex_load,
            self.debug_dump,
        )
    }
}

impl MeshPartitioner for ScotchPartitioner {
    fn partition(&mut self, mesh: &Mesh, nb_parts: usize) -> Result<MeshPartition> {
        self.partition_with_loads(mesh, nb_parts, &|_, _| 1, &|_| 1, None)
    }
}

/// `std::fs::write` with the path in the error.
pub(crate) fn write_debug_file(path: &Path, contents: &str) -> Result<()> {
    std::fs::write(path, contents).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })
}

pub(crate) fn c_path(dir: Option<&Path>, name: &str) -> Result<Option<CString>> {
    dir.map(|d| {
        let p = d.join(name);
        // the path's bytes as they are (a lossy UTF-8
        // conversion would silently write somewhere else)
        #[cfg(unix)]
        let bytes = {
            use std::os::unix::ffi::OsStrExt as _;
            p.as_os_str().as_bytes().to_vec()
        };
        #[cfg(not(unix))]
        let bytes = p
            .to_str()
            .ok_or_else(|| {
                Error::invalid(format!(
                    "debug dump path {} is not valid UTF-8",
                    p.display()
                ))
            })?
            .as_bytes()
            .to_vec();
        CString::new(bytes)
            .map_err(|_| Error::invalid(format!("debug dump path {} contains NUL", p.display())))
    })
    .transpose()
}

/// `GeomIniFile.xyz`: dimension, vertex count, then per element of the
/// mesh dimension (not ghost, types in enum order) its index and the mean
/// of its nodes (`mid += x` node by node, then `mid /= nb_nodes`).
fn geom_ini_file(mesh: &Mesh, vertnbr: usize) -> Result<String> {
    let dim = mesh.spatial_dimension();
    let mut s = format!("{dim}\n{vertnbr}\n");
    let nodes_g = mesh.nodes();
    let nodes = nodes_g.as_slice();
    let mut lin = 0usize;
    let filter = TypeFilter::new().dim(dim).ghost(GhostType::NotGhost);
    for t in mesh.element_types(filter) {
        let npe = t.nb_nodes_per_element();
        for conn in mesh
            .connectivity(t, GhostType::NotGhost)?
            .as_slice()
            .chunks(npe.max(1))
        {
            let mut mid = vec![0.0; dim];
            for &n in conn {
                let base = akantu_core::try_to_usize(n)? * dim;
                for (m, x) in mid.iter_mut().zip(&nodes[base..base + dim]) {
                    *m += x;
                }
            }
            #[allow(clippy::cast_precision_loss)] // small node counts (≤ 27), exact
            let n = conn.len() as f64;
            let _ = write!(s, "{lin} ");
            for m in &mut mid {
                *m /= n;
                let _ = write!(s, "{} ", crate::scotch_reorder::ostream_default(*m));
            }
            s.push('\n');
            lin += 1;
        }
    }
    Ok(s)
}

impl ScotchPartitioner {
    /// Upstream `partitionate(nb_part, edge_load_func, vertex_load_func)`,
    /// with the `dblDump` files written into `debug_dump` when it is set
    /// (see the module documentation); every public entry point
    /// ([`MeshPartitioner::partition`] of [`ScotchPartitioner`] and of
    /// [`WeightedScotchPartitioner`]) goes through it.
    ///
    /// Upstream ignores every Scotch return code (UD-673, category A): a
    /// failed `SCOTCH_graphPart` leaves the zero-initialised part table as
    /// the partition (the oracle returns an all-zero partition for
    /// `partitionate(0)`). Here a request for 0 parts is refused before
    /// Scotch is called, and every Scotch failure is an error.
    ///
    /// # Errors
    /// 0 parts (UD-673), a 64-bit `SCOTCH_Num`, a count, load or seed
    /// overflowing `SCOTCH_Num`, a Scotch failure (e.g. inconsistent loads
    /// rejected by `SCOTCH_graphCheck`), a debug file that cannot be written
    /// (debug files of periodic meshes are refused: upstream writes them
    /// from its tweaked connectivity).
    fn partition_with_loads(
        &self,
        mesh: &Mesh,
        nb_parts: usize,
        edge_load: &dyn Fn(Element, Element) -> Int,
        vertex_load: &dyn Fn(Element) -> Int,
        debug_dump: Option<&Path>,
    ) -> Result<MeshPartition> {
        if scotch_num_bits() != 32 {
            return Err(Error::Unsupported(format!(
                "Scotch with a {}-bit SCOTCH_Num (upstream requires sizeof(Int) == \
                 sizeof(SCOTCH_Num), 32 bits)",
                scotch_num_bits()
            )));
        }
        if nb_parts == 0 {
            return Err(Error::InvalidArgument(
                "Scotch: a partition into 0 parts (upstream ignores SCOTCH_graphPart's failure and \
                 returns an all-zero partition, UD-673)"
                    .into(),
            ));
        }
        if debug_dump.is_some() && mesh.is_periodic() {
            return Err(Error::Unsupported(
                "Scotch debug files of a periodic mesh".into(),
            ));
        }
        let graph = build_dual_graph(mesh, edge_load, vertex_load)?;
        let n = graph.vertex_loads.len();
        let num = |v: usize, what: &str| -> Result<ScotchNum> {
            ScotchNum::try_from(v).map_err(|_| {
                Error::InvalidArgument(format!("Scotch: {what} {v} overflows SCOTCH_Num"))
            })
        };
        // the counts are checked against SCOTCH_Num before anything else
        num(n, "vertex count")?;
        num(graph.adjncy.len(), "edge count")?;
        let partnbr = num(nb_parts, "part count")?;
        let seed = ScotchNum::try_from(self.seed).map_err(|_| {
            Error::InvalidArgument(format!("Scotch: seed {} overflows SCOTCH_Num", self.seed))
        })?;
        // private copies: Scotch receives pointers derived from unique
        // borrows, so even a write by the library could not alias a shared
        // borrow
        let cast = |v: &[Int]| -> Vec<ScotchNum> { v.to_vec() };
        debug_assert!(
            graph
                .adjncy
                .iter()
                .all(|&v| usize::try_from(v).is_ok_and(|v| v < n))
        );
        let ini_file = c_path(debug_dump, "GraphIniFile.grf")?;
        let final_file = c_path(debug_dump, "GraphFile.grf")?;
        if let Some(dir) = debug_dump {
            write_debug_file(&dir.join("GeomIniFile.xyz"), &geom_ini_file(mesh, n)?)?;
        }
        let linear: Vec<Idx> = graph_part_csr(
            ScotchCsr {
                xadj: cast(&graph.xadj),
                adjncy: cast(&graph.adjncy),
                vertex_loads: cast(&graph.vertex_loads),
                edge_loads: cast(&graph.edge_loads),
            },
            partnbr,
            seed,
            ini_file.as_deref(),
            final_file.as_deref(),
        )?;
        if let Some(dir) = debug_dump {
            let mut map = format!("{n}\n");
            for (i, p) in linear.iter().enumerate() {
                let _ = writeln!(map, "{i}    {p}");
            }
            write_debug_file(&dir.join("MapFile.map"), &map)?;
        }
        fill_partition_information_with(
            mesh,
            mesh.spatial_dimension(),
            nb_parts,
            &linear,
            PartitionMetadata {
                kind: PartitionerKind::Scotch {
                    version: scotch_version(),
                },
                strategy: String::new(),
                seed: Some(self.seed),
                random_reset: true,
                index_width: scotch_num_bits(),
                graph_hash: graph.hash(),
                nb_parts,
                pthread_number: std::env::var("SCOTCH_PTHREAD_NUMBER").ok(),
            },
            self.compat,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path inside a fresh scratch directory for the debug saves.
    fn scratch(tag: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("akantu-scotch-ur11-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A 4-cycle 0–1–2–3–0 in compact CSR form, every load 1.
    fn ring() -> ScotchCsr {
        ScotchCsr {
            xadj: vec![0, 2, 4, 6, 8],
            adjncy: vec![1, 3, 0, 2, 1, 3, 2, 0],
            vertex_loads: vec![1; 4],
            edge_loads: vec![1; 8],
        }
    }

    /// The FFI-level contract of `graph_part_csr`. A
    /// valid graph partitions; an adjacency target outside `0..vertnbr`
    /// (above, and negative) is refused by Scotch (`rc ≠ 0`, an error)
    /// before `SCOTCH_graphPart`, also with the debug saves, whose
    /// `SCOTCH_graphSave` runs before `SCOTCH_graphCheck`. Run under
    /// memcheck (the evidence that `SCOTCH_graphBuild` and `graphSave` do
    /// not index by the targets): no invalid read or write.
    #[test]
    fn graph_part_rejects_out_of_range_targets() {
        let parts = graph_part_csr(ring(), 2, 1, None, None).unwrap();
        assert_eq!(parts.len(), 4);
        assert!(parts.iter().all(|&p| p == 0 || p == 1), "{parts:?}");
        let dir = scratch("debug");
        let ini = c_path(Some(&dir), "GraphIniFile.grf").unwrap();
        let fin = c_path(Some(&dir), "GraphFile.grf").unwrap();
        for bad in [4, 1000, -1, ScotchNum::MAX, ScotchNum::MIN] {
            let mut g = ring();
            g.adjncy[5] = bad;
            let e = graph_part_csr(g, 2, 1, None, None).expect_err("out-of-range target");
            assert!(format!("{e}").contains("Scotch partitioning failed"), "{e}");
            let mut g = ring();
            g.adjncy[5] = bad;
            let e = graph_part_csr(g, 2, 1, ini.as_deref(), fin.as_deref())
                .expect_err("out-of-range target with the debug saves");
            assert!(format!("{e}").contains("Scotch partitioning failed"), "{e}");
            // the initial graph was saved (before the check), the final one not
            assert!(dir.join("GraphIniFile.grf").is_file());
            assert!(!dir.join("GraphFile.grf").exists());
            std::fs::remove_file(dir.join("GraphIniFile.grf")).unwrap();
        }
        // inconsistent start indices are refused in Rust, before Scotch
        let mut g = ring();
        g.xadj[4] = 9;
        assert!(matches!(
            graph_part_csr(g, 2, 1, None, None),
            Err(Error::Mesh(m)) if m.contains("inconsistent dual graph")
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
