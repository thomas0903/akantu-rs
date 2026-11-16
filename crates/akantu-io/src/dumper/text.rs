// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/dumper/dumper_text.{hh,cc}
// (DumperText) and third-party/iohelper/src/dumper_text.{hh,cc}
// (iohelper::DumperText; IOHelper, Copyright EPFL/LSMS,
// LGPL-3.0-or-later).
// IOHelper (third-party/iohelper, v1.1.1): Copyright (©) 2010-2012, 2014 EPFL
// (Ecole Polytechnique Fédérale de Lausanne), Laboratory (LSMS - Laboratoire
// de Simulation en Mécanique des Solides). IOHelper is free software: you can
// redistribute it and/or modify it under the terms of the GNU Lesser General
// Public License as published by the Free Software Foundation, either version
// 3 of the License, or (at your option) any later version.

//! Text dumper (upstream `DumperText`, DUMP-005).
//!
//! Files (byte-compatible with upstream, `fixtures/dump/text*`):
//! * `<dir>/<base>.info`, `<dir>/<base>.fields` — written at the first
//!   dump (description of the registered fields and variables);
//! * `<dir>/<base>-DataFiles/<base>_<field>[.proc<rank>].out|.csv` — one
//!   line per tuple, then an empty line, appended at every dump
//!   (truncated at the first);
//! * `<dir>/<base>-DataFiles/<base>_<variable>.out|.csv` — one line per
//!   dump (root rank only);
//! * `<dir>/<base>.time` — `<step> <time>` per dump once a time step is
//!   set (root rank only).
//!
//! Numbers: `std::scientific` with the precision (default 6) for reals,
//! plain integers, `0`/`1` for booleans; separator `' '` (`.out`) or
//! `','` (`.csv`).
//!
//! UD-052 (fixed): in parallel upstream `registerMesh` pairs the
//! `NodeFlag` nodal field with a `ContactState` conversion functor and
//! throws `std::bad_cast` (`fixtures/dump/text_mpi_mesh`); akantu-rs dumps
//! `nodes_type` as the integer value of each `NodeFlag` on both paths, as
//! upstream's filtered path does (`fixtures/dump/text_mpi_filtered`).
//! Deviations: a tuple index beyond the tuple (padded nodal fields) is
//! written as 0 (upstream reads out of bounds); files that cannot be
//! opened are an error (upstream silently drops the output). A field
//! restricted to an element group is written over the dumper's mesh part
//! (rows match `position`) and must contain every written node/element
//! (as the ParaView writer). A failed dump — render, directory or
//! write error — appends nothing (appended bytes are rolled back), keeps
//! the count, clock and the variable/time files of earlier successful
//! dumps, and does not remember the paths it failed to write.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use akantu_core::{Error, Real, Result};
use akantu_mesh::Mesh;

use super::base::{Common, Entry, OutFile, Parallel, WriteMode, file_name, write_files};
use super::cfmt::{format_e, push_e, push_i64, push_u64};
use super::eval::{self, EvalContext, Evaluated, Values};
use super::{
    Dumper, FieldDescriptor, FieldResolver, FieldSource, FieldSupport, FieldValueType,
    MeshSelection, Padding, ParallelContext,
};

/// IOHelper `TextDumpMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextDumpMode {
    /// `_tdm_space`: `' '` separator, `.out` files.
    #[default]
    Space,
    /// `_tdm_csv`: `','` separator, `.csv` files.
    Csv,
}

impl TextDumpMode {
    fn separator(self) -> char {
        match self {
            TextDumpMode::Space => ' ',
            TextDumpMode::Csv => ',',
        }
    }

    fn extension(self) -> &'static str {
        match self {
            TextDumpMode::Space => ".out",
            TextDumpMode::Csv => ".csv",
        }
    }

    fn name(self) -> &'static str {
        match self {
            TextDumpMode::Space => "space",
            TextDumpMode::Csv => "csv",
        }
    }
}

/// Text dumper (upstream `DumperText`).
#[derive(Clone, Debug)]
pub struct DumperText {
    common: Common,
    mode: TextDumpMode,
    precision: usize,
    data_sub_directory: String,
    is_first_dump: bool,
    /// `registerMesh`: add `nodes_type` when running on several ranks.
    nodes_type_auto: bool,
    /// Files kept open by upstream (variables, time): path and the
    /// precision they were opened with.
    open_files: BTreeMap<String, (String, usize)>,
}

const TIME_FILE: &str = "\u{0}time";

/// Largest precision accepted by [`DumperText::set_precision`] (the
/// limit of Rust's formatting; upstream accepts any `int`, but digits past
/// the ~770 significant digits of an exact `double` are all zeros).
pub const MAX_TEXT_PRECISION: usize = u16::MAX as usize;

/// The files of one dump and the state to commit once they are written.
struct Rendered {
    files: Vec<OutFile>,
    dirs: Vec<String>,
    /// Variable/time files opened by this dump (kept by upstream).
    opened: Vec<(String, (String, usize))>,
    /// The time description advanced the clock.
    advance_time: bool,
}

impl DumperText {
    /// `DumperText(basename = "dumper_text", mode = _tdm_space, parallel =
    /// true)`; output directory `./`.
    #[must_use]
    pub fn new(basename: &str, mode: TextDumpMode, parallel: bool) -> Self {
        let parallel = if parallel {
            ParallelContext::FromMesh
        } else {
            ParallelContext::Serial
        };
        Self {
            common: Common::new(basename, "./", parallel),
            mode,
            precision: 6,
            data_sub_directory: format!("{basename}-DataFiles/"),
            is_first_dump: true,
            nodes_type_auto: false,
            open_files: BTreeMap::new(),
        }
    }

    /// `setPrecision` (digits after the decimal point of reals; at most
    /// [`MAX_TEXT_PRECISION`]).
    pub fn set_precision(&mut self, precision: usize) -> Result<()> {
        if precision > MAX_TEXT_PRECISION {
            return Err(Error::InvalidArgument(format!(
                "text dumper precision {precision} exceeds {MAX_TEXT_PRECISION}"
            )));
        }
        self.precision = precision;
        Ok(())
    }

    /// `setDumpMode`.
    pub fn set_dump_mode(&mut self, mode: TextDumpMode) {
        self.mode = mode;
    }

    /// `setDataSubDirectory` (set to `<base>-DataFiles` by
    /// `set_base_name`; the raw IOHelper dumper uses `""`).
    pub fn set_data_sub_directory(&mut self, sub: &str) {
        self.data_sub_directory = super::base::directory_name(sub);
    }

    /// `iohelper::Dumper::setParallelContext`.
    pub fn set_parallel_context(&mut self, ctx: ParallelContext) {
        self.common.parallel = ctx;
    }

    fn format_value(&self, v: &Values, k: usize, precision: usize, out: &mut String) {
        match v {
            Values::F64(a) => push_e(out, a.get(k).copied().unwrap_or(0.0), precision),
            Values::I32(a) => push_i64(out, i64::from(a.get(k).copied().unwrap_or(0))),
            Values::U32(a) => push_u64(out, u64::from(a.get(k).copied().unwrap_or(0))),
            Values::Bool(a) => out.push(if a.get(k).copied().unwrap_or(false) {
                '1'
            } else {
                '0'
            }),
        }
    }

    /// `visitField`: `dim` values per tuple (linear index into the tuple,
    /// 0 beyond it), then an empty line.
    fn field_text(&self, f: &Evaluated) -> String {
        let sep = self.mode.separator();
        let mut s = String::new();
        for b in &f.blocks {
            for t in 0..b.nb_tuples {
                for i in 0..f.dim {
                    if i != 0 {
                        s.push(sep);
                    }
                    if i < b.tuple_len {
                        self.format_value(&b.values, t * b.tuple_len + i, self.precision, &mut s);
                    } else {
                        self.format_value(&b.values, usize::MAX, self.precision, &mut s);
                    }
                }
                s.push('\n');
            }
        }
        s.push('\n');
        s
    }

    fn nodes_type_descriptor() -> FieldDescriptor {
        FieldDescriptor {
            name: "nodes_type".into(),
            source: FieldSource::Mesh("nodes_flags".into()),
            support: FieldSupport::Nodal,
            value_type: FieldValueType::Int,
            group: None,
            padding: Padding::None,
            homogenize: false,
        }
    }

    /// Validate and render one dump without touching the disk or the
    /// dumper state.
    #[allow(clippy::too_many_lines)] // one linear port of DumperText::dump
    fn render(
        &self,
        mesh: &Mesh,
        resolver: &dyn FieldResolver,
        step: usize,
        par: &Parallel,
    ) -> Result<Rendered> {
        let ctx = EvalContext {
            mesh,
            resolver,
            rank: akantu_core::Rank::try_from(par.rank).map_err(|_| {
                Error::InvalidArgument(format!("dump rank {} exceeds the rank range", par.rank))
            })?,
        };
        let selection = self.common.selection_for(mesh);
        let sel = selection.resolve(mesh, None)?;
        // nodal and elemental fields in name order
        let mut nodal: BTreeMap<String, Evaluated> = BTreeMap::new();
        let mut elemental: BTreeMap<String, Evaluated> = BTreeMap::new();
        let auto = (self.nodes_type_auto
            && par.size > 1
            && !self.common.entries.contains_key("nodes_type"))
        .then(Self::nodes_type_descriptor);
        let entries = self
            .common
            .entries
            .iter()
            .map(|(n, e)| (n.clone(), e.clone()))
            .chain(auto.map(|d| ("nodes_type".to_owned(), Entry::Field(d))));
        for (name, entry) in entries {
            match entry {
                Entry::Positions => {
                    nodal.insert(name, eval::positions(mesh, &sel, false));
                }
                Entry::Connectivity => {
                    let blocks = eval::connectivity(mesh, &sel)?;
                    let dim = blocks.first().map_or(0, |b| b.tuple_len);
                    let homogeneous = blocks.iter().all(|b| b.tuple_len == dim);
                    elemental.insert(
                        name,
                        Evaluated {
                            scalar: eval::Scalar::I32,
                            dim,
                            homogeneous,
                            padded: false,
                            blocks,
                        },
                    );
                }
                Entry::Field(desc) => {
                    if let Some(g) = desc.group.as_deref() {
                        let fsel = selection.resolve(mesh, Some(g))?;
                        eval::check_group_covers(&name, g, desc.support, &sel, &fsel)?;
                    }
                    let f = eval::evaluate(&ctx, &desc, &sel)?;
                    if !f.homogeneous {
                        return Err(Error::InvalidArgument(format!(
                            "dump field '{name}' is not homogeneous"
                        )));
                    }
                    match desc.support {
                        FieldSupport::Nodal => nodal.insert(name, f),
                        _ => elemental.insert(name, f),
                    };
                }
            }
        }
        let mut variables = BTreeMap::new();
        for name in &self.common.variables {
            variables.insert(name.clone(), eval::variable(resolver, name)?);
        }

        let prefix = self.common.prefix.clone();
        let base = self.common.base_name.clone();
        let data_dir = self.data_sub_directory.clone();
        let ext = self.mode.extension();
        let mut files: Vec<OutFile> = Vec::new();
        let mut opened = Vec::new();
        let mut advance_time = false;
        if self.is_first_dump {
            let mut fields = String::from(
                "# [0]-field_name [1]-nodal_element_global_data [2]-data_type [3]-dump_mode [4]-nb_components [5]-rel_file_path\n",
            );
            let mode = self.mode.name();
            for (tag, list) in [("N", &nodal), ("E", &elemental)] {
                for (name, f) in list {
                    let _ = writeln!(
                        fields,
                        "{name} {tag} {} {mode} {} {data_dir}",
                        f.scalar.text_name(),
                        f.dim
                    );
                }
            }
            for (name, v) in &variables {
                let len = match v {
                    Values::F64(a) => a.len(),
                    Values::I32(a) => a.len(),
                    Values::U32(a) => a.len(),
                    Values::Bool(a) => a.len(),
                };
                let _ = writeln!(
                    fields,
                    "{name} G {} {mode} {len} {data_dir}",
                    v.scalar().text_name()
                );
            }
            files.push(OutFile::new(
                format!("{prefix}{base}.fields"),
                fields,
                WriteMode::Replace,
            ));
            let mut info = String::from(
                "# [0]-version [1]-counter-width [2]-world-size [3]-proc-width [4]-file-per-time-step\n",
            );
            let _ = writeln!(info, "0-1 4 {} {} 0\n", par.size, par.rank_width);
            let _ = writeln!(info, "base_name {base}");
            let _ = writeln!(info, "field_description {base}.fields");
            if self.common.write_time_desc {
                let _ = writeln!(info, "time_description {base}.time");
            }
            files.push(OutFile::new(
                format!("{prefix}{base}.info"),
                info,
                WriteMode::Replace,
            ));
        }
        for (name, f) in nodal.iter().chain(elemental.iter()) {
            let path = format!(
                "{prefix}{data_dir}{}",
                file_name(&format!("{base}_{name}"), None, Some((par.rank, par)), ext)
            );
            let mode = if self.is_first_dump {
                WriteMode::Replace
            } else {
                WriteMode::Append
            };
            files.push(OutFile::new(path, self.field_text(f), mode));
        }
        if par.rank == par.root {
            let sep = self.mode.separator();
            for (name, v) in &variables {
                let (path, prec, mode) = match self.open_files.get(name) {
                    Some((p, prec)) => (p.clone(), *prec, WriteMode::Append),
                    None => {
                        let p = format!("{prefix}{data_dir}{base}_{name}{ext}");
                        opened.push((name.clone(), (p.clone(), self.precision)));
                        (p, self.precision, WriteMode::Replace)
                    }
                };
                let len = match v {
                    Values::F64(a) => a.len(),
                    Values::I32(a) => a.len(),
                    Values::U32(a) => a.len(),
                    Values::Bool(a) => a.len(),
                };
                let mut line = String::new();
                for i in 0..len {
                    if i != 0 {
                        line.push(sep);
                    }
                    self.format_value(v, i, prec, &mut line);
                }
                line.push('\n');
                files.push(OutFile::new(path, line, mode));
            }
            if self.common.write_time_desc {
                let (path, prec, mode) = match self.open_files.get(TIME_FILE) {
                    Some((p, prec)) => (p.clone(), *prec, WriteMode::Append),
                    None => {
                        let p = format!("{prefix}{base}.time");
                        opened.push((TIME_FILE.to_owned(), (p.clone(), self.precision)));
                        (p, self.precision, WriteMode::Replace)
                    }
                };
                let line = format!("{step} {}\n", format_e(self.common.current_time, prec));
                files.push(OutFile::new(path, line, mode));
                advance_time = true;
            }
        }
        let dirs = vec![
            format!("{prefix}{data_dir}"),
            if prefix.is_empty() {
                ".".to_owned()
            } else {
                prefix
            },
        ];
        Ok(Rendered {
            files,
            dirs,
            opened,
            advance_time,
        })
    }

    fn dump_steps(
        &mut self,
        mesh: &Mesh,
        resolver: &dyn FieldResolver,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        let par = self.common.parallel.resolve(Some(mesh))?;
        let step = self.common.begin_dump(time, step)?;
        let r = self.render(mesh, resolver, step, &par)?;
        let dirs: Vec<&str> = r.dirs.iter().map(String::as_str).collect();
        write_files(&dirs, &r.files)?;
        // commit
        self.open_files.extend(r.opened);
        if r.advance_time {
            self.common.current_time += self.common.time_step;
        }
        self.is_first_dump = false;
        self.common.end_dump();
        Ok(())
    }
}

impl Dumper for DumperText {
    /// `registerMesh`: the `position` field (plus `nodes_type` on several
    /// ranks).
    fn register_mesh(
        &mut self,
        spatial_dimension: usize,
        ghost: akantu_core::GhostType,
        kind: akantu_core::ElementKind,
    ) -> Result<()> {
        self.register_mesh_selection(MeshSelection::new(Some(spatial_dimension), ghost, kind))
    }

    fn register_mesh_selection(&mut self, selection: MeshSelection) -> Result<()> {
        self.common.insert("position", Entry::Positions)?;
        self.common.selection = Some(selection);
        self.nodes_type_auto = true;
        Ok(())
    }

    fn register_field(&mut self, field: FieldDescriptor) -> Result<()> {
        let name = field.name.clone();
        self.common.insert(&name, Entry::Field(field))
    }

    fn unregister_field(&mut self, name: &str) -> Result<()> {
        self.common.remove(name)
    }

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
        self.data_sub_directory = format!("{base_name}-DataFiles/");
        Ok(())
    }

    fn set_time_step(&mut self, time_step: Real) -> Result<()> {
        self.common.set_time_step(time_step);
        Ok(())
    }

    fn dump(
        &mut self,
        mesh: &Mesh,
        resolver: &dyn FieldResolver,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        let saved = self.common.snapshot();
        let r = self.dump_steps(mesh, resolver, time, step);
        if r.is_err() {
            self.common.restore(saved);
        }
        r
    }

    fn set_parallel_context(&mut self, ctx: ParallelContext) -> Result<()> {
        self.common.parallel = ctx;
        Ok(())
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
