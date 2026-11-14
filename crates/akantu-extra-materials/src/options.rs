// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/*
// (the upstream-defect switches of the port; no upstream counterpart).

//! Upstream-defect switches of the extra-materials laws (decision D-003).
//!
//! The extra package does not compile at the reference commit,
//! so no oracle exists (D-007): every switch documents a reading of
//! the bit-rotted code, registered in `docs/UPSTREAM_DEFECTS.md`
//! (UD-572…586, UD-780…786, UD-870, UD-1005). [`ExtraLawOptions::default`] is the corrected
//! behaviour, [`ExtraLawOptions::upstream`] the code as written (where it
//! is defined; undefined behaviour is replaced by the documented definition
//! in both modes).
//!
//! The switches are fixed when a law is created: pass them to
//! [`crate::register_extra_materials_with`] (and
//! `register_extra_non_local_materials_with` with the crate feature
//! `damage-non-local`,
//! [`crate::Fe2Config::extra_options`] for the RVEs of `material_FE2`) so
//! that they are in effect when the input file is parsed (UD-578 acts on
//! the parameter binding) and in every model the package creates. Changing
//! the public `extra_options` field of a law after its creation is possible
//! for the switches that act at run time, but use
//! [`HasExtraOptions::set_extra_options`] (before the input is parsed) for
//! the construction-time defaults.

/// Switches of the extra-materials laws; `true` = corrected (default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // one documented switch per upstream defect
pub struct ExtraLawOptions {
    /// UD-573 (category B): `damage_iterative`,
    /// `iterative_stiffness_reduction` and `orthotropic_damage_iterative`
    /// reset the maximum normalised equivalent stress to 0 in
    /// `computeStress(type, ghost)` **before every element type** (and for
    /// ghost blocks), so the "global" maximum is the one of the last type
    /// evaluated. `true`: the maximum over every not-ghost type, reset once
    /// per `computeAllStresses`; `false`: upstream's per-type reset.
    pub equivalent_stress_max_over_types: bool,
    /// UD-575 (category B): `damage_linear` advances the damage pointer but
    /// not the threshold pointer `K` in its quadrature loop, so every point
    /// of a block reads and writes the threshold of the first point.
    /// `true`: one threshold per point; `false`: upstream's shared `K`.
    pub damage_linear_threshold_per_point: bool,
    /// UD-780 (category B): `damage_linear` writes the damage of its
    /// softening branch as `d = (ε̂ − ε_min)/(ε_max − ε_min)·(ε̂/ε_max)`,
    /// the inverse of the ratio of linear softening: the uniaxial stress
    /// keeps rising after `ε_min` (peak 2.1–5.2 × `Sigc` for typical
    /// parameters) and the work to failure exceeds the one the parameters
    /// define (`ε_max = 2·Gc/Sigc + ε_min`). `true`: the linear softening
    /// law `d = (ε_max/ε̂)·(ε̂ − ε_min)/(ε_max − ε_min)` (uniaxial stress
    /// `σ = Sigc·(ε_max − ε)/(ε_max − ε_min)` on the branch, peak `Sigc`,
    /// softening area `Gc`); `false`: upstream's formula.
    pub damage_linear_linear_softening: bool,
    /// UD-576 (category A for the overwrite, B for the strain):
    /// `damage_linear` computes the principal values with the removed
    /// `Math::matrix33_eigenvalues` on the `dim × dim` `∇u` read as a
    /// 3 × 3 matrix (out-of-bounds read for dim < 3; the non-symmetric
    /// gradient; in the Akantu 4 semantics the LAPACK `dgeev` call also
    /// overwrites the stored `∇u` in place, which the stress then uses —
    /// not reproduced in either mode, category A). Both modes embed the
    /// `dim × dim` block in a zero 3 × 3 matrix on a copy (definition of the
    /// out-of-bounds read) and compute the stress from the unmodified `∇u`.
    /// `true`: principal values of the strain `sym(∇u)`; `false`:
    /// eigenvalues of the raw embedded `∇u` (real parts) — this reproduces
    /// upstream's equivalent strain only, not the overwrite.
    pub damage_linear_symmetric_strain: bool,
    /// UD-577 (category B): `brittle` uses the constant `A` as fracture
    /// stress above the strain-rate threshold `E_0` instead of the cubic
    /// `A ε̇³ + B ε̇² + C ε̇ + D` documented by its header and evaluated by
    /// the original 2014 code (continuous with `S_0` at `E_0` for the
    /// default coefficients). `true`: the polynomial; `false`: `A`.
    pub brittle_polynomial_fracture_stress: bool,
    /// UD-578 (category A/B): `vreepeerlings` registers the input key
    /// `Crate` on the member `Brate`, leaving `Crate` uninitialised.
    /// `true`: `Crate` is its own parameter with the default documented by
    /// the upstream header since 2013 (`0.0001`, "`Kapaoi ≥ Crate`"), so
    /// that `κ₀ = Kapaoi` at rest; `false`: the key `Crate` writes `Brate`
    /// and the member `Crate` is defined as 1 (a choice for the
    /// uninitialised member). The switch acts when the input is parsed:
    /// set it at registration ([`crate::register_extra_materials_with`]).
    pub vreepeerlings_own_crate: bool,
    /// UD-579 (category B): `vreepeerlings` raises "This instruction here
    /// was commented but has to be checked" whenever the point is not
    /// loading (`I1·İ1 ≤ 0` and `J̇2 ≤ 0`, e.g. at rest), where the
    /// original code kept the stored equivalent strain rate. `true`: keep
    /// it; `false`: the error.
    pub vreepeerlings_keep_rate_when_unloading: bool,
    /// UD-580 (category B): `vreepeerlings` rate of `J2`: the 1-D branch
    /// subtracts `I1·İ1/6` (the coded `J2 = ε²/3` gives `2εε̇/3`, i.e.
    /// `/3` as the code comment and the n-D branch), and the n-D shear
    /// terms multiply `∇u_ij` with `∇v_ji` instead of the symmetric parts
    /// the coded `J2` is built from. `true`: the exact rate of the coded
    /// `J2` (`ε:ε̇ − I1·İ1/3`, symmetric parts); `false`: upstream.
    pub vreepeerlings_exact_j2_rate: bool,
    /// UD-581 (category B): `orthotropic_damage_iterative::updateDamage`
    /// diagonalises the incremented damage expressed in the principal
    /// stress frame and stores its eigenvectors as the damage directions
    /// **in the computation frame** (missing rotation by the stress
    /// directions). `true`: directions `V_σ·W` (computation frame);
    /// `false`: upstream's `W`.
    pub orthotropic_directions_in_computation_frame: bool,
    /// UD-582 (category B for the flag): the orthotropic damage laws do not
    /// derive from `MaterialDamage`, so upstream's
    /// `hasStiffnessMatrixChanged` is `MaterialElastic`'s (`false` once the
    /// stiffness was assembled): a damage update between solves keeps the
    /// assembled undamaged stiffness, which changes the result of one-shot
    /// linear solves and of any analysis reusing `K`. `true`: the flag is
    /// always set (as every damage law); `false`: `MaterialElastic`'s flag.
    pub orthotropic_stiffness_changed_after_damage: bool,
    /// UD-870 (category B): on more than one rank,
    /// `orthotropic_damage_iterative::updateDamage`
    /// (`material_orthotropic_damage_iterative.cc:300-365`) compares the
    /// **global** maximum (all-reduced in `computeAllStresses`) with 1 and
    /// then damages the **local** maximum point `q_max` of every rank, so a
    /// run on `n` ranks damages `n` points (one per rank, whatever its local
    /// stress) where the serial run damages one; a rank without elements
    /// reads an unset `q_max`. `true`: only the lowest rank whose local
    /// maximum equals the global maximum damages its point (the serial
    /// pattern; identical to upstream on one rank); `false`: every rank
    /// damages its local maximum (a rank without a maximum point is
    /// refused).
    pub orthotropic_damage_global_maximum_only: bool,
    /// UD-1005 (category B): upstream's
    /// `damage_orthotropic_iterative_non_local` does not override
    /// `computeNonLocalStresses`, so the maximum normalised equivalent
    /// stress of its non-local pass is never all-reduced (the only
    /// all-reduce, the local `computeAllStresses` epilogue, runs before the
    /// pass on the reset value): on more than one rank `updateDamage` would
    /// compare rank-local maxima. `true`: the `Max` all-reduce declared
    /// after the non-local pass (as `damage_iterative_non_local`); `false`:
    /// none. On a distributed mesh `false` combined with UD-870's default
    /// (`orthotropic_damage_global_maximum_only`) is refused at
    /// `init_full` (and again, agreed on every rank, at `updateDamage`).
    pub orthotropic_non_local_max_all_reduced: bool,
    /// UD-583 (category B): `visco_plastic` never advances its previous
    /// stress **and** previous inelastic-strain iterators, so every point
    /// of a block builds `σ = σ_prev[0] + C:(Δ∇u − Δε_p)` and `ε_p =
    /// ε_p,prev[0] + Δε_p` from the first point's committed state. `true`:
    /// per point; `false`: upstream.
    pub viscoplastic_previous_state_per_point: bool,
    /// UD-583 (category B, second part): `visco_plastic` flow: the factor
    /// `‖σ‖^{m−1}` is `+∞` at `σ = 0` for `m < 1` (default `m = 0`), so the
    /// first step gives `0·∞ = NaN`; and the flow direction, its magnitude
    /// and the hardening are read from the **current** (last evaluated)
    /// stress and hardening, so repeated evaluations of one step accumulate.
    /// `true`: `Δε_p = 0` at `σ = 0` and the flow evaluated from the state
    /// committed at the last `savePreviousState` (identical to upstream for
    /// one evaluation per step, the explicit scheme); `false`: upstream.
    pub viscoplastic_committed_flow: bool,
    /// UD-584 (category B): `material_FE2` runs the thermal pre-step of its
    /// `MaterialThermal` parent, whose defaults (`E = 0`, `ν = 0.5`) give
    /// `σ_th = −E/(1 − 2ν)·α·ΔT = (0/0)·0 = NaN` when the section sets no
    /// thermal parameters (upstream's own `test_material_FE2` input).
    /// `true`: `σ_th = 0` at points with `α·ΔT = 0`; `false`: upstream.
    pub fe2_no_thermal_stress_without_expansion: bool,
    /// UD-585 (category B): `SolidMechanicsModelRVE` volume: the loop over
    /// element types assigns (`=`) instead of accumulating. `true`: the sum
    /// over types; `false`: the last type's volume.
    pub rve_volume_sum_over_types: bool,
    /// UD-585 (category B): `SolidMechanicsModelRVE::homogenizeStiffness`
    /// clears the eigenstrain of the material named `gel`, which fails
    /// (unknown material) for an RVE without gel such as upstream's
    /// `test_elastic_homogenization`. `true`: skip when absent; `false`:
    /// the error.
    pub rve_optional_gel: bool,
}

impl Default for ExtraLawOptions {
    fn default() -> Self {
        Self {
            equivalent_stress_max_over_types: true,
            damage_linear_threshold_per_point: true,
            damage_linear_linear_softening: true,
            damage_linear_symmetric_strain: true,
            brittle_polynomial_fracture_stress: true,
            vreepeerlings_own_crate: true,
            vreepeerlings_keep_rate_when_unloading: true,
            vreepeerlings_exact_j2_rate: true,
            orthotropic_directions_in_computation_frame: true,
            orthotropic_stiffness_changed_after_damage: true,
            orthotropic_damage_global_maximum_only: true,
            orthotropic_non_local_max_all_reduced: true,
            viscoplastic_previous_state_per_point: true,
            viscoplastic_committed_flow: true,
            fe2_no_thermal_stress_without_expansion: true,
            rve_volume_sum_over_types: true,
            rve_optional_gel: true,
        }
    }
}

impl ExtraLawOptions {
    /// The upstream code as written for every switch.
    #[must_use]
    pub const fn upstream() -> Self {
        Self {
            equivalent_stress_max_over_types: false,
            damage_linear_threshold_per_point: false,
            damage_linear_linear_softening: false,
            damage_linear_symmetric_strain: false,
            brittle_polynomial_fracture_stress: false,
            vreepeerlings_own_crate: false,
            vreepeerlings_keep_rate_when_unloading: false,
            vreepeerlings_exact_j2_rate: false,
            orthotropic_directions_in_computation_frame: false,
            orthotropic_stiffness_changed_after_damage: false,
            orthotropic_damage_global_maximum_only: false,
            orthotropic_non_local_max_all_reduced: false,
            viscoplastic_previous_state_per_point: false,
            viscoplastic_committed_flow: false,
            fe2_no_thermal_stress_without_expansion: false,
            rve_volume_sum_over_types: false,
            rve_optional_gel: false,
        }
    }
}

/// Laws carrying [`ExtraLawOptions`].
pub trait HasExtraOptions {
    /// The switches of the law.
    fn extra_options(&self) -> &ExtraLawOptions;

    /// Set the switches as at construction: also re-applies the
    /// construction-time defaults that depend on them (UD-578: the default
    /// of `vreepeerlings`' `Crate`). Call it before the input parameters
    /// are parsed (the registration functions of the crate do).
    fn set_extra_options(&mut self, options: ExtraLawOptions);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_upstream_differ_everywhere() {
        let d = ExtraLawOptions::default();
        let u = ExtraLawOptions::upstream();
        let dbg_d = format!("{d:?}");
        let dbg_u = format!("{u:?}");
        assert_eq!(dbg_d.matches("true").count(), 17);
        assert_eq!(dbg_u.matches("false").count(), 17);
    }
}
