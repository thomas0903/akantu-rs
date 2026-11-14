// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/mesh_io/mesh_io_msh.hh
// (`MSHElementType`), src/io/mesh_io/mesh_io_msh.cc (`MeshIOMSH::MeshIOMSH`:
// `_msh_nodes_per_elem`, `_msh_to_akantu_element_types`,
// `_akantu_to_msh_element_types`, `_read_order`),
// src/io/mesh_io/mesh_io_msh_struct.cc (`MeshIOMSHStruct`),
// src/io/mesh_io/mesh_io_diana.cc (`MeshIODiana::MeshIODiana`).

//! Element-type and node-order tables of the Gmsh and Diana formats.

use akantu_core::ElementType;

/// Gmsh element type codes known to upstream (`MSHElementType`).
pub(crate) mod msh {
    pub(crate) const SEGMENT_2: u32 = 1;
    pub(crate) const TRIANGLE_3: u32 = 2;
    pub(crate) const QUADRANGLE_4: u32 = 3;
    pub(crate) const TETRAHEDRON_4: u32 = 4;
    pub(crate) const HEXAHEDRON_8: u32 = 5;
    pub(crate) const PRISM_6: u32 = 6;
    pub(crate) const SEGMENT_3: u32 = 8;
    pub(crate) const TRIANGLE_6: u32 = 9;
    pub(crate) const TETRAHEDRON_10: u32 = 11;
    pub(crate) const POINT: u32 = 15;
    pub(crate) const QUADRANGLE_8: u32 = 16;
    pub(crate) const HEXAHEDRON_20: u32 = 17;
    pub(crate) const PRISM_15: u32 = 18;
}

/// Number of nodes of a Gmsh element type, used to validate the lines of
/// MSH 4 element blocks of types Akantu does not read (UD-090).
///
/// Codes 1–31, 92, 93 are the element types of the Gmsh reference manual's
/// MSH format section (`elm-type` list); 1–31 are also listed, with the
/// same node counts, in the comment block of upstream `mesh_io_msh.cc`
/// (upstream's own `_msh_nodes_per_elem` covers 0–18 and is only used by
/// the writer). The other codes are not in the manual's list: they are
/// higher-order types Gmsh 4.13.1 writes. Every entry was
/// measured black-box on ASCII MSH 2.2 files written by Gmsh 4.13.1
/// (`oracle/probes/mesh_io/gmsh_element_nodes.sh`, results committed as
/// `gmsh_element_nodes.txt` next to it) and `tests::node_counts_match_gmsh`
/// checks the table against those measurements. Unknown codes are `None`:
/// their element lines are skipped after the element tag, without checking
/// the node count.
#[must_use]
pub(crate) fn gmsh_nodes_per_element(code: u32) -> Option<usize> {
    Some(match code {
        1 => 2,    // 2-node line
        2 => 3,    // 3-node triangle
        3 => 4,    // 4-node quadrangle
        4 => 4,    // 4-node tetrahedron
        5 => 8,    // 8-node hexahedron
        6 => 6,    // 6-node prism
        7 => 5,    // 5-node pyramid
        8 => 3,    // 3-node second order line
        9 => 6,    // 6-node second order triangle
        10 => 9,   // 9-node second order quadrangle
        11 => 10,  // 10-node second order tetrahedron
        12 => 27,  // 27-node second order hexahedron
        13 => 18,  // 18-node second order prism
        14 => 14,  // 14-node second order pyramid
        15 => 1,   // 1-node point
        16 => 8,   // 8-node second order quadrangle
        17 => 20,  // 20-node second order hexahedron
        18 => 15,  // 15-node second order prism
        19 => 13,  // 13-node second order pyramid
        20 => 9,   // 9-node third order incomplete triangle
        21 => 10,  // 10-node third order triangle
        22 => 12,  // 12-node fourth order incomplete triangle
        23 => 15,  // 15-node fourth order triangle
        24 => 15,  // 15-node fifth order incomplete triangle
        25 => 21,  // 21-node fifth order complete triangle
        26 => 4,   // 4-node third order edge
        27 => 5,   // 5-node fourth order edge
        28 => 6,   // 6-node fifth order edge
        29 => 20,  // 20-node third order tetrahedron
        30 => 35,  // 35-node fourth order tetrahedron
        31 => 56,  // 56-node fifth order tetrahedron
        92 => 64,  // 64-node third order hexahedron
        93 => 125, // 125-node fourth order hexahedron
        // Not in the manual's list (measured, see above).
        32 => 22,  // fourth order incomplete tetrahedron
        33 => 28,  // fifth order incomplete tetrahedron
        36 => 16,  // third order quadrangle
        37 => 25,  // fourth order quadrangle
        38 => 36,  // fifth order quadrangle
        39 => 12,  // third order incomplete quadrangle
        40 => 16,  // fourth order incomplete quadrangle
        41 => 20,  // fifth order incomplete quadrangle
        94 => 216, // fifth order hexahedron
        99 => 32,  // third order incomplete hexahedron
        100 => 44, // fourth order incomplete hexahedron
        101 => 56, // fifth order incomplete hexahedron
        118 => 30, // third order pyramid
        119 => 55, // fourth order pyramid
        120 => 91, // fifth order pyramid
        137 => 16, // third order incomplete tetrahedron
        _ => return None,
    })
}

/// Gmsh → Akantu element type of the standard reader
/// (`_msh_to_akantu_element_types`): `None` = skipped with a warning
/// (pyramids, 9-node quadrangles, 27-node hexahedra, 18-node prisms, …).
#[must_use]
pub(crate) fn msh_to_akantu(code: u32) -> Option<ElementType> {
    use ElementType as Et;
    Some(match code {
        msh::SEGMENT_2 => Et::Segment2,
        msh::TRIANGLE_3 => Et::Triangle3,
        msh::QUADRANGLE_4 => Et::Quadrangle4,
        msh::TETRAHEDRON_4 => Et::Tetrahedron4,
        msh::HEXAHEDRON_8 => Et::Hexahedron8,
        msh::PRISM_6 => Et::Pentahedron6,
        msh::SEGMENT_3 => Et::Segment3,
        msh::TRIANGLE_6 => Et::Triangle6,
        msh::TETRAHEDRON_10 => Et::Tetrahedron10,
        msh::POINT => Et::Point1,
        msh::QUADRANGLE_8 => Et::Quadrangle8,
        msh::HEXAHEDRON_20 => Et::Hexahedron20,
        msh::PRISM_15 => Et::Pentahedron15,
        _ => return None,
    })
}

/// Gmsh → Akantu type of the structural reader (`MeshIOMSHStruct`):
/// 2-node lines become Bernoulli beams (`_bernoulli_beam_3` in a 3-D mesh,
/// `_bernoulli_beam_2` otherwise), 3-node triangles discrete Kirchhoff
/// triangles; every other type is skipped.
#[must_use]
pub(crate) fn msh_to_akantu_struct(code: u32, spatial_dimension: usize) -> Option<ElementType> {
    match code {
        msh::SEGMENT_2 if spatial_dimension == 3 => Some(ElementType::BernoulliBeam3),
        msh::SEGMENT_2 => Some(ElementType::BernoulliBeam2),
        msh::TRIANGLE_3 => Some(ElementType::DiscreteKirchhoffTriangle18),
        _ => None,
    }
}

/// Akantu → Gmsh code used by the writer (`_akantu_to_msh_element_types`,
/// structural types included as upstream does with
/// `AKANTU_STRUCTURAL_MECHANICS`). `None`: no Gmsh equivalent (cohesive and
/// IGFEM types).
#[must_use]
pub(crate) fn akantu_to_msh(ty: ElementType) -> Option<u32> {
    use ElementType as Et;
    Some(match ty {
        Et::Segment2 | Et::BernoulliBeam2 | Et::BernoulliBeam3 => msh::SEGMENT_2,
        Et::Segment3 => msh::SEGMENT_3,
        Et::Triangle3 | Et::DiscreteKirchhoffTriangle18 => msh::TRIANGLE_3,
        Et::Triangle6 => msh::TRIANGLE_6,
        Et::Tetrahedron4 => msh::TETRAHEDRON_4,
        Et::Tetrahedron10 => msh::TETRAHEDRON_10,
        Et::Quadrangle4 => msh::QUADRANGLE_4,
        Et::Quadrangle8 => msh::QUADRANGLE_8,
        Et::Hexahedron8 => msh::HEXAHEDRON_8,
        Et::Hexahedron20 => msh::HEXAHEDRON_20,
        Et::Pentahedron6 => msh::PRISM_6,
        Et::Pentahedron15 => msh::PRISM_15,
        Et::Point1 => msh::POINT,
        _ => return None,
    })
}

/// Gmsh → Akantu local node order of the standard reader: Gmsh node `j`
/// goes to Akantu slot `read_order[j]` (`local_connect(read_order[j]) =
/// node_j`). `None` = identity.
#[must_use]
pub(crate) fn msh_read_order(ty: ElementType) -> Option<&'static [usize]> {
    use ElementType as Et;
    const TET10: [usize; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 9, 8];
    const PENTA6: [usize; 6] = [2, 0, 1, 5, 3, 4];
    const PENTA15: [usize; 15] = [2, 0, 1, 5, 3, 4, 8, 7, 11, 6, 9, 10, 14, 13, 12];
    const HEX20: [usize; 20] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 9, 13, 10, 14, 15, 16, 19, 17, 18,
    ];
    match ty {
        Et::Tetrahedron10 => Some(&TET10),
        Et::Pentahedron6 => Some(&PENTA6),
        Et::Pentahedron15 => Some(&PENTA15),
        Et::Hexahedron20 => Some(&HEX20),
        _ => None,
    }
}

/// Diana element names (`_diana_to_akantu_element_types`).
#[must_use]
pub(crate) fn diana_to_akantu(name: &str) -> Option<ElementType> {
    use ElementType as Et;
    Some(match name {
        "T9TM" => Et::Triangle3,
        "CT6CM" => Et::Triangle6,
        "Q12TM" => Et::Quadrangle4,
        "CQ8CM" => Et::Quadrangle8,
        "TP18L" => Et::Pentahedron6,
        "CTP45" => Et::Pentahedron15,
        "TE12L" => Et::Tetrahedron4,
        "HX24L" => Et::Hexahedron8,
        "CHX60" => Et::Hexahedron20,
        _ => return None,
    })
}

/// Diana → Akantu local node order (Diana node `j` goes to Akantu slot
/// `read_order[j]`). `None` = identity.
#[must_use]
pub(crate) fn diana_read_order(ty: ElementType) -> Option<&'static [usize]> {
    use ElementType as Et;
    const PENTA15: [usize; 15] = [2, 8, 0, 6, 1, 7, 11, 9, 10, 5, 14, 3, 12, 4, 13];
    const HEX20: [usize; 20] = [
        5, 16, 4, 19, 7, 18, 6, 17, 13, 12, 15, 14, 1, 8, 0, 11, 3, 10, 2, 9,
    ];
    match ty {
        Et::Pentahedron15 => Some(&PENTA15),
        Et::Hexahedron20 => Some(&HEX20),
        _ => None,
    }
}

/// Inverse of a read order: Akantu slot `s` holds Gmsh node `inv[s]`.
#[cfg(test)]
#[must_use]
pub(crate) fn invert_order(order: &[usize]) -> Vec<usize> {
    let mut inv = vec![0; order.len()];
    for (j, &s) in order.iter().enumerate() {
        inv[s] = j;
    }
    inv
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_permutation(p: &[usize]) -> bool {
        let mut seen = vec![false; p.len()];
        p.iter()
            .all(|&i| i < p.len() && !std::mem::replace(&mut seen[i], true))
    }

    #[test]
    fn read_orders_are_permutations_of_the_right_size() {
        for ty in ElementType::ALL {
            for order in [msh_read_order(ty), diana_read_order(ty)]
                .into_iter()
                .flatten()
            {
                assert_eq!(order.len(), ty.nb_nodes_per_element(), "{ty}");
                assert!(is_permutation(order), "{ty}");
                let inv = invert_order(order);
                for (j, &s) in order.iter().enumerate() {
                    assert_eq!(inv[s], j);
                }
            }
        }
    }

    #[test]
    fn gmsh_codes_round_trip_and_node_counts_agree() {
        for code in 0..40 {
            if let Some(ty) = msh_to_akantu(code) {
                assert_eq!(akantu_to_msh(ty), Some(code));
                assert_eq!(
                    gmsh_nodes_per_element(code),
                    Some(ty.nb_nodes_per_element())
                );
            }
        }
        for (code, dim) in [(1, 1), (1, 2), (1, 3), (2, 2), (2, 3)] {
            let ty = msh_to_akantu_struct(code, dim).unwrap();
            assert_eq!(akantu_to_msh(ty), Some(code));
            assert_eq!(
                gmsh_nodes_per_element(code),
                Some(ty.nb_nodes_per_element())
            );
        }
        assert_eq!(msh_to_akantu(7), None); // pyramid
        assert_eq!(akantu_to_msh(ElementType::Cohesive2d4), None);
    }

    /// Every entry of [`gmsh_nodes_per_element`] equals the
    /// node count measured on files written by Gmsh 4.13.1
    /// (`oracle/probes/mesh_io/gmsh_element_nodes.sh`), and every measured
    /// code is in the table.
    #[test]
    fn node_counts_match_gmsh() {
        let measured: Vec<(u32, usize)> =
            include_str!("../../../../oracle/probes/mesh_io/gmsh_element_nodes.txt")
                .lines()
                .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
                .map(|l| {
                    let mut t = l.split_whitespace();
                    let code = t.next().unwrap().parse().unwrap();
                    let n = t.next().unwrap().parse().unwrap();
                    (code, n)
                })
                .collect();
        assert!(measured.len() >= 33, "{measured:?}");
        let mut codes: Vec<u32> = measured.iter().map(|&(c, _)| c).collect();
        codes.dedup();
        assert_eq!(codes.len(), measured.len(), "one node count per code");
        for &(code, n) in &measured {
            assert_eq!(gmsh_nodes_per_element(code), Some(n), "code {code}");
        }
        for code in 0..=1000 {
            if gmsh_nodes_per_element(code).is_some() {
                assert!(codes.contains(&code), "code {code} not measured");
            }
        }
        // The documented list (Gmsh manual, upstream comment block).
        for code in (1..=31).chain([92, 93]) {
            assert!(codes.contains(&code), "documented code {code} not measured");
        }
        assert_eq!(gmsh_nodes_per_element(26), Some(4));
    }
}
