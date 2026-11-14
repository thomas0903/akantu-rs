// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests
// (COH-013): independent checks of the cohesive and facet dumpers in the
// port's default layout. The probe scenarios (cohesive_dump_common) are
// replayed, the written ParaView files are read back (base64 decoded
// exactly, ASCII parsed) and compared with the model's own arrays: point
// and cell counts after every insertion, the cohesive connectivity in
// IOHelper's VTK node order, nodal fields (padding), the flattened and
// homogenised cohesive / facet internals, material indices, the .pvd time
// series and the registration rules (UD-754, UD-755). Upstream sources:
// src/model/solid_mechanics/solid_mechanics_model_cohesive/
// solid_mechanics_model_cohesive.cc (dumper, setTimeStep,
// addDumpGroupFieldToDumper, onDump), src/model/common/constitutive_laws/
// constitutive_law_tmpl.hh (flattenInternal),
// src/io/dumper/dumper_homogenizing_field.hh (AvgHomogenizingFunctor),
// third-party/iohelper/src/paraview_helper.cc (write_reorder,
// paraview_code_type).

mod cohesive_dump_common;
mod common;

use akantu_cohesive::material::{nb_quad_cohesive, nb_quad_facet};
use akantu_cohesive::{COHESIVE_DUMPER, FACETS_DUMPER, SolidMechanicsModelCohesive};
use akantu_core::{ElementType, GhostType, Real};
use akantu_solid::Material;
use cohesive_dump_common::{
    DumpCase, ReplayOptions, Run, Vtu, assert_bits, cases, files_below, parse_pvd, parse_vtu,
    replay, replay_with, scratch,
};

const NG: GhostType = GhostType::NotGhost;

fn case(id: &str) -> DumpCase {
    cases()
        .into_iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("no case {id}"))
}

/// IOHelper `write_reorder` and `paraview_code_type` of the cohesive types
/// (upstream third-party/iohelper/src/paraview_helper.cc).
fn vtk_order(ty: ElementType) -> (&'static [usize], u32) {
    match ty {
        ElementType::Cohesive2d4 => (&[0, 1, 3, 2], 7),
        ElementType::Cohesive2d6 => (&[0, 2, 1, 4, 5, 3], 7),
        ElementType::Cohesive3d6 => (&[0, 1, 2, 3, 4, 5], 13),
        other => panic!("{other:?}"),
    }
}

fn pad3(values: &[Real], nc: usize) -> Vec<Real> {
    if nc >= 3 {
        return values.to_vec();
    }
    let mut out = Vec::with_capacity(values.len() / nc * 3);
    for t in values.chunks(nc) {
        out.extend_from_slice(t);
        out.extend(std::iter::repeat_n(0.0, 3 - nc));
    }
    out
}

/// `AvgHomogenizingFunctor`: per element, the sequential sum of its `nq`
/// points divided by `nq`.
fn average(values: &[Real], nq: usize, nc: usize) -> Vec<Real> {
    let mut out = Vec::new();
    for e in values.chunks(nq * nc) {
        let mut r = vec![0.0; nc];
        for p in e.chunks(nc) {
            for (a, &v) in r.iter_mut().zip(p) {
                *a += v;
            }
        }
        for a in &mut r {
            *a /= nq as Real;
        }
        out.extend(r);
    }
    out
}

/// The cohesive internal `name` of the law `insertion`, flattened over the
/// mesh's cohesive elements of `ty` and homogenised (zeros for elements the
/// law does not hold).
fn expected_internal(
    model: &SolidMechanicsModelCohesive,
    name: &str,
    ty: ElementType,
) -> Vec<Real> {
    let mat = model.cohesive_material("insertion").unwrap();
    let field = mat.internal(name).unwrap();
    let nc = field.nb_component();
    let nq = nb_quad_cohesive(ty).unwrap();
    let ne = model
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nb_element(ty, NG);
    let mut flat = vec![0.0; ne * nq * nc];
    let filter = mat.base().element_filter.get(ty, NG).unwrap();
    let vals_g = field.get(ty, NG).unwrap();
    let vals = vals_g.as_slice();
    for (l, &e) in filter.as_slice().iter().enumerate() {
        let e = e as usize;
        flat[e * nq * nc..(e + 1) * nq * nc].copy_from_slice(&vals[l * nq * nc..(l + 1) * nq * nc]);
    }
    average(&flat, nq, nc)
}

fn vtu_of(dir: &std::path::Path, base: &str, step: usize) -> Vtu {
    parse_vtu(&dir.join(format!("{base}-VTU/{base}_{step:04}.vtu")))
}

/// Points, cells, connectivity, offsets, types of a cohesive dump.
fn check_cells(vtu: &Vtu, run: &Run, ty: ElementType) {
    let m = run.mesh.read().unwrap();
    assert_eq!(vtu.nb_points, m.nb_nodes(), "points = every node");
    assert_bits(
        "positions",
        vtu.arrays["positions"].values.f64(),
        &pad3(m.nodes().as_slice(), m.spatial_dimension()),
    );
    let conn = m.connectivity(ty, NG).unwrap();
    assert_eq!(vtu.nb_cells, conn.len(), "cells = cohesive elements");
    let (order, code) = vtk_order(ty);
    let mut want = Vec::new();
    let mut offsets = Vec::new();
    for e in 0..conn.len() {
        let t = conn.tuple(e);
        want.extend(order.iter().map(|&i| i64::from(t[i])));
        offsets.push(want.len() as i64);
    }
    assert_eq!(vtu.arrays["connectivity"].values.i64(), want);
    assert_eq!(vtu.arrays["offsets"].values.i64(), offsets);
    assert_eq!(
        vtu.arrays["types"].values.i64(),
        vec![i64::from(code); conn.len()]
    );
}

/// The intrinsic 2-D case: every field of the cohesive dumper at each dump.
#[test]
fn cohesive_dumper_intrinsic_2d_every_field() {
    let _g = common::lock();
    let c = case("intrinsic_tri3_bilinear_base64");
    let out = scratch("intrinsic2d");
    let ty = c.cohesive_type;
    let mut checked = 0;
    let run = replay_with(&c, &out, &ReplayOptions::default(), |i, run| {
        let vtu = vtu_of(&out, "cohesive", i);
        check_cells(&vtu, run, ty);
        assert_eq!(vtu.nb_cells, 5, "intrinsic: the 5 pre-inserted elements");
        let s = &run.model.smm;
        for (name, a, padded) in [
            ("displacement", s.displacement().unwrap(), true),
            ("velocity", s.velocity().unwrap(), true),
            ("acceleration", s.acceleration().unwrap(), true),
            ("internal_force", s.internal_force().unwrap(), true),
            ("external_force", s.external_force().unwrap(), false),
            ("mass", s.mass().unwrap(), false),
        ] {
            let d = &vtu.arrays[name];
            let want = if padded {
                pad3(a.as_slice(), 2)
            } else {
                a.as_slice().to_vec()
            };
            assert_eq!(d.nb_components, if padded { 3 } else { 2 }, "{name}");
            assert_bits(name, d.values.f64(), &want);
        }
        let blocked: Vec<i64> = s
            .blocked_dofs()
            .unwrap()
            .as_slice()
            .iter()
            .map(|&b| i64::from(b))
            .collect();
        assert_eq!(vtu.arrays["blocked_dofs"].values.i64(), blocked);
        for name in [
            "damage",
            "contact_opening",
            "contact_tractions",
            "delta max",
            "normal",
            "reversible_energy",
            "total_energy",
            "opening",
            "tractions",
            "sigma_c_eff",
            "delta_c_eff",
            "insertion_stress",
        ] {
            let d = &vtu.arrays[name];
            let want = expected_internal(&run.model, name, ty);
            assert_bits(name, d.values.f64(), &want);
            // elemental cohesive fields are not padded (upstream pads only
            // stress/strain), even with addDumpFieldVectorToDumper
            assert_eq!(d.nb_components * vtu.nb_cells, want.len(), "{name}");
        }
        let idx = run
            .model
            .smm
            .physics
            .materials
            .law_index_map()
            .get(ty, NG)
            .unwrap();
        assert_eq!(
            vtu.arrays["material_index"].values.i64(),
            idx.as_slice()
                .iter()
                .map(|&i| i64::from(i))
                .collect::<Vec<_>>()
        );
        assert_eq!(vtu.arrays["partitions"].values.i64(), vec![0; vtu.nb_cells]);
        checked += 1;
    });
    assert_eq!(checked, 4);
    // the opening grows under the loading: the dumps differ
    let d0 = vtu_of(&out, "cohesive", 0);
    let d3 = vtu_of(&out, "cohesive", 3);
    assert_ne!(
        d0.arrays["opening"].values.f64(),
        d3.arrays["opening"].values.f64()
    );
    // .pvd: the fixture's set_time_step reached the cohesive dumper
    let pvd = parse_pvd(&out.join("cohesive.pvd"));
    assert_eq!(pvd.len(), 4);
    for (k, (_, file)) in pvd.iter().enumerate() {
        assert_eq!(file, &format!("cohesive_{k:04}.pvtu"));
    }
    let times: Vec<f64> = pvd.iter().map(|(t, _)| t.parse().unwrap()).collect();
    assert!(times.windows(2).all(|w| w[0] < w[1]), "{times:?}");
    // `.pvd` times are printed with 6 significant digits (`%g`)
    assert_eq!(pvd[1].0, format_g(run.dt), "{pvd:?}");
    std::fs::remove_dir_all(&out).ok();
}

/// The extrinsic 2-D case: cells and points grow between dumps (0, 3, 5, 5
/// cohesive elements), files named after the model id, refused names.
#[test]
fn cohesive_dumper_follows_insertions_2d() {
    let _g = common::lock();
    let c = case("extrinsic_tri3_linear_base64");
    let out = scratch("extrinsic2d");
    let ty = c.cohesive_type;
    let base = "solid_mechanics_model_cohesive";
    let mut nodes_at_dump = Vec::new();
    let run = replay_with(&c, &out, &ReplayOptions::default(), |i, run| {
        let vtu = vtu_of(&out, base, i);
        check_cells(&vtu, run, ty);
        nodes_at_dump.push(vtu.nb_points);
        for name in [
            "damage",
            "opening",
            "tractions",
            "sigma_c_eff",
            "delta_c_eff",
            "insertion_stress",
        ] {
            assert_bits(
                name,
                vtu.arrays[name].values.f64(),
                &expected_internal(&run.model, name, ty),
            );
        }
        let v = run.model.smm.velocity().unwrap();
        assert_eq!(vtu.arrays["velocity"].nb_components, 2, "unpadded");
        assert_bits(
            "velocity",
            vtu.arrays["velocity"].values.f64(),
            v.as_slice(),
        );
    });
    assert_eq!(run.nb_cohesive_at_dump, vec![0, 3, 5, 5]);
    assert!(
        nodes_at_dump.windows(2).all(|w| w[0] <= w[1]) && nodes_at_dump[0] < nodes_at_dump[3],
        "doubled nodes appear in the dumps: {nodes_at_dump:?}"
    );
    // default base name = model id (UD-753: also the default dumper's)
    let files = files_below(&out);
    assert!(files.contains(&format!("{base}.pvd")), "{files:?}");
    assert!(files.contains(&format!("{base}_0003.pvtu")), "{files:?}");
    std::fs::remove_dir_all(&out).ok();
}

/// C `%g` (6 significant digits, shortest of `%e`/`%f`, trailing zeros
/// removed) for the positive times of the tests.
fn format_g(x: f64) -> String {
    let e = format!("{x:.5e}");
    let (m, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let m = m.trim_end_matches('0').trim_end_matches('.');
    if (-4..6).contains(&exp) {
        let decimals = usize::try_from(5 - exp).unwrap();
        let f = format!("{x:.decimals$}");
        if f.contains('.') {
            f.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            f
        }
    } else {
        format!("{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    }
}

fn ascii_value(x: f64) -> f64 {
    // IOHelper writes `%22.15e`: the correctly rounded 16-digit decimal
    format!("{x:.15e}").parse().unwrap()
}

/// The ASCII cases (2-D quadratic, 3-D): cells, VTK orders, rounded values.
#[test]
fn cohesive_dumper_text_mode_quadratic_and_3d() {
    let _g = common::lock();
    for id in ["extrinsic_tri6_linear_text", "intrinsic_tet4_bilinear_text"] {
        let c = case(id);
        let out = scratch(id);
        let ty = c.cohesive_type;
        let dim = ty.spatial_dimension();
        let run = replay_with(&c, &out, &ReplayOptions::default(), |i, run| {
            let vtu = vtu_of(&out, "cohesive", i);
            check_cells(&vtu, run, ty);
            let u = run.model.smm.displacement().unwrap();
            let want: Vec<f64> = pad3(u.as_slice(), dim)
                .into_iter()
                .map(ascii_value)
                .collect();
            assert_bits(
                "displacement",
                vtu.arrays["displacement"].values.f64(),
                &want,
            );
            let want: Vec<f64> = expected_internal(&run.model, "opening", ty)
                .into_iter()
                .map(ascii_value)
                .collect();
            assert_bits("opening", vtu.arrays["opening"].values.f64(), &want);
            assert_eq!(vtu.arrays["opening"].nb_components, dim);
        });
        if id.starts_with("extrinsic") {
            assert_eq!(run.nb_cohesive_at_dump[0], 0);
            assert!(run.nb_cohesive_at_dump[3] > run.nb_cohesive_at_dump[0]);
        } else {
            assert!(run.nb_cohesive_at_dump.iter().all(|&n| n > 0));
        }
        std::fs::remove_dir_all(&out).ok();
    }
}

/// The facets dumper: the facet mesh's cells, the facet internal sigma_c
/// (law values on the facets of its facet filter, zeros elsewhere), before
/// and after insertions (facets appended to the facet mesh).
#[test]
fn facets_dumper_cells_and_sigma_c() {
    let _g = common::lock();
    let mut c = case("facets_tri3_extrinsic");
    c.dumps = vec![0, 10, 26, 30]; // the probe's two dumps, then after insertions
    let out = scratch("facets");
    let ft = ElementType::Segment2;
    let run = replay_with(&c, &out, &ReplayOptions::default(), |i, run| {
        let vtu = vtu_of(&out, "facets", i);
        let m = run.mesh.read().unwrap();
        let fm = m.mesh_facets().unwrap();
        assert_eq!(fm.nb_nodes(), m.nb_nodes(), "facet mesh shares the nodes");
        assert_eq!(vtu.nb_points, fm.nb_nodes());
        let conn = fm.connectivity(ft, GhostType::NotGhost).unwrap();
        assert_eq!(vtu.nb_cells, conn.len());
        assert_eq!(
            vtu.arrays["connectivity"].values.i64(),
            conn.as_slice()
                .iter()
                .map(|&n| i64::from(n))
                .collect::<Vec<_>>()
        );
        assert_eq!(vtu.arrays["types"].values.i64(), vec![3; conn.len()]); // VTK_LINE
        // sigma_c: the law's facet values, averaged per facet; zeros outside
        // its facet filter (facets appended by the insertion)
        let mat = run.model.cohesive_material("insertion").unwrap();
        let nq = nb_quad_facet(ft).unwrap();
        let filter = mat.core.facet_filter.get(ft, NG).unwrap();
        let vals_g = mat.array("sigma_c", ft, NG).unwrap();
        let vals = vals_g.as_slice();
        let mut flat = vec![0.0; conn.len() * nq];
        for (l, &f) in filter.as_slice().iter().enumerate() {
            let f = f as usize;
            flat[f * nq..(f + 1) * nq].copy_from_slice(&vals[l * nq..(l + 1) * nq]);
        }
        let want = average(&flat, nq, 1);
        assert_bits("sigma_c", vtu.arrays["sigma_c"].values.f64(), &want);
        if i >= 2 {
            assert!(
                conn.len() > filter.len(),
                "insertion appended facets outside the facet filter"
            );
            assert!(want[filter.len()..].iter().all(|&x| x == 0.0));
        }
        assert_bits(
            "displacement",
            vtu.arrays["displacement"].values.f64(),
            &pad3(run.model.smm.displacement().unwrap().as_slice(), 2),
        );
    });
    assert!(run.nb_facets_at_dump[2] > run.nb_facets_at_dump[1]);
    // its own time step (set_time_step_to_dumper): a time series
    assert_eq!(parse_pvd(&out.join("facets.pvd")).len(), 4);
    std::fs::remove_dir_all(&out).ok();
}

/// UD-754 / UD-755: names refused on each dumper; delegation of the other
/// dumpers to the solid model.
#[test]
fn registration_rules() {
    let _g = common::lock();
    let c = case("extrinsic_tri3_linear_base64");
    let out = scratch("rules");
    let opt = ReplayOptions {
        stop_after: Some(0),
        ..ReplayOptions::default()
    };
    let mut run = replay(&c, &out, &opt);
    let m = &mut run.model;
    m.register_facets_dumper(None).unwrap();
    // the refusals point to the field to use instead
    let e = m
        .add_dump_field_to_dumper(COHESIVE_DUMPER, "sigma_c")
        .unwrap_err();
    assert!(e.to_string().contains("sigma_c_eff"), "{e}");
    let e = m
        .add_dump_field_to_dumper(COHESIVE_DUMPER, "delta_c")
        .unwrap_err();
    assert!(e.to_string().contains("delta_c_eff"), "{e}");
    for (dumper, name, ud) in [
        (COHESIVE_DUMPER, "sigma_c", "UD-754"),
        (COHESIVE_DUMPER, "delta_c", "UD-754"),
        (COHESIVE_DUMPER, "stress", "UD-754"),
        (COHESIVE_DUMPER, "grad_u", "UD-754"),
        (FACETS_DUMPER, "damage", "UD-754"),
        (FACETS_DUMPER, "opening", "UD-754"),
        (FACETS_DUMPER, "stress", "UD-754"),
        (FACETS_DUMPER, "material_index", "UD-755"),
        (FACETS_DUMPER, "partitions", "UD-755"),
    ] {
        let e = m.add_dump_field_to_dumper(dumper, name).unwrap_err();
        assert!(e.to_string().contains(ud), "{dumper} {name}: {e}");
    }
    // derived quantities need grad_u/stress of the dumper's kind
    assert!(
        m.add_dump_field_to_dumper(COHESIVE_DUMPER, "strain")
            .is_err()
    );
    assert!(
        m.add_dump_field_to_dumper(COHESIVE_DUMPER, "Von Mises stress")
            .is_err()
    );
    // accepted
    m.add_dump_field_to_dumper(FACETS_DUMPER, "sigma_c")
        .unwrap();
    m.add_dump_field_vector_to_dumper(FACETS_DUMPER, "displacement")
        .unwrap();
    m.add_dump_field_to_dumper(COHESIVE_DUMPER, "partitions")
        .unwrap();
    m.add_dump_field_tensor_to_dumper(COHESIVE_DUMPER, "normal")
        .unwrap();
    // duplicate registration
    assert!(
        m.add_dump_field_to_dumper(COHESIVE_DUMPER, "damage")
            .is_err()
    );
    // other dumpers are the solid model's (regular elements)
    m.add_dump_field("stress").unwrap();
    m.add_dump_field_vector("displacement").unwrap();
    m.set_base_name_to_dumper("solid_mechanics_model", "bulk")
        .unwrap();
    m.set_directory_to_dumper("solid_mechanics_model", &out)
        .unwrap();
    m.dump(None, None).unwrap();
    m.dump_named("solid_mechanics_model", None, None).unwrap();
    assert!(out.join("bulk_0001.pvtu").exists());
    // unknown dumper
    assert!(m.dump_named("nope", None, None).is_err());
    assert!(m.add_dump_field_to_dumper("nope", "damage").is_err());
    std::fs::remove_dir_all(&out).ok();
}

/// A facet internal of an intrinsic model has no facet filter: refused at
/// dump time (UD-755; upstream writes an empty data array, oracle case
/// evidence_facets_tri3_intrinsic).
#[test]
fn facets_dumper_refuses_sigma_c_of_an_intrinsic_model() {
    let _g = common::lock();
    let c = case("evidence_facets_tri3_intrinsic");
    let out = scratch("facets-intrinsic");
    let opt = ReplayOptions {
        allow_dump_error: true,
        ..ReplayOptions::default()
    };
    let run = replay(&c, &out, &opt);
    assert_eq!(run.dump_errors.len(), 1);
    assert!(
        run.dump_errors[0].contains("UD-755"),
        "{:?}",
        run.dump_errors
    );
    // nothing written by the failed dump
    assert!(files_below(&out).is_empty(), "{:?}", files_below(&out));
    std::fs::remove_dir_all(&out).ok();
}

/// `set_time_step` sets the cohesive dumper's time step (upstream
/// `SolidMechanicsModelCohesive::setTimeStep`), also when called again.
#[test]
fn set_time_step_reaches_the_cohesive_dumper() {
    let _g = common::lock();
    let c = case("extrinsic_tri3_linear_base64");
    let out = scratch("timestep");
    let opt = ReplayOptions {
        stop_after: Some(0),
        ..ReplayOptions::default()
    };
    let mut run = replay(&c, &out, &opt);
    let dt = run.dt;
    // restarts the clock: the next dump is at t = 0 again, then 2 dt
    run.model.set_time_step(2.0 * dt, None).unwrap();
    run.model.dump_named(COHESIVE_DUMPER, None, None).unwrap();
    run.model.dump_named(COHESIVE_DUMPER, None, None).unwrap();
    let pvd = parse_pvd(&out.join("solid_mechanics_model_cohesive.pvd"));
    let times: Vec<f64> = pvd.iter().map(|(t, _)| t.parse().unwrap()).collect();
    assert_eq!(times.len(), 3, "{pvd:?}");
    assert_eq!(times[1], 0.0, "{pvd:?}");
    assert_eq!(pvd[2].0, format_g(2.0 * dt), "{pvd:?}");
    std::fs::remove_dir_all(&out).ok();
}

/// The cohesive model's dumpers dumped through the solid
/// model (`model.smm.dump_named`) write the same files as through the
/// cohesive model — the facet mesh with the facet internal `sigma_c` (not
/// the main mesh's `d − 1` boundary segments), and the cohesive elements —
/// as upstream, whose `Model::dump(name)` is `mesh.dump(name)` for a
/// `SolidMechanicsModel&` too. Before the route the solid model wrote the
/// 23 boundary segments of this mesh under the facets dumper's name (75
/// facets), silently.
#[test]
fn solid_model_dump_named_routes_the_cohesive_dumpers() {
    let _g = common::lock();
    let c = case("facets_tri3_extrinsic");
    let out = scratch("route");
    let mut run = replay(&c, &out, &ReplayOptions::default());
    let (nb_facets, nb_main_segments) = {
        let m = run.mesh.read().unwrap();
        let seg = ElementType::Segment2;
        (
            m.mesh_facets()
                .unwrap()
                .nb_element(seg, GhostType::NotGhost),
            m.nb_element(seg, GhostType::NotGhost),
        )
    };
    assert_ne!(nb_facets, nb_main_segments);
    for dumper in [FACETS_DUMPER, COHESIVE_DUMPER] {
        let mut vtus = Vec::new();
        for via_smm in [false, true] {
            let dir = out.join(format!("{}-{via_smm}", dumper.replace(' ', "_")));
            std::fs::create_dir_all(&dir).unwrap();
            run.model.set_directory_to_dumper(dumper, &dir).unwrap();
            if via_smm {
                run.model.smm.dump_named(dumper, None, None).unwrap();
            } else {
                run.model.dump_named(dumper, None, None).unwrap();
            }
            let files: Vec<String> = files_below(&dir)
                .into_iter()
                .filter(|f| f.ends_with(".vtu"))
                .collect();
            assert_eq!(files.len(), 1, "{dumper}: {files:?}");
            if dumper == FACETS_DUMPER {
                assert_eq!(
                    parse_vtu(&dir.join(&files[0])).nb_cells,
                    nb_facets,
                    "{dumper} via smm = {via_smm}: not the facet mesh"
                );
            }
            vtus.push(std::fs::read(dir.join(&files[0])).unwrap());
        }
        assert!(
            vtus[0] == vtus[1],
            "{dumper}: model.smm.dump_named differs from the cohesive model's dump_named"
        );
    }
    // a dumper of the solid model itself is still dumped by its resolver
    let dir = out.join("solid");
    std::fs::create_dir_all(&dir).unwrap();
    run.model
        .smm
        .set_directory_to_dumper("solid_mechanics_model", &dir)
        .unwrap();
    run.model
        .smm
        .dump_named("solid_mechanics_model", None, None)
        .unwrap();
    assert!(
        files_below(&dir).iter().any(|f| f.ends_with(".vtu")),
        "solid model dump"
    );
    std::fs::remove_dir_all(&out).ok();
}
