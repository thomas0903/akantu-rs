// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): third-party/iohelper/src/dumper_lammps.{hh,cc}
// (DumperLammps<atomic>, DumperLammps<bond>; IOHelper, Copyright
// EPFL/LSMS, LGPL-3.0-or-later).
// IOHelper (third-party/iohelper, v1.1.1): Copyright (©) 2010-2012, 2014 EPFL
// (Ecole Polytechnique Fédérale de Lausanne), Laboratory (LSMS - Laboratoire
// de Simulation en Mécanique des Solides). IOHelper is free software: you can
// redistribute it and/or modify it under the terms of the GNU Lesser General
// Public License as published by the Free Software Foundation, either version
// 3 of the License, or (at your option) any later version.

//! IOHelper LAMMPS data-file writers (DUMP-017 atomic, DUMP-018 bond).
//!
//! `<dir>/<base>[.proc<rank>]` (no extension, no step; every dump
//! overwrites the file):
//!
//! ```text
//! LAMMPS data file
//!
//!
//! <n> atoms                              (padded to 37 characters)
//! 0 bonds
//! 1 atom types
//! 0 bond types
//!
//! <xlo> <xhi> xlo xhi                    (only with bounds)
//! <ylo> <yhi> ylo yhi
//! <zlo> <zhi> zlo zhi
//!
//! Atoms
//!
//! <i> 1 <x> <y> [<z>]                    (atomic)
//! <i> 3 1 <x> <y> [<z>]                  (bond: grain id 1 + 2)
//! ```
//!
//! Coordinates use `%.16e`; the bounds use the stream's state, which is
//! `%g` for the first dump of a dumper and `%.16e` afterwards (the
//! scientific flags set by the first header persist on the member stream;
//! reproduced, `fixtures/dump/lammps`).
//!
//! Variables are accepted and ignored, as upstream. A failed
//! dump leaves the count and the stream state unchanged.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use akantu_core::{Error, Real, Result};
use akantu_mesh::Mesh;

use super::base::{Common, Entry, OutFile, Parallel, WriteMode, file_name, write_files};
use super::cfmt::{format_default, format_e};
use super::eval;
use super::{Dumper, FieldDescriptor, FieldResolver, MeshSelection, ParallelContext};

/// IOHelper `LammpsAtomStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LammpsAtomStyle {
    /// `atomic`: `id type x y z`.
    Atomic,
    /// `bond`: `id molecule type x y z` (molecule = grain id + 2 = 3).
    Bond,
}

/// LAMMPS writer (upstream `iohelper::DumperLammps<style>`).
#[derive(Clone, Debug)]
pub struct DumperLammps {
    common: Common,
    style: LammpsAtomStyle,
    bounds: Option<[Real; 6]>,
    /// The member stream has been switched to `scientific, precision 16`.
    scientific_stream: bool,
}

impl DumperLammps {
    /// `DumperLammps<style>(bounds, prefix = "./")` with base name `name`.
    #[must_use]
    pub fn new(name: &str, style: LammpsAtomStyle, bounds: Option<[Real; 6]>) -> Self {
        Self {
            common: Common::new(name, "./", ParallelContext::IoHelperDefault),
            style,
            bounds,
            scientific_stream: false,
        }
    }

    /// Render one data file for `points` (`nb × dim`, tuple-major); the
    /// caller sets `scientific_stream` once the file is written.
    fn render(&self, points: &[Real], dim: usize) -> String {
        let mut s = String::from("LAMMPS data file\n\n\n");
        let nb = points.len().checked_div(dim).unwrap_or(0);
        let _ = writeln!(s, "{:<37}", format!("{nb} atoms"));
        s.push_str("0 bonds\n1 atom types\n0 bond types\n");
        if let Some(b) = self.bounds {
            let f = |x: Real| {
                if self.scientific_stream {
                    format_e(x, 16)
                } else {
                    format_default(x)
                }
            };
            s.push('\n');
            let _ = writeln!(s, "{} {} xlo xhi", f(b[0]), f(b[1]));
            let _ = writeln!(s, "{} {} ylo yhi", f(b[2]), f(b[3]));
            let _ = writeln!(s, "{} {} zlo zhi", f(b[4]), f(b[5]));
            s.push('\n');
        }
        s.push_str("Atoms\n\n");
        for (i, p) in points.chunks(dim.max(1)).take(nb).enumerate() {
            match self.style {
                LammpsAtomStyle::Atomic => {
                    let _ = write!(s, "{} 1 ", i + 1);
                }
                LammpsAtomStyle::Bond => {
                    let _ = write!(s, "{} 3 1 ", i + 1);
                }
            }
            for &x in p {
                let _ = write!(s, "{} ", format_e(x, 16));
            }
            s.push('\n');
        }
        s
    }

    /// Dump raw points (upstream `setPoints(points, dim, nb, name)` +
    /// `dump()`), e.g. atoms not stored in a mesh.
    pub fn dump_points(&mut self, points: &[Real], dim: usize) -> Result<()> {
        if dim == 0 || !points.len().is_multiple_of(dim) {
            return Err(Error::DimensionMismatch {
                context: "LAMMPS points (values vs dimension)",
                expected: dim,
                got: points.len(),
            });
        }
        self.transaction(|me| {
            let par = me.common.parallel.resolve(None)?;
            me.common.begin_dump(None, None)?;
            Ok((me.render(points, dim), par))
        })
    }

    /// Run `render`, write its file and commit; restore the state on any
    /// error.
    fn transaction(
        &mut self,
        render: impl FnOnce(&mut Self) -> Result<(String, Parallel)>,
    ) -> Result<()> {
        let saved = self.common.snapshot();
        let r = render(self).and_then(|(text, par)| {
            let prefix = &self.common.prefix;
            let path = format!(
                "{prefix}{}",
                file_name(&self.common.base_name, None, Some((par.rank, &par)), "")
            );
            write_files(
                &[prefix.as_str()],
                &[OutFile::new(path, text, WriteMode::Replace)],
            )
        });
        match r {
            Ok(()) => {
                self.scientific_stream = true;
                self.common.end_dump();
                Ok(())
            }
            Err(e) => {
                self.common.restore(saved);
                Err(e)
            }
        }
    }
}

impl Dumper for DumperLammps {
    fn register_mesh(
        &mut self,
        spatial_dimension: usize,
        ghost: akantu_core::GhostType,
        kind: akantu_core::ElementKind,
    ) -> Result<()> {
        self.register_mesh_selection(MeshSelection::new(Some(spatial_dimension), ghost, kind))
    }

    fn register_mesh_selection(&mut self, selection: MeshSelection) -> Result<()> {
        self.common.insert("positions", Entry::Positions)?;
        self.common.selection = Some(selection);
        Ok(())
    }

    /// Fields other than the positions are ignored by the LAMMPS writers
    /// (upstream only visits `positions`); registering one is accepted.
    fn register_field(&mut self, field: FieldDescriptor) -> Result<()> {
        let name = field.name.clone();
        self.common.insert(&name, Entry::Field(field))
    }

    fn unregister_field(&mut self, name: &str) -> Result<()> {
        self.common.remove(name)
    }

    /// Accepted and never evaluated: the LAMMPS writers do not visit
    /// variables.
    fn register_variable(&mut self, name: &str) -> Result<()> {
        self.common.add_variable(name)
    }

    fn unregister_variable(&mut self, name: &str) -> Result<()> {
        self.common.remove_variable(name)
    }

    fn set_directory(&mut self, directory: &Path) -> Result<()> {
        self.common.set_directory(directory)
    }

    fn set_base_name(&mut self, base_name: &str) -> Result<()> {
        self.common.base_name = base_name.to_owned();
        Ok(())
    }

    fn set_parallel_context(&mut self, ctx: ParallelContext) -> Result<()> {
        self.common.parallel = ctx;
        Ok(())
    }

    fn dump(
        &mut self,
        mesh: &Mesh,
        _resolver: &dyn FieldResolver,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        if !self.common.entries.contains_key("positions") {
            return Err(Error::InvalidArgument(
                "LAMMPS dump without positions: register a mesh first".into(),
            ));
        }
        self.transaction(|me| {
            let par = me.common.parallel.resolve(Some(mesh))?;
            me.common.begin_dump(time, step)?;
            let selection = me.common.selection_for(mesh);
            let sel = selection.resolve(mesh, None)?;
            let pos = eval::positions(mesh, &sel, false);
            let Some(eval::Values::F64(points)) = pos.blocks.first().map(|b| &b.values) else {
                return Err(Error::InvalidArgument("positions must be real".into()));
            };
            Ok((me.render(points, pos.dim), par))
        })
    }

    fn rename_groups(&mut self, element: bool, renames: &BTreeMap<String, String>) {
        self.common.rename_groups(element, renames);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
