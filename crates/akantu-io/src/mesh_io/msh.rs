// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/mesh_io/mesh_io_msh.cc
// (`MeshIOMSH::read`, `populateReaders2`, `populateReaders4`, data-block
// readers), src/io/mesh_io/mesh_io_msh_struct.cc (`MeshIOMSHStruct::read`),
// src/io/mesh_io.cc (`MeshIO::constructPhysicalNames`),
// src/mesh/group_manager.cc (`createGroupsFromMeshData`).

//! Gmsh `.msh` reader (ASCII MSH 1.0, 2.0–2.2, 4.0, 4.1).
//!
//! Sections are processed in file order with upstream's state machine
//! (`$MeshFormat` selects the 2.x or 4.x readers; `$PhysicalNames`,
//! `$NodeData`, `$ElementData`, `$ElementNodeData` are version independent;
//! unknown sections are skipped up to their `$End…` line). Upstream behavior
//! is reproduced except where it is undefined or wrong (see
//! `docs/UPSTREAM_DEFECTS.md`):
//!
//! * UD-046: MSH 1.0 (`$NOD`/`$ELM`, no `$MeshFormat`) is read (tags
//!   `reg-phys`/`reg-elem` → `tag_0`/`tag_1`, as upstream's dead code
//!   intended) instead of scanning to end of file;
//! * UD-064: per-element tag counts may differ within a type; an element
//!   without a `j`-th tag gets `tag_j = 0` (upstream: uninitialised);
//! * UD-045: elements of an unnamed physical group join the
//!   `msh_block_<tag>` group upstream creates but leaves empty
//!   ([`ReadOptions::fill_unnamed_physical_groups`]);
//! * UD-047: `$ElementNodeData` lines are read completely, node values in
//!   Akantu local node order;
//! * UD-090: an MSH 4 element block of an unsupported type is skipped
//!   (upstream parses its element lines as block headers);
//! * UD-091: data-block string tags are read as quoted strings (upstream
//!   truncates at the first blank); `$NodeData` arrays always have one
//!   tuple per node (upstream: number of lines in the block);
//! * every token is validated: malformed input is an [`Error::Parse`]
//!   (`file:line:column`); undefined or duplicate node tags and undefined
//!   element tags are errors (UD-095: upstream silently maps them to node 0
//!   or to a default element); an empty `$PhysicalNames` section gives no
//!   groups (upstream throws looking for the absent `physical_names` data).

use std::collections::{BTreeMap, HashMap};

use akantu_core::{ElementType, Error, Idx, Int, Real, Result};

use super::ReadOptions;
use super::lexer::{Line, Source, parse_error};
use super::staging::{MeshBuild, RealElementalData};
use super::tables::{gmsh_nodes_per_element, msh_read_order, msh_to_akantu, msh_to_akantu_struct};

/// Which Gmsh reader (upstream `MeshIOMSH` or `MeshIOMSHStruct`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flavor {
    /// `_miot_gmsh`.
    Standard,
    /// `_miot_gmsh_struct` (structural elements, identity node order).
    Structural,
}

/// What a file referenced by an element tag turned into.
#[derive(Clone, Copy, Debug)]
enum ElementRef {
    Element(ElementType, Idx),
    /// Element of a type the reader skips.
    Skipped,
    /// The tag was used by several elements (only an error if a data block
    /// references it).
    Duplicate,
}

/// Result of a Gmsh read.
#[derive(Debug)]
pub(crate) struct MshResult {
    pub(crate) build: MeshBuild,
    /// `major * 1000 + minor` (`1000` for MSH 1.0), upstream's
    /// `File::version` encoding.
    pub(crate) version: i32,
    pub(crate) warnings: Vec<String>,
}

/// Largest number of components per entry of a data block (Gmsh writes
/// scalars, vectors and tensors: 1, 3, 9).
const MAX_DATA_COMPONENTS: usize = 4096;

/// Parsed header of a data block.
struct DataHeader {
    /// Dataset name (first string tag).
    name: String,
    /// Number of components per entry (second integer tag).
    nc: usize,
    /// Number of entry lines (third integer tag).
    count: usize,
    /// `(line, column)` of the number-of-components tag.
    nc_at: (usize, usize),
}

struct Reader<'a> {
    src: Source<'a>,
    flavor: Flavor,
    opts: ReadOptions,
    dim: usize,
    version: Option<i32>,
    build: MeshBuild,
    node_tags: HashMap<u64, Idx>,
    element_tags: HashMap<u64, ElementRef>,
    physical_names: BTreeMap<(i32, i32), String>,
    has_physical_names: bool,
    entity_phys: HashMap<(i32, i32), Vec<i32>>,
    /// MSH 2: `(line, column)` of the tag count of the first element of
    /// each type that has no tag (diagnostic of `$PhysicalNames` without
    /// `tag_0`).
    untagged_at: BTreeMap<ElementType, (usize, usize)>,
    nodes_read: bool,
    warnings: Vec<String>,
}

/// Read Gmsh text `text` (file name `name` for messages) for a mesh of
/// spatial dimension `dim`.
pub(crate) fn read(
    name: &str,
    text: &str,
    dim: usize,
    flavor: Flavor,
    opts: &ReadOptions,
) -> Result<MshResult> {
    let mut r = Reader {
        src: Source::new(name, text),
        flavor,
        opts: opts.clone(),
        dim,
        version: None,
        build: MeshBuild::new(dim),
        node_tags: HashMap::new(),
        element_tags: HashMap::new(),
        physical_names: BTreeMap::new(),
        has_physical_names: false,
        entity_phys: HashMap::new(),
        untagged_at: BTreeMap::new(),
        nodes_read: false,
        warnings: Vec::new(),
    };
    if flavor == Flavor::Structural && dim == 3 {
        r.warnings.push(
            "The MeshIOMSHStruct is reading bernoulli beam 3D be sure to provide the missing \
             normals with the element data \"extra_normal\""
                .into(),
        );
    }
    r.run()?;
    r.finish()
}

impl<'a> Reader<'a> {
    fn warn(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    /// Data arrays are zero-filled for entities without values; keep their
    /// size proportional to the input (a corrupt header must not make the
    /// reader allocate unbounded memory).
    fn check_alloc(&self, rows: usize, comps: usize, section: &str) -> Result<()> {
        let n = rows.checked_mul(comps);
        let limit = (1usize << 24).max(self.src.len().saturating_mul(4));
        match n {
            Some(n) if n <= limit => Ok(()),
            _ => Err(Error::CapacityExceeded {
                what: "mesh data block (entries × components)",
                requested: rows as u128 * comps as u128,
                limit: limit as u128,
            })
            .map_err(|e| self.src.error_here(format!("{section}: {e}"))),
        }
    }

    fn is_v2(&self) -> bool {
        matches!(self.version, Some(v) if v < 4000)
    }

    fn is_v4(&self) -> bool {
        matches!(self.version, Some(v) if v >= 4000)
    }

    fn akantu_type(&self, code: u32) -> Option<ElementType> {
        match self.flavor {
            Flavor::Standard => msh_to_akantu(code),
            Flavor::Structural => msh_to_akantu_struct(code, self.dim),
        }
    }

    fn read_order(&self, ty: ElementType) -> Option<&'static [usize]> {
        match self.flavor {
            Flavor::Standard => msh_read_order(ty),
            Flavor::Structural => None,
        }
    }

    fn run(&mut self) -> Result<()> {
        while let Some(mut line) = self.src.next_line() {
            let Some((col, block)) = line.next_token() else {
                continue;
            };
            let Some(name) = block.strip_prefix('$') else {
                return Err(line.error_at(
                    col,
                    format!("unexpected '{block}' outside a section (expected a '$Section' line)"),
                ));
            };
            let name = name.to_owned();
            match block {
                "$MeshFormat" => self.mesh_format(&line)?,
                "$PhysicalNames" => self.physical_names_section()?,
                "$NodeData" => self.node_data()?,
                "$ElementData" => self.element_data(false)?,
                "$ElementNodeData" => self.element_data(true)?,
                "$NOD" | "$ELM" if self.version.is_none() || self.version == Some(1000) => {
                    self.version = Some(1000);
                    if block == "$NOD" {
                        self.nodes_v2(&line)?;
                    } else {
                        self.elements_v2()?;
                    }
                }
                "$Nodes" if self.is_v2() => self.nodes_v2(&line)?,
                "$Elements" if self.is_v2() => self.elements_v2()?,
                "$Periodic" if self.is_v2() => self.periodic_v2()?,
                "$Entities" if self.is_v4() => self.entities_v4()?,
                "$Nodes" if self.is_v4() => self.nodes_v4(&line)?,
                "$Elements" if self.is_v4() => self.elements_v4()?,
                "$Nodes" | "$Elements" | "$Entities" if self.version.is_none() => {
                    return Err(line.error_at(
                        col,
                        format!("section {block} before $MeshFormat (not an MSH 2/4 file)"),
                    ));
                }
                _ => {
                    self.skip_section(&name, line.no())?;
                    continue;
                }
            }
            self.expect_end(&name)?;
        }
        Ok(())
    }

    /// Upstream "Unsupported" reader: skip to `$End<name>`.
    fn skip_section(&mut self, name: &str, start: usize) -> Result<()> {
        self.warn(format!("Unsupported block_kind {name} at line {start}"));
        let end = format!("$End{name}");
        loop {
            let line = self
                .src
                .expect_line(&format!("section ${name} (no {end} line)"))?;
            if line.trimmed() == end {
                return Ok(());
            }
        }
    }

    /// The line after a section must start with `$End<name>` (`$ENDNOD` /
    /// `$ENDELM` for MSH 1.0).
    fn expect_end(&mut self, name: &str) -> Result<()> {
        let end = format!("$End{name}");
        let mut line = self
            .src
            .expect_line(&format!("section ${name} (expected {end})"))?;
        let tok = line.next_token().map(|(_, t)| t);
        let ok = tok == Some(end.as_str())
            || (matches!(name, "NOD" | "ELM") && tok == Some(&format!("$END{name}")[..]));
        if ok {
            Ok(())
        } else {
            Err(line.error_at(
                1,
                format!(
                    "the reader failed to properly read the block {name}: expected {end}, found '{}'",
                    line.trimmed()
                ),
            ))
        }
    }

    // ------------------------------------------------------------ format

    fn mesh_format(&mut self, header: &Line<'a>) -> Result<()> {
        if self.version.is_some() {
            return Err(header.error_at(1, "duplicate $MeshFormat section"));
        }
        let mut line = self.src.expect_line("$MeshFormat")?;
        let (vcol, vtok) = line.token("format version")?;
        let version = super::lexer::parse_f64(vtok)
            .ok_or_else(|| line.error_at(vcol, format!("invalid format version '{vtok}'")))?;
        let fcol = line.column();
        let file_type = line.i64("file type")?;
        // data-size: read upstream (informative only).
        let major = version.trunc();
        let minor = (10.0 * (version - major)).round();
        #[allow(clippy::cast_possible_truncation)] // validated range below
        let code = (major * 1000.0 + minor) as i64;
        if file_type != 0 {
            return Err(Error::Unsupported(format!(
                "{}:{}:{fcol}: binary Gmsh files are not supported (Akantu reads ASCII MSH only)",
                self.src.name(),
                line.no()
            )));
        }
        let code = match code {
            2000..=2002 | 4000 | 4001 => i32::try_from(code).unwrap_or(0),
            _ => {
                return Err(Error::Unsupported(format!(
                    "{}:{}:{vcol}: Gmsh MSH format version {vtok} is not supported \
                     (supported: 1.0, 2.0, 2.1, 2.2, 4.0, 4.1)",
                    self.src.name(),
                    line.no()
                )));
            }
        };
        self.version = Some(code);
        Ok(())
    }

    fn physical_names_section(&mut self) -> Result<()> {
        self.has_physical_names = true;
        let n = self
            .src
            .expect_line("$PhysicalNames")?
            .count("number of physical names")?;
        for _ in 0..n {
            let mut line = self.src.expect_line("$PhysicalNames")?;
            let dim = line.i32("physical dimension")?;
            let tag = line.i32("physical tag")?;
            let name = line.quoted("physical name")?;
            self.physical_names.insert((dim, tag), name);
        }
        Ok(())
    }

    // ---------------------------------------------------------- MSH 1/2

    fn nodes_v2(&mut self, header: &Line<'a>) -> Result<()> {
        if self.nodes_read {
            return Err(header.error_at(1, "duplicate nodes section"));
        }
        self.nodes_read = true;
        let n = self
            .src
            .expect_line("nodes section")?
            .count("number of nodes")?;
        self.build
            .nodes
            .reserve(n.min(self.src.remaining()).saturating_mul(self.dim));
        for _ in 0..n {
            let mut line = self.src.expect_line("nodes section")?;
            let tag = line.u64("node number")?;
            let x = [
                line.f64("x coordinate")?,
                line.f64("y coordinate")?,
                line.f64("z coordinate")?,
            ];
            self.add_node(&line, tag, &x)?;
        }
        Ok(())
    }

    fn add_node(&mut self, line: &Line<'a>, tag: u64, x: &[Real]) -> Result<()> {
        let idx = self.build.push_node(x)?;
        if self.node_tags.insert(tag, idx).is_some() {
            return Err(line.error_at(1, format!("duplicate node number {tag}")));
        }
        Ok(())
    }

    fn node_index(&self, line: &Line<'a>, col: usize, tag: u64) -> Result<Idx> {
        self.node_tags.get(&tag).copied().ok_or_else(|| {
            line.error_at(
                col,
                format!("node number {tag} is not defined in the nodes section"),
            )
        })
    }

    fn record_element_tag(&mut self, tag: u64, r: ElementRef) {
        self.element_tags
            .entry(tag)
            .and_modify(|e| *e = ElementRef::Duplicate)
            .or_insert(r);
    }

    /// Read `n` node numbers of an element and place them in Akantu order.
    fn element_nodes(&self, line: &mut Line<'a>, ty: ElementType) -> Result<Vec<Idx>> {
        let nnpe = ty.nb_nodes_per_element();
        let order = self.read_order(ty);
        let mut local = vec![0; nnpe];
        for j in 0..nnpe {
            let col = line.column();
            let tag = line.u64("node number of the element")?;
            let idx = self.node_index(line, col, tag)?;
            local[order.map_or(j, |o| o[j])] = idx;
        }
        Ok(local)
    }

    fn elements_v2(&mut self) -> Result<()> {
        let v1 = self.version == Some(1000);
        let n = self
            .src
            .expect_line("elements section")?
            .count("number of elements")?;
        for _ in 0..n {
            let mut line = self.src.expect_line("elements section")?;
            let tag = line.u64("element number")?;
            let tcol = line.column();
            let code = line.u64("element type")?;
            let code = u32::try_from(code)
                .map_err(|_| line.error_at(tcol, format!("invalid element type {code}")))?;
            let Some(ty) = self.akantu_type(code) else {
                self.warn(format!(
                    "Unsuported element kind {code} at line {}",
                    line.no()
                ));
                self.record_element_tag(tag, ElementRef::Skipped);
                continue;
            };
            let tags: Vec<Int> = if v1 {
                let reg_phys = line.i32("reg-phys")?;
                let reg_elem = line.i32("reg-elem")?;
                let ncol = line.column();
                let nb_nodes = line.count("number of nodes")?;
                if nb_nodes != ty.nb_nodes_per_element() {
                    return Err(line.error_at(
                        ncol,
                        format!(
                            "element type {code} has {} nodes, the line declares {nb_nodes}",
                            ty.nb_nodes_per_element()
                        ),
                    ));
                }
                vec![reg_phys, reg_elem]
            } else {
                let tags_at = (line.no(), line.column());
                let nb_tags = line.count("number of tags")?;
                if nb_tags == 0 {
                    self.untagged_at.entry(ty).or_insert(tags_at);
                }
                let mut t = Vec::with_capacity(nb_tags.min(64));
                for _ in 0..nb_tags {
                    t.push(line.i32("element tag")?);
                }
                t
            };
            let local = self.element_nodes(&mut line, ty)?;
            let e = self.build.push_element(ty, &local)?;
            let eu = usize::try_from(e).unwrap_or(0);
            for (j, &t) in tags.iter().enumerate() {
                self.build.set_int(&format!("tag_{j}"), ty, eu, t, 0);
            }
            self.record_element_tag(tag, ElementRef::Element(ty, e));
        }
        Ok(())
    }

    /// `$Periodic` is parsed and ignored, as upstream does ("The info in
    /// the mesh seem inconsistent so they are ignored for now").
    fn periodic_v2(&mut self) -> Result<()> {
        let n = self
            .src
            .expect_line("$Periodic")?
            .count("number of periodic entities")?;
        for _ in 0..n {
            let mut l = self.src.expect_line("$Periodic")?;
            l.i32("dimension")?;
            let mut l = self.src.expect_line("$Periodic")?;
            if l.trimmed().starts_with("Affine") {
                l = self.src.expect_line("$Periodic")?;
            }
            let nb = l.count("number of periodic nodes")?;
            for _ in 0..nb {
                self.src.expect_line("$Periodic")?;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------ MSH 4

    fn entities_v4(&mut self) -> Result<()> {
        let mut line = self.src.expect_line("$Entities")?;
        let mut num = [0usize; 4];
        for (d, slot) in num.iter_mut().enumerate() {
            *slot = line.count(&format!("number of {}-dimensional entities", d))?;
        }
        let v41 = self.version >= Some(4001);
        for (d, &count) in num.iter().enumerate() {
            let di = i32::try_from(d).unwrap_or(0);
            for _ in 0..count {
                let mut line = self.src.expect_line("$Entities")?;
                let tag = line.i32("entity tag")?;
                for _ in 0..3 {
                    line.f64("entity bounding box")?;
                }
                if d > 0 || !v41 {
                    for _ in 0..3 {
                        line.f64("entity bounding box")?;
                    }
                }
                let nphys = line.count("number of physical tags")?;
                for _ in 0..nphys {
                    let phys = line.i32("physical tag")?;
                    let gname = self.group_name(di, phys);
                    self.build.group_mut(&gname, Some(d));
                    self.entity_phys.entry((tag, di)).or_default().push(phys);
                }
            }
        }
        Ok(())
    }

    /// Name of physical group `(dim, phys)`: its `$PhysicalNames` entry or
    /// `msh_block_<phys>`.
    fn group_name(&self, dim: i32, phys: i32) -> String {
        self.physical_names
            .get(&(dim, phys))
            .cloned()
            .unwrap_or_else(|| format!("msh_block_{phys}"))
    }

    fn nodes_v4(&mut self, header: &Line<'a>) -> Result<()> {
        if self.nodes_read {
            return Err(header.error_at(1, "duplicate $Nodes section"));
        }
        self.nodes_read = true;
        let v41 = self.version >= Some(4001);
        let mut line = self.src.expect_line("$Nodes")?;
        let num_blocks = line.count("number of node blocks")?;
        let num_nodes = line.count("number of nodes")?;
        self.build
            .nodes
            .reserve(num_nodes.min(self.src.remaining()).saturating_mul(self.dim));
        let mut total = 0usize;
        for _ in 0..num_blocks {
            let mut h = self.src.expect_line("$Nodes block header")?;
            // Entity dimension and tag (4.1) or tag and dimension (4.0):
            // not used.
            h.i32("entity dimension or tag")?;
            h.i32("entity tag or dimension")?;
            let pcol = h.column();
            let parametric = h.i32("parametric flag")?;
            let n = h.count("number of nodes in block")?;
            total = total.saturating_add(n);
            if v41 {
                if parametric != 0 {
                    return Err(Error::Unsupported(format!(
                        "{}:{}:{pcol}: Akantu does not support parametric nodes in msh files",
                        self.src.name(),
                        h.no()
                    )));
                }
                let mut tags = Vec::with_capacity(n.min(self.src.remaining()));
                for _ in 0..n {
                    let mut l = self.src.expect_line("$Nodes block (node tags)")?;
                    tags.push((l.u64("node tag")?, l.clone()));
                }
                for (tag, tline) in tags {
                    let mut l = self.src.expect_line("$Nodes block (coordinates)")?;
                    let x = [
                        l.f64("x coordinate")?,
                        l.f64("y coordinate")?,
                        l.f64("z coordinate")?,
                    ];
                    self.add_node(&tline, tag, &x)?;
                }
            } else {
                for _ in 0..n {
                    let mut l = self.src.expect_line("$Nodes block")?;
                    let tag = l.u64("node tag")?;
                    let x = [
                        l.f64("x coordinate")?,
                        l.f64("y coordinate")?,
                        l.f64("z coordinate")?,
                    ];
                    self.add_node(&l, tag, &x)?;
                }
            }
        }
        if total != num_nodes {
            self.warn(format!(
                "$Nodes header announces {num_nodes} nodes, the blocks contain {total}"
            ));
        }
        Ok(())
    }

    fn elements_v4(&mut self) -> Result<()> {
        let v41 = self.version >= Some(4001);
        let mut line = self.src.expect_line("$Elements")?;
        let num_blocks = line.count("number of element blocks")?;
        let _num_elements = line.count("number of elements")?;
        for _ in 0..num_blocks {
            let mut h = self.src.expect_line("$Elements block header")?;
            let (entity_dim, entity_tag) = if v41 {
                let ed = h.i32("entity dimension")?;
                (ed, h.i32("entity tag")?)
            } else {
                let et = h.i32("entity tag")?;
                (h.i32("entity dimension")?, et)
            };
            let tcol = h.column();
            let code = h.u64("element type")?;
            let n = h.count("number of elements in block")?;
            let code = u32::try_from(code)
                .map_err(|_| h.error_at(tcol, format!("invalid element type {code}")))?;
            let Some(ty) = self.akantu_type(code) else {
                // UD-090: skip the block's element lines.
                self.warn(format!("Unsuported element kind {code} at line {}", h.no()));
                let nnpe = gmsh_nodes_per_element(code);
                for _ in 0..n {
                    let mut l = self.src.expect_line("$Elements block")?;
                    let tag = l.u64("element tag")?;
                    if let Some(k) = nnpe {
                        for _ in 0..k {
                            l.u64("node tag")?;
                        }
                    }
                    self.record_element_tag(tag, ElementRef::Skipped);
                }
                continue;
            };
            let phys = self
                .entity_phys
                .get(&(entity_tag, entity_dim))
                .cloned()
                .unwrap_or_default();
            let names: Vec<(i32, Option<String>)> = phys
                .iter()
                .map(|&p| (p, self.physical_names.get(&(entity_dim, p)).cloned()))
                .collect();
            for _ in 0..n {
                let mut l = self.src.expect_line("$Elements block")?;
                let tag = l.u64("element tag")?;
                let local = self.element_nodes(&mut l, ty)?;
                let e = self.build.push_element(ty, &local)?;
                let eu = usize::try_from(e).unwrap_or(0);
                self.record_element_tag(tag, ElementRef::Element(ty, e));
                // tag_0 = first physical tag (0 if none), physical_names =
                // name of the first physical tag if it is named ("" else).
                let first = names.first();
                self.build
                    .set_int("tag_0", ty, eu, first.map_or(0, |(p, _)| *p), 0);
                let pname = first.and_then(|(_, n)| n.clone()).unwrap_or_default();
                let sd = self
                    .build
                    .string_data
                    .entry("physical_names".into())
                    .or_default();
                let v = sd.entry(ty).or_default();
                v.resize(eu, String::new());
                v.push(pname);
                for (p, name) in &names {
                    match name {
                        Some(name) => self.build.group_add(name, ty, e, true),
                        None if self.opts.fill_unnamed_physical_groups => {
                            self.build.group_add(&format!("msh_block_{p}"), ty, e, true);
                        }
                        None => {}
                    }
                }
            }
        }
        for g in self.build.element_groups.values_mut() {
            g.optimize_nodes();
        }
        Ok(())
    }

    // ------------------------------------------------------- data blocks

    /// Common header of `$NodeData`/`$ElementData`/`$ElementNodeData`.
    ///
    /// String tags are decoded exactly once, as `std::quoted` does: a
    /// `"…"` string with `\` escapes gives its content (boundary quotes
    /// that were escaped inside it are kept), a bare word is taken
    /// verbatim. Upstream extracts a blank-delimited word and then trims
    /// `"` from both ends (UD-091), which loses such quotes.
    fn data_header(&mut self, section: &str) -> Result<DataHeader> {
        let start = self.src.line_no() + 1;
        let n_str = self
            .src
            .expect_line(section)?
            .count("number of string tags")?;
        let mut strings = Vec::new();
        for _ in 0..n_str {
            let mut l = self.src.expect_line(section)?;
            strings.push(l.quoted("string tag")?);
        }
        let n_real = self
            .src
            .expect_line(section)?
            .count("number of real tags")?;
        for _ in 0..n_real {
            self.src.expect_line(section)?.f64("real tag")?;
        }
        let n_int = self
            .src
            .expect_line(section)?
            .count("number of integer tags")?;
        // (value, line, column) of every integer tag, for diagnostics.
        let mut ints: Vec<(i64, usize, usize)> = Vec::new();
        for _ in 0..n_int {
            let mut l = self.src.expect_line(section)?;
            let col = l.column();
            ints.push((l.i64("integer tag")?, l.no(), col));
        }
        let name_src = self.src.name();
        let Some(name) = strings.into_iter().next() else {
            return Err(self.src.error_here(format!(
                "{section} starting at line {start} has no string tag (dataset name)"
            )));
        };
        if ints.len() < 3 {
            return Err(self.src.error_here(format!(
                "{section} starting at line {start} needs 3 integer tags \
                 (time step, number of components, number of entries)"
            )));
        }
        let at = |k: usize, msg: String| -> Error {
            let (_, line, col) = ints[k];
            parse_error(name_src, line, col, msg)
        };
        let (nc_tag, count_tag) = (ints[1].0, ints[2].0);
        let nc = usize::try_from(nc_tag)
            .ok()
            .filter(|&c| c > 0)
            .ok_or_else(|| {
                at(
                    1,
                    format!("{section}: invalid number of components {nc_tag}"),
                )
            })?;
        if nc > MAX_DATA_COMPONENTS {
            return Err(at(
                1,
                format!(
                    "{section}: {nc} components per entry exceed the supported {MAX_DATA_COMPONENTS}"
                ),
            ));
        }
        let count = usize::try_from(count_tag).map_err(|_| {
            at(
                2,
                format!("{section}: invalid number of entries {count_tag}"),
            )
        })?;
        let (_, nc_line, nc_col) = ints[1];
        Ok(DataHeader {
            name,
            nc,
            count,
            nc_at: (nc_line, nc_col),
        })
    }

    /// `$NodeData`: the dataset has one tuple per node (UD-091; upstream
    /// resizes it to the entry count of the block). A dataset may be
    /// defined by several blocks with the same number of components; each
    /// block first resizes it to the number of nodes read so far (new
    /// tuples are zero) and then overwrites the tuples of the nodes it
    /// lists. A block before `$Nodes` can only be empty (its node tags
    /// would be undefined) and creates an empty dataset that the next
    /// block, or the end of the file, extends to every node.
    fn node_data(&mut self) -> Result<()> {
        let h = self.data_header("$NodeData")?;
        let nc = h.nc;
        let nb_nodes = self.build.nb_nodes();
        self.check_alloc(nb_nodes, nc, "$NodeData")?;
        let (have, values) = self
            .build
            .nodal_data
            .entry(h.name.clone())
            .or_insert_with(|| (nc, Vec::new()));
        if *have != nc {
            return Err(parse_error(
                self.src.name(),
                h.nc_at.0,
                h.nc_at.1,
                format!(
                    "$NodeData '{}' has {nc} components, an earlier block {have}",
                    h.name
                ),
            ));
        }
        // Nodes are read once: an existing dataset is never longer.
        debug_assert!(values.len() <= nb_nodes * nc);
        values.resize(nb_nodes * nc, 0.0);
        let mut tuple = Vec::with_capacity(nc);
        for _ in 0..h.count {
            let mut l = self.src.expect_line("$NodeData")?;
            let col = l.column();
            let tag = l.u64("node tag")?;
            let n = usize::try_from(self.node_index(&l, col, tag)?)
                .map_err(|_| l.error_at(col, format!("node number {tag}: invalid index")))?;
            tuple.clear();
            for _ in 0..nc {
                tuple.push(l.f64("node value")?);
            }
            let dst = self
                .build
                .nodal_data
                .get_mut(&h.name)
                .and_then(|(_, v)| v.get_mut(n * nc..(n + 1) * nc))
                .ok_or_else(|| {
                    l.error_at(
                        col,
                        format!("node number {tag} outside the $NodeData array"),
                    )
                })?;
            dst.copy_from_slice(&tuple);
        }
        Ok(())
    }

    fn element_data(&mut self, per_node: bool) -> Result<()> {
        let section = if per_node {
            "$ElementNodeData"
        } else {
            "$ElementData"
        };
        let DataHeader {
            name, nc, count, ..
        } = self.data_header(section)?;
        if let Some(d) = self.build.real_data.get(&name)
            && d.nodal != per_node
        {
            return Err(self.src.error_here(format!(
                "{section} '{name}' mixes element and element-node values"
            )));
        }
        if self.build.int_data.contains_key(&name) || self.build.string_data.contains_key(&name) {
            return Err(self.src.error_here(format!(
                "{section} '{name}' clashes with a non-real mesh dataset of the same name"
            )));
        }
        let mut skipped = 0usize;
        for _ in 0..count {
            let mut l = self.src.expect_line(section)?;
            let col = l.column();
            let tag = l.u64("element tag")?;
            let (ty, e) = match self.element_tags.get(&tag) {
                Some(ElementRef::Element(ty, e)) => (*ty, usize::try_from(*e).unwrap_or(0)),
                Some(ElementRef::Skipped) => {
                    skipped += 1;
                    continue;
                }
                Some(ElementRef::Duplicate) => {
                    return Err(l.error_at(
                        col,
                        format!("element tag {tag} is ambiguous (used by several elements)"),
                    ));
                }
                None => {
                    return Err(l.error_at(
                        col,
                        format!("element tag {tag} is not defined in the elements section"),
                    ));
                }
            };
            let nnpe = ty.nb_nodes_per_element();
            let comps = if per_node {
                let ncol = l.column();
                let k = l.count("number of nodes per element")?;
                if k != nnpe {
                    return Err(l.error_at(
                        ncol,
                        format!("{ty} elements have {nnpe} nodes, the line declares {k}"),
                    ));
                }
                nc * nnpe
            } else {
                nc
            };
            let nb_el = self.build.nb_elements(ty);
            self.check_alloc(nb_el, comps, section)?;
            let order = if per_node { self.read_order(ty) } else { None };
            let mut values = vec![0.0; comps];
            if per_node {
                for j in 0..nnpe {
                    let slot = order.map_or(j, |o| o[j]);
                    for c in 0..nc {
                        values[slot * nc + c] = l.f64("element-node value")?;
                    }
                }
            } else {
                for v in &mut values {
                    *v = l.f64("element value")?;
                }
            }
            let d = self
                .build
                .real_data
                .entry(name.clone())
                .or_insert_with(|| RealElementalData {
                    per_type: BTreeMap::new(),
                    nodal: per_node,
                });
            let (have, arr) = d
                .per_type
                .entry(ty)
                .or_insert_with(|| (comps, vec![0.0; nb_el * comps]));
            if *have != comps {
                return Err(l.error_at(
                    1,
                    format!("{section} '{name}': {ty} values have {comps} components, earlier ones {have}"),
                ));
            }
            if arr.len() < nb_el * comps {
                arr.resize(nb_el * comps, 0.0);
            }
            arr[e * comps..(e + 1) * comps].copy_from_slice(&values);
        }
        if skipped > 0 {
            self.warn(format!(
                "{section} '{name}': {skipped} values of elements of unsupported types ignored"
            ));
        }
        Ok(())
    }

    // ------------------------------------------------------------ finish

    fn finish(mut self) -> Result<MshResult> {
        let Some(version) = self.version else {
            return Err(self
                .src
                .error_here("not a Gmsh mesh: no $MeshFormat (MSH 2/4) or $NOD (MSH 1) section"));
        };
        if version < 4000 {
            if !self.physical_names.is_empty() {
                self.construct_physical_names()?;
            }
            if self.has_physical_names {
                self.build
                    .create_groups_from_string_data("physical_names")?;
            }
        }
        // Element data arrays allocated before later elements of the same
        // type were read are padded with zeros.
        let counts: BTreeMap<ElementType, usize> = self
            .build
            .conn
            .keys()
            .map(|&t| (t, self.build.nb_elements(t)))
            .collect();
        for d in self.build.real_data.values_mut() {
            for (ty, (comps, v)) in &mut d.per_type {
                let want = counts.get(ty).copied().unwrap_or(0) * *comps;
                if v.len() < want {
                    v.resize(want, 0.0);
                }
            }
        }
        // Likewise nodal datasets whose only blocks preceded `$Nodes`:
        // one zero tuple per node.
        let nb_nodes = self.build.nb_nodes();
        for (name, (nc, _)) in &self.build.nodal_data {
            self.check_alloc(nb_nodes, *nc, &format!("$NodeData '{name}'"))?;
        }
        for (nc, v) in self.build.nodal_data.values_mut() {
            if v.len() < nb_nodes * *nc {
                v.resize(nb_nodes * *nc, 0.0);
            }
        }
        Ok(MshResult {
            build: self.build,
            version,
            warnings: self.warnings,
        })
    }

    /// Upstream `MeshIO::constructPhysicalNames("tag_0", mesh)`.
    fn construct_physical_names(&mut self) -> Result<()> {
        let types: Vec<ElementType> = self.build.conn.keys().copied().collect();
        let mut out: BTreeMap<ElementType, Vec<String>> = BTreeMap::new();
        for ty in types {
            let n = self.build.nb_elements(ty);
            let tags = self
                .build
                .int_data
                .get("tag_0")
                .and_then(|d| d.per_type.get(&ty).map(|v| (v, d.fill)))
                .ok_or_else(|| {
                    // No element of `ty` has a tag: point at the first one.
                    let (line, col) = self.untagged_at.get(&ty).copied().unwrap_or((1, 1));
                    parse_error(
                        self.src.name(),
                        line,
                        col,
                        format!(
                            "cannot build physical names: no {ty} element has a tag \
                             (no data named tag_0 for {ty} elements)"
                        ),
                    )
                })?;
            let (tags, fill) = tags;
            let sd = Int::try_from(ty.spatial_dimension()).unwrap_or(0);
            let names = (0..n)
                .map(|e| {
                    let t = tags.get(e).copied().unwrap_or(fill);
                    self.physical_names
                        .get(&(sd, t))
                        .cloned()
                        .unwrap_or_else(|| t.to_string())
                })
                .collect();
            out.insert(ty, names);
        }
        self.build.string_data.insert("physical_names".into(), out);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::lexer::Source;

    #[test]
    fn quoted_names() {
        let text = "2 3 \"my name\" rest\n\"a\\\"b\\\\c\"\nword other\n\"\"\n\"open\n";
        let mut src = Source::new("t", text);
        let mut l = src.next_line().unwrap();
        l.i32("a").unwrap();
        l.i32("b").unwrap();
        assert_eq!(l.quoted("n").unwrap(), "my name");
        assert_eq!(l.next_token().map(|t| t.1), Some("rest"));
        let mut l = src.next_line().unwrap();
        assert_eq!(l.quoted("n").unwrap(), "a\"b\\c");
        assert!(l.at_end());
        let mut l = src.next_line().unwrap();
        assert_eq!(l.quoted("n").unwrap(), "word");
        let mut l = src.next_line().unwrap();
        assert_eq!(l.quoted("n").unwrap(), "");
        let mut l = src.next_line().unwrap();
        assert!(l.quoted("n").is_err());
    }
}
