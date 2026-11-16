// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/dumper/dumper_iohelper_paraview.{hh,cc}
// (DumperParaview), third-party/iohelper/src/dumper_paraview.{hh,cc},
// paraview_helper.{hh,cc,tcc} (VTU/PVTU/PVD writing, ASCII and base64
// modes; IOHelper, Copyright EPFL/LSMS, LGPL-3.0-or-later).
// IOHelper (third-party/iohelper, v1.1.1): Copyright (©) 2010-2012, 2014 EPFL
// (Ecole Polytechnique Fédérale de Lausanne), Laboratory (LSMS - Laboratoire
// de Simulation en Mécanique des Solides). IOHelper is free software: you can
// redistribute it and/or modify it under the terms of the GNU Lesser General
// Public License as published by the Free Software Foundation, either version
// 3 of the License, or (at your option) any later version.

//! ParaView dumper: `.pvtu` + per-rank `.vtu` pieces + `.pvd` time series.
//!
//! Byte-compatible with IOHelper's writer (`fixtures/dump/*`): same XML
//! text, same base64 stream (see [`super::base64`]), same ASCII number
//! layout (`%22.15e` doubles, 3/`dim` per line; integers on one line; the
//! running value counter that is only reset by data fields), same file
//! names (`<dir>/<base>_<step:04>.pvtu`, `<dir>/<base>-VTU/<base>_<step:04>
//! [.proc<rank>].vtu`, `<dir>/<first base>.pvd`).
//!
//! Corrected by default (UD-110, UD-113, UD-114; oracle evidence in
//! `fixtures/dump/smm3d`, `m2_raw_qp`): matrix-valued fields (`strain`,
//! `Green strain`, principal strains) are written with exactly
//! `NumberOfComponents` values per tuple (upstream writes `dim × dim`
//! zero-filled values, e.g. 81 for a 9-component strain), and tuples whose
//! length differs from `NumberOfComponents` are an error (upstream writes
//! them as they are); base64 arrays of fewer than 2 bytes use the standard
//! encoding (see [`super::base64`]). [`DumperParaview::set_compat_layout`] reproduces the
//! upstream bytes for differential tests.
//!
//! Robustness (upstream writes whatever it is given):
//! * every field is written over the dumper's mesh part; a field
//!   restricted to an element group takes its values by node/element
//!   identity and is rejected unless its group contains every written
//!   node/cell ([`eval::check_group_covers`]); the tuple counts
//!   and element-type blocks of every data array are checked against the
//!   points and cells;
//! * user-supplied names and paths are XML-escaped in attributes; names
//!   holding characters XML 1.0 cannot represent are an error;
//! * everything is validated and rendered before the first byte is
//!   written (the `.vtu` piece is validated first and then written straight
//!   to its file through a buffer when the files are written, without a
//!   copy of the whole file in memory); the `.vtu` piece is written before
//!   the `.pvtu` that lists it
//!   and the `.pvd` (replaced atomically) last; a failed dump — render,
//!   directory or write error — leaves the dump count, clock and time
//!   series unchanged, so a retry writes a correct history.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use akantu_core::{Error, Real, Result};
use akantu_mesh::Mesh;

use super::base::{
    Common, Entry, OutFile, Parallel, StreamContents, WriteMode, file_name, write_files,
};
use super::base64::Base64Stream;
use super::cfmt::{format_default, push_e_width, push_i64};
use super::eval::{self, Block, EvalContext, Evaluated, Scalar, Values};
use super::vtk::iohelper_type;
use super::{
    Dumper, FieldDescriptor, FieldResolver, FieldSupport, MeshFilter, MeshSelection,
    ParallelContext,
};

/// IOHelper `FileStorageMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FileStorageMode {
    /// `TEXT`: ASCII data arrays (`setTextModeToDumper`).
    Text,
    /// `BASE64` (default of `DumperParaview`): inline base64 binary.
    #[default]
    Base64,
    /// `COMPRESSED`: never implemented by IOHelper — the helper keeps only
    /// the base64 bit, so the output is ASCII (reproduced).
    Compressed,
}

/// ParaView dumper (upstream `DumperParaview`).
#[derive(Clone, Debug)]
pub struct DumperParaview {
    common: Common,
    mode: FileStorageMode,
    vtu_sub_directory: String,
    compat_layout: bool,
    /// Root rank only: `(time, relative .pvtu path)` of every dump.
    pvtu_file_names: Vec<(Real, String)>,
}

/// The `.vtu` piece of one dump: its validated data, written to the file
/// when the dump's files are written.
#[derive(Debug)]
struct VtuJob {
    base64: bool,
    compat_layout: bool,
    positions: Evaluated,
    cells: Option<Vec<Block>>,
    nodal: NamedFields,
    elemental: NamedFields,
}

/// Number of written cells (1: the poly-vertex of a dumper without
/// elements).
fn nb_cells(cells: Option<&[Block]>) -> usize {
    cells.map_or(1, |c| c.iter().map(|b| b.nb_tuples).sum())
}

/// The checks of `DumperParaview::dump` on the data of a piece, in the
/// order the writer meets them: element types without a VTK cell, nodal
/// fields of another size than the dumped nodes, elemental fields on a
/// dumper without elements or not aligned with the written cells.
fn check_vtu(
    positions: &Evaluated,
    cells: Option<&[Block]>,
    nodal: &[(String, Evaluated)],
    elemental: &[(String, Evaluated)],
) -> Result<()> {
    let nb_nodes = positions.nb_tuples();
    let nb_cells = nb_cells(cells);
    if let Some(blocks) = cells {
        for b in blocks {
            if let Some(ty) = b.ty {
                iohelper_type(ty)?;
            }
        }
    }
    for (_, f) in nodal {
        if f.nb_tuples() != nb_nodes {
            return Err(Error::DimensionMismatch {
                context: "nodal dump field vs dumped nodes",
                expected: nb_nodes,
                got: f.nb_tuples(),
            });
        }
    }
    for (name, f) in elemental {
        let Some(cells) = cells else {
            return Err(Error::InvalidArgument(format!(
                "elemental dump field '{name}' on a dumper without elements (node group)"
            )));
        };
        // one tuple per written cell, block by block
        let aligned = f.blocks.len() == cells.len()
            && f.blocks
                .iter()
                .zip(cells)
                .all(|(b, c)| b.ty == c.ty && b.nb_tuples == c.nb_tuples);
        if !aligned {
            return Err(Error::DimensionMismatch {
                context: "elemental dump field vs dumped cells",
                expected: nb_cells,
                got: f.nb_tuples(),
            });
        }
    }
    Ok(())
}

/// An error of a validated piece (cannot happen).
fn internal(e: &Error) -> std::io::Error {
    std::io::Error::other(format!("internal error while writing a VTU piece: {e}"))
}

impl StreamContents for VtuJob {
    /// The text of `DumperParaview::dump` (upstream `ParaviewHelper`).
    #[allow(clippy::too_many_lines)] // one linear port of DumperParaview::dump
    fn write_to(&self, w: &mut dyn std::io::Write) -> std::io::Result<()> {
        let mut v = Vtu::new(self.base64, self.compat_layout, w);
        let cells = self.cells.as_deref();
        let positions = &self.positions;
        let nb_nodes = positions.nb_tuples();
        let nb_cells = nb_cells(cells);
        v.out
            .push_str("<VTKFile type=\"UnstructuredGrid\" version=\"0.1\" ");
        v.out.push_str("byte_order=\"LittleEndian\">\n");
        v.out.push_str(" <UnstructuredGrid>\n");
        let _ = writeln!(
            v.out,
            "  <Piece NumberOfPoints= \"{nb_nodes}\" NumberOfCells=\"{nb_cells}\">"
        );
        v.out.push_str("   <Points>\n");
        v.start_data("positions", 3, "Float64", field_payload(positions, true));
        v.write_field(positions, true);
        v.end_data();
        v.out.push_str("   </Points>\n");
        v.out.push_str("   <Cells>\n");
        // connectivity
        let payload = match cells {
            None => 4 * nb_nodes,
            Some(blocks) => {
                let mut n = 0;
                for b in blocks {
                    let Some(ty) = b.ty else { continue };
                    if matches!(b.values, Values::I32(_)) {
                        let h = iohelper_type(ty).map_err(|e| internal(&e))?;
                        n += 4 * b.nb_tuples * h.reorder().len();
                    }
                }
                n
            }
        };
        v.start_data("connectivity", 0, "Int32", payload);
        match cells {
            None => {
                for i in 0..nb_nodes {
                    let i = u32::try_from(i).unwrap_or(u32::MAX);
                    v.push_int(i, &i.to_le_bytes());
                }
            }
            Some(blocks) => {
                for b in blocks {
                    let Some(ty) = b.ty else { continue };
                    let h = iohelper_type(ty).map_err(|e| internal(&e))?;
                    let Values::I32(conn) = &b.values else {
                        continue;
                    };
                    for t in 0..b.nb_tuples {
                        let row = &conn[t * b.tuple_len..(t + 1) * b.tuple_len];
                        for &k in h.reorder() {
                            v.push_int(row[k], &row[k].to_le_bytes());
                        }
                    }
                }
            }
        }
        v.end_data();
        // offsets and types: one value per cell
        let per_cell = match cells {
            None => 4,
            Some(blocks) => blocks
                .iter()
                .filter(|b| b.ty.is_some())
                .map(|b| 4 * b.nb_tuples)
                .sum(),
        };
        v.start_data("offsets", 0, "Int32", per_cell);
        match cells {
            None => {
                let n = u32::try_from(nb_nodes).unwrap_or(u32::MAX);
                v.push_int(n, &n.to_le_bytes());
            }
            Some(blocks) => {
                let mut count: u32 = 0;
                for b in blocks {
                    let Some(ty) = b.ty else { continue };
                    let h = iohelper_type(ty).map_err(|e| internal(&e))?;
                    let n = u32::try_from(h.nb_nodes()).unwrap_or(0);
                    for _ in 0..b.nb_tuples {
                        count = count.wrapping_add(n);
                        v.push_int(count, &count.to_le_bytes());
                    }
                }
            }
        }
        v.end_data();
        v.start_data("types", 0, "UInt32", per_cell);
        match cells {
            None => v.push_int(2u32, &2u32.to_le_bytes()), // VTK_POLY_VERTEX
            Some(blocks) => {
                for b in blocks {
                    let Some(ty) = b.ty else { continue };
                    let code = iohelper_type(ty).map_err(|e| internal(&e))?.vtk_cell_type();
                    for _ in 0..b.nb_tuples {
                        v.push_int(code, &code.to_le_bytes());
                    }
                }
            }
        }
        v.end_data();
        v.out.push_str("   </Cells>\n");
        // data fields
        v.out.push_str("   <PointData>\n");
        for (name, f) in &self.nodal {
            v.start_data(
                name,
                f.dim,
                f.scalar.vtk_name(),
                field_payload(f, self.compat_layout),
            );
            v.write_field(f, self.compat_layout);
            v.end_data();
        }
        v.out.push_str("   </PointData>\n");
        v.out.push_str("   <CellData>\n");
        for (name, f) in &self.elemental {
            v.start_data(
                name,
                f.dim,
                f.scalar.vtk_name(),
                field_payload(f, self.compat_layout),
            );
            v.write_field(f, self.compat_layout);
            v.end_data();
        }
        v.out.push_str("   </CellData>\n");
        v.out.push_str("  </Piece>\n");
        v.out.push_str(" </UnstructuredGrid>\n");
        v.out.push_str("</VTKFile>\n");
        v.finish()
    }
}

impl DumperParaview {
    /// `DumperParaview(filename, directory = "./paraview", parallel =
    /// true)`; `parallel = false` writes as rank 0 of 1, otherwise rank and
    /// size come from the mesh communicator at dump time.
    #[must_use]
    pub fn new(filename: &str, directory: &str, parallel: bool) -> Self {
        let parallel = if parallel {
            ParallelContext::FromMesh
        } else {
            ParallelContext::Serial
        };
        Self {
            common: Common::new(filename, directory, parallel),
            mode: FileStorageMode::Base64,
            vtu_sub_directory: format!("{filename}-VTU"),
            compat_layout: false,
            pvtu_file_names: Vec::new(),
        }
    }

    /// `DumperParaview(filename)` with the defaults.
    #[must_use]
    pub fn with_name(filename: &str) -> Self {
        Self::new(filename, "./paraview", true)
    }

    /// `setMode` / `setTextModeToDumper`.
    pub fn set_mode(&mut self, mode: FileStorageMode) {
        self.mode = mode;
    }

    /// Storage mode.
    #[must_use]
    pub fn mode(&self) -> FileStorageMode {
        self.mode
    }

    /// `setVTUSubDirectory` (set to `<base>-VTU` by `set_base_name`).
    pub fn set_vtu_sub_directory(&mut self, sub: &str) {
        self.vtu_sub_directory = super::base::directory_name(sub);
    }

    /// Reproduce upstream's data layout bugs (matrix tuples written as
    /// zero-filled `dim × dim` squares, tuples longer than
    /// `NumberOfComponents` written as they are). For differential tests
    /// against the oracle only: such files are not valid VTK.
    pub fn set_compat_layout(&mut self, on: bool) {
        self.compat_layout = on;
    }

    /// `iohelper::Dumper::setParallelContext` (overrides the communicator).
    pub fn set_parallel_context(&mut self, ctx: ParallelContext) {
        self.common.parallel = ctx;
    }

    /// Current dump count (`dump()` writes this step, then increments).
    #[must_use]
    pub fn count(&self) -> usize {
        self.common.count
    }

    /// Output base name.
    #[must_use]
    pub fn base_name(&self) -> &str {
        &self.common.base_name
    }

    fn vtu_folder(&self) -> String {
        super::base::directory_name(&self.vtu_sub_directory)
    }
}

/// `true` for the characters an XML 1.0 document can hold (`Char`
/// production: tab, line feed, carriage return, `U+0020..=U+D7FF`,
/// `U+E000..=U+FFFD`, `U+10000..`).
fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

/// Escape a user-supplied string for a double-quoted XML attribute:
/// `&`, `<`, `>`, `"` and the whitespace that attribute-value
/// normalisation would turn into spaces (tab, line feed, carriage return)
/// become references; characters XML cannot represent are an error.
/// Ordinary names are returned unchanged (byte-identical output).
pub(crate) fn xml_attr<'a>(what: &str, s: &'a str) -> Result<Cow<'a, str>> {
    if let Some(c) = s.chars().find(|&c| !is_xml_char(c)) {
        return Err(Error::InvalidArgument(format!(
            "{what} {s:?} contains the character U+{:04X}, which an XML (VTK) file cannot hold",
            u32::from(c)
        )));
    }
    if !s.contains(['&', '<', '>', '"', '\t', '\n', '\r']) {
        return Ok(Cow::Borrowed(s));
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    Ok(Cow::Owned(out))
}

/// Text written to the sink in chunks of about this size.
const VTU_CHUNK: usize = 1 << 16;

/// Bytes of one value of `v` in a binary array.
fn value_size(v: &Values) -> usize {
    match v {
        Values::F64(_) => 8,
        Values::I32(_) | Values::U32(_) => 4,
        Values::Bool(_) => 1,
    }
}

/// Bytes of one value of `s` in a binary array.
fn scalar_size(s: Scalar) -> usize {
    match s {
        Scalar::F64 => 8,
        Scalar::I32 | Scalar::U32 => 4,
        Scalar::Bool => 1,
    }
}

/// Payload bytes [`Vtu::write_field`] pushes for `f` (the same loops,
/// counted).
fn field_payload(f: &Evaluated, compat: bool) -> usize {
    let dim = f.dim;
    let zs = scalar_size(f.scalar);
    f.blocks
        .iter()
        .map(|b| {
            let vs = value_size(&b.values);
            let per_tuple = if b.rows > 0 && compat {
                let cols = b.tuple_len / b.rows;
                let values = dim.min(b.rows) * dim.min(cols);
                values * vs + (dim * dim - values) * zs
            } else {
                b.tuple_len * vs + dim.saturating_sub(b.tuple_len) * zs
            };
            b.nb_tuples * per_tuple
        })
        .sum()
}

/// Text writer implementing `ParaviewHelper`'s push functions; the text
/// goes to `sink` in chunks.
struct Vtu<'w> {
    out: String,
    sink: &'w mut dyn std::io::Write,
    /// First write error (later writes are skipped).
    err: Option<std::io::Error>,
    base64: bool,
    b64: Base64Stream,
    /// `compteur`: values pushed since the last data field started.
    compteur: usize,
}

impl<'w> Vtu<'w> {
    fn new(base64: bool, compat: bool, sink: &'w mut dyn std::io::Write) -> Self {
        Self {
            out: String::with_capacity(VTU_CHUNK + 4096),
            sink,
            err: None,
            base64,
            b64: Base64Stream::new(compat),
            compteur: 0,
        }
    }

    fn flush(&mut self) {
        if self.err.is_none()
            && let Err(e) = self.sink.write_all(self.out.as_bytes())
        {
            self.err = Some(e);
        }
        self.out.clear();
    }

    #[inline]
    fn maybe_flush(&mut self) {
        if self.out.len() >= VTU_CHUNK {
            self.flush();
        }
    }

    /// Flush the rest; the first error, if any.
    fn finish(mut self) -> std::io::Result<()> {
        self.flush();
        self.err.map_or(Ok(()), Err)
    }

    fn format_attr(&self) -> &'static str {
        if self.base64 { "binary" } else { "ascii" }
    }

    /// `name` is an XML-escaped attribute value; `payload` the number of
    /// bytes the array will push (binary mode).
    fn start_data(&mut self, name: &str, nb_components: usize, ty: &str, payload: usize) {
        let _ = write!(self.out, "    <DataArray type=\"{ty}\" ");
        if nb_components != 0 {
            let _ = write!(self.out, "NumberOfComponents=\"{nb_components}\" ");
        }
        let _ = writeln!(
            self.out,
            "Name=\"{name}\" format=\"{}\">",
            self.format_attr()
        );
        if self.base64 {
            self.b64.begin(payload);
        }
    }

    fn end_data(&mut self) {
        if self.base64 && !self.b64.end(&mut self.out) && self.err.is_none() {
            self.err = Some(std::io::Error::other(
                "internal error: VTU array size differs from its announced size",
            ));
        }
        self.out.push_str("\n    </DataArray>\n");
        self.maybe_flush();
    }

    /// `pushDatum<double>(n, size)`.
    #[inline]
    fn push_f64(&mut self, x: Real, size: usize) {
        if self.base64 {
            self.b64.push(&mut self.out, &x.to_le_bytes());
        } else {
            let size = size.max(1);
            if self.compteur.is_multiple_of(size) {
                self.out.push_str("     ");
            }
            push_e_width(&mut self.out, x, 15, 22);
            self.out.push(' ');
            self.compteur += 1;
            if self.compteur.is_multiple_of(size) {
                self.out.push('\n');
            }
        }
        self.maybe_flush();
    }

    /// Generic `pushDatum<T>(n)` for integral types.
    #[inline]
    fn push_int(&mut self, text: impl Into<i64>, bytes: &[u8]) {
        if self.base64 {
            self.b64.push(&mut self.out, bytes);
        } else {
            if self.compteur == 0 {
                self.out.push_str("      ");
            }
            self.compteur += 1;
            push_i64(&mut self.out, text.into());
            self.out.push(' ');
        }
        self.maybe_flush();
    }

    fn push_value(&mut self, v: &Values, k: usize, size: usize) {
        match v {
            Values::F64(a) => self.push_f64(a[k], size),
            Values::I32(a) => self.push_int(a[k], &a[k].to_le_bytes()),
            Values::U32(a) => self.push_int(a[k], &a[k].to_le_bytes()),
            Values::Bool(a) => self.push_int(u8::from(a[k]), &[u8::from(a[k])]),
        }
    }

    fn push_zero(&mut self, s: Scalar, size: usize) {
        match s {
            Scalar::F64 => self.push_f64(0.0, size),
            Scalar::I32 => self.push_int(0, &0i32.to_le_bytes()),
            Scalar::U32 => self.push_int(0, &0u32.to_le_bytes()),
            Scalar::Bool => self.push_int(0, &[0]),
        }
    }

    /// `writeField` (data fields and positions).
    fn write_field(&mut self, f: &Evaluated, compat: bool) {
        self.compteur = 0;
        let dim = f.dim;
        for b in &f.blocks {
            for t in 0..b.nb_tuples {
                let base = t * b.tuple_len;
                if b.rows > 0 && compat {
                    // pushData(matrix, dim): i < dim, j < dim, zero outside.
                    let cols = b.tuple_len / b.rows;
                    for i in 0..dim {
                        for j in 0..dim {
                            if i >= b.rows || j >= cols {
                                self.push_zero(f.scalar, dim);
                            } else {
                                self.push_value(&b.values, base + i + b.rows * j, dim);
                            }
                        }
                    }
                } else {
                    // pushData(vector, dim): all values, then zeros to dim.
                    for k in 0..b.tuple_len {
                        self.push_value(&b.values, base + k, dim);
                    }
                    for _ in b.tuple_len..dim {
                        self.push_zero(f.scalar, dim);
                    }
                }
            }
        }
    }
}

/// Evaluated fields in name order, with their XML-escaped names.
type NamedFields = Vec<(String, Evaluated)>;

/// The files of one dump and the state to commit once they are written.
struct Rendered {
    /// Data pieces first, then the `.pvtu`, then the `.pvd`.
    files: Vec<OutFile>,
    /// Directories to create first.
    dirs: Vec<String>,
    /// New `.pvd` entry (root rank with a time description).
    pvd_entry: Option<(Real, String)>,
}

/// Validate a field for the corrected layout.
fn check_layout(name: &str, f: &Evaluated) -> Result<()> {
    if !f.homogeneous {
        return Err(Error::InvalidArgument(format!(
            "dump field '{name}' is not homogeneous (different numbers of components across element types)"
        )));
    }
    for b in &f.blocks {
        let ok = if f.padded && b.rows == 0 {
            b.tuple_len <= f.dim
        } else {
            b.tuple_len == f.dim
        };
        if !ok && b.nb_tuples > 0 {
            return Err(Error::DimensionMismatch {
                context: "dump field tuples vs NumberOfComponents (homogenise quadrature fields)",
                expected: f.dim,
                got: b.tuple_len,
            });
        }
    }
    let _ = name;
    Ok(())
}

impl DumperParaview {
    fn evaluate_fields(
        &self,
        ctx: &EvalContext<'_>,
        selection: &MeshSelection,
    ) -> Result<(NamedFields, NamedFields)> {
        let mesh = ctx.mesh;
        let sel = selection.resolve(mesh, None)?;
        let mut nodal = Vec::new();
        let mut elemental = Vec::new();
        for (name, entry) in &self.common.entries {
            let Entry::Field(desc) = entry else { continue };
            if let Some(g) = desc.group.as_deref() {
                let fsel = selection.resolve(mesh, Some(g))?;
                eval::check_group_covers(name, g, desc.support, &sel, &fsel)?;
            }
            let f = eval::evaluate(ctx, desc, &sel)?;
            let name = xml_attr("dump field name", name)?.into_owned();
            if self.compat_layout {
                if !f.homogeneous {
                    return Err(Error::InvalidArgument(format!(
                        "dump field '{name}' is not homogeneous"
                    )));
                }
            } else {
                check_layout(&name, &f)?;
            }
            match desc.support {
                FieldSupport::Nodal => nodal.push((name, f)),
                _ => elemental.push((name, f)),
            }
        }
        Ok((nodal, elemental))
    }

    /// `nodal`/`elemental` names and `pieces` are XML-escaped.
    fn write_pvtu(
        &self,
        nodal: &[(String, Evaluated)],
        elemental: &[(String, Evaluated)],
        pieces: &[String],
    ) -> String {
        let fmt = if self.is_base64() { "binary" } else { "ascii" };
        let mut s = String::new();
        s.push_str("<VTKFile type=\"PUnstructuredGrid\" version=\"0.1\" \n");
        s.push_str("byte_order=\"LittleEndian\">\n");
        s.push_str(" <PUnstructuredGrid GhostLevel=\"0\">\n");
        s.push_str("  <PPoints>\n");
        let _ = writeln!(
            s,
            "   <PDataArray type=\"Float64\" NumberOfComponents=\"3\" format=\"{fmt}\" />"
        );
        s.push_str("  </PPoints>\n");
        let pdata = |s: &mut String, list: &[(String, Evaluated)]| {
            for (name, f) in list {
                let _ = writeln!(
                    s,
                    "   <PDataArray type=\"{}\" NumberOfComponents=\"{}\" Name=\"{name}\" format=\"{fmt}\"></PDataArray>",
                    f.scalar.vtk_name(),
                    f.dim
                );
            }
        };
        s.push_str("  <PPointData>\n");
        pdata(&mut s, nodal);
        s.push_str("  </PPointData>\n");
        s.push_str("  <PCellData>\n");
        pdata(&mut s, elemental);
        s.push_str("  </PCellData>\n");
        for p in pieces {
            let _ = writeln!(s, "  <Piece Source=\"{p}\" />");
        }
        s.push_str(" </PUnstructuredGrid>\n");
        s.push_str("</VTKFile>\n");
        s
    }

    fn is_base64(&self) -> bool {
        self.mode == FileStorageMode::Base64
    }

    /// `ParaviewHelper::writeTimePVD` over the committed entries plus
    /// `pending` (file names are XML-escaped here).
    fn write_pvd(&self, pending: &(Real, String)) -> Result<String> {
        let mut s = String::new();
        s.push_str("<?xml version=\"1.0\"?>\n");
        s.push_str("<VTKFile type=\"Collection\" version=\"0.1\" byte_order=\"LittleEndian\">\n");
        s.push_str("  <Collection>\n");
        for (t, f) in self.pvtu_file_names.iter().chain(std::iter::once(pending)) {
            let _ = writeln!(
                s,
                "    <DataSet timestep=\"{}\" group=\"\" part=\"0\" file=\"{}\"/>",
                format_default(*t),
                xml_attr("dump file name", f)?
            );
        }
        s.push_str("  </Collection>\n");
        s.push_str("</VTKFile>\n");
        Ok(s)
    }

    /// Validate and render one dump without touching the disk or the
    /// dumper state.
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
        if !self.common.entries.contains_key("positions") {
            return Err(Error::InvalidArgument(
                "ParaView dump without positions: register a mesh first".into(),
            ));
        }
        let selection = self.common.selection_for(mesh);
        let sel = selection.resolve(mesh, None)?;
        let positions = eval::positions(mesh, &sel, true);
        let cells = if self.common.entries.contains_key("connectivities")
            && !matches!(selection.filter, MeshFilter::NodeGroup(_))
        {
            Some(eval::connectivity(mesh, &sel)?)
        } else {
            None
        };
        let (nodal, elemental) = self.evaluate_fields(&ctx, &selection)?;
        let base = &self.common.base_name;
        let prefix = &self.common.prefix;
        let vtu_folder = self.vtu_folder();
        check_vtu(&positions, cells.as_deref(), &nodal, &elemental)?;
        let vtu_path = format!(
            "{prefix}{vtu_folder}{}",
            file_name(base, Some(step), Some((par.rank, par)), ".vtu")
        );
        let mut files = Vec::new();
        let mut pvd_entry = None;
        if par.rank == par.root {
            let mut pieces = Vec::with_capacity(par.size);
            for i in 0..par.size {
                let p = format!(
                    "{vtu_folder}{}",
                    file_name(base, Some(step), Some((i, par)), ".vtu")
                );
                pieces.push(xml_attr("dump piece path", &p)?.into_owned());
            }
            files.push(OutFile::new(
                format!("{prefix}{}", file_name(base, Some(step), None, ".pvtu")),
                self.write_pvtu(&nodal, &elemental, &pieces),
                WriteMode::Replace,
            ));
            if self.common.write_time_desc {
                let entry = (
                    self.common.current_time,
                    file_name(base, Some(step), None, ".pvtu"),
                );
                let name = self
                    .common
                    .time_description_file_name
                    .as_deref()
                    .unwrap_or(base);
                files.push(OutFile::new(
                    format!("{prefix}{name}.pvd"),
                    self.write_pvd(&entry)?,
                    WriteMode::Atomic,
                ));
                pvd_entry = Some(entry);
            }
        }
        let job = VtuJob {
            base64: self.is_base64(),
            compat_layout: self.compat_layout,
            positions,
            cells,
            nodal,
            elemental,
        };
        files.insert(
            0,
            OutFile::stream(vtu_path, Box::new(job), WriteMode::Replace),
        );
        Ok(Rendered {
            files,
            dirs: vec![prefix.clone(), format!("{prefix}{vtu_folder}")],
            pvd_entry,
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
        if let Some(entry) = r.pvd_entry {
            self.pvtu_file_names.push(entry);
            self.common.current_time += self.common.time_step;
        }
        self.common.end_dump();
        Ok(())
    }
}

impl Dumper for DumperParaview {
    fn register_mesh(
        &mut self,
        spatial_dimension: usize,
        ghost: akantu_core::GhostType,
        kind: akantu_core::ElementKind,
    ) -> Result<()> {
        self.register_mesh_selection(MeshSelection::new(Some(spatial_dimension), ghost, kind))
    }

    fn register_mesh_selection(&mut self, selection: MeshSelection) -> Result<()> {
        let point_set = matches!(selection.filter, MeshFilter::NodeGroup(_));
        if !point_set {
            self.common.insert("connectivities", Entry::Connectivity)?;
        }
        if let Err(e) = self.common.insert("positions", Entry::Positions) {
            if !point_set {
                let _ = self.common.remove("connectivities");
            }
            return Err(e);
        }
        self.common.selection = Some(selection);
        Ok(())
    }

    fn register_field(&mut self, field: FieldDescriptor) -> Result<()> {
        let name = field.name.clone();
        self.common.insert(&name, Entry::Field(field))
    }

    fn unregister_field(&mut self, name: &str) -> Result<()> {
        self.common.remove(name)
    }

    /// Accepted and never evaluated: upstream's ParaView writer does not
    /// visit variables.
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
        self.vtu_sub_directory = format!("{base_name}-VTU");
        Ok(())
    }

    fn set_time_step(&mut self, time_step: Real) -> Result<()> {
        self.common.set_time_step(time_step);
        Ok(())
    }

    fn set_text_mode(&mut self) -> Result<()> {
        self.mode = FileStorageMode::Text;
        Ok(())
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

    fn dump(
        &mut self,
        mesh: &Mesh,
        resolver: &dyn FieldResolver,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        let saved = (self.common.snapshot(), self.pvtu_file_names.len());
        let r = self.dump_steps(mesh, resolver, time, step);
        if r.is_err() {
            self.common.restore(saved.0);
            self.pvtu_file_names.truncate(saved.1);
        }
        r
    }
}
