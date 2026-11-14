// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/igfem_enrichment{.hh,.cc,_inline_impl.hh}.
// The spherical implementation (`SphericalEnrichment`) is compiled only with
// the feature `cgal`: it uses the GPL-3.0-or-later crate
// `akantu-geometry-cgal` (decision D-042); this file itself is LGPL.

//! IGFEM enrichment (IGFEM-003): the geometry registry of upstream
//! `IGFEMEnrichment` and the inside test used by the IGFEM materials.
//!
//! [`InterfaceGeometry`] is the interface the model and the materials use
//! (upstream `IGFEMEnrichment::isInside(point, domain)`). Upstream has a
//! single implementation, spheres ("gel pockets",
//! `std::list<SK::Sphere_3>` per domain): `SphericalEnrichment` (feature
//! `cgal`), which also drives the mesh generation
//! (`crate::growing_gel::MeshIgfemSphericalGrowingGel`, feature `cgal`).

use akantu_core::{Real, Result};

/// Inside test of an interface geometry (upstream
/// `IGFEMEnrichment::isInside(point, domain)`): `true` when `point` lies in
/// (or on the boundary of) the domain `domain` (`""`: the default domain).
pub trait InterfaceGeometry: std::fmt::Debug + Send + Sync {
    /// `isInside(point, domain)`.
    ///
    /// # Errors
    /// Unknown domain, wrong point dimension.
    fn is_inside(&self, point: &[Real], domain: &str) -> Result<bool>;
}

#[cfg(feature = "cgal")]
pub use spherical::{SphereGeometry, SphericalEnrichment, has_on_unbounded_side};

#[cfg(feature = "cgal")]
mod spherical {
    use std::cmp::Ordering;
    use std::collections::BTreeMap;

    use akantu_core::{Error, Real, Result};
    use akantu_geometry::number::Quotient;
    use akantu_geometry_cgal::SkSphere;
    use akantu_mesh::{Mesh, MeshTransaction};

    use super::InterfaceGeometry;
    use crate::growing_gel::{GrowingGelPass, MeshIgfemSphericalGrowingGel};

    /// `true` if `p = (x, y, 0)` lies on the unbounded side of `s`
    /// (CGAL `Sphere_3::has_on_unbounded_side`, Kernel_23 — LGPL —
    /// `bounded_side(p) == ON_UNBOUNDED_SIDE`, i.e. `r² < |p − c|²` with
    /// exact arithmetic: `Quotient<MP_Float>` from the doubles, no
    /// rounding, so the order of the operations does not matter).
    ///
    /// # Errors
    /// Non-finite coordinates, exponent overflow.
    pub fn has_on_unbounded_side(s: &SkSphere, p: [Real; 3]) -> Result<bool> {
        let mut d2 = Quotient::from_i32(0);
        for (i, &x) in p.iter().enumerate() {
            let diff = Quotient::try_from_f64(x)?.sub(&s.center.c[i])?;
            d2 = d2.add(&diff.square()?)?;
        }
        Ok(s.squared_radius.compare(&d2)? == Ordering::Less)
    }

    /// The geometry registry of upstream `IGFEMEnrichment`: sphere lists
    /// by domain name (upstream `std::map<std::string, Geometry *>`: sorted
    /// by name) and the default domain. Implements the inside test.
    #[derive(Clone, Debug, Default)]
    pub struct SphereGeometry {
        geometries: BTreeMap<String, Vec<SkSphere>>,
        default_geometry: String,
    }

    impl SphereGeometry {
        fn resolve<'a>(&'a self, domain: &'a str) -> &'a str {
            if domain.is_empty() {
                &self.default_geometry
            } else {
                domain
            }
        }

        /// Upstream `getGeometry(domain)` (`""`: the default geometry).
        ///
        /// # Errors
        /// Unregistered domain (upstream debug assertion).
        pub fn geometry(&self, domain: &str) -> Result<&[SkSphere]> {
            let d = self.resolve(domain);
            self.geometries.get(d).map(Vec::as_slice).ok_or_else(|| {
                Error::not_found(
                    "IGFEM geometry",
                    format!("The geometry {d} is not registered"),
                )
            })
        }

        /// The default domain.
        #[must_use]
        pub fn default_domain(&self) -> &str {
            &self.default_geometry
        }
    }

    impl InterfaceGeometry for SphereGeometry {
        /// Upstream `isInside`: `true` if `(x, y, 0)` is **not** on the
        /// unbounded side of one of the domain's spheres (boundary
        /// included), exact predicate.
        fn is_inside(&self, point: &[Real], domain: &str) -> Result<bool> {
            if point.len() < 2 {
                return Err(Error::DimensionMismatch {
                    context: "IGFEMEnrichment::isInside: point",
                    expected: 2,
                    got: point.len(),
                });
            }
            let p = [point[0], point[1], 0.0];
            for s in self.geometry(domain)? {
                if !has_on_unbounded_side(s, p)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }

    /// Upstream `IGFEMEnrichment` with its spherical geometry: the
    /// geometry registry ([`SphereGeometry`]) and the growing-gel mesh
    /// generator.
    #[derive(Debug)]
    pub struct SphericalEnrichment {
        intersector_sphere: MeshIgfemSphericalGrowingGel,
        geometry: SphereGeometry,
    }

    impl SphericalEnrichment {
        /// Upstream `IGFEMEnrichment(mesh)`.
        ///
        /// # Errors
        /// See [`MeshIgfemSphericalGrowingGel::new`].
        pub fn new(mesh: &Mesh) -> Result<Self> {
            Ok(Self {
                intersector_sphere: MeshIgfemSphericalGrowingGel::new(mesh)?,
                geometry: SphereGeometry::default(),
            })
        }

        /// Upstream `initialize()` (`intersector_sphere.init()`).
        ///
        /// # Errors
        /// See [`MeshIgfemSphericalGrowingGel::init`].
        pub fn initialize(&mut self, mesh: &Mesh) -> Result<()> {
            self.intersector_sphere.init(mesh)
        }

        /// The mesh generator.
        #[must_use]
        pub fn intersector(&self) -> &MeshIgfemSphericalGrowingGel {
            &self.intersector_sphere
        }

        /// Mutable access to the mesh generator (options, tolerance).
        pub fn intersector_mut(&mut self) -> &mut MeshIgfemSphericalGrowingGel {
            &mut self.intersector_sphere
        }

        /// Upstream `getGeometry(domain)` (`""`: the default geometry).
        ///
        /// # Errors
        /// Unregistered domain (upstream debug assertion).
        pub fn geometry(&self, domain: &str) -> Result<&[SkSphere]> {
            self.geometry.geometry(domain)
        }

        /// A copy of the geometry registry (for the materials' inside
        /// tests).
        #[must_use]
        pub fn geometry_snapshot(&self) -> SphereGeometry {
            self.geometry.clone()
        }

        /// Upstream `registerGeometryObject(geometry, domain)`: the first
        /// registered domain becomes the default.
        ///
        /// # Errors
        /// A domain registered twice (upstream debug assertion).
        pub fn register_geometry_object(
            &mut self,
            geometry: Vec<SkSphere>,
            domain: &str,
        ) -> Result<()> {
            let g = &mut self.geometry;
            if g.geometries.contains_key(domain) {
                return Err(Error::AlreadyExists {
                    kind: "IGFEM geometry object",
                    id: domain.into(),
                });
            }
            if g.geometries.is_empty() {
                g.default_geometry = domain.to_owned();
            }
            g.geometries.insert(domain.to_owned(), geometry);
            Ok(())
        }

        /// Upstream `unRegisterGeometryObject(domain)`: the default becomes
        /// the first remaining domain (name order) when any remain.
        ///
        /// # Errors
        /// Unregistered domain.
        pub fn unregister_geometry_object(&mut self, domain: &str) -> Result<()> {
            let g = &mut self.geometry;
            if g.geometries.remove(domain).is_none() {
                return Err(Error::not_found(
                    "IGFEM geometry object",
                    format!("Geometry object with domain {domain} was not found"),
                ));
            }
            if let Some(first) = g.geometries.keys().next() {
                g.default_geometry = first.clone();
            }
            Ok(())
        }

        /// Upstream `update(domain)`: re-mesh with the spheres of `domain`
        /// (`buildIGFEMMeshFromSpheres`), as the topology closure of a mesh
        /// mutation.
        ///
        /// # Errors
        /// Unregistered domain; see
        /// [`MeshIgfemSphericalGrowingGel::build_igfem_mesh_from_spheres`].
        pub fn update(
            &mut self,
            tx: &mut MeshTransaction<'_>,
            domain: &str,
        ) -> Result<GrowingGelPass> {
            let spheres = self.geometry(domain)?.to_vec();
            self.intersector_sphere
                .build_igfem_mesh_from_spheres(tx, &spheres)
        }

        /// Phase (i) of [`SphericalEnrichment::update`]
        /// ([`MeshIgfemSphericalGrowingGel::build_igfem_elements_from_spheres`]
        /// with the spheres of `domain`): node and element additions and the
        /// erasure of the replaced elements, the previous enriched nodes left
        /// for phase (ii) ([`SphericalEnrichment::remove_previous_enriched_nodes`]).
        ///
        /// # Errors
        /// Unregistered domain; see the generator.
        pub fn update_additions(
            &mut self,
            tx: &mut MeshTransaction<'_>,
            domain: &str,
        ) -> Result<GrowingGelPass> {
            let spheres = self.geometry(domain)?.to_vec();
            self.intersector_sphere
                .build_igfem_elements_from_spheres(tx, &spheres)
        }

        /// Phase (ii) of [`SphericalEnrichment::update`]: the enriched nodes
        /// of the previous pass removed
        /// ([`MeshIgfemSphericalGrowingGel::remove_previous_enriched_nodes`]).
        ///
        /// # Errors
        /// See the generator.
        pub fn remove_previous_enriched_nodes(
            &mut self,
            tx: &mut MeshTransaction<'_>,
        ) -> Result<usize> {
            self.intersector_sphere.remove_previous_enriched_nodes(tx)
        }

        /// Replace the spheres of a registered domain (upstream registers
        /// the geometry **by reference**, `geometries[domain] = &geometry`,
        /// and its tests edit the registered `std::list` between updates).
        ///
        /// # Errors
        /// Unregistered domain.
        pub fn set_geometry_object(&mut self, geometry: Vec<SkSphere>, domain: &str) -> Result<()> {
            let d = self.geometry.resolve(domain).to_owned();
            let g = self.geometry.geometries.get_mut(&d).ok_or_else(|| {
                Error::not_found(
                    "IGFEM geometry",
                    format!("The geometry {d} is not registered"),
                )
            })?;
            *g = geometry;
            Ok(())
        }

        /// Upstream `moveInterface(new_position, domain)` without the
        /// re-meshing: every sphere of the domain gets the radius
        /// `new_position` (`r² = new_position²`, centres kept). (Upstream
        /// also calls `geometry.zero()` on the `std::list`, which does not
        /// exist.) Call [`SphericalEnrichment::update`] afterwards.
        ///
        /// # Errors
        /// Unregistered domain, non-finite radius.
        pub fn set_radius(&mut self, new_position: Real, domain: &str) -> Result<()> {
            let d = self.geometry.resolve(domain).to_owned();
            let r2 = Quotient::try_from_f64(new_position * new_position)?;
            let g = self.geometry.geometries.get_mut(&d).ok_or_else(|| {
                Error::not_found(
                    "IGFEM geometry",
                    format!("The geometry {d} is not registered"),
                )
            })?;
            for s in g.iter_mut() {
                s.squared_radius = r2.clone();
            }
            Ok(())
        }

        /// Upstream `moveInterface(new_position, domain)`:
        /// [`SphericalEnrichment::set_radius`] then
        /// [`SphericalEnrichment::update`].
        ///
        /// # Errors
        /// See both.
        pub fn move_interface(
            &mut self,
            tx: &mut MeshTransaction<'_>,
            new_position: Real,
            domain: &str,
        ) -> Result<GrowingGelPass> {
            self.set_radius(new_position, domain)?;
            let d = self.geometry.resolve(domain).to_owned();
            self.update(tx, &d)
        }

        /// Upstream `getNbStandardNodes()`.
        #[must_use]
        pub fn nb_standard_nodes(&self) -> usize {
            self.intersector_sphere.nb_standard_nodes()
        }

        /// Upstream `getNbEnrichedNodes()`.
        #[must_use]
        pub fn nb_enriched_nodes(&self) -> usize {
            self.intersector_sphere.nb_enriched_nodes()
        }
    }

    impl InterfaceGeometry for SphericalEnrichment {
        fn is_inside(&self, point: &[Real], domain: &str) -> Result<bool> {
            self.geometry.is_inside(point, domain)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The boundary belongs to the inside (upstream `!has_on_unbounded_side`);
        /// one ulp outside / inside is decided exactly.
        #[test]
        fn unbounded_side_is_exact() {
            let s = SkSphere::try_from_f64([0.5, 0.0, 0.0], 0.25).unwrap();
            assert!(!has_on_unbounded_side(&s, [1.0, 0.0, 0.0]).unwrap()); // on the sphere
            assert!(!has_on_unbounded_side(&s, [0.5, 0.1, 0.0]).unwrap());
            assert!(has_on_unbounded_side(&s, [1.0 + f64::EPSILON, 0.0, 0.0]).unwrap());
            assert!(!has_on_unbounded_side(&s, [1.0 - f64::EPSILON / 2.0, 0.0, 0.0]).unwrap());
            // a point whose squared distance differs from r² below the
            // rounding of a double evaluation: |p − c|² = r² + 2⁻¹⁰⁶
            let tiny = 2f64.powi(-53);
            let s2 = SkSphere::try_from_f64([0.0, 0.0, 0.0], 1.0).unwrap();
            assert!(has_on_unbounded_side(&s2, [1.0, tiny, 0.0]).unwrap());
            assert_eq!(1.0f64 * 1.0 + tiny * tiny, 1.0); // the double test would say "on"
        }
    }
}
