// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); the manual adapts
// doc/dev-doc/manual/*.rst of the reference (see docs/manual/NOTICE).

//! The user manual (`docs/manual/src`) as doc-tests: every `rust` code
//! block of every chapter is compiled and run by
//! `cargo test -p akantu --doc`. Compiled only for doc-tests.

macro_rules! chapters {
    ($($name:ident => $file:literal),* $(,)?) => {$(
        #[doc = include_str!(concat!("../../../docs/manual/src/", $file))]
        #[allow(dead_code)] // carrier of the chapter's doc-tests
        pub struct $name;
    )*};
}

chapters!(
    Introduction => "introduction.md",
    GettingStarted => "getting-started.md",
    BasicTypes => "basic-types.md",
    Meshes => "meshes.md",
    InputFiles => "input-files.md",
    SolverConfiguration => "solver-configuration.md",
    Mumps => "mumps.md",
    Materials => "materials.md",
    ConstitutiveLaws => "constitutive-laws.md",
    LawsNeohookean => "laws/neohookean.md",
    LawsViscoelastic => "laws/viscoelastic.md",
    LawsPlasticity => "laws/plasticity.md",
    LawsDamage => "laws/damage.md",
    LawsNonLocal => "laws/non-local.md",
    LawsPhaseField => "laws/phasefield.md",
    Couplers => "models/couplers.md",
    NewConstitutiveLaws => "new-constitutive-laws.md",
    CommandLine => "command-line.md",
    SolidMechanics => "solid-mechanics.md",
    Output => "output.md",
    Parallel => "parallel.md",
    Geometry => "geometry.md",
    Differences => "differences.md",
    UpstreamChanges => "upstream-changes.md",
    Elements => "appendix/elements.md",
    Structural => "models/structural.md",
    Contact => "models/contact.md",
    PhaseField => "models/phase-field.md",
    Diffusion => "models/diffusion.md",
    NtnContact => "models/ntn-contact.md",
    Cohesive => "models/cohesive.md",
);

/// The repository front page.
#[doc = include_str!("../../../README.md")]
#[allow(dead_code)] // carrier of the README's doc-tests
pub struct Readme;
