// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/diffusion_model/{diffusion_model.hh,
// diffusion_model.cc, heat_transfer_model.hh, diffusion_law.hh,
// diffusion_law.cc, heat_diffusion.hh, heat_diffusion.cc}.

//! # akantu-diffusion
//!
//! The heat transfer (diffusion) model (upstream `DiffusionModel`,
//! `HeatTransferModel`) with its diffusion laws (`DiffusionLaw`,
//! `HeatDiffusion`): static, implicit dynamic (backward Euler) and explicit
//! lumped-capacity (forward Euler) analyses, conductivity and capacity
//! matrices, stable time step, thermal energy, dumps and the parallel data
//! accessors (parity rows DIFF-001…012), ported from upstream's
//! `src/model/diffusion_model`. It runs on one process or on a distributed
//! mesh. Cargo features, off by default: `mpi` and `scotch` (test builds
//! under real MPI and with Scotch).
//!
//! ```no_run
//! # fn main() -> akantu_core::Result<()> {
//! use akantu_core::enums::AnalysisMethod;
//! use akantu_diffusion::HeatTransferModel;
//! let mut mesh = akantu_mesh::Mesh::new(2, "mesh")?;
//! akantu_io::read_mesh(&mut mesh, std::path::Path::new("square.msh"),
//!     akantu_core::enums::MeshIoType::Gmsh)?;
//! let parser = akantu_parser::Parser::parse_file("material.dat")?;
//! let mut model = HeatTransferModel::with_parser(
//!     akantu_mesh::SharedMesh::new(mesh), None, "heat_transfer", parser)?;
//! model.init_full_with(AnalysisMethod::ExplicitLumpedMass)?;
//! let dt = model.stable_time_step()? * 0.8;
//! model.set_time_step(dt, None)?;
//! model.solve_step(None)?;
//! # Ok(()) }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod heat_diffusion;
pub mod law;
pub mod model;

pub use heat_diffusion::HeatDiffusion;
pub use law::{
    DiffusionLaw, DiffusionLawBase, DiffusionLawContext, DiffusionLawFactory, DiffusionModelKind,
    DiffusionUpstreamCompat, register_builtin_laws,
};
pub use model::{DiffusionDumpers, DiffusionModel, DiffusionPhysics, HeatTransferModel};
