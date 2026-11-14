// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// {test_solid_mechanics_model_reassign_material.cc, cube_two_materials.geo,
// two_materials.dat} (registered PARALLEL: 1, 2, 4, … ranks).

//! The program of upstream `test_solid_mechanics_model_reassign_material`
//! (TEST-105), shared by the local-world test
//! `upstream_solid_mechanics_model_reassign_material.rs` and the real-MPI
//! binary `mpi_smm.rs`: see the test
//! file for the port's choices (UD-205 vacuity guards, ghost elements
//! checked, RCB partition). Failures are returned, not panicked, and a
//! local failure is agreed on every rank before the next collective, so
//! that real-MPI peers never wait for a crashed rank.

#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use akantu_comm::Communicator;
use akantu_core::enums::{AnalysisMethod, MeshIoType, SparseSolverType};
use akantu_core::{Element, ElementType, GhostType, Idx, Result, TypeFilter};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::utils::build_facets;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ConstitutiveLawSelector, SelectorContext};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;

/// `two_materials.dat` (verbatim).
const MATERIALS: &str = "material elastic [\n\
\t name = mat_1\n\
\t rho = 2.2e-9   # density\n\
\t E   = 12e9 # young's modulus\n\
\t nu  = 0.3    # poisson's ratio\n\
]\n\
\n\
material elastic [\n\
\t name = mat_2\n\
\t rho = 2.7e-9   # density\n\
\t E   = 60e9 # young's modulus\n\
\t nu  = 0.3    # poisson's ratio\n\
]\n";

const DIM: usize = 3;

/// Upstream `StraightInterfaceMaterialSelector` with real barycentres.
#[derive(Debug)]
struct StraightInterface {
    horizontal: AtomicUsize,
    pos_interface: f64,
}

impl StraightInterface {
    fn expected(&self, bary: &[f64]) -> &'static str {
        if bary[self.horizontal.load(Ordering::Relaxed)] < self.pos_interface {
            "mat_1"
        } else {
            "mat_2"
        }
    }
}

/// Shared handle (the model owns its selector; the test moves the
/// interface).
#[derive(Debug)]
struct Shared(Arc<StraightInterface>);

impl ConstitutiveLawSelector for Shared {
    fn select(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        let bary = ctx.mesh.barycenter(element)?;
        Ok((ctx.law_by_name)(self.0.expected(&bary)))
    }
}

/// Upstream `isConditonVerified` (+ ghost elements): per ghost type, the
/// number of elements of each material; an error on a wrong index
/// (upstream: `AKANTU_ERROR`, `EXIT_FAILURE`).
fn condition_verified(
    model: &SolidMechanicsModel,
    sel: &StraightInterface,
) -> std::result::Result<[[usize; 2]; 2], String> {
    let s = |e: akantu_core::Error| e.to_string();
    let mesh = model.physics.base.mesh.read().map_err(s)?;
    let ids = [
        model.material_index("mat_1").map_err(s)?,
        model.material_index("mat_2").map_err(s)?,
    ];
    let mut counts = [[0usize; 2]; 2];
    for (g, ghost) in GhostType::ALL.into_iter().enumerate() {
        let types: Vec<ElementType> = mesh
            .element_types(TypeFilter {
                spatial_dimension: Some(DIM),
                ghost_type: ghost,
                kind: Some(akantu_core::ElementKind::Regular),
            })
            .collect();
        for ty in types {
            let index = model.material_by_element().get(ty, ghost).map_err(s)?;
            for e in 0..mesh.nb_element(ty, ghost) {
                let el = Element::new(ty, e as Idx, ghost);
                let bary = mesh.barycenter(el).map_err(s)?;
                let want = usize::from(sel.expected(&bary) != "mat_1");
                let got = index.as_slice()[e];
                if got != ids[want] as Idx {
                    return Err(format!(
                        "{el}: material {got}, expected {} (barycentre {bary:?})",
                        ids[want]
                    ));
                }
                counts[g][want] += 1;
            }
        }
    }
    Ok(counts)
}

/// A local outcome agreed on every rank (collective): every rank fails if
/// one did.
fn agree<T>(
    comm: &Communicator,
    local: std::result::Result<T, String>,
) -> std::result::Result<T, String> {
    let mut failed = [u64::from(local.is_err())];
    comm.all_reduce(&mut failed, akantu_comm::ReduceOp::Max)
        .map_err(|e| format!("agree: {e}"))?;
    match local {
        Err(e) => Err(e),
        Ok(_) if failed[0] != 0 => Err("failed on another rank".into()),
        Ok(v) => Ok(v),
    }
}

/// The upstream program on `comm` (`serial`: no distribution, facets built).
pub fn scenario(comm: Communicator, serial: bool) -> std::result::Result<(), String> {
    let path = fixtures_root().join(
        "meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/cube_two_materials.msh",
    );
    fn e(what: &'static str) -> impl Fn(akantu_core::Error) -> String {
        move |err| format!("{what}: {err}")
    }
    let mut mesh = Mesh::with_communicator(DIM, "mesh", comm.clone()).map_err(e("mesh"))?;
    if comm.rank() == 0 {
        akantu_io::read_mesh(&mut mesh, &path, MeshIoType::Gmsh).map_err(e("read"))?;
    }
    if serial {
        let _ = build_facets(&mut mesh).map_err(e("buildFacets"))?;
    } else {
        mesh.compute_bounding_box().map_err(e("bbox"))?;
        let mut p = RcbPartitioner;
        let _ = distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0)
            .map_err(e("distribute"))?;
    }
    let parser = Parser::parse_str(MATERIALS, "two_materials.dat").map_err(e("parser"))?;
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .map_err(e("model"))?;
    let sel = Arc::new(StraightInterface {
        horizontal: AtomicUsize::new(0),
        pos_interface: 0.,
    });
    model.set_material_selector(Box::new(Shared(sel.clone())));
    let mut options = ModelOptions::new(AnalysisMethod::Static);
    if !serial {
        options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    }
    agree(&comm, model.init_full(&options).map_err(e("initFull")))?;

    // "materials not correctly assigned"
    let before = agree(&comm, condition_verified(&model, &sel))?;
    let index_before: Vec<Idx> = {
        let mesh = model.physics.base.mesh.read().map_err(e("read"))?;
        let mut v = Vec::new();
        for ty in mesh.element_types(TypeFilter {
            spatial_dimension: Some(DIM),
            ghost_type: GhostType::NotGhost,
            kind: Some(akantu_core::ElementKind::Regular),
        }) {
            v.extend_from_slice(
                model
                    .material_by_element()
                    .get(ty, GhostType::NotGhost)
                    .map_err(e("material index"))?
                    .as_slice(),
            );
        }
        v
    };

    // moveInterface(pos_interface, _y)
    sel.horizontal.store(1, Ordering::Relaxed);
    agree(
        &comm,
        model.reassign_material().map_err(e("reassignMaterial")),
    )?;

    // "materials not correctly reassigned"
    let after = agree(&comm, condition_verified(&model, &sel))?;
    let index_after: Vec<Idx> = {
        let mesh = model.physics.base.mesh.read().map_err(e("read"))?;
        let mut v = Vec::new();
        for ty in mesh.element_types(TypeFilter {
            spatial_dimension: Some(DIM),
            ghost_type: GhostType::NotGhost,
            kind: Some(akantu_core::ElementKind::Regular),
        }) {
            v.extend_from_slice(
                model
                    .material_by_element()
                    .get(ty, GhostType::NotGhost)
                    .map_err(e("material index"))?
                    .as_slice(),
            );
        }
        v
    };
    let moved = index_before
        .iter()
        .zip(&index_after)
        .filter(|(a, b)| a != b)
        .count();

    // vacuity guards (UD-205), summed over the ranks
    let local = [
        before[0][0] as f64,
        before[0][1] as f64,
        after[0][0] as f64,
        after[0][1] as f64,
        moved as f64,
        (after[1][0] + after[1][1]) as f64,
    ];
    let mut global = local;
    comm.all_reduce(&mut global, akantu_comm::ReduceOp::Sum)
        .map_err(e("allReduce"))?;
    if comm.rank() == 0 {
        eprintln!(
            "{} rank(s): before mat_1/mat_2 = {}/{}, after = {}/{}, moved {}, ghost elements checked {}",
            comm.size(),
            global[0],
            global[1],
            global[2],
            global[3],
            global[4],
            global[5]
        );
    }
    if comm.size() > 1 && global[5] == 0. {
        return Err("distributed run without ghost elements".into());
    }
    for (k, what) in [
        (0, "mat_1 before"),
        (1, "mat_2 before"),
        (2, "mat_1 after"),
        (3, "mat_2 after"),
        (4, "moved elements"),
    ] {
        if global[k] == 0. {
            return Err(format!("vacuous: no {what}"));
        }
    }
    Ok(())
}
