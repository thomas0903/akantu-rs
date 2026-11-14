// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Shared support of the
// Tests (COH-013): the scenario of oracle/probes/cohesive_dump/
// cohesive_dump_probe.cc (upstream TestSMMCFixture, test/test_model/
// test_solid_mechanics_model/test_cohesive/test_cohesive_fixture.hh, mode I)
// replayed on the port with the same dumper calls, and a minimal reader of
// the ParaView files (ASCII and IOHelper base64 data arrays).

#![allow(dead_code)] // shared by several test binaries

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use akantu_cohesive::{COHESIVE_DUMPER, FACETS_DUMPER, SolidMechanicsModelCohesive};
use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_io::DumperParaview;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;

use crate::common::{self, StrainIncrement};

/// A registration: `vector` = `addDumpFieldVectorToDumper`.
#[derive(Clone, Copy, Debug)]
pub struct FieldReg {
    pub name: &'static str,
    pub vector: bool,
}

const fn f(name: &'static str, vector: bool) -> FieldReg {
    FieldReg { name, vector }
}

/// One probe case (same fields as the probe's `DumpCase`).
#[derive(Clone, Debug)]
pub struct DumpCase {
    pub id: &'static str,
    pub cohesive_type: ElementType,
    pub mesh: &'static str,
    pub extrinsic: bool,
    pub material: &'static str,
    pub dumper: &'static str,
    pub base_name: &'static str,
    pub text: bool,
    pub fields: Vec<FieldReg>,
    /// Names registered after the fields that the port must refuse.
    pub ignored: Vec<&'static str>,
    /// Step counts after which the dumper dumps.
    pub dumps: Vec<usize>,
    /// Compared byte for byte (`false`: upstream evidence of a defect).
    pub golden: bool,
}

const TRI3: &str = "_cohesive_2d_4_triangle_3.msh";
const TRI6: &str = "_cohesive_2d_6_triangle_6.msh";
const TET4: &str = "_cohesive_3d_6_tetrahedron_4.msh";

fn all_cohesive() -> Vec<FieldReg> {
    let mut v = vec![
        f("displacement", true),
        f("velocity", true),
        f("acceleration", true),
        f("internal_force", true),
        f("external_force", false),
        f("mass", false),
        f("blocked_dofs", false),
    ];
    for n in [
        "damage",
        "contact_opening",
        "contact_tractions",
        "delta max",
        "normal",
        "reversible_energy",
        "total_energy",
        "material_index",
        "partitions",
    ] {
        v.push(f(n, false));
    }
    v.push(f("opening", true));
    v.push(f("tractions", true));
    for n in ["sigma_c_eff", "delta_c_eff", "insertion_stress"] {
        v.push(f(n, false));
    }
    v
}

/// No `material_index` before the first insertion: upstream's dump raises
/// (`evidence_material_index_before_insertion`, UD-756).
fn extrinsic_fields() -> Vec<FieldReg> {
    vec![
        f("displacement", true),
        f("damage", false),
        f("opening", true),
        f("tractions", true),
        f("sigma_c_eff", false),
        f("delta_c_eff", false),
        f("insertion_stress", false),
        f("velocity", false),
    ]
}

/// The probe's cases, in the probe's order.
pub fn cases() -> Vec<DumpCase> {
    let coh = COHESIVE_DUMPER;
    let base =
        |id, ct, mesh, extrinsic, material, dumper, base_name, text, fields, dumps| DumpCase {
            id,
            cohesive_type: ct,
            mesh,
            extrinsic,
            material,
            dumper,
            base_name,
            text,
            fields,
            ignored: Vec::new(),
            dumps,
            golden: true,
        };
    let mut v = vec![
        base(
            "intrinsic_tri3_bilinear_base64",
            ElementType::Cohesive2d4,
            TRI3,
            false,
            "material_1.dat",
            coh,
            "cohesive",
            false,
            all_cohesive(),
            vec![0, 10, 20, 30],
        ),
        base(
            "extrinsic_tri3_linear_base64",
            ElementType::Cohesive2d4,
            TRI3,
            true,
            "material_0.dat",
            coh,
            "",
            false,
            extrinsic_fields(),
            vec![0, 25, 26, 40],
        ),
        base(
            "extrinsic_tri3_linear_text",
            ElementType::Cohesive2d4,
            TRI3,
            true,
            "material_0.dat",
            coh,
            "cohesive_text",
            true,
            extrinsic_fields(),
            vec![0, 25, 26, 40],
        ),
        base(
            "extrinsic_tet4_linear_base64",
            ElementType::Cohesive3d6,
            TET4,
            true,
            "material_0.dat",
            coh,
            "cohesive",
            false,
            {
                let mut x = extrinsic_fields();
                x.push(f("normal", false));
                x.push(f("partitions", false));
                x.push(f("material_index", false));
                x
            },
            vec![35, 39, 50],
        ),
        base(
            "intrinsic_tet4_bilinear_text",
            ElementType::Cohesive3d6,
            TET4,
            false,
            "material_1.dat",
            coh,
            "cohesive",
            true,
            all_cohesive(),
            vec![0, 15, 30],
        ),
        base(
            "extrinsic_tri6_linear_text",
            ElementType::Cohesive2d6,
            TRI6,
            true,
            "material_0.dat",
            coh,
            "cohesive",
            true,
            extrinsic_fields(),
            vec![0, 36, 37, 45],
        ),
        base(
            "facets_tri3_extrinsic",
            ElementType::Cohesive2d4,
            TRI3,
            true,
            "material_0.dat",
            FACETS_DUMPER,
            "",
            false,
            vec![
                f("displacement", true),
                f("sigma_c", false),
                f("velocity", false),
            ],
            vec![0, 10],
        ),
    ];
    v.push(DumpCase {
        golden: false,
        ..base(
            "evidence_material_index_before_insertion",
            ElementType::Cohesive2d4,
            TRI3,
            true,
            "material_0.dat",
            coh,
            "material_index",
            false,
            vec![f("material_index", false)],
            vec![0, 25],
        )
    });
    v.push(DumpCase {
        golden: false,
        ..base(
            "evidence_facets_tri3_intrinsic",
            ElementType::Cohesive2d4,
            TRI3,
            false,
            "material_1.dat",
            FACETS_DUMPER,
            "",
            true,
            vec![f("displacement", false), f("sigma_c", false)],
            vec![0],
        )
    });
    v
}

/// The upstream test material files (`test_cohesive/material_{0,1}.dat`).
pub fn material_text(name: &str) -> &'static str {
    match name {
        "material_0.dat" => common::MATERIAL_0,
        "material_1.dat" => common::MATERIAL_1,
        other => panic!("unknown material {other}"),
    }
}

/// State of a replayed case.
pub struct Run {
    pub mesh: SharedMesh,
    pub model: SolidMechanicsModelCohesive,
    pub dt: Real,
    pub nb_cohesive_at_dump: Vec<i64>,
    pub nb_facets_at_dump: Vec<i64>,
    /// Errors of the `ignored` registrations.
    pub ignored_errors: Vec<String>,
    /// Dump errors (with [`ReplayOptions::allow_dump_error`]).
    pub dump_errors: Vec<String>,
}

/// Options of a replay.
#[derive(Clone, Debug, Default)]
pub struct ReplayOptions {
    /// Upstream byte layout of the ParaView writer
    /// (`DumperParaview::set_compat_layout`, UD-110/113/114).
    pub compat_layout: bool,
    /// Upstream 3-D facet tangents (UD-217, as the fixture tests of
    /// `fixtures/model/cohesive`).
    pub upstream_tangents: bool,
    /// Material file (default: the upstream text of the case's file).
    pub material_file: Option<PathBuf>,
    /// Mesh file (default: the frozen upstream test mesh of the case).
    pub mesh_file: Option<PathBuf>,
    /// Stop after the dump of this index.
    pub stop_after: Option<usize>,
    /// Record dump errors in [`Run::dump_errors`] instead of panicking.
    pub allow_dump_error: bool,
}

/// Replay `case` (the probe's `run_case`), writing the dumps to `out`.
pub fn replay(case: &DumpCase, out: &Path, opt: &ReplayOptions) -> Run {
    replay_with(case, out, opt, |_, _| {})
}

/// [`replay`] with `after_dump(i, &model)` called after the `i`-th dump.
pub fn replay_with(
    case: &DumpCase,
    out: &Path,
    opt: &ReplayOptions,
    mut after_dump: impl FnMut(usize, &mut Run),
) -> Run {
    let dim = case.cohesive_type.spatial_dimension();
    let mesh_path = opt
        .mesh_file
        .clone()
        .unwrap_or_else(|| common::cohesive_mesh(case.mesh));
    let mesh = common::read_mesh(&mesh_path, dim);
    let parser = match &opt.material_file {
        Some(p) => Parser::parse_file(p).unwrap(),
        None => Parser::parse_str(material_text(case.material), case.material).unwrap(),
    };
    akantu_core::random::RandomGenerator::<akantu_core::random::IdxSlot>::seed(7);
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    model.ext.compat.tangent_rotation_2d_in_3d = opt.upstream_tangents && dim == 3;
    model
        .init_full(
            &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
            case.extrinsic,
        )
        .unwrap();
    let stable = model.stable_time_step().unwrap();
    let dt = stable * 0.01;
    model.set_time_step(dt, None).unwrap();

    // TestSMMCFixture::testModeI
    let sigma_c = common::real_param(&model, "insertion", "sigma_c");
    let e = common::real_param(&model, "body", "E");
    let nu = common::real_param(&model, "body", "nu");
    let mut strain: Vec<Real> = match dim {
        2 => vec![-nu * (1. + nu), 0., 0., (1. - nu) * (1. + nu)],
        _ => vec![-nu, 0., 0., 0., 1., 0., 0., 0., -nu],
    };
    for x in &mut strain {
        *x *= sigma_c / e;
    }
    let init_factor = 1. - 1e-5;
    let inc_factor = 2e-2;
    let init: Vec<Real> = strain.iter().map(|s| init_factor * s).collect();
    {
        let nodes = mesh.read().unwrap().nodes().as_slice().to_vec();
        let mut u = model.smm.displacement_mut().unwrap();
        for (x, ui) in nodes.chunks(dim).zip(u.as_mut_slice().chunks_mut(dim)) {
            for i in 0..dim {
                let mut acc = 0.0;
                for k in 0..dim {
                    acc += init[i + dim * k] * x[k];
                }
                ui[i] = acc;
            }
        }
    }
    for s in &mut strain {
        *s = 1. / 300. * (inc_factor * *s);
    }
    let functor = StrainIncrement {
        strain,
        dim,
        dir: 1,
    };

    // as the probe: the lumped mass exists before "mass" is registered
    model.smm.assemble_mass_lumped().unwrap();

    // the dumper (same calls as the probe)
    if case.dumper == FACETS_DUMPER {
        model.register_facets_dumper(Some(FACETS_DUMPER)).unwrap();
        model.set_time_step_to_dumper(FACETS_DUMPER, dt).unwrap();
    }
    model.set_directory_to_dumper(case.dumper, out).unwrap();
    if !case.base_name.is_empty() {
        model
            .set_base_name_to_dumper(case.dumper, case.base_name)
            .unwrap();
    }
    if case.text {
        model.set_text_mode_to_dumper(case.dumper).unwrap();
    }
    for r in &case.fields {
        if r.vector {
            model
                .add_dump_field_vector_to_dumper(case.dumper, r.name)
                .unwrap_or_else(|e| panic!("{}: {}: {e}", case.id, r.name));
        } else {
            model
                .add_dump_field_to_dumper(case.dumper, r.name)
                .unwrap_or_else(|e| panic!("{}: {}: {e}", case.id, r.name));
        }
    }
    let mut ignored_errors = Vec::new();
    for n in &case.ignored {
        match model.add_dump_field_to_dumper(case.dumper, n) {
            Ok(()) => panic!("{}: '{n}' must be refused on {}", case.id, case.dumper),
            Err(e) => ignored_errors.push(format!("{n}: {e}")),
        }
    }
    model
        .smm
        .dumpers
        .dumpable
        .dumper_as::<DumperParaview>(case.dumper)
        .unwrap()
        .set_compat_layout(opt.compat_layout);

    let mut run = Run {
        mesh: mesh.clone(),
        model,
        dt,
        nb_cohesive_at_dump: Vec::new(),
        nb_facets_at_dump: Vec::new(),
        ignored_errors,
        dump_errors: Vec::new(),
    };
    let facet_type = case.cohesive_type.facet_type(0).unwrap();
    let mut done = 0;
    for (i, &target) in case.dumps.iter().enumerate() {
        while done < target {
            run.model
                .smm
                .apply_dirichlet_bc(&functor, "loading")
                .unwrap();
            run.model.smm.apply_dirichlet_bc(&functor, "fixed").unwrap();
            if case.extrinsic {
                run.model.check_cohesive_stress().unwrap();
            }
            run.model.solve_step(None).unwrap();
            done += 1;
        }
        if let Err(e) = run.model.dump_named(case.dumper, None, None) {
            assert!(opt.allow_dump_error, "{}: dump {i}: {e}", case.id);
            run.dump_errors.push(e.to_string());
        }
        {
            let m = mesh.read().unwrap();
            run.nb_cohesive_at_dump
                .push(m.nb_element(case.cohesive_type, GhostType::NotGhost) as i64);
            run.nb_facets_at_dump.push(
                m.mesh_facets()
                    .unwrap()
                    .nb_element(facet_type, GhostType::NotGhost) as i64,
            );
        }
        after_dump(i, &mut run);
        if opt.stop_after == Some(i) {
            break;
        }
    }
    run
}

/// A fresh scratch directory.
pub fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("akantu-cohdump-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Every file below `dir`, relative (`/`-separated), sorted.
pub fn files_below(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, d: &Path, out: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(d).unwrap().flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::path);
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let rel = p.strip_prefix(base).unwrap();
                out.push(
                    rel.components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }
    let mut v = Vec::new();
    walk(dir, dir, &mut v);
    v.sort();
    v
}

/* ------------------------------------------------------------------ */
/* VTU reader                                                          */
/* ------------------------------------------------------------------ */

/// Values of a data array.
#[derive(Clone, Debug, PartialEq)]
pub enum Values {
    F64(Vec<f64>),
    I32(Vec<i32>),
    U32(Vec<u32>),
    U8(Vec<u8>),
}

impl Values {
    pub fn len(&self) -> usize {
        match self {
            Values::F64(v) => v.len(),
            Values::I32(v) => v.len(),
            Values::U32(v) => v.len(),
            Values::U8(v) => v.len(),
        }
    }

    pub fn f64(&self) -> &[f64] {
        match self {
            Values::F64(v) => v,
            other => panic!("not Float64: {other:?}"),
        }
    }

    pub fn i64(&self) -> Vec<i64> {
        match self {
            Values::F64(_) => panic!("not an integer array"),
            Values::I32(v) => v.iter().map(|&x| i64::from(x)).collect(),
            Values::U32(v) => v.iter().map(|&x| i64::from(x)).collect(),
            Values::U8(v) => v.iter().map(|&x| i64::from(x)).collect(),
        }
    }
}

/// One `<DataArray>`.
#[derive(Clone, Debug)]
pub struct DataArray {
    pub ty: String,
    pub nb_components: usize,
    pub values: Values,
}

/// A parsed `.vtu` piece.
#[derive(Clone, Debug)]
pub struct Vtu {
    pub nb_points: usize,
    pub nb_cells: usize,
    /// `positions`, `connectivity`, `offsets`, `types` and every field.
    pub arrays: BTreeMap<String, DataArray>,
    /// Field names of `<PointData>`.
    pub point_data: Vec<String>,
    /// Field names of `<CellData>`.
    pub cell_data: Vec<String>,
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=");
    let i = tag.find(&key)? + key.len();
    let rest = tag[i..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let j = rest.find('"')?;
    Some(&rest[..j])
}

/// Standard base64 decoding (the port's default layout: IOHelper's stream
/// `[u32 header = payload + 4][payload]`, UD-114 corrected).
pub fn base64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("invalid base64 character {c}"),
        }
    };
    let bytes: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    assert!(
        bytes.len().is_multiple_of(4),
        "base64 length {}",
        bytes.len()
    );
    let mut out = Vec::new();
    for q in bytes.as_chunks::<4>().0 {
        let pad = q.iter().filter(|&&c| c == b'=').count();
        let mut n = 0u32;
        for &c in q {
            n = (n << 6) | if c == b'=' { 0 } else { val(c) };
        }
        let b = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&b[..3 - pad]);
    }
    out
}

fn decode_values(ty: &str, format: &str, text: &str) -> Values {
    if format == "binary" {
        let raw = base64_decode(text);
        let header = u32::from_le_bytes(raw[..4].try_into().unwrap()) as usize;
        let payload = &raw[4..];
        assert_eq!(header, payload.len() + 4, "IOHelper header counts itself");
        match ty {
            "Float64" => Values::F64(
                payload
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|c| f64::from_le_bytes(*c))
                    .collect(),
            ),
            "Int32" => Values::I32(
                payload
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| i32::from_le_bytes(*c))
                    .collect(),
            ),
            "UInt32" => Values::U32(
                payload
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| u32::from_le_bytes(*c))
                    .collect(),
            ),
            "UInt8" => Values::U8(payload.to_vec()),
            other => panic!("data type {other}"),
        }
    } else {
        let tokens = text.split_ascii_whitespace();
        match ty {
            "Float64" => Values::F64(tokens.map(|t| t.parse().unwrap()).collect()),
            "Int32" => Values::I32(tokens.map(|t| t.parse().unwrap()).collect()),
            "UInt32" => Values::U32(tokens.map(|t| t.parse().unwrap()).collect()),
            "UInt8" => Values::U8(tokens.map(|t| t.parse().unwrap()).collect()),
            other => panic!("data type {other}"),
        }
    }
}

/// Parse a `.vtu` file written by the ParaView dumper.
pub fn parse_vtu(path: &Path) -> Vtu {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let piece = &text[text.find("<Piece").expect("<Piece")..];
    let piece_tag = &piece[..piece.find('>').unwrap()];
    let nb_points = attr(piece_tag, "NumberOfPoints").unwrap().parse().unwrap();
    let nb_cells = attr(piece_tag, "NumberOfCells").unwrap().parse().unwrap();
    let mut arrays = BTreeMap::new();
    let mut point_data = Vec::new();
    let mut cell_data = Vec::new();
    let mut section = "";
    let mut rest = piece;
    while let Some(i) = rest.find('<') {
        rest = &rest[i..];
        let end = rest.find('>').unwrap();
        let tag = &rest[..=end];
        if tag.starts_with("<PointData") {
            section = "point";
        } else if tag.starts_with("<CellData") {
            section = "cell";
        } else if tag.starts_with("</PointData") || tag.starts_with("</CellData") {
            section = "";
        } else if tag.starts_with("<DataArray") {
            let name = attr(tag, "Name").unwrap_or("positions").to_owned();
            let ty = attr(tag, "type").unwrap().to_owned();
            let nb_components = attr(tag, "NumberOfComponents").map_or(1, |c| c.parse().unwrap());
            let format = attr(tag, "format").unwrap().to_owned();
            let body_start = end + 1;
            let body_end = rest.find("</DataArray>").unwrap();
            let values = decode_values(&ty, &format, &rest[body_start..body_end]);
            match section {
                "point" => point_data.push(name.clone()),
                "cell" => cell_data.push(name.clone()),
                _ => {}
            }
            arrays.insert(
                name,
                DataArray {
                    ty,
                    nb_components,
                    values,
                },
            );
            rest = &rest[body_end + "</DataArray>".len()..];
            continue;
        }
        rest = &rest[end + 1..];
    }
    Vtu {
        nb_points,
        nb_cells,
        arrays,
        point_data,
        cell_data,
    }
}

/// `(timestep, file)` entries of a `.pvd`.
pub fn parse_pvd(path: &Path) -> Vec<(String, String)> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let mut out = Vec::new();
    for line in text.lines() {
        if let (Some(t), Some(f)) = (attr(line, "timestep"), attr(line, "file")) {
            out.push((t.to_owned(), f.to_owned()));
        }
    }
    out
}

/// Exact equality of two `f64` slices (bit patterns), with the first
/// difference in the message.
pub fn assert_bits(what: &str, got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "{what}: length");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(
            g.to_bits() == w.to_bits(),
            "{what}[{i}]: got {g:e} ({:#018x}), want {w:e} ({:#018x})",
            g.to_bits(),
            w.to_bits()
        );
    }
}
