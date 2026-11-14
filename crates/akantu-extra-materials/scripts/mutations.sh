#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (port of Akantu, EPFL/LSMS). Mutation tests of
# crates/akantu-extra-materials: M01-M16 (M11/M12 retargeted to
# the multi-type tests added) and N01-N12 (one per fixed defect): apply one source mutation, run the named test, record
# KILLED (test fails) / SURVIVED, restore the file. Run from the repository
# root of a clean checkout; never commit the result. Results at the
# Head: docs/NUMERICAL_VALIDATION.md.
set -u
CRATE=crates/akantu-extra-materials
out=${1:-/dev/stdout}
run() { # id file perl-substitution test-binary filter
  local id=$1 f=$CRATE/src/$2 sub=$3 bin=$4 filt=$5
  cp "$f" "$f.xmfbak"
  perl -0pi -e "$sub" "$f"
  if cmp -s "$f" "$f.xmfbak"; then echo -e "$id\tNOT-APPLIED\t$2" >> "$out"; mv "$f.xmfbak" "$f"; touch "$f"; return; fi
  # a mutant that does not compile is COMPILE-FAIL, not KILLED
  if cargo test -q -p akantu-extra-materials --test "$bin" "$filt" > "$f.xmflog" 2>&1; then r=SURVIVED; elif grep -q "could not compile" "$f.xmflog"; then r=COMPILE-FAIL; else r=KILLED; fi
  rm -f "$f.xmflog"
  echo -e "$id\t$r\t$2\t$bin::$filt" >> "$out"
  mv "$f.xmfbak" "$f"
  touch "$f" # the restored file keeps the backup's mtime: without this Cargo keeps the mutant compiled
}
run M01 iterative_stiffness_reduction.rs 's/Real::from\(\*r\)/Real::from(*r + 1)/' upstream_damage_iterative test_material_iterative_stiffness_reduction
run M02 orthotropic_damage.rs 's/vs\.matmul\(&w\)/w/' orthotropic update_damage_follows_the_principal_stress_direction
run M03 damage_iterative.rs 's/let threshold = \(1\. - self\.dam_tolerance\)/let threshold = (1. + self.dam_tolerance)/' upstream_damage_iterative test_material_damage_iterative
run M04 c_rand.rs 's/let seed = if seed == 0 \{ 1 \} else \{ seed \};/let seed = seed;/' upstream_fe2 c_rand_matches_glibc_black_box
run M05 viscoplastic.rs 's/prev_iso\.as_slice\(\)\[q\],/iso.as_slice()[q],/' point_laws viscoplastic_explicit_update_from_committed_state
run M06 brittle.rs 's/self\.a \* pow\(r, 3\.\)/self.a * pow(r, 2.)/' point_laws brittle_high_rate_fracture_stress_polynomial_vs_upstream_constant
run M07 vreepeerlings.rs 's/let div = if exact \{ 3\. \}/let div = if exact { 6. }/' point_laws vreepeerlings_j2_rate_exact_vs_upstream
run M08 damage_linear.rs 's/let kq = if per_point \{ q \} else \{ 0 \};/let kq = 0;/' point_laws damage_linear_threshold_per_point_vs_upstream_shared_k
run M09 fe2.rs 's/if alpha \* d == 0\. \{/if false {/' upstream_fe2 test_material_fe2
run M10 orthotropic_damage.rs 's/let td2 = trace_d - 2\.;/let td2 = trace_d - 2.0001;/' orthotropic tangent_2d_against_finite_differences
run M11 rve.rs 's/volume \+= v;/volume = v;/' multi_type rve_volume
run M12 damage_iterative.rs 's/if !self\.extra_options\.equivalent_stress_max_over_types \{/if true {/' multi_type damage_iterative_max_over_types
run M13 kernels.rs 's/if m < v \{ v \} else \{ m \}/if m < v { m } else { v }/' upstream_damage_iterative test_material_damage_iterative
run M14 rve.rs 's/h\[1\]\[0\] = 0\.01; \/\/ H\(0, 1\)/h[0][1] = 0.01; \/\/ H(0, 1)/' upstream_fe2 test_elastic_homogenization
run M15 c_rand.rs 's/word \+= 2_147_483_647;/word += 2_147_483_646;/' upstream_fe2 c_rand_matches_glibc_black_box
run M16 vreepeerlings.rs 's/if kapao < kapac_99 \{ kapao \} else \{ kapac_99 \}/if kapao < kapac_99 { kapac_99 } else { kapao }/' point_laws vreepeerlings_
run N01 damage_linear.rs 's/self\.eps_max \/ ehat \* \(\(ehat - self\.eps_min\) \/ \(self\.eps_max - self\.eps_min\)\)/(ehat - self.eps_min) \/ (self.eps_max - self.eps_min) * (ehat \/ self.eps_max)/' point_laws damage_linear_default_is_linear_softening
run N01b damage_linear.rs 's/self\.eps_max \/ ehat \* \(\(ehat - self\.eps_min\) \/ \(self\.eps_max - self\.eps_min\)\)/(ehat - self.eps_min) \/ (self.eps_max - self.eps_min) * (ehat \/ self.eps_max)/' v86_independent_checks xm01
run N02 vreepeerlings.rs 's/pub const DEFAULT_CRATE: Real = 0\.0001;/pub const DEFAULT_CRATE: Real = 1.;/' point_laws vreepeerlings_default_crate
run N02b vreepeerlings.rs 's/pub const DEFAULT_CRATE: Real = 0\.0001;/pub const DEFAULT_CRATE: Real = 1.;/' v86_independent_checks xm02
run N03 viscoplastic.rs 's/as_matrix\(&prev_inel\.as_slice\(\)\[rp\]\)/as_matrix(&prev_inel.as_slice()[r.clone()])/' point_laws viscoplastic_compat_composes
run N03b viscoplastic.rs 's/as_matrix\(&prev_inel\.as_slice\(\)\[rp\]\)/as_matrix(&prev_inel.as_slice()[r.clone()])/' v86_independent_checks xm03_
run N04 fe2.rs 's/let val = acc \+ Real::from\(u8::from\(i == j\)\) \* th;/let val = acc + if i == j { th } else { 0. };/' v86_independent_checks xm03b
run N04b fe2.rs 's/let val = acc \+ Real::from\(u8::from\(i == j\)\) \* th;/let val = acc + if i == j { th } else { 0. };/' upstream_fe2 fe2_ud584
run N05 rve.rs 's/\|\| a\.is::<MaterialIterativeStiffnessReduction<2>>\(\)//' v86_independent_checks xm05
run N06 brittle.rs 's/rate \+= 2\. \/ 3\. \* pow\(x, 2\.\);/rate += 2. \/ 3. * (x * x);/' point_laws brittle_equivalent_strain_rate_uses_glibc_pow
run N07 orthotropic_damage.rs 's/Material::has_stiffness_matrix_changed\(&self\.elastic\)/true/' point_laws orthotropic_stiffness_flag
run N08 damage_iterative.rs 's/if self\.prescribed_dam\.is_nan\(\) \|\| self\.prescribed_dam <= 0\. \{/if self.prescribed_dam <= 0. {/' point_laws damage_iterative_refuses_nan
run N09 orthotropic_damage.rs 's/if self\.prescribed_dam\.is_nan\(\) \|\| self\.prescribed_dam <= 0\. \{/if self.prescribed_dam <= 0. {/' point_laws damage_iterative_refuses_nan
run N10 rve.rs 's/crate::register_extra_materials_with\(&mut model\.physics\.factory, options\)/crate::register_extra_materials(\&mut model.physics.factory)/' upstream_fe2 fe2_ud584
run N11 fe2.rs 's/                self\.extra_options,\n            \)\?;/                crate::ExtraLawOptions::default(),\n            )?;/' upstream_fe2 fe2_ud584
run N12 vreepeerlings.rs 's/            COMPAT_CRATE\n/            DEFAULT_CRATE\n/' point_laws vreepeerlings_crate_binding

# The non-local wrappers on the solid model's manager
run X01 non_local.rs 's/(impl<const DIM: usize> MaterialDamageIterativeNonLocal<DIM> \{.*?compute_stress_with_criterion\(ctx, ty, ghost, )Some\(&nl\)/${1}None/s' non_local iterative_non_local_laws_average_grad_u
run X02 non_local.rs 's/vec!\[variable\("grad_u", "grad_u non local"/vec![variable("eigen_grad_u", "grad_u non local"/' non_local iterative_non_local_laws_average_grad_u
run X03 non_local.rs 's/(MaterialNonLocal for MaterialDamageIterativeNonLocal<DIM> \{.*?)self\.local\.name\(\)\.to_owned\(\)/${1}"other".to_owned()/s' non_local iterative_non_local_laws_average_grad_u
run X04 non_local.rs 's/(MaterialNonLocal for MaterialDamageIterativeNonLocal<DIM> \{.*?)all_reduce_max\(self\.local\.norm_max_equivalent_stress_mut\(\), reducer\)/${1}{ let _ = reducer; Ok(()) }/s' non_local damage_iterative_non_local_declares
run X05 non_local.rs 's/let nl = self\.sigma_max_nl\.get/let nl = self.sigma_max.get/' non_local brittle_non_local_averages
run X06 non_local.rs 's/variable\("Sigma max", "Sigma_max non local", 1\)/variable("Sigma_fracture", "Sigma_max non local", 1)/' non_local brittle_non_local_averages
run X07 non_local.rs 's/(MaterialNonLocal for MaterialBrittleNonLocal<DIM> \{.*?)self\.local\.name\(\)\.to_owned\(\)/${1}"other".to_owned()/s' non_local brittle_non_local_averages
run X08 non_local.rs 's/let es = self\.equi_strain_nl\.get/let es = self.local.equi_strain.get/' non_local vreepeerlings_non_local_averages
run X09 non_local.rs 's/ *variable\("equi-strain-rate", "equi-strain-rate_non_local", 1\),\n//' non_local vreepeerlings_non_local_averages
run X10 non_local.rs 's/(MaterialNonLocal for MaterialVreePeerlingsNonLocal<DIM> \{.*?)self\.local\.name\(\)\.to_owned\(\)/${1}"other".to_owned()/s' non_local vreepeerlings_non_local_averages
run X11 non_local.rs 's/(impl<const DIM: usize> MaterialOrthotropicDamageIterativeNonLocal<DIM> \{.*?compute_stress_with_criterion\(ctx, ty, ghost, )Some\(&nl\)/${1}None/s' non_local iterative_non_local_laws_average_grad_u
run X12 non_local.rs 's/(MaterialNonLocal for MaterialOrthotropicDamageIterativeNonLocal<DIM> \{.*?)variable\("grad_u"/${1}variable("eigen_grad_u"/s' non_local iterative_non_local_laws_average_grad_u
run X13 non_local.rs 's/(MaterialNonLocal for MaterialOrthotropicDamageIterativeNonLocal<DIM> \{.*?)self\.local\.name\(\)\.to_owned\(\)/${1}"other".to_owned()/s' non_local iterative_non_local_laws_average_grad_u
run X14 non_local.rs 's/if !self\n\s+\.local\n\s+\.extra_options\n\s+\.orthotropic_non_local_max_all_reduced\n\s+\{/if true {/' non_local orthotropic_non_local_declares
run X15 non_local.rs 's/(impl<const DIM: usize> MaterialDamageIterativeNonLocal<DIM> \{.*?compute_stress_with_criterion\(ctx, ty, ghost, )Some\(&nl\)/${1}None/s' upstream_damage_iterative_non_local test_material_damage_iterative_non_local_serial

# Two mutants (R2, R4) that only tests/review_v129_adversarial.rs kills
run X16 non_local.rs 's/let esr = self\.equi_strain_rate_nl\.get\(ty, ghost\)\?\.clone\(\);/let esr = self.local.equi_strain_rate.get(ty, ghost)?.clone();/' review_v129_adversarial v129_a4
run X17 non_local.rs 's/(\/\/ reset \+ empty local pass[^\n]*\n\s*)if ghost == NG \{\n\s*self\.local\.reset_norm_max_equivalent_stress\(\);\n\s*\}/${1}let _ = NG;/' review_v129_adversarial v129_a3
