// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/structural_mechanics/
// structural_mechanics_model.hh (StructuralMaterial),
// structural_mechanics_model_inline_impl.hh (addMaterial, getMaterial*),
// structural_elements/structural_element_{bernoulli_beam_2,bernoulli_beam_3,
// kirchhoff_shell}.hh (computeTangentModuli).

//! Structural materials (STRUCT-002) and the generalised tangent moduli of
//! the three structural element types (STRUCT-003/004/005, model side).
//!
//! A [`StructuralMaterial`] is a plain parameter set, as upstream (not parsed
//! from input files). The model stores the materials in insertion order and
//! maps every structural element to one of them (upstream
//! `getElementMaterial(type)`, a user-filled `UInt` array, default `0`).

use akantu_core::{Array, ElementType, Error, Real, Result, UInt};
use akantu_fe::structural::kernels::{
    DktBendingStiffness, tangent_bernoulli_beam_2, tangent_bernoulli_beam_3,
    tangent_discrete_kirchhoff_triangle_18,
};

/// Parameters of a structural material (upstream `StructuralMaterial`;
/// field names are the upstream ones in snake case: `E → e`, `A → a`,
/// `I → i`, `Iz → iz`, `Iy → iy`, `GJ → gj`).
///
/// Which parameters a type reads: `_bernoulli_beam_2` `E, A, I` (and `rho`
/// for the masses); `_bernoulli_beam_3` `E, A, Iz, Iy, GJ` (`rho`, and
/// `Iy + Iz` for the corrected torsional inertia, UD-507); the discrete
/// Kirchhoff triangle `E, nu, t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructuralMaterial {
    /// Young's modulus `E` (default 0).
    pub e: Real,
    /// Cross-section area `A` (default 1).
    pub a: Real,
    /// Second moment of area `I` of 2-D beams (default 0).
    pub i: Real,
    /// Second moment of area about the local `z` axis, `Iz` (default 0).
    pub iz: Real,
    /// Second moment of area about the local `y` axis, `Iy` (default 0).
    pub iy: Real,
    /// Torsional stiffness `GJ` (default 0).
    pub gj: Real,
    /// Density `rho` (mass per volume; the masses use `rho · A` per unit
    /// length; default 0).
    pub rho: Real,
    /// Shell thickness `t` (default 0).
    pub t: Real,
    /// Poisson's ratio `nu` (default 0).
    pub nu: Real,
}

impl Default for StructuralMaterial {
    /// Upstream defaults `{E=0, A=1, I=0, Iz=0, Iy=0, GJ=0, rho=0, t=0,
    /// nu=0}`.
    fn default() -> Self {
        Self {
            e: 0.0,
            a: 1.0,
            i: 0.0,
            iz: 0.0,
            iy: 0.0,
            gj: 0.0,
            rho: 0.0,
            t: 0.0,
            nu: 0.0,
        }
    }
}

/// Number of generalised stress components of a structural type (upstream
/// `ElementClass<type>::getNbStressComponents()`).
///
/// # Errors
/// `Error::Unsupported` for a non-structural type.
pub fn nb_stress_components(ty: ElementType) -> Result<usize> {
    match ty {
        ElementType::BernoulliBeam2 => Ok(2),
        ElementType::BernoulliBeam3 => Ok(4),
        ElementType::DiscreteKirchhoffTriangle18 => Ok(6),
        other => Err(not_structural(other)),
    }
}

/// DOFs per node of a structural type (upstream
/// `ElementClass<type>::getNbDegreeOfFreedom()`).
///
/// # Errors
/// `Error::Unsupported` for a non-structural type.
pub fn nb_degree_of_freedom(ty: ElementType) -> Result<usize> {
    match ty {
        ElementType::BernoulliBeam2 => Ok(3),
        ElementType::BernoulliBeam3 | ElementType::DiscreteKirchhoffTriangle18 => Ok(6),
        other => Err(not_structural(other)),
    }
}

fn not_structural(ty: ElementType) -> Error {
    Error::Unsupported(format!(
        "{} is not a structural element type (bernoulli_beam_2, bernoulli_beam_3, \
         discrete_kirchhoff_triangle_18)",
        ty.name()
    ))
}

/// Material index of every element of a block, checked against the number
/// of materials (upstream indexes `materials[mat]` unchecked, UD-512).
pub(crate) fn checked_material_indices(
    element_material: &Array<UInt>,
    nb_element: usize,
    nb_materials: usize,
    ty: ElementType,
) -> Result<()> {
    if element_material.len() != nb_element || element_material.nb_component() != 1 {
        return Err(Error::DimensionMismatch {
            context: "structural element_material block (one material index per element)",
            expected: nb_element,
            got: element_material.len(),
        });
    }
    if let Some((e, &m)) = element_material
        .as_slice()
        .iter()
        .enumerate()
        .find(|&(_, &m)| m as usize >= nb_materials)
    {
        return Err(Error::invalid(if nb_materials == 0 {
            format!(
                "element {e} of {}: no structural material was added (call add_material \
                 before assembling)",
                ty.name()
            )
        } else {
            format!(
                "element {e} of {}: structural material index {m} out of range ({nb_materials} \
                 material(s))",
                ty.name()
            )
        }));
    }
    Ok(())
}

/// Generalised tangent moduli of every quadrature point of a structural
/// block (upstream `computeTangentModuli<type>`): `nb_element · nb_quad`
/// tuples of `nb_stress²` values (column-major), the moduli of the
/// element's material repeated on its quadrature points.
///
/// * `_bernoulli_beam_2`: `diag(E·A, E·I)`;
/// * `_bernoulli_beam_3`: `diag(E·A, E·Iz, E·Iy, GJ)`;
/// * `_discrete_kirchhoff_triangle_18`: membrane block
///   `E t/(1−ν²) [[1,ν,0],[ν,1,0],[0,0,(1−ν)/2]]`, bending block per
///   `bending` (UD-007: corrected `E t³/(12(1−ν²))`, legacy `E t⁴/…`).
///
/// # Errors
/// A non-structural type; a malformed material map or an element whose
/// material index is out of range (UD-512).
pub fn tangent_moduli(
    ty: ElementType,
    materials: &[StructuralMaterial],
    element_material: &Array<UInt>,
    nb_element: usize,
    nb_quad: usize,
    bending: DktBendingStiffness,
) -> Result<Array<Real>> {
    let ns = nb_stress_components(ty)?;
    checked_material_indices(element_material, nb_element, materials.len(), ty)?;
    let size = ns * ns;
    let mut out = Array::try_zeros(
        nb_element
            .checked_mul(nb_quad)
            .ok_or_else(|| Error::invalid("tangent moduli size overflow"))?,
        size,
    )?;
    let o = out.as_mut_slice();
    for (e, &mi) in element_material.as_slice().iter().enumerate() {
        let m = &materials[mi as usize];
        let d: &[Real] = match ty {
            ElementType::BernoulliBeam2 => &tangent_bernoulli_beam_2(m.e, m.a, m.i),
            ElementType::BernoulliBeam3 => &tangent_bernoulli_beam_3(m.e, m.a, m.iz, m.iy, m.gj),
            _ => &tangent_discrete_kirchhoff_triangle_18(m.e, m.nu, m.t, bending),
        };
        for q in 0..nb_quad {
            let s = (e * nb_quad + q) * size;
            o[s..s + size].copy_from_slice(d);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_upstream() {
        let m = StructuralMaterial::default();
        assert_eq!(
            (m.e, m.a, m.i, m.iz, m.iy, m.gj, m.rho, m.t, m.nu),
            (0., 1., 0., 0., 0., 0., 0., 0., 0.)
        );
    }

    #[test]
    fn tangents_repeat_per_quadrature_point_and_check_indices() {
        let mats = [
            StructuralMaterial {
                e: 2.,
                a: 3.,
                i: 5.,
                ..StructuralMaterial::default()
            },
            StructuralMaterial {
                e: 7.,
                a: 11.,
                i: 13.,
                ..StructuralMaterial::default()
            },
        ];
        let map = Array::from_vec(vec![1u32, 0], 1).unwrap();
        let d = tangent_moduli(
            ElementType::BernoulliBeam2,
            &mats,
            &map,
            2,
            3,
            DktBendingStiffness::Corrected,
        )
        .unwrap();
        assert_eq!(d.len(), 6);
        assert_eq!(d.tuple(0), &[77., 0., 0., 91.]);
        assert_eq!(d.tuple(2), &[77., 0., 0., 91.]);
        assert_eq!(d.tuple(3), &[6., 0., 0., 10.]);
        let bad = Array::from_vec(vec![2u32, 0], 1).unwrap();
        assert!(matches!(
            tangent_moduli(
                ElementType::BernoulliBeam2,
                &mats,
                &bad,
                2,
                3,
                DktBendingStiffness::Corrected
            ),
            Err(Error::InvalidArgument(_))
        ));
        assert!(nb_stress_components(ElementType::Triangle3).is_err());
        assert_eq!(
            nb_degree_of_freedom(ElementType::BernoulliBeam2).unwrap(),
            3
        );
    }
}
