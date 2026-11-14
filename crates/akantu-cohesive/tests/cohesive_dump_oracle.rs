// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests
// (COH-013): differential tests of the cohesive and facet dumpers against
// the oracle cases fixtures/model/cohesive_dump/* (oracle/probes/
// cohesive_dump/cohesive_dump_probe.cc, upstream TestSMMCFixture mode I with
// dumps): the scenario is replayed with the same dumper calls in upstream's
// byte layout (`DumperParaview::set_compat_layout`, UD-110/113/114) and
// every written file (.pvtu, .vtu, .pvd) is compared byte for byte with the
// oracle's; the evidence case checks the port's refusal against upstream's
// defective output (UD-755). `COH_DUMP_FIXTURE_ROOT` replays a staged
// fixture tree (no golden/checksum requirement).

mod cohesive_dump_common;
mod common;

use std::path::{Path, PathBuf};

use akantu_validation::{Fixture, fixtures_root};
use cohesive_dump_common::{
    DumpCase, ReplayOptions, cases, files_below, parse_vtu, replay, scratch,
};

fn root() -> (PathBuf, bool) {
    match std::env::var("COH_DUMP_FIXTURE_ROOT") {
        Ok(d) if !d.is_empty() => (PathBuf::from(d), true),
        _ => (fixtures_root(), false),
    }
}

fn open(case: &str) -> Fixture {
    let (r, staged) = root();
    let dir = r.join("model/cohesive_dump").join(case);
    let f = Fixture::open(&dir).unwrap_or_else(|e| panic!("{case}: {e}"));
    if !staged {
        f.require_golden().unwrap();
        f.verify_checksums().unwrap();
    }
    f
}

fn param<'a>(f: &'a Fixture, key: &str) -> &'a serde_json::Value {
    &f.manifest().parameters[key]
}

fn assert_same_file(got: &Path, want: &Path) {
    let g = std::fs::read(got).unwrap_or_else(|e| panic!("{got:?}: {e}"));
    let w = std::fs::read(want).unwrap_or_else(|e| panic!("{want:?}: {e}"));
    if g != w {
        let i = g
            .iter()
            .zip(&w)
            .position(|(a, b)| a != b)
            .unwrap_or(g.len().min(w.len()));
        let ctx = |b: &[u8]| {
            String::from_utf8_lossy(&b[i.saturating_sub(120)..(i + 120).min(b.len())]).into_owned()
        };
        panic!(
            "{got:?} differs from {want:?} at byte {i} (sizes {} / {}):\n--- port\n{}\n--- oracle\n{}",
            g.len(),
            w.len(),
            ctx(&g),
            ctx(&w)
        );
    }
}

/// `NumberOfPoints` / `NumberOfCells` of a `.vtu` (without decoding the
/// arrays: upstream's empty base64 arrays are not valid base64, UD-114).
fn piece_counts(path: &Path) -> (usize, usize) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let num = |key: &str| -> usize {
        let i = text.find(key).unwrap_or_else(|| panic!("{path:?}: {key}")) + key.len();
        let rest = text[i..].trim_start().trim_start_matches('"');
        rest[..rest.find('"').unwrap()].parse().unwrap()
    };
    (num("NumberOfPoints="), num("NumberOfCells="))
}

/// The probe's case definition must be the one replayed.
fn check_definition(f: &Fixture, c: &DumpCase) {
    let id = c.id;
    assert_eq!(param(f, "dumper").as_str(), Some(c.dumper), "{id}");
    assert_eq!(param(f, "base_name").as_str(), Some(c.base_name), "{id}");
    assert_eq!(param(f, "text_mode").as_bool(), Some(c.text), "{id}");
    assert_eq!(param(f, "extrinsic").as_bool(), Some(c.extrinsic), "{id}");
    assert_eq!(
        param(f, "cohesive_type").as_str(),
        Some(c.cohesive_type.name()),
        "{id}"
    );
    let dumps: Vec<usize> = param(f, "dump_after_steps")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect();
    assert_eq!(dumps, c.dumps, "{id}");
    let fields: Vec<(String, bool)> = param(f, "fields")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["name"].as_str().unwrap().to_owned(),
                v["vector"].as_bool().unwrap(),
            )
        })
        .collect();
    let ours: Vec<(String, bool)> = c
        .fields
        .iter()
        .map(|r| (r.name.to_owned(), r.vector))
        .collect();
    assert_eq!(fields, ours, "{id}");
    assert_eq!(
        param(f, "comparison").as_str(),
        Some(if c.golden { "bytes" } else { "evidence" }),
        "{id}"
    );
}

fn run_case(id: &str) {
    let _g = common::lock();
    let c = cases()
        .into_iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("no case {id}"));
    let f = open(id);
    check_definition(&f, &c);
    let out = scratch(&format!("oracle-{id}"));
    let opt = ReplayOptions {
        compat_layout: true,
        upstream_tangents: true,
        material_file: Some(f.input_path("material").unwrap()),
        mesh_file: Some(f.input_path("mesh").unwrap()),
        allow_dump_error: !c.golden,
        ..ReplayOptions::default()
    };
    let run = replay(&c, &out, &opt);
    assert_eq!(
        run.nb_cohesive_at_dump,
        f.load_i64("nb_cohesive_at_dump").unwrap().data,
        "{id}: cohesive elements at each dump"
    );
    assert_eq!(
        run.nb_facets_at_dump,
        f.load_i64("nb_facets_at_dump").unwrap().data,
        "{id}: facets at each dump"
    );
    // names registered after the fields (none in the current cases)
    let ignored = param(&f, "ignored_fields").as_array().unwrap();
    assert_eq!(ignored.len(), c.ignored.len(), "{id}");
    for (u, n) in ignored.iter().zip(&c.ignored) {
        assert_eq!(u.as_str(), Some(*n), "{id}: upstream raised for {n}: {u}");
    }
    assert_eq!(run.ignored_errors.len(), c.ignored.len(), "{id}");
    let want: Vec<String> = param(&f, "dump_files")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    if c.golden {
        assert!(run.dump_errors.is_empty(), "{id}: {:?}", run.dump_errors);
        assert_eq!(files_below(&out), want, "{id}: written files");
        for rel in &want {
            let w = f.input_path(&format!("dump:{rel}")).unwrap();
            assert_same_file(&out.join(rel), &w);
        }
        for n in &c.ignored {
            for rel in want.iter().filter(|r| r.ends_with(".vtu")) {
                let v = parse_vtu(&f.input_path(&format!("dump:{rel}")).unwrap());
                assert!(!v.arrays.contains_key(*n), "{id}: upstream wrote '{n}'");
            }
        }
    } else if id == "evidence_material_index_before_insertion" {
        // evidence (UD-756): upstream's dump before the first insertion
        // raises (no cohesive block in the law-index map) without advancing
        // the dump counter, so the dump after the insertion is written as
        // step 0 (one .vtu, a one-entry .pvd); the port writes both dumps
        // (zero cells and an empty material_index array, then the law
        // indices) and its second .vtu is upstream's file byte for byte
        let errors = param(&f, "dump_errors").as_array().unwrap();
        assert_eq!(errors.len(), 2, "{id}");
        assert!(
            !errors[0].as_str().unwrap().is_empty(),
            "{id}: upstream dump 0"
        );
        assert_eq!(errors[1].as_str(), Some(""), "{id}: upstream dump 1");
        assert!(run.dump_errors.is_empty(), "{id}: {:?}", run.dump_errors);
        assert_eq!(
            want,
            [
                "material_index-VTU/material_index_0000.vtu",
                "material_index.pvd",
                "material_index_0000.pvtu"
            ],
            "{id}"
        );
        let vtu = |k: usize| out.join(format!("material_index-VTU/material_index_{k:04}.vtu"));
        assert_eq!(
            piece_counts(&vtu(0)).1,
            0,
            "{id}: the port's dump before the insertion"
        );
        assert_eq!(piece_counts(&vtu(1)).1, 3, "{id}");
        assert_same_file(
            &vtu(1),
            &f.input_path("dump:material_index-VTU/material_index_0000.vtu")
                .unwrap(),
        );
    } else {
        // evidence (UD-755): with no facet filter (intrinsic model)
        // upstream's dump raises in the middle of the facet .vtu (a truncated
        // file stays on disk); the port refuses the dump before writing
        let errors = param(&f, "dump_errors").as_array().unwrap();
        assert_eq!(errors.len(), 1, "{id}");
        assert!(
            errors[0].as_str().unwrap().contains("sigma_c"),
            "{id}: {errors:?}"
        );
        let vtu_rel = want.iter().find(|r| r.ends_with(".vtu")).unwrap();
        let text =
            std::fs::read_to_string(f.input_path(&format!("dump:{vtu_rel}")).unwrap()).unwrap();
        assert!(text.contains(r#"Name="sigma_c""#), "{id}");
        assert!(
            !text.trim_end().ends_with("</VTKFile>"),
            "{id}: upstream file truncated"
        );
        assert_eq!(run.dump_errors.len(), 1, "{id}");
        assert!(
            run.dump_errors[0].contains("UD-755"),
            "{:?}",
            run.dump_errors
        );
        assert!(files_below(&out).is_empty(), "{id}: nothing written");
    }
    std::fs::remove_dir_all(&out).ok();
}

/// Every probe case has a fixture and every fixture is a probe case.
#[test]
fn every_case_has_a_fixture() {
    let (r, _) = root();
    let mut on_disk: Vec<String> = std::fs::read_dir(r.join("model/cohesive_dump"))
        .unwrap()
        .flatten()
        .filter(|e| e.path().join("manifest.json").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut ours: Vec<String> = cases().iter().map(|c| c.id.to_owned()).collect();
    ours.push("evidence_cohesive_unknown_fields".to_owned());
    ours.sort();
    assert_eq!(on_disk, ours);
}

macro_rules! oracle_cases {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            run_case(stringify!($name));
        }
    )*};
}

oracle_cases!(
    intrinsic_tri3_bilinear_base64,
    extrinsic_tri3_linear_base64,
    extrinsic_tri3_linear_text,
    extrinsic_tet4_linear_base64,
    intrinsic_tet4_bilinear_text,
    extrinsic_tri6_linear_text,
    facets_tri3_extrinsic,
    evidence_material_index_before_insertion,
    evidence_facets_tri3_intrinsic,
);

/// UD-754 evidence: upstream registers a field for a name that is not a
/// field of the cohesive-element dumper (falling through to
/// `createFieldFromAttachedData`, which creates an empty mesh data array of
/// that name) and the next dump raises; the port refuses the name at
/// registration and leaves the mesh untouched.
#[test]
fn evidence_cohesive_unknown_fields() {
    let _g = common::lock();
    let f = open("evidence_cohesive_unknown_fields");
    assert_eq!(param(&f, "comparison").as_str(), Some("evidence"));
    let results = param(&f, "results").as_array().unwrap();
    assert_eq!(results.len(), 5);
    let mut c = cases()
        .into_iter()
        .find(|c| c.id == "extrinsic_tri3_linear_base64")
        .unwrap();
    c.dumps = vec![0];
    let out = scratch("oracle-unknown-fields");
    let opt = ReplayOptions {
        material_file: Some(f.input_path("material").unwrap()),
        mesh_file: Some(f.input_path("mesh").unwrap()),
        ..ReplayOptions::default()
    };
    let mut run = replay(&c, &out, &opt);
    for r in results {
        let name = r["name"].as_str().unwrap();
        assert_eq!(
            r["registered"].as_bool(),
            Some(true),
            "upstream registers {name}"
        );
        assert_eq!(r["registration_error"].as_str(), Some(""), "{name}");
        assert!(
            !r["dump_error"].as_str().unwrap().is_empty(),
            "upstream's dump raises after registering {name}"
        );
        assert_eq!(r["mesh_data_created"].as_bool(), Some(true), "{name}");
        let e = run
            .model
            .add_dump_field_to_dumper(akantu_cohesive::COHESIVE_DUMPER, name)
            .unwrap_err();
        // strain: the derived field needs grad_u of the dumper's kind
        assert!(
            e.to_string().contains("UD-754") || name == "strain",
            "{name}: {e}"
        );
        assert!(
            run.mesh.read().unwrap().data().elemental(name).is_err(),
            "{name}: the port creates no mesh data"
        );
    }
    std::fs::remove_dir_all(&out).ok();
}
