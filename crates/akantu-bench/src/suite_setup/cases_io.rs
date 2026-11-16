// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Parser, dumper and
// Python-twin cases of the set-up suite: the ports of src/io/parser
// (parser.cc, input_file_parser.hh, algebraic_parser.hh), src/io/dumper
// (dumper_paraview, dumper_text, IOHelper) and the explicit loop of
// examples/python/solid_mechanics_model/dynamics.

//! Parser, dumper and Python-twin cases.

use std::fmt::Write as _;

use akantu_core::enums::MeshIoType;
use akantu_core::{ElementKind, GhostType, Result};
use akantu_io::MeshSelection;
use akantu_parser::{Parser, ParserSection, SearchScope};
use akantu_solid::SolidMechanicsModel;

use super::cases_dist::explicit_model;
use super::cases_mesh::{Scratch, tet4_n};
use super::harness::{CaseOut, Ctx, Digest};
use super::meshgen::{self, Grid};
use super::nb_volume_elements;

/// A `.dat` input of `n` material sections (expressions referring to two
/// global parameters, a vector parameter and a nested section each).
fn dat_text(n: usize) -> String {
    let mut s = String::with_capacity(n * 260);
    s.push_str("a = 2.5\nb = a * 3 + sin(0.5)\nc = sqrt(b) / exp(0.1)\n\n");
    for k in 0..n {
        let _ = write!(
            s,
            "material elastic [\n    name = mat_{k}\n    rho = 7800 * a + {k}\n    \
             E = 2.1e11 / (1 + {k} * 1e-3)\n    nu = 0.3 - 1e-6 * {k}\n    \
             eigen = [0.1 * {k}, a, b * c]\n    \
             finite_deformation = false\n    \
             non_local extra [\n        h = b + {k} * c\n        w = pow(a, 2) - {k}\n    ]\n]\n\n"
        );
    }
    s
}

fn evaluate(sec: &ParserSection, d: &mut Digest) -> Result<usize> {
    let mut count = 0;
    for p in sec.parameters_sorted() {
        let name = p.name.as_bytes();
        let value = p.value.as_bytes();
        let r = sec.get_parameter(name, SearchScope::CurrentAndParent)?;
        if value.first() == Some(&b'[') {
            d.reals(&r.to_vector()?);
        } else if name == b"name" {
            d.bytes(value);
        } else if value == b"false" || value == b"true" {
            d.bytes(&[u8::from(r.to_bool())]);
        } else {
            d.reals(&[r.to_real()?]);
        }
        count += 1;
    }
    for sub in sec.sub_sections_sorted() {
        count += evaluate(sub, d)?;
    }
    Ok(count)
}

/// Parse and evaluate a large input.
pub fn parser_dat(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let n = ctx.size.pick(50, 2000, 16_000);
    let text = out.phase("generate", || Ok(dat_text(n)))?;
    let mut parser = Parser::default();
    out.loop_phase(ctx, "parse", ctx.size.pick(2, 200, 50), || {
        parser = Parser::parse_str(&text, "input.dat")?;
        Ok(())
    })?;
    let mut d = Digest::new();
    let mut count = 0;
    out.loop_phase(ctx, "evaluate", ctx.size.pick(2, 200, 50), || {
        d = Digest::new();
        count = evaluate(&parser.root, &mut d)?;
        Ok(())
    })?;
    out.n_elements = n;
    #[allow(clippy::cast_precision_loss)] // a count
    out.extra.push(("n_parameters", count as f64));
    out.digest = d.value();
    Ok(out)
}

/// An elastic Tet4 model after two explicit steps (non-zero fields).
fn dumped_model(ctx: &Ctx, out: &mut CaseOut) -> Result<SolidMechanicsModel> {
    let mesh = out.phase("mesh", || meshgen::mesh(Grid::Tet4, tet4_n(ctx), "mesh"))?;
    out.n_elements = nb_volume_elements(&mesh);
    out.n_nodes = mesh.nb_nodes();
    out.n_dofs = 3 * out.n_nodes;
    let mut model = explicit_model(mesh, out)?;
    model.solve_step(None)?;
    model.solve_step(None)?;
    Ok(model)
}

fn first_file_digest(dir: &std::path::Path, ext: &str) -> Result<u64> {
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        let rd = std::fs::read_dir(&p).map_err(|source| akantu_core::Error::Io {
            path: p.display().to_string(),
            source,
        })?;
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|x| x.to_str()) == Some(ext) {
                files.push(path);
            }
        }
    }
    files.sort();
    let Some(f) = files.first() else {
        return Err(akantu_core::Error::invalid(format!(
            "no .{ext} file written"
        )));
    };
    let bytes = std::fs::read(f).map_err(|source| akantu_core::Error::Io {
        path: f.display().to_string(),
        source,
    })?;
    Ok(Digest::new().bytes(&bytes).value())
}

/// Dumps per loop: 2–3 s at `m` (the text mode takes about 6× the
/// base64 mode).
fn dump_calls(ctx: &Ctx, ascii: bool) -> usize {
    if ascii {
        ctx.size.pick(1, 3, 1)
    } else {
        ctx.size.pick(1, 15, 4)
    }
}

fn dump_case(ctx: &Ctx, ascii: bool) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mut model = dumped_model(ctx, &mut out)?;
    let dir = Scratch::new(if ascii { "vtu-ascii" } else { "vtu" })?;
    model.set_directory(&dir.0)?;
    model.set_base_name("suite")?;
    for f in [
        "displacement",
        "velocity",
        "acceleration",
        "stress",
        "strain",
    ] {
        model.add_dump_field(f)?;
    }
    if ascii {
        model.dumpers.dumpable.set_text_mode()?;
    }
    out.loop_phase(ctx, "dump", dump_calls(ctx, ascii), || {
        model.dump(None, None)
    })?;
    out.digest = first_file_digest(&dir.0, "vtu")?;
    Ok(out)
}

/// ParaView (base64) dumps.
pub fn dump_paraview_tet4(ctx: &Ctx) -> Result<CaseOut> {
    dump_case(ctx, false)
}

/// ParaView (ASCII) dumps.
pub fn dump_paraview_ascii_tet4(ctx: &Ctx) -> Result<CaseOut> {
    dump_case(ctx, true)
}

/// Text dumps of the nodal fields.
pub fn dump_text_tet4(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mut model = dumped_model(ctx, &mut out)?;
    let dir = Scratch::new("text")?;
    model
        .dumpers
        .dumpable
        .register_text("text", Some("suite"), false);
    model.dumpers.dumpable.add_dump_mesh_to_dumper(
        "text",
        MeshSelection::new(Some(3), GhostType::NotGhost, ElementKind::Regular),
    )?;
    model.set_directory_to_dumper("text", &dir.0)?;
    for f in ["displacement", "velocity", "acceleration"] {
        model.add_dump_field_to_dumper("text", f)?;
    }
    out.loop_phase(ctx, "dump", ctx.size.pick(1, 30, 8), || {
        model.dump_named("text", None, None)
    })?;
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir.0)
        .map_err(|source| akantu_core::Error::Io {
            path: dir.0.display().to_string(),
            source,
        })?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    let mut d = Digest::new();
    // the files of the first dump (their names carry the dump counter)
    for f in files.iter().filter(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.contains("0000"))
    }) {
        let bytes = std::fs::read(f).map_err(|source| akantu_core::Error::Io {
            path: f.display().to_string(),
            source,
        })?;
        d.bytes(&bytes);
    }
    out.digest = d.value();
    Ok(out)
}

/// Cells per side of the Tri3 square of the Python twin (s: 128 triangles;
/// m: 80,000; l: 640,000). `python/benchmarks/suite_python.py` uses the
/// same values.
pub fn tri3_n(ctx: &Ctx) -> usize {
    ctx.size.pick(8, 200, 566)
}

/// Steps of the Python twin's loop (after three warm-up steps).
pub fn tri3_steps(ctx: &Ctx) -> usize {
    ctx.size.pick(5, 400, 100)
}

fn tri3_model(n: usize, out: &mut CaseOut) -> Result<SolidMechanicsModel> {
    let text = meshgen::msh22(Grid::Tri3, n);
    let mut mesh = akantu_mesh::Mesh::new(2, "mesh")?;
    akantu_io::read_mesh_from_str(
        &mut mesh,
        "square.msh",
        &text,
        MeshIoType::Gmsh,
        &akantu_io::ReadOptions::default(),
    )?;
    out.n_elements = nb_volume_elements(&mesh);
    out.n_nodes = mesh.nb_nodes();
    out.n_dofs = 2 * out.n_nodes;
    explicit_model(mesh, out)
}

/// Rust twin of the Python explicit loop: Tri3 square, steel, the pulse in
/// x, 0.8 × the stable time step, three warm-up steps then the timed loop;
/// the digest is FNV-1a over the displacement then the velocity (the
/// Python case computes the same and must find the same value). Then
/// `solve_step_tiny`: the same loop on a 4 × 4 square, which exposes the
/// per-call overhead.
pub fn py_twin_explicit_tri3(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mut model = tri3_model(tri3_n(ctx), &mut out)?;
    out.loop_phase(ctx, "solve_step", tri3_steps(ctx), || {
        model.solve_step(None)
    })?;
    let mut d = Digest::new();
    d.reals(model.displacement()?.as_slice());
    d.reals(model.velocity()?.as_slice());
    out.digest = d.value();
    let (ne, nn, nd) = (out.n_elements, out.n_nodes, out.n_dofs);
    let mut tiny_out = CaseOut::default();
    let mut tiny = tri3_model(4, &mut tiny_out)?;
    out.loop_phase(
        ctx,
        "solve_step_tiny",
        ctx.size.pick(20, 300_000, 300_000),
        || tiny.solve_step(None),
    )?;
    (out.n_elements, out.n_nodes, out.n_dofs) = (ne, nn, nd);
    Ok(out)
}
