// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/mesh_io.hh, mesh_io.cc,
// src/io/mesh_io/{mesh_io_msh*, mesh_io_diana*}, src/io/dumper/*,
// third-party/iohelper (ParaView/text writers).

//! # akantu-io
//!
//! Mesh readers and writers and the dumpers, ported from upstream's
//! `src/io` (`mesh_io`, `dumper`) and the writers of its third-party
//! `iohelper`. The crate has no Cargo features.
//!
//! * [`mesh_io`]: readers/writers (upstream `MeshIO`): Gmsh `.msh` (all
//!   versions upstream reads), Gmsh "struct" reinterpretation, Diana;
//!   `MeshIoType::Auto` selects by extension. Readers fill a `Mesh` through
//!   `MeshAccessor`, create `tag_*`/`physical_names` mesh data and groups,
//!   and validate every index (malformed files are `Error::Parse`, never a
//!   panic).
//! * [`dumper`]: dumpers (upstream `Dumpable` + IOHelper): ParaView VTU
//!   (ASCII/base64 binary, `.pvd` time series, per-rank pieces listed by a
//!   `.pvtu`), text/CSV and LAMMPS data files. Dumpers store [`dumper::FieldDescriptor`]s under the
//!   upstream names (`displacement`, `stress`, `grad_u`, `material_index`,
//!   …) and read live data at dump time through a
//!   [`dumper::FieldResolver`] supplied by the model.
//!
//! Output files are byte-compatible with upstream where the format is
//! deterministic (text dumper, ASCII VTU) so existing post-processing
//! scripts keep working. The exception is the default ParaView layout of
//! matrix-valued fields, of tuples whose length differs from the declared
//! component count and of base64 arrays under 2 bytes (UD-110, UD-113,
//! UD-114); `DumperParaview::set_compat_layout(true)` restores the upstream
//! bytes.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dumper;
pub mod mesh_io;

pub use dumper::{
    DerivedQuantity, DumpBuffer, Dumpable, Dumper, DumperLammps, DumperParaview, DumperText,
    FieldDescriptor, FieldResolver, FieldSource, FieldSupport, FieldValueType, FileStorageMode,
    LammpsAtomStyle, MAX_EXPLICIT_PARALLEL_SIZE, MAX_TEXT_PRECISION, MeshFilter, MeshSelection,
    Padding, ParallelContext, TextDumpMode, TextDumper, pad_to_3d, solid_mechanics_field,
};
pub use mesh_io::{
    ReadOptions, ReadReport, WriteOptions, detect_format, read_mesh, read_mesh_from_str,
    read_mesh_with, write_mesh, write_mesh_to_string, write_mesh_with,
};
