// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/io/
// dumper/locomotive_tools.cc, locomotive_tools.hh.

//! Helpers of the two dumper examples (`dumper_low_level`,
//! `dumpable_interface`): wheel rotations, train colours, and the
//! [`FieldResolver`] serving the example's external arrays to the dumpers.
//!
//! **UD-473** (D-003 category B): upstream's `applyRotation` writes the
//! rotated displacement into a *copy* of each node's displacement
//! (`Vector<Real> dis = disp_it[node]` copies the `Eigen::Map`), so the
//! wheels never turn in the upstream output; in `dumper_low_level` the
//! centre and relative position also have an uninitialised third
//! component (`Vector<Real>(3)` is not zero-initialised; only reachable
//! through the discarded copy). The port applies the rotation (evident
//! intent, zero third components) by default; `upstream_compat` reproduces
//! the no-op.

#![allow(dead_code)] // shared by the two dumper examples; each uses a subset

use akantu::core::{ElementType, Error, GhostType, Idx, Int, Real, Result};
use akantu::io::{
    DumpBuffer, FieldDescriptor, FieldResolver, FieldSource, FieldSupport, FieldValueType, Padding,
};
use akantu::mesh::Mesh;

/// Upstream `Math::getTolerance()` default (`1e2 ε`).
const TOLERANCE: Real = 1e2 * f64::EPSILON;

/// Upstream `applyRotation(center, angle, nodes, displacement,
/// node_group)`: the displacement of every node of `node_group` that turns
/// it by `−angle` around `center`. `center.len()` is the number of
/// components of the displacement view (upstream
/// `displacement.begin(center.size())`); `nodes` has `node_dim` components
/// per node.
///
/// # Errors
/// A node index out of range.
#[allow(clippy::too_many_arguments)] // upstream signature plus the D-003 switch
pub fn apply_rotation(
    center: &[Real],
    angle: Real,
    nodes: &[Real],
    node_dim: usize,
    displacement: &mut [Real],
    node_group: &[Idx],
    upstream_compat: bool,
) -> Result<()> {
    if upstream_compat {
        // UD-473: upstream writes into a copy; the displacement is unchanged
        return Ok(());
    }
    let n = center.len();
    let mut pos_rel = vec![0.0; n];
    for &node in node_group {
        let node = akantu::core::try_to_usize(node)?;
        let pos = nodes
            .get(node * node_dim..(node + 1) * node_dim)
            .ok_or_else(|| Error::invalid("applyRotation: node out of range"))?;
        for (i, &p) in pos.iter().enumerate().take(n) {
            pos_rel[i] = p;
        }
        for (p, c) in pos_rel.iter_mut().zip(center) {
            *p -= c;
        }
        // Eigen norm of a dynamic vector: sqrt of the sequential sum
        let mut sq = 0.;
        for p in &pos_rel {
            sq += p * p;
        }
        let radius = sq.sqrt();

        if radius.abs() < TOLERANCE {
            continue;
        }

        let mut phi_i = (pos_rel[0] / radius).acos();
        if pos_rel[1] < 0. {
            phi_i *= -1.;
        }

        let dis = displacement
            .get_mut(node * n..(node + 1) * n)
            .ok_or_else(|| Error::invalid("applyRotation: displacement too short"))?;
        dis[0] = (phi_i - angle).cos() * radius - pos_rel[0];
        dis[1] = (phi_i - angle).sin() * radius - pos_rel[1];
    }
    Ok(())
}

/// Upstream `fillColour(mesh, colour)`: one colour per `_triangle_3` from
/// its physical name (`red` → 3; `white`, `lwheel_1`, `rwheel_1` → 2;
/// otherwise 1).
///
/// # Errors
/// No `physical_names` data for the triangles.
pub fn fill_colour(mesh: &Mesh) -> Result<Vec<Int>> {
    let names = mesh
        .data()
        .strings("physical_names")?
        .get(ElementType::Triangle3, GhostType::NotGhost)?;
    Ok(names
        .as_slice()
        .iter()
        .map(|name| match name.as_str() {
            "red" => 3,
            "white" | "lwheel_1" | "rwheel_1" => 2,
            _ => 1,
        })
        .collect())
}

/// Group the nodes and elements of the four wheel groups of the train mesh
/// into the element group `wheels`; returns the nodes of each wheel
/// (`lwheel_1`, `lwheel_2`, `rwheel_1`, `rwheel_2`).
///
/// # Errors
/// Missing groups.
pub fn group_wheels(mesh: &mut Mesh) -> Result<[Vec<Idx>; 4]> {
    let spatial_dimension = mesh.spatial_dimension();
    mesh.groups_mut()
        .create_element_group("wheels", spatial_dimension)?;
    for wheel in ["lwheel_1", "lwheel_2", "rwheel_1", "rwheel_2"] {
        mesh.groups_mut().append_element_group("wheels", wheel)?;
    }
    let nodes_of =
        |g: &str| -> Result<Vec<Idx>> { Ok(mesh.groups().element_group(g)?.nodes().to_vec()) };
    let wheels = [
        nodes_of("lwheel_1")?,
        nodes_of("lwheel_2")?,
        nodes_of("rwheel_1")?,
        nodes_of("rwheel_2")?,
    ];
    Ok(wheels)
}

/// External field `name` served by [`TrainFields`] (upstream
/// `dumpers::NodalField` / `dumpers::ElementalField` of an external array).
#[must_use]
pub fn external_field(name: &str, source: &str, padding: Padding) -> FieldDescriptor {
    let (source, support, value_type) = if source == "colour" {
        (
            FieldSource::ModelElemental(source.into()),
            FieldSupport::Elemental,
            FieldValueType::Int,
        )
    } else {
        (
            FieldSource::ModelNodal(source.into()),
            FieldSupport::Nodal,
            FieldValueType::Real,
        )
    };
    FieldDescriptor {
        name: name.into(),
        source,
        support,
        value_type,
        group: None,
        padding,
        homogenize: false,
    }
}

/// The train's translation and the wheels' rotation at step `i` of
/// `nb_steps` (upstream loop body before the dumps).
///
/// # Errors
/// Node indices out of range.
#[allow(clippy::too_many_arguments)] // the upstream loop's variables
pub fn move_train(
    i: usize,
    nb_steps: usize,
    centers: (&[Real], &[Real]),
    nodes: &[Real],
    node_dim: usize,
    wheels: &[Vec<Idx>; 4],
    displacement: &mut [Real],
    upstream_compat: bool,
) -> Result<()> {
    let tot_displacement = 50.;
    let radius = 1.;
    let theta = tot_displacement / radius;
    let n = centers.0.len();

    displacement.fill(0.);

    #[allow(clippy::cast_precision_loss)] // upstream (Real)i / (Real)nb_steps
    let step_ratio = i as Real / nb_steps as Real;
    let angle = step_ratio * theta;
    let (l_center, r_center) = centers;
    apply_rotation(
        l_center,
        angle,
        nodes,
        node_dim,
        displacement,
        &wheels[0],
        upstream_compat,
    )?;
    apply_rotation(
        l_center,
        angle,
        nodes,
        node_dim,
        displacement,
        &wheels[1],
        upstream_compat,
    )?;
    apply_rotation(
        r_center,
        angle,
        nodes,
        node_dim,
        displacement,
        &wheels[2],
        upstream_compat,
    )?;
    apply_rotation(
        r_center,
        angle,
        nodes,
        node_dim,
        displacement,
        &wheels[3],
        upstream_compat,
    )?;

    for d in displacement.chunks_exact_mut(n) {
        d[0] += step_ratio * tot_displacement;
    }
    Ok(())
}

/// The arrays a dumper example registers as external fields: a nodal
/// `displacement` and the elemental `colour` of the triangles (zero for
/// any other element type, as an `ElementTypeMapArray<Int>` initialised
/// with `_with_nb_element`).
#[derive(Debug)]
pub struct TrainFields<'a> {
    /// Displacement, `nb_component` values per node.
    pub displacement: &'a [Real],
    /// Components of `displacement`.
    pub nb_component: usize,
    /// Colour of every `_triangle_3`.
    pub colour: &'a [Int],
}

impl FieldResolver for TrainFields<'_> {
    fn resolve(
        &self,
        mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        match (&field.source, ty) {
            (FieldSource::ModelNodal(name), None) if name == "displacement" => {
                Ok(DumpBuffer::Real {
                    nb_component: self.nb_component,
                    values: self.displacement.to_vec(),
                })
            }
            (FieldSource::ModelElemental(name), Some(ty)) if name == "colour" => {
                let values = if ty == ElementType::Triangle3 && ghost == GhostType::NotGhost {
                    self.colour.to_vec()
                } else {
                    vec![0; mesh.nb_element(ty, ghost)]
                };
                Ok(DumpBuffer::Int {
                    nb_component: 1,
                    values,
                })
            }
            _ => Err(Error::not_found("dump field", field.name.clone())),
        }
    }
}
