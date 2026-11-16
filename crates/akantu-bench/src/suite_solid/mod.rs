// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Solid-mechanics
// and cohesive cases of the benchmark suite (binary `suite_solid`); the
// problems follow upstream's examples and tests named in each case.

//! Cases of the `suite_solid` benchmark binary (`crates/akantu-bench/README.md`,
//! section "Benchmark suite: solid mechanics").

pub mod cohesive;
pub mod common;
pub mod explicit;
pub mod implicit;
pub mod mesh;
pub mod parallel;
pub mod setup;

use crate::suite::Case;

/// Every case of `suite_solid`, in `--list` order.
#[must_use]
pub fn cases() -> Vec<Case> {
    use explicit::{elastic, laws};
    let c = |name, description, run| Case {
        name,
        description,
        run,
    };
    vec![
        c(
            "explicit_tet4_ss",
            "explicit elastic step, tetrahedron_4, small strain: memory-bound volume loop (1 point/element)",
            elastic::tet4_ss,
        ),
        c(
            "explicit_tet4_fd",
            "explicit elastic step, tetrahedron_4, finite deformation (Green strain, PK2, B_NL)",
            elastic::tet4_fd,
        ),
        c(
            "explicit_tet10_ss",
            "explicit elastic step, tetrahedron_10, small strain: 4 points, 30-column B",
            elastic::tet10_ss,
        ),
        c(
            "explicit_tet10_fd",
            "explicit elastic step, tetrahedron_10, finite deformation",
            elastic::tet10_fd,
        ),
        c(
            "explicit_hex8_ss",
            "explicit elastic step, hexahedron_8, small strain: 8 points/element, history copies",
            elastic::hex8_ss,
        ),
        c(
            "explicit_hex8_fd",
            "explicit elastic step, hexahedron_8, finite deformation",
            elastic::hex8_fd,
        ),
        c(
            "explicit_tri3_ss",
            "explicit elastic step, triangle_3 (2-D plane strain), small strain",
            elastic::tri3_ss,
        ),
        c(
            "explicit_tri3_fd",
            "explicit elastic step, triangle_3, finite deformation",
            elastic::tri3_fd,
        ),
        c(
            "explicit_plastic",
            "explicit step, linear isotropic hardening plasticity (return mapping) under a stretch ramp",
            laws::plastic,
        ),
        c(
            "explicit_marigo",
            "explicit step, Marigo damage (energy-driven damage, stress degradation)",
            laws::marigo,
        ),
        c(
            "explicit_mazars",
            "explicit step, Mazars damage (equivalent strain from principal strains: eigenvalues)",
            laws::mazars,
        ),
        c(
            "explicit_sls",
            "explicit step, standard linear solid (deviatoric viscoelastic history)",
            laws::sls,
        ),
        c(
            "explicit_maxwell",
            "explicit step, generalised Maxwell viscoelasticity, two branches",
            laws::maxwell,
        ),
        c(
            "explicit_neohookean",
            "explicit step, Neo-Hookean hyperelasticity (finite deformation)",
            laws::neohookean,
        ),
        c(
            "explicit_orthotropic",
            "explicit step, orthotropic elasticity with rotated axes (full 6x6 C)",
            laws::orthotropic,
        ),
        c(
            "explicit_thermal",
            "explicit step, elastic law with a thermal strain (sigma_th, eigen-gradient)",
            laws::thermal,
        ),
        c(
            "explicit_multi_material",
            "explicit step, three laws in one mesh (selector, per-law filters, scatter)",
            laws::multi_material,
        ),
        c(
            "explicit_mazars_non_local",
            "explicit step, non-local Mazars: neighbourhood build/rebuild and per-step averaging",
            laws::mazars_non_local,
        ),
        c(
            "static_linear_eigen",
            "linear static solve, Eigen SparseLU translation (default solver): assembly, analysis, factorisation, back-substitution",
            implicit::static_linear_eigen,
        ),
        c(
            "static_linear_faer",
            "linear static solve, faer sparse LU: the same phases",
            implicit::static_linear_faer,
        ),
        c(
            "static_linear_mumps",
            "linear static solve, MUMPS (feature mumps, MPI world): solve_step phases only; the digest does not repeat above 2 ranks",
            implicit::static_linear_mumps,
        ),
        c(
            "static_newton_plastic",
            "nonlinear static Newton-Raphson with plasticity: tangent and residual per iteration",
            implicit::static_newton_plastic,
        ),
        c(
            "implicit_dynamic",
            "implicit dynamics (trapezoidal rule): consistent mass, K/M/J assembly and the solve per step",
            implicit::implicit_dynamic,
        ),
        c(
            "setup_tet4",
            "model set-up, tetrahedron_4: init_full, shape functions, lumped/consistent mass, stiffness assembly",
            setup::tet4,
        ),
        c(
            "setup_tet10",
            "model set-up, tetrahedron_10 (4 points, 10-node blocks)",
            setup::tet10,
        ),
        c(
            "setup_hex8",
            "model set-up, hexahedron_8 (8 points)",
            setup::hex8,
        ),
        c(
            "cohesive_intrinsic_bilinear",
            "intrinsic cohesive (bilinear), explicit: cohesive elements in a slab, bulk + cohesive forces per step",
            cohesive::intrinsic_bilinear,
        ),
        c(
            "cohesive_intrinsic_exponential",
            "intrinsic cohesive (exponential law, exponential contact penalty), explicit",
            cohesive::intrinsic_exponential,
        ),
        c(
            "cohesive_intrinsic_implicit",
            "intrinsic cohesive, implicit static Newton (triangle_6, 2-D): cohesive tangent, factorisations",
            cohesive::intrinsic_implicit,
        ),
        c(
            "cohesive_extrinsic_check",
            "extrinsic cohesive, no insertion: the insertion check (facet stresses) and the step",
            cohesive::extrinsic_check,
        ),
        c(
            "cohesive_extrinsic_insertion",
            "extrinsic cohesive with progressive insertions: check, facet doubling, model updates, step",
            cohesive::extrinsic_insertion,
        ),
        c(
            "par_explicit_tet4_ss",
            "explicit elastic step on the distributed cube (mpirun -n P): ghost exchanges and reductions per step",
            parallel::explicit_tet4_ss,
        ),
        c(
            "par_cohesive_extrinsic",
            "extrinsic cohesive insertions on the distributed cube (mpirun -n P): distributed check and insertion",
            parallel::cohesive_extrinsic,
        ),
    ]
}
