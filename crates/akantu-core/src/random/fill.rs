// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the draw order of
// src/model/common/constitutive_laws/internal_field_tmpl.hh
// (InternalField::reset, InternalField::resize) together with
// random_internal_field_tmpl.hh (RandomInternalField::setArrayValues) and
// RandomParameter::setValues (src/common/aka_random_generator.hh).

//! Filling arrays with random parameter draws in upstream order.

use super::engine::UniformRandomBitGenerator;
use super::parameter::RandomParameter;
use crate::element_type::GhostType;
use crate::element_type_map::{ElementTypeMap, ElementTypeMapArray};
use crate::error::{Error, Result};
use crate::types::Real;

/// Upstream `RandomParameter::setValues(begin, end)` on a flat slice (one
/// draw per scalar, in index order).
pub fn fill_slice<G: UniformRandomBitGenerator>(
    param: &mut RandomParameter,
    g: &mut G,
    values: &mut [Real],
) {
    param.set_values_with(g, values);
}

/// Draws into an `ElementTypeMapArray` exactly like upstream
/// `RandomInternalField` does in `InternalField::reset()` (`old_lens =
/// None`: every value is redrawn) or `InternalField::resize()` (`old_lens =
/// Some(map)`: only the tuples at index `>= old_len` of each array are drawn;
/// a missing entry means old length 0):
///
/// ```text
/// for ghost_type in [_not_ghost, _ghost]:
///     for type in field.elementTypes(ghost_type):      // enum order, allocated types
///         vect = field(type, ghost_type)
///         setArrayValues(vect.data() + old_len * nb_component,
///                        vect.data() + vect.size() * nb_component)
/// ```
///
/// For a per-quadrature-point field the flat order is element-major, then
/// quadrature point, then component.
///
/// # Errors
/// [`Error::InvalidArgument`] if an old length exceeds the current length
/// (nothing is drawn in that case).
pub fn fill_element_type_map_array<G: UniformRandomBitGenerator>(
    param: &mut RandomParameter,
    g: &mut G,
    field: &mut ElementTypeMapArray<Real>,
    old_lens: Option<&ElementTypeMap<usize>>,
) -> Result<()> {
    // Validate first so that a failure does not leave a partially drawn field.
    if let Some(old) = old_lens {
        for ghost in GhostType::ALL {
            for (ty, array) in field.as_map().iter(ghost) {
                let old_len = old.get(ty, ghost).copied().unwrap_or(0);
                if old_len > array.len() {
                    return Err(Error::InvalidArgument(format!(
                        "random field {}: old length {old_len} > current length {} for ({ty}, {ghost})",
                        field.id(),
                        array.len()
                    )));
                }
            }
        }
    }
    for ghost in GhostType::ALL {
        for (ty, array) in field.as_map_mut().iter_mut(ghost) {
            let old_len = old_lens
                .and_then(|old| old.get(ty, ghost).copied())
                .unwrap_or(0);
            let nbc = array.nb_component();
            param.set_values_with(g, &mut array.as_mut_slice()[old_len * nbc..]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::array::Array;
    use crate::element_type::ElementType;
    use crate::random::{MinstdRand0, RandomDistributionType};

    #[test]
    fn order_is_ghost_then_enum_then_flat() {
        let mut field = ElementTypeMapArray::<Real>::new("f");
        // Insert in non-enum order on purpose.
        let m = field.as_map_mut();
        m.insert(
            ElementType::Triangle6,
            GhostType::NotGhost,
            Array::zeros(2, 2),
        )
        .unwrap();
        m.insert(ElementType::Triangle3, GhostType::Ghost, Array::zeros(1, 2))
            .unwrap();
        m.insert(
            ElementType::Triangle3,
            GhostType::NotGhost,
            Array::zeros(3, 2),
        )
        .unwrap();
        let mut p = RandomParameter::make(1.0, RandomDistributionType::Uniform, 0.0, 1.0);
        let mut e = MinstdRand0::with_seed(7);
        fill_element_type_map_array(&mut p, &mut e, &mut field, None).unwrap();

        let mut q = RandomParameter::make(1.0, RandomDistributionType::Uniform, 0.0, 1.0);
        let mut f = MinstdRand0::with_seed(7);
        let mut expected = vec![0.0; 6 + 4 + 2];
        q.set_values_with(&mut f, &mut expected);
        let got: Vec<Real> = [
            (ElementType::Triangle3, GhostType::NotGhost),
            (ElementType::Triangle6, GhostType::NotGhost),
            (ElementType::Triangle3, GhostType::Ghost),
        ]
        .iter()
        .flat_map(|&(t, g)| field.get(t, g).unwrap().as_slice().to_vec())
        .collect();
        assert_eq!(got, expected);
        assert_eq!(e, f);
    }

    #[test]
    fn resize_draws_only_the_tail() {
        let mut field = ElementTypeMapArray::<Real>::new("f");
        field
            .as_map_mut()
            .insert(
                ElementType::Segment2,
                GhostType::NotGhost,
                Array::zeros(4, 1),
            )
            .unwrap();
        let mut old = ElementTypeMap::new();
        old.insert(ElementType::Segment2, GhostType::NotGhost, 3)
            .unwrap();
        let mut p = RandomParameter::new(5.0);
        let mut e = MinstdRand0::new();
        fill_element_type_map_array(&mut p, &mut e, &mut field, Some(&old)).unwrap();
        let a = field
            .get(ElementType::Segment2, GhostType::NotGhost)
            .unwrap();
        assert_eq!(a.as_slice(), &[0.0, 0.0, 0.0, 5.0]);
        // uniform [0, 0] still consumed two engine calls.
        let mut f = MinstdRand0::new();
        f.discard(2);
        assert_eq!(e, f);
        old.replace(ElementType::Segment2, GhostType::NotGhost, 9);
        assert!(fill_element_type_map_array(&mut p, &mut e, &mut field, Some(&old)).is_err());
    }
}
