// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/mesh_utils/mesh_partition/
// mesh_partition_scotch.cc (createMesh, reorder, and the dblDump debug files
// of createMesh / reorder / partitionate). Feature `scotch` (C shim
// src/scotch_shim.c).

//! Scotch node reordering (MUTIL-014, PART-004) and the `dblDump` debug
//! files of the Scotch partitioner.
//!
//! **Reordering** (upstream `MeshPartitionScotch::reorder`): the
//! element–node mesh of the not-ghost elements of the mesh dimension (every
//! kind; nodes first, then elements, `createMesh`) is given to
//! `SCOTCH_meshGraph` and `SCOTCH_graphOrder` (default strategy); the
//! resulting `permtab` renumbers node `i` into `permtab[i]`.
//!
//! Upstream then renumbers the connectivities of every element type (all
//! dimensions, both ghost types) and moves the coordinates, **and nothing
//! else**: node groups, mesh nodal data, node flags, global ids, periodic
//! pairs and an existing facet mesh (which shares the coordinates) keep the
//! old numbering, and no event is sent (UD-670, category B). The corrected
//! default ([`ReorderMode::Corrected`]) applies the permutation as a node
//! renumbering of the whole mesh domain (`removal::remove_nodes` with a
//! permutation: every node-indexed state, the facet mesh, one `NodesRemoved`
//! event with no removed node per mesh part: two with a facet mesh, parent
//! then facet mesh); [`ReorderMode::Upstream`] reproduces
//! upstream (refused when a facet mesh exists, whose connectivities would
//! no longer match its shared coordinates).
//!
//! **Seed.** Upstream's constructor seeds Scotch with
//! `RandomGenerator<Int>::seed()`; here the seed is explicit and Scotch's
//! generator is reset after seeding, so every call behaves as in a fresh
//! process (as the partitioner, [`crate::scotch`]). Reproducible orderings
//! need `SCOTCH_PTHREAD_NUMBER=1` (UD-236).
//!
//! **Debug files** (upstream: debug level `dblDump`, written to the current
//! directory; here into an explicit directory): `ScotchMesh.msh`
//! (`SCOTCH_meshSave`), `ScotchMesh.xyz` (dimension, node count, then
//! `i x y z ` per node with C++ stream formatting) and `ScotchMesh.grf`
//! (`SCOTCH_graphSave` of the mesh graph); the partitioner writes
//! `GraphIniFile.grf`, `GeomIniFile.xyz` (element barycenters),
//! `GraphFile.grf` and `MapFile.map` ([`crate::scotch::ScotchPartitioner`]).

#![allow(unsafe_code)] // the Scotch FFI edge (feature `scotch`); every block has a SAFETY comment

use std::ffi::{c_char, c_int};
use std::fmt::Write as _;
use std::path::Path;
use std::sync::PoisonError;

use akantu_core::{Error, GhostType, Idx, Result, TypeFilter, try_to_usize};

use crate::events::MeshEvents;
use crate::mesh::Mesh;
use crate::scotch::{SCOTCH_CONTEXT, ScotchNum, c_path, scotch_num_bits, write_debug_file};

unsafe extern "C" {
    fn akantu_scotch_mesh_order(
        velmbas: ScotchNum,
        vnodbas: ScotchNum,
        velmnbr: ScotchNum,
        vnodnbr: ScotchNum,
        verttab: *const ScotchNum,
        edgenbr: ScotchNum,
        edgetab: *const ScotchNum,
        seed: ScotchNum,
        permtab: *mut ScotchNum,
        mesh_file: *const c_char,
        graph_file: *const c_char,
    ) -> c_int;
}

/// How [`reorder`] applies Scotch's permutation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReorderMode {
    /// Renumber every node-indexed state of the mesh and its facet mesh
    /// (UD-670 corrected default).
    #[default]
    Corrected,
    /// Upstream: connectivities and coordinates only, no event.
    Upstream,
}

/// Result of [`reorder`].
#[derive(Debug)]
pub struct ReorderOutcome {
    /// `permtab`: old node `i` becomes node `permutation[i]`.
    pub permutation: Vec<Idx>,
    /// Events to dispatch (empty in [`ReorderMode::Upstream`]).
    pub events: MeshEvents,
}

/// C++ `std::ostream << double` with the default flags and precision 6
/// (glibc `%.6g`): what upstream's `.xyz` debug files contain.
pub(crate) fn ostream_default(x: f64) -> String {
    if x.is_nan() {
        return if x.is_sign_negative() { "-nan" } else { "nan" }.to_owned();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-inf" } else { "inf" }.to_owned();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.to_owned();
    }
    const P: i32 = 6;
    // correctly rounded P significant digits (as glibc)
    let e = format!("{:.*e}", (P - 1) as usize, x);
    let (mant, exp) = e.split_once('e').unwrap_or((e.as_str(), "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let strip = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s.to_owned()
        }
    };
    if !(-4..P).contains(&exp) {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", strip(mant), exp.unsigned_abs())
    } else {
        let decimals = usize::try_from(P - 1 - exp).unwrap_or(0);
        strip(&format!("{x:.decimals$}"))
    }
}

fn num(v: usize, what: &str) -> Result<ScotchNum> {
    ScotchNum::try_from(v)
        .map_err(|_| Error::InvalidArgument(format!("Scotch: {what} {v} overflows SCOTCH_Num")))
}

/// The element–node mesh of upstream `createMesh`: `(verttab, edgetab,
/// velmnbr)` with nodes `0..nb_nodes` (`vnodbas = 0`) and elements
/// `nb_nodes..` (`velmbas = vnodnbr`).
fn scotch_mesh(mesh: &Mesh) -> Result<(Vec<ScotchNum>, Vec<ScotchNum>, usize)> {
    let dim = mesh.spatial_dimension();
    let nb_nodes = mesh.nb_nodes();
    let filter = TypeFilter::new().dim(dim).ghost(GhostType::NotGhost);
    let types: Vec<_> = mesh.element_types(filter).collect();
    let mut total = 0usize;
    let mut nb_edge = 0usize;
    for &t in &types {
        let n = mesh.nb_element(t, GhostType::NotGhost);
        total += n;
        nb_edge += n * t.nb_nodes_per_element();
    }
    let vnodnbr = nb_nodes;
    // node vertices 0..vnodnbr, element vertices vnodnbr.. (velmbas)
    num(vnodnbr, "node count")?;
    let mut verttab = vec![0 as ScotchNum; vnodnbr + total + 1];
    let node = |c: Idx| -> Result<usize> {
        let u = try_to_usize(c)?;
        if u < nb_nodes {
            Ok(u)
        } else {
            Err(Error::OutOfRange {
                what: "node",
                index: u,
                len: nb_nodes,
            })
        }
    };
    // occurrences of every node
    for &t in &types {
        for &c in mesh.connectivity(t, GhostType::NotGhost)?.as_slice() {
            verttab[node(c)?] += 1;
        }
    }
    // occurrence counts → start indices (upstream: inclusive sum, shift)
    for i in 1..nb_nodes {
        verttab[i] += verttab[i - 1];
    }
    for i in (1..=nb_nodes).rev() {
        verttab[i] = verttab[i - 1];
    }
    verttab[0] = 0;
    let incidences = try_to_usize(verttab[vnodnbr])?;
    let edgenbr = incidences + nb_edge;
    let mut edgetab = vec![0 as ScotchNum; edgenbr];
    // node → element lists
    let mut linearized: usize = 0;
    for &t in &types {
        let npe = t.nb_nodes_per_element();
        let conn_g = mesh.connectivity(t, GhostType::NotGhost)?;
        let conn = conn_g.as_slice();
        for el in conn.chunks(npe.max(1)) {
            let v = num(linearized + vnodnbr, "element vertex")?;
            for &c in el {
                let k = node(c)?;
                edgetab[try_to_usize(verttab[k])?] = v;
                verttab[k] += 1;
            }
            linearized += 1;
        }
    }
    for i in (1..=nb_nodes).rev() {
        verttab[i] = verttab[i - 1];
    }
    verttab[0] = 0;
    // element → node lists
    let mut vt = vnodnbr + 1;
    let mut et = incidences;
    for &t in &types {
        let npe = t.nb_nodes_per_element();
        let conn_g = mesh.connectivity(t, GhostType::NotGhost)?;
        let conn = conn_g.as_slice();
        for el in conn.chunks(npe.max(1)) {
            verttab[vt] = verttab[vt - 1] + num(npe, "nodes per element")?;
            vt += 1;
            for &c in el {
                edgetab[et] = num(node(c)?, "node")?;
                et += 1;
            }
        }
    }
    Ok((verttab, edgetab, total))
}

/// `ScotchMesh.xyz` of upstream `createMesh`.
fn xyz_file(mesh: &Mesh) -> String {
    let dim = mesh.spatial_dimension();
    let mut s = format!("{dim}\n{}\n", mesh.nb_nodes());
    for (i, x) in mesh.nodes().tuples().enumerate() {
        let _ = write!(s, "{i} ");
        for &v in x {
            let _ = write!(s, "{} ", ostream_default(v));
        }
        s.push('\n');
    }
    s
}

/// Upstream `MeshPartitionScotch(mesh, dim).reorder()` with an explicit
/// `seed` (see the module documentation for `mode` and `debug_dump`).
///
/// # Errors
/// A distributed mesh or a facet mesh part (unsupported), a facet mesh in
/// [`ReorderMode::Upstream`], an inconsistent mesh, a count that overflows
/// `SCOTCH_Num`, a Scotch failure, a debug file that cannot be written.
pub fn reorder(
    mesh: &mut Mesh,
    seed: i64,
    mode: ReorderMode,
    debug_dump: Option<&Path>,
) -> Result<ReorderOutcome> {
    if scotch_num_bits() != 32 {
        return Err(Error::Unsupported(format!(
            "Scotch with a {}-bit SCOTCH_Num (upstream requires 32 bits)",
            scotch_num_bits()
        )));
    }
    if mesh.is_distributed() {
        return Err(Error::Unsupported(
            "Scotch node reordering of a distributed mesh".into(),
        ));
    }
    if mesh.is_mesh_facets() {
        return Err(Error::Mesh(
            "reorder a facet mesh through its parent mesh".into(),
        ));
    }
    if mode == ReorderMode::Upstream && mesh.mesh_facets().is_some() {
        return Err(Error::Mesh(
            "upstream-compatible reordering with a facet mesh would leave the facet \
             connectivities on the old numbering (UD-670)"
                .into(),
        ));
    }
    mesh.check_consistency()?;
    // private buffers: Scotch receives pointers derived from unique borrows
    let (mut verttab, mut edgetab, velmnbr) = scotch_mesh(mesh)?;
    let nb_nodes = mesh.nb_nodes();
    let seed = ScotchNum::try_from(seed)
        .map_err(|_| Error::InvalidArgument(format!("Scotch: seed {seed} overflows SCOTCH_Num")))?;
    let mesh_file = c_path(debug_dump, "ScotchMesh.msh")?;
    let graph_file = c_path(debug_dump, "ScotchMesh.grf")?;
    if let Some(dir) = debug_dump {
        write_debug_file(&dir.join("ScotchMesh.xyz"), &xyz_file(mesh))?;
    }
    // The sizes `SCOTCH_meshBuild` reads before `SCOTCH_meshCheck` runs
    // (`scotch_mesh` builds them so; checked once per call):
    // `vnodnbr + velmnbr + 1` start indices, the last one the edge
    // count.
    let starts_ok = verttab.len() == nb_nodes + velmnbr + 1
        && usize::try_from(verttab.last().copied().unwrap_or(-1)).ok() == Some(edgetab.len());
    if !starts_ok {
        return Err(Error::Mesh(format!(
            "Scotch: inconsistent element-node graph ({} start indices for {nb_nodes} nodes and \
             {velmnbr} elements, {} edges)",
            verttab.len(),
            edgetab.len()
        )));
    }
    let mut perm = vec![0 as ScotchNum; nb_nodes];
    let mut dummy: [ScotchNum; 1] = [0];
    let edge_ptr = if edgetab.is_empty() {
        dummy.as_mut_ptr()
    } else {
        edgetab.as_mut_ptr()
    };
    let mut dummy_out: [ScotchNum; 1] = [0];
    let perm_ptr = if perm.is_empty() {
        dummy_out.as_mut_ptr()
    } else {
        perm.as_mut_ptr()
    };
    let context = SCOTCH_CONTEXT
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    // SAFETY: `verttab` has `vnodnbr + velmnbr + 1` entries, the last one
    // `edgenbr` (checked above; Scotch reads `vendtab = verttab + 1` as the
    // compact end array), `edgetab`
    // `edgenbr` entries (or the one-entry dummy when empty), `perm`
    // `vnodnbr` entries (or a dummy when there is no node), all private,
    // uniquely borrowed buffers of this function; the path
    // pointers are null or NUL-terminated strings alive for the call. Scotch
    // only reads the input arrays and writes `vnodnbr` permutation entries.
    // `SCOTCH_CONTEXT` is held, so no other thread uses Scotch's global
    // random context meanwhile.
    let rc = unsafe {
        akantu_scotch_mesh_order(
            num(nb_nodes, "node count")?,
            0,
            num(velmnbr, "element count")?,
            num(nb_nodes, "node count")?,
            verttab.as_mut_ptr(),
            num(edgetab.len(), "edge count")?,
            edge_ptr,
            seed,
            perm_ptr,
            mesh_file.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
            graph_file.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
        )
    };
    drop(context);
    if rc != 0 {
        return Err(Error::Mesh(format!("Scotch ordering failed (code {rc})")));
    }
    // a permutation of 0..nb_nodes (checked: it indexes arrays below)
    let mut seen = vec![false; nb_nodes];
    for &p in &perm {
        let u = try_to_usize(p)?;
        match seen.get_mut(u) {
            Some(s) if !*s => *s = true,
            _ => {
                return Err(Error::Mesh(format!(
                    "Scotch ordering is not a permutation (entry {p})"
                )));
            }
        }
    }
    let permutation: Vec<Idx> = perm;
    let events = match mode {
        ReorderMode::Corrected => {
            crate::removal::remove_nodes(mesh, &permutation, "MeshPartitionScotch::reorder")?
        }
        ReorderMode::Upstream => {
            apply_upstream(mesh, &permutation)?;
            MeshEvents::none()
        }
    };
    Ok(ReorderOutcome {
        permutation,
        events,
    })
}

/// Upstream's renumbering: connectivities of every type (both ghost types)
/// and the coordinates; nothing else.
fn apply_upstream(mesh: &mut Mesh, perm: &[Idx]) -> Result<()> {
    for g in GhostType::ALL {
        for (_, c) in mesh.connectivities.as_map_mut().iter_mut(g) {
            let mut c_g = c.write()?;
            for n in c_g.as_mut_slice() {
                *n = perm[try_to_usize(*n)?];
            }
        }
    }
    let dim = mesh.spatial_dimension;
    let old = mesh.nodes.read()?.as_slice().to_vec();
    let mut new_g = mesh.nodes.write()?;
    let new = new_g.as_mut_slice();
    for (i, x) in old.chunks(dim.max(1)).enumerate() {
        let j = try_to_usize(perm[i])?;
        new[j * dim..(j + 1) * dim].copy_from_slice(x);
    }
    drop(new_g);
    mesh.revision += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Debug-file paths keep their bytes (no lossy UTF-8
    /// conversion); an interior NUL is refused.
    #[cfg(unix)]
    #[test]
    fn debug_paths_keep_their_bytes() {
        use std::os::unix::ffi::OsStrExt as _;
        let dir = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/dump-\xff\xfe"));
        let c = c_path(Some(dir), "ScotchMesh.grf").unwrap().unwrap();
        assert_eq!(c.as_bytes(), b"/tmp/dump-\xff\xfe/ScotchMesh.grf");
        assert!(c_path(None, "x").unwrap().is_none());
        let nul = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/a\0b"));
        assert!(c_path(Some(nul), "x").is_err());
    }

    #[test]
    fn ostream_default_matches_printf_g6() {
        for (x, s) in [
            (0.0, "0"),
            (-0.0, "-0"),
            (1.0, "1"),
            (0.5, "0.5"),
            (1.0 / 3.0, "0.333333"),
            (123_456.0, "123456"),
            (1_234_567.0, "1.23457e+06"),
            (0.0001, "0.0001"),
            (0.000_012_345, "1.2345e-05"),
            (-2.5e-10, "-2.5e-10"),
            (999_999.5, "1e+06"),
            (1e100, "1e+100"),
            (0.1 + 0.2, "0.3"),
        ] {
            assert_eq!(ostream_default(x), s, "{x:e}");
        }
    }
}
