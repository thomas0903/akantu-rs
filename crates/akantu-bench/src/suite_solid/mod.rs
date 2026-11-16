// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Solid-mechanics
// and cohesive cases of the benchmark suite (binary `suite_solid`); the
// problems follow upstream's examples and tests named in each case.

//! Cases of the `suite_solid` benchmark binary (`crates/akantu-bench/README.md`,
//! section "Benchmark suite: solid mechanics").

pub mod common;
pub mod explicit;
pub mod mesh;

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
    ]
}
