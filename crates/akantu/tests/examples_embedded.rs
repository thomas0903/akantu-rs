// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/embedded (port of
// examples/c++/embedded_model/embedded.cc; upstream builds it only with the
// package `embedded`, which does not compile at the reference commit).
// Feature `cgal`. On the patched embedded variant of D-060 the upstream
// example crashes in `initFull` on its second-order mesh (UD-851, root cause
// UD-1140); the oracle comparison uses the first-order version of the mesh,
// on which upstream is memory-safe.

#[path = "../examples/embedded/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod embedded;
mod examples_common;

use akantu::core::{ElementType, GhostType, TypeFilter};
use akantu::model::Model;
use examples_common::{files_below, scratch};

/// Independent checks: the bar is cut into segments by the concrete
/// triangles and stays in tension below its pre-stress; the applied
/// traction `(0, −1)` on the top edge (resultant `(0, −10)`) is balanced
/// by the reactions of the blocked DOFs in force and moment, and the free
/// DOFs are in equilibrium (the tolerances of the same set-up's crate test
/// `akantu-embedded/tests/example_embedded_model.rs`, justified in
/// docs/NUMERICAL_VALIDATION.md: one direct solve of a linear
/// problem); two dumps of each dumper.
#[test]
fn embedded_reinforcement_in_tension() {
    let out = scratch("embedded");
    let mut model = embedded::run(&embedded::example().for_test(&out)).unwrap();
    // global equilibrium: reactions (−(f_ext + f_int) on blocked dofs)
    // balance the applied traction in force and moment
    model.assemble_internal_forces().unwrap();
    let nodes = model.smm.mesh().read().unwrap().nodes().as_slice().to_vec();
    let f_int = model.smm.internal_force().unwrap().as_slice().to_vec();
    let f_ext = model.smm.external_force().unwrap().as_slice().to_vec();
    let blocked = model.smm.blocked_dofs().unwrap().as_slice().to_vec();
    let (mut fx, mut fy, mut m) = (0.0, 0.0, 0.0);
    let mut free_residual: f64 = 0.0;
    for (n, x) in nodes.as_chunks::<2>().0.iter().enumerate() {
        let r = [
            f_ext[2 * n] + f_int[2 * n],
            f_ext[2 * n + 1] + f_int[2 * n + 1],
        ];
        for i in 0..2 {
            if !blocked[2 * n + i] {
                free_residual = free_residual.max(r[i].abs());
            }
        }
        let (ex, ey) = (f_ext[2 * n], f_ext[2 * n + 1]);
        fx += ex;
        fy += ey;
        m += x[0] * ey - x[1] * ex;
        if blocked[2 * n] {
            fx -= r[0];
            m -= -x[1] * r[0];
        }
        if blocked[2 * n + 1] {
            fy -= r[1];
            m -= x[0] * r[1];
        }
    }
    let load: f64 = f_ext.iter().skip(1).step_by(2).sum();
    assert!((load + 10.0).abs() <= 1e-12 * 10.0, "applied load {load}");
    assert!(free_residual <= 1e-6, "free residual {free_residual}");
    assert!(
        fx.abs() <= 1e-6 && fy.abs() <= 1e-6 && m.abs() <= 1e-5,
        "{fx} {fy} {m}"
    );
    let bar = model.smm.material_by_name("reinforcement").unwrap();
    let s = bar
        .base()
        .stress
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap();
    assert!(s.len() > 1, "the bar crosses several triangles");
    for &x in s.as_slice() {
        assert!(x > 0.0 && x < 10e6, "bar stress {x}");
    }
    assert_eq!(
        files_below(&out),
        [
            "paraview/concrete-VTU/concrete_0000.vtu",
            "paraview/concrete_0000.pvtu",
            "paraview/reinforcement-VTU/reinforcement_0000.vtu",
            "paraview/reinforcement_0000.pvtu",
        ]
    );
}

/// EX-008 / UD-1140: the oracle case `embedded_eigen_p1`
/// (the Eigen-pinned upstream example on the first-order version of its
/// concrete mesh, `oracle/probes/examples/embedded_p1/concrete.msh`: corner
/// nodes of the frozen second-order mesh) against the port's example on the
/// same meshes with `--upstream-compat` (upstream's interface pieces, the
/// UD-110 layout of the padded stress, the reinforcement dump without the
/// unknown field) and the Eigen solver.
///
/// Goldens only where upstream is memory-safe (the rule):
/// the oracle variant computed UD-1140's precondition (no regular 1-D
/// element type other than `_segment_2` in the background mesh) and
/// recorded it in its standard output; the port recomputes it on its own
/// mesh and compares.
///
/// Comparison (docs/NUMERICAL_VALIDATION.md): the file set and
/// both `.pvtu` files byte for byte; the interface pieces of the
/// reinforcement dump as a **set** (each segment's two end points bitwise,
/// orientation free: the LGPL search structure does not reproduce the CGAL
/// AABB tree's order of upstream's pieces, UD-408); concrete points, cells
/// and external force bitwise; the solved displacement, internal force and
/// stress under the rule `emb_order` (PROPOSED, the same pieces
/// assembled in another order, `1e-12` of the field's maximum); the four
/// out-of-plane shear entries of each padded stress tensor are skipped
/// (upstream leaves them uninitialised, UD-111; memcheck in UD-1140), the
/// padding zeros of the UD-110 layout compared exactly.
#[test]
fn embedded_eigen_p1_matches_the_oracle() {
    use examples_common::read_vtu;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = scratch("embedded_eigen_p1");
    let meshes = out.join("meshes");
    std::fs::create_dir_all(&meshes).unwrap();
    std::fs::copy(
        root.join("oracle/probes/examples/embedded_p1/concrete.msh"),
        meshes.join("concrete.msh"),
    )
    .unwrap();
    std::fs::copy(
        examples_common::frozen_example_mesh("embedded_model/reinforcement.msh"),
        meshes.join("reinforcement.msh"),
    )
    .unwrap();
    let run_dir = out.join("run");
    let mut ex = embedded::example().for_test(&run_dir);
    ex.mesh_dir = meshes;
    ex.upstream_compat = true;
    ex.sparse_solver = Some(akantu::core::enums::SparseSolverType::Eigen);
    let model = embedded::run(&ex).unwrap();
    // UD-1140's precondition on the port's background mesh
    let other_1d: Vec<ElementType> = {
        let m = model.smm.mesh().read().unwrap();
        [GhostType::NotGhost, GhostType::Ghost]
            .into_iter()
            .flat_map(|ghost_type| {
                m.element_types(TypeFilter {
                    spatial_dimension: Some(1),
                    ghost_type,
                    kind: Some(akantu::core::ElementKind::Regular),
                })
                .filter(|t| *t != ElementType::Segment2)
                .collect::<Vec<_>>()
            })
            .collect()
    };
    assert!(other_1d.is_empty(), "{other_1d:?}");
    let o = examples_common::oracle::oracle("embedded_eigen_p1");
    let line = "akantu-rs UD-851 precondition: regular 1-D element types of the background mesh \
                other than _segment_2: none: memory-safe";
    assert!(o.stdout().lines().any(|l| l == line), "{}", o.stdout());
    // the file set, the .pvtu files byte for byte
    let files = files_below(&run_dir);
    assert_eq!(files, o.digests.keys().cloned().collect::<Vec<_>>());
    for f in files.iter().filter(|f| f.ends_with(".pvtu")) {
        assert_eq!(
            std::fs::read(run_dir.join(f)).unwrap(),
            std::fs::read(o.kept(f)).unwrap(),
            "{f}"
        );
    }
    // reinforcement: the same pieces (set of segments, bitwise end points)
    let rel = "paraview/reinforcement-VTU/reinforcement_0000.vtu";
    let (mine, theirs) = (read_vtu(&run_dir.join(rel)), read_vtu(&o.kept(rel)));
    for name in ["connectivity", "offsets", "types"] {
        assert_eq!(
            mine.array(name).values,
            theirs.array(name).values,
            "{rel}: {name}"
        );
    }
    let pieces = |v: &examples_common::Vtu| -> Vec<[u64; 6]> {
        let x = v.array("positions").values.reals();
        let c = v.array("connectivity").values.ints();
        let mut out: Vec<[u64; 6]> = c
            .as_chunks::<2>()
            .0
            .iter()
            .map(|s| {
                let p = |n: i64| -> [u64; 3] {
                    let n = usize::try_from(n).unwrap();
                    [
                        x[3 * n].to_bits(),
                        x[3 * n + 1].to_bits(),
                        x[3 * n + 2].to_bits(),
                    ]
                };
                let (a, b) = (p(s[0]), p(s[1]));
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                [lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]]
            })
            .collect();
        out.sort_unstable();
        out
    };
    let (pm, po) = (pieces(&mine), pieces(&theirs));
    assert_eq!(pm.len(), 84, "interface pieces");
    assert_eq!(pm, po, "{rel}: interface pieces as a set");
    // concrete
    let rel = "paraview/concrete-VTU/concrete_0000.vtu";
    let (mine, theirs) = (read_vtu(&run_dir.join(rel)), read_vtu(&o.kept(rel)));
    for name in [
        "positions",
        "connectivity",
        "offsets",
        "types",
        "external_force",
    ] {
        assert_eq!(
            mine.array(name).values,
            theirs.array(name).values,
            "{rel}: {name}"
        );
    }
    // emb_order: max |port − oracle| ≤ 1e-12 · max |oracle| over `keep`
    let emb_order = |name: &str, keep: &dyn Fn(usize) -> bool| {
        let (a, b) = (mine.array(name), theirs.array(name));
        assert_eq!(
            (a.nb_component, a.values.len()),
            (b.nb_component, b.values.len()),
            "{rel}: {name} layout"
        );
        let (a, b) = (a.values.reals(), b.values.reals());
        let scale = (0..b.len())
            .filter(|&i| keep(i))
            .fold(0.0_f64, |m, i| m.max(b[i].abs()));
        let diff = (0..b.len())
            .filter(|&i| keep(i))
            .fold(0.0_f64, |m, i| m.max((a[i] - b[i]).abs()));
        println!("embedded_eigen_p1 {name}: max |port − oracle| {diff:e} of max {scale:e}");
        assert!(
            diff <= 1e-12 * scale,
            "{rel}: {name}: {diff:e} > 1e-12 · {scale:e}"
        );
    };
    emb_order("displacement", &|_| true);
    emb_order("internal_force", &|_| true);
    // UD-110 layout: per cell a 9 × 9 square, the padded 3 × 3 tensor in
    // its first three rows (row stride 9); UD-111: (0,2), (1,2), (2,0),
    // (2,1) uninitialised upstream
    assert_eq!(theirs.array("stress").values.len(), 81 * theirs.nb_cells);
    emb_order("stress", &|i| !matches!(i % 81, 2 | 11 | 18 | 19));
}
