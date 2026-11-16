// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/dumper/dumper_iohelper.{hh,cc}
// (DumperIOHelper: field registry, count, setTimeStep, dump variants,
// setParallelContext) and third-party/iohelper/src/dumper.{hh,cc}
// (iohelper::Dumper: prefix, base name, file naming, dump step, time
// description, parallel context).
// IOHelper (third-party/iohelper, v1.1.1): Copyright (©) 2010-2012, 2014 EPFL
// (Ecole Polytechnique Fédérale de Lausanne), Laboratory (LSMS - Laboratoire
// de Simulation en Mécanique des Solides). IOHelper is free software: you can
// redistribute it and/or modify it under the terms of the GNU Lesser General
// Public License as published by the Free Software Foundation, either version
// 3 of the License, or (at your option) any later version.

//! State shared by the ParaView, text and LAMMPS dumpers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use akantu_core::{ElementKind, Error, GhostType, Rank, Real, Result};
use akantu_mesh::Mesh;

use super::{FieldDescriptor, MeshFilter, MeshSelection, ParallelContext};

/// A registered name (upstream: one `std::map` of fields per dumper).
#[derive(Clone, Debug)]
pub(crate) enum Entry {
    /// The mesh connectivity (`"connectivities"`).
    Connectivity,
    /// The mesh positions (`"positions"`, text dumper `"position"`).
    Positions,
    /// A field.
    Field(FieldDescriptor),
}

/// Resolved parallel context of one dump.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Parallel {
    pub(crate) rank: usize,
    pub(crate) size: usize,
    pub(crate) root: usize,
    /// Width of the `.proc` rank suffix.
    pub(crate) rank_width: usize,
}

/// `ceil(log10(n))` (number of decimal digits for ranks `0..n`).
fn rank_width(n: usize) -> usize {
    let mut w = 0;
    let mut p: usize = 1;
    while p < n {
        p = p.saturating_mul(10);
        w += 1;
    }
    w
}

fn rank_of(r: Rank) -> usize {
    usize::try_from(r).unwrap_or(0)
}

/// Largest size of a [`ParallelContext::Explicit`].
///
/// The root rank renders the `.pvtu` in memory, with one piece reference
/// (≈ 50 bytes plus the base name) per rank, before writing anything; an
/// explicit size near `i32::MAX` would exhaust memory. 2²⁰ ranks keep that
/// below ≈ 100 MB. Upstream takes the size from a real communicator
/// (`ParallelContext::FromMesh` here), which is not bounded.
pub const MAX_EXPLICIT_PARALLEL_SIZE: usize = 1 << 20;

impl ParallelContext {
    /// Rank, size and root of one dump. An explicit context must name a
    /// rank and a root below its size, with a size of at most
    /// [`MAX_EXPLICIT_PARALLEL_SIZE`] (checked before anything is rendered
    /// or written).
    pub(crate) fn resolve(&self, mesh: Option<&Mesh>) -> Result<Parallel> {
        Ok(match *self {
            ParallelContext::FromMesh => {
                let (rank, size) = mesh.map_or((0, 1), |m| {
                    (
                        rank_of(m.communicator().rank()),
                        m.communicator().size().max(1),
                    )
                });
                Parallel {
                    rank,
                    size,
                    root: 0,
                    rank_width: rank_width(size),
                }
            }
            ParallelContext::Serial => Parallel {
                rank: 0,
                size: 1,
                root: 0,
                rank_width: 0,
            },
            ParallelContext::Explicit { rank, size, root } => {
                let size = size.max(1);
                if size > MAX_EXPLICIT_PARALLEL_SIZE
                    || Rank::try_from(size).is_err()
                    || rank >= size
                    || root >= size
                {
                    return Err(Error::InvalidArgument(format!(
                        "dumper parallel context: rank {rank} and root {root} must be below the size {size} (at most {MAX_EXPLICIT_PARALLEL_SIZE})"
                    )));
                }
                Parallel {
                    rank,
                    size,
                    root,
                    rank_width: rank_width(size),
                }
            }
            ParallelContext::IoHelperDefault => Parallel {
                rank: 0,
                size: 1,
                root: 0,
                rank_width: 3,
            },
        })
    }
}

/// `Dumper::checkDirectoryName`: append the separator unless empty.
pub(crate) fn directory_name(dir: &str) -> String {
    if dir.is_empty() || dir.ends_with('/') {
        dir.to_owned()
    } else {
        format!("{dir}/")
    }
}

/// The part of [`Common`] that a dump changes (restored on failure).
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    count: usize,
    current_time: Real,
    time_description_file_name: Option<String>,
}

/// Common dumper state.
#[derive(Clone, Debug)]
pub(crate) struct Common {
    /// `DumperIOHelper::filename` / IOHelper base name.
    pub(crate) base_name: String,
    /// Output directory with trailing separator (IOHelper `prefix`).
    pub(crate) prefix: String,
    /// `DumperIOHelper::count`.
    pub(crate) count: usize,
    pub(crate) time_step: Real,
    pub(crate) current_time: Real,
    pub(crate) write_time_desc: bool,
    /// Fixed at the first dump (IOHelper `time_description_file_name`).
    pub(crate) time_description_file_name: Option<String>,
    pub(crate) parallel: ParallelContext,
    /// Registered mesh part (`None`: the regular elements of the mesh's
    /// spatial dimension, as `Model::addDumpField` creates fields).
    pub(crate) selection: Option<MeshSelection>,
    pub(crate) entries: BTreeMap<String, Entry>,
    pub(crate) variables: BTreeSet<String>,
}

impl Common {
    pub(crate) fn new(base_name: &str, directory: &str, parallel: ParallelContext) -> Self {
        Self {
            base_name: base_name.to_owned(),
            prefix: directory_name(directory),
            count: 0,
            time_step: 0.0,
            current_time: 0.0,
            write_time_desc: false,
            time_description_file_name: None,
            parallel,
            selection: None,
            entries: BTreeMap::new(),
            variables: BTreeSet::new(),
        }
    }

    /// Register a name (error on a duplicate; upstream warns and ignores).
    pub(crate) fn insert(&mut self, name: &str, entry: Entry) -> Result<()> {
        if self.entries.contains_key(name) || self.variables.contains(name) {
            return Err(Error::AlreadyExists {
                kind: "dump field",
                id: name.to_owned(),
            });
        }
        self.entries.insert(name.to_owned(), entry);
        Ok(())
    }

    /// `unRegisterField` (error if unknown; upstream warns).
    pub(crate) fn remove(&mut self, name: &str) -> Result<()> {
        if self.entries.remove(name).is_some() {
            Ok(())
        } else {
            Err(Error::not_found("dump field", name))
        }
    }

    /// `unRegisterVariable` (error if unknown; upstream warns).
    pub(crate) fn remove_variable(&mut self, name: &str) -> Result<()> {
        if self.variables.remove(name) {
            Ok(())
        } else {
            Err(Error::not_found("dump variable", name))
        }
    }

    /// Follow renamed groups (`dumper/group.rs`): for every
    /// `old → new` of `renames` (applied simultaneously), the selection on
    /// group `old` (element group if `element`, else node group) and field
    /// descriptors restricted to element group `old` name `new` afterwards.
    pub(crate) fn rename_groups(&mut self, element: bool, renames: &BTreeMap<String, String>) {
        if let Some(sel) = &mut self.selection {
            let target = match &mut sel.filter {
                MeshFilter::ElementGroup(g) if element => Some(g),
                MeshFilter::NodeGroup(g) if !element => Some(g),
                _ => None,
            };
            if let Some(g) = target
                && let Some(new) = renames.get(g.as_str())
            {
                *g = new.clone();
            }
        }
        if element {
            for e in self.entries.values_mut() {
                if let Entry::Field(f) = e
                    && let Some(new) = f.group.as_ref().and_then(|g| renames.get(g))
                {
                    f.group = Some(new.clone());
                }
            }
        }
    }

    pub(crate) fn add_variable(&mut self, name: &str) -> Result<()> {
        if self.entries.contains_key(name) || self.variables.contains(name) {
            return Err(Error::AlreadyExists {
                kind: "dump variable",
                id: name.to_owned(),
            });
        }
        self.variables.insert(name.to_owned());
        Ok(())
    }

    pub(crate) fn set_directory(&mut self, directory: &Path) -> Result<()> {
        let s = directory.to_str().ok_or_else(|| {
            Error::InvalidArgument(format!("non UTF-8 dump directory {directory:?}"))
        })?;
        self.prefix = directory_name(s);
        Ok(())
    }

    /// `DumperIOHelper::setTimeStep`: upstream never sets `time_activated`,
    /// so every call re-activates the time description and restarts the
    /// clock at 0 (`activateTimeDescFiles(dt)`).
    pub(crate) fn set_time_step(&mut self, dt: Real) {
        self.time_step = dt;
        self.current_time = 0.0;
        self.write_time_desc = true;
    }

    /// Start a dump (`dump()`, `dump(step)`, `dump(time, step)`): returns
    /// the dump step; the count is incremented by [`Common::end_dump`].
    /// A step whose successor does not fit the counter is an error,
    /// reported before any state changes or file is written.
    pub(crate) fn begin_dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<usize> {
        let s = step.unwrap_or(self.count);
        if s.checked_add(1).is_none() {
            return Err(Error::InvalidArgument(format!(
                "dump step {s}: the dump counter cannot be advanced past it"
            )));
        }
        if let Some(t) = time {
            self.current_time = t;
        }
        self.count = s;
        if self.time_description_file_name.is_none() {
            self.time_description_file_name = Some(self.base_name.clone());
        }
        Ok(s)
    }

    /// Finish a successful dump (the successor was checked by
    /// [`Common::begin_dump`]).
    pub(crate) fn end_dump(&mut self) {
        self.count = self.count.saturating_add(1);
    }

    /// State a dump may change before it is known to succeed.
    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            count: self.count,
            current_time: self.current_time,
            time_description_file_name: self.time_description_file_name.clone(),
        }
    }

    /// Undo a failed dump (render, directory or write error).
    pub(crate) fn restore(&mut self, s: Snapshot) {
        self.count = s.count;
        self.current_time = s.current_time;
        self.time_description_file_name = s.time_description_file_name;
    }

    /// The registered selection, or the default one for `mesh`.
    pub(crate) fn selection_for(&self, mesh: &Mesh) -> MeshSelection {
        self.selection.clone().unwrap_or(MeshSelection {
            spatial_dimension: Some(mesh.spatial_dimension()),
            ghost: GhostType::NotGhost,
            kind: Some(ElementKind::Regular),
            filter: MeshFilter::All,
        })
    }
}

/// `Dumper::getFileName`: `name[_step:04][.proc<rank:width>]ext`.
pub(crate) fn file_name(
    name: &str,
    step: Option<usize>,
    proc_: Option<(usize, &Parallel)>,
    ext: &str,
) -> String {
    let mut s = String::from(name);
    if let Some(step) = step {
        s.push_str(&format!("_{step:04}"));
    }
    if let Some((rank, p)) = proc_
        && p.size > 1
    {
        let w = p.rank_width;
        s.push_str(&format!(".proc{rank:0w$}"));
    }
    s.push_str(ext);
    s
}

/// Create a dump directory (upstream `mkdir`, one level, errors ignored;
/// akantu-rs creates missing parents and reports failures).
pub(crate) fn make_dir(path: &str) -> Result<()> {
    if path.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}

fn io_error(path: &str) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_owned(),
        source,
    }
}

/// How a rendered file reaches the disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WriteMode {
    /// Create or truncate, then write.
    Replace,
    /// Write a sibling temporary file and rename it over the target, so
    /// that readers see the previous or the new contents, never a partial
    /// file (collection metadata: `.pvd`).
    Atomic,
    /// Append (created if missing).
    Append,
}

/// A file whose contents are written straight to the disk when the dump
/// commits (validated beforehand: only I/O can fail while it is written).
pub(crate) trait StreamContents: std::fmt::Debug {
    /// Write the whole contents to `w`.
    fn write_to(&self, w: &mut dyn std::io::Write) -> std::io::Result<()>;
}

/// Contents of a rendered file.
#[derive(Debug)]
pub(crate) enum Contents {
    /// Rendered in memory.
    Text(String),
    /// Written when the files are written ([`StreamContents`]).
    Stream(Box<dyn StreamContents>),
}

/// One rendered file.
#[derive(Debug)]
pub(crate) struct OutFile {
    pub(crate) path: String,
    pub(crate) contents: Contents,
    pub(crate) mode: WriteMode,
}

impl OutFile {
    pub(crate) fn new(path: String, contents: String, mode: WriteMode) -> Self {
        Self {
            path,
            contents: Contents::Text(contents),
            mode,
        }
    }

    pub(crate) fn stream(path: String, contents: Box<dyn StreamContents>, mode: WriteMode) -> Self {
        Self {
            path,
            contents: Contents::Stream(contents),
            mode,
        }
    }
}

/// Size of the write buffer of streamed files.
const STREAM_BUFFER: usize = 1 << 20;

/// Write `contents` to the open file `f`.
fn write_contents(f: std::fs::File, contents: &Contents) -> std::io::Result<()> {
    use std::io::Write;
    match contents {
        Contents::Text(s) => {
            let mut f = f;
            f.write_all(s.as_bytes())
        }
        Contents::Stream(c) => {
            let mut w = std::io::BufWriter::with_capacity(STREAM_BUFFER, f);
            c.write_to(&mut w)?;
            w.flush()
        }
    }
}

/// Create or truncate `path` and write `contents` (`std::fs::write`).
fn write_replace(path: &str, contents: &Contents) -> Result<()> {
    std::fs::File::create(path)
        .and_then(|f| write_contents(f, contents))
        .map_err(io_error(path))
}

/// Undo record of an appended file: its length before the dump
/// (`None`: it did not exist).
struct Appended<'a> {
    path: &'a str,
    len: Option<u64>,
}

/// Length of `path` before this dump appends to it (`None`: absent).
fn length_before_append(path: &str) -> Result<Option<u64>> {
    match std::fs::metadata(path) {
        Ok(m) => Ok(Some(m.len())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_error(path)(e)),
    }
}

fn append(path: &str, contents: &Contents) -> Result<()> {
    let f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(io_error(path))?;
    write_contents(f, contents).map_err(io_error(path))
}

fn write_atomic(path: &str, contents: &Contents) -> Result<()> {
    let tmp = format!("{path}.akantu-tmp");
    if let Err(e) = write_replace(&tmp, contents) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io_error(path)(e)
    })
}

/// Best-effort rollback of the appends of a failed dump.
fn undo_appends(done: &[Appended<'_>]) {
    for a in done.iter().rev() {
        match a.len {
            Some(len) => {
                if let Ok(f) = std::fs::OpenOptions::new().write(true).open(a.path) {
                    let _ = f.set_len(len);
                }
            }
            None => {
                let _ = std::fs::remove_file(a.path);
            }
        }
    }
}

/// Write the files of one dump in order after creating `dirs`. On the
/// first failure the bytes appended by this call are removed again
/// (best effort) and the error is returned, so that the caller can
/// restore its state and a retry neither duplicates appended lines nor
/// references missing data. Files are given in dependency order (data
/// pieces before the metadata that references them).
pub(crate) fn write_files(dirs: &[&str], files: &[OutFile]) -> Result<()> {
    write_files_with(dirs, files, append)
}

/// [`write_files`] with the append primitive injectable (tests simulate a
/// write failing part-way, e.g. `EFBIG`/`ENOSPC`).
fn write_files_with(
    dirs: &[&str],
    files: &[OutFile],
    append: impl Fn(&str, &Contents) -> Result<()>,
) -> Result<()> {
    for d in dirs {
        make_dir(d)?;
    }
    let mut done: Vec<Appended<'_>> = Vec::new();
    for f in files {
        let r = match f.mode {
            WriteMode::Replace => write_replace(&f.path, &f.contents),
            WriteMode::Atomic => write_atomic(&f.path, &f.contents),
            // The undo record is taken *before* writing, so a write that
            // fails part-way is rolled back too.
            WriteMode::Append => length_before_append(&f.path).and_then(|len| {
                done.push(Appended { path: &f.path, len });
                append(&f.path, &f.contents)
            }),
        };
        if let Err(e) = r {
            undo_appends(&done);
            return Err(e);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An append failing part-way leaves no partial
    /// bytes — a new file is removed, an existing one truncated back — and
    /// earlier appends of the same dump are rolled back as well.
    #[test]
    fn dmp2_01_partial_append_is_rolled_back() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!(
            "akantu-dump-rollback-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = |n: &str| dir.join(n).to_str().unwrap().to_owned();
        let (a, b, c) = (path("a.out"), path("b.out"), path("c.out"));
        std::fs::write(&a, "a0\n").unwrap();
        std::fs::write(&c, "c0\n").unwrap();
        let files = [
            OutFile::new(a.clone(), "a1\n".into(), WriteMode::Append),
            OutFile::new(b.clone(), "1.000000e+00\n".into(), WriteMode::Append),
            OutFile::new(c.clone(), "c1 c1 c1\n".into(), WriteMode::Append),
        ];
        // Fail part-way through the second (new) file.
        let failing_at = |target: String| {
            move |p: &str, contents: &Contents| -> Result<()> {
                if p != target {
                    return append(p, contents);
                }
                let Contents::Text(text) = contents else {
                    unreachable!("text contents in this test")
                };
                let bytes = text.as_bytes();
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(p)
                    .unwrap();
                f.write_all(&bytes[..bytes.len() / 2]).unwrap();
                Err(Error::invalid("simulated EFBIG"))
            }
        };
        assert!(write_files_with(&[], &files, failing_at(b.clone())).is_err());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "a0\n");
        assert!(!std::path::Path::new(&b).exists(), "new file removed");
        assert_eq!(std::fs::read_to_string(&c).unwrap(), "c0\n");
        // Fail part-way through an existing file.
        assert!(write_files_with(&[], &files, failing_at(c.clone())).is_err());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "a0\n");
        assert!(!std::path::Path::new(&b).exists());
        assert_eq!(
            std::fs::read_to_string(&c).unwrap(),
            "c0\n",
            "truncated back"
        );
        // A retry without failure appends exactly one record per file.
        write_files_with(&[], &files, append).unwrap();
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "a0\na1\n");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "1.000000e+00\n");
        assert_eq!(std::fs::read_to_string(&c).unwrap(), "c0\nc1 c1 c1\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rank_width_is_ceil_log10() {
        assert_eq!(rank_width(1), 0);
        assert_eq!(rank_width(2), 1);
        assert_eq!(rank_width(10), 1);
        assert_eq!(rank_width(11), 2);
        assert_eq!(rank_width(100), 2);
        assert_eq!(rank_width(101), 3);
    }

    #[test]
    fn file_names_follow_iohelper() {
        let p = Parallel {
            rank: 0,
            size: 11,
            root: 0,
            rank_width: 2,
        };
        assert_eq!(file_name("m2", Some(7), None, ".pvtu"), "m2_0007.pvtu");
        assert_eq!(
            file_name("m2", Some(12345), Some((3, &p)), ".vtu"),
            "m2_12345.proc03.vtu"
        );
        let serial = Parallel {
            size: 1,
            rank_width: 0,
            ..p
        };
        assert_eq!(
            file_name("m2", Some(0), Some((0, &serial)), ".vtu"),
            "m2_0000.vtu"
        );
        assert_eq!(directory_name("./paraview"), "./paraview/");
        assert_eq!(directory_name(""), "");
    }
}
