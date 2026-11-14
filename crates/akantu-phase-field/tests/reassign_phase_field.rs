// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). `reassignConstitutiveLaw`
// of the phase-field model (D-078; upstream
// src/model/common/constitutive_laws/constitutive_laws_handler_tmpl.hh:337-362,
// ConstitutiveLaw::removeElements / addElements): a reassigned model is the
// model initialised with the final assignment.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use akantu_core::enums::{AnalysisMethod, SparseSolverType};
use akantu_core::{Element, ElementType, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ConstitutiveLawSelector, InternalFieldRef, SelectorContext};
use akantu_parser::Parser;
use akantu_phase_field::PhaseFieldModel;

const NX: usize = 4;
const NY: usize = 3;
const Q4: ElementType = ElementType::Quadrangle4;
const NG: GhostType = GhostType::NotGhost;

const DAT: &str = "phasefield quadratic [\n name = a\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.2\n]\n\
phasefield quadratic [\n name = b\n E = 70\n nu = 0.2\n gc = 2e-3\n l0 = 0.25\n]\n";

/// Phase field 0 below element `threshold`, 1 from it on.
#[derive(Debug)]
struct Split(Arc<AtomicUsize>);

impl ConstitutiveLawSelector for Split {
    fn select(&self, _ctx: &SelectorContext<'_>, e: Element) -> Result<Option<usize>> {
        let t = self.0.load(Ordering::SeqCst);
        Ok(Some(usize::from(
            usize::try_from(e.index()).unwrap_or(0) >= t,
        )))
    }
}

fn quad_mesh() -> Mesh {
    let mut mesh = Mesh::new(2, "quads").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for j in 0..=NY {
            for i in 0..=NX {
                a.push_node(&[i as Real / NX as Real, 0.75 * j as Real / NY as Real])
                    .unwrap();
            }
        }
        let mut c = a.connectivity_mut(Q4, NG);
        let id = |i: usize, j: usize| (i + j * (NX + 1)) as Idx;
        for j in 0..NY {
            for i in 0..NX {
                c.push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
    }
    mesh.compute_bounding_box().unwrap();
    mesh
}

fn model(threshold: usize) -> (PhaseFieldModel, Arc<AtomicUsize>) {
    let parser = Parser::parse_str(DAT, "m.dat").unwrap();
    let mut m =
        PhaseFieldModel::with_parser(SharedMesh::new(quad_mesh()), None, "pf", parser).unwrap();
    let t = Arc::new(AtomicUsize::new(threshold));
    m.set_phase_field_selector(Box::new(Split(Arc::clone(&t))));
    let mut o = ModelOptions::new(AnalysisMethod::Static);
    o.solver_options.sparse_solver_type = Some(SparseSolverType::Eigen);
    m.init_full(&o).unwrap();
    (m, t)
}

/// `(law of every element, law-local index, filter of every law, the size
/// of every real internal block of every law)`.
type Assignment = (Vec<i32>, Vec<i32>, Vec<Vec<i32>>, Vec<Vec<(String, usize)>>);

fn assignment(m: &PhaseFieldModel) -> Assignment {
    let laws = &m.physics.phasefields;
    let by_el = laws
        .law_index_map()
        .get(Q4, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    let local = laws
        .local_numbering_map()
        .get(Q4, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    let mut filters = Vec::new();
    let mut sizes = Vec::new();
    for i in 0..laws.nb_laws() {
        let law = laws.law(i).unwrap();
        filters.push(
            law.element_filter()
                .get(Q4, NG)
                .map(|a| a.as_slice().to_vec())
                .unwrap_or_default(),
        );
        let mut s = Vec::new();
        law.for_each_internal(&mut |name, f| {
            if let InternalFieldRef::Real(f) = f {
                let n = f.values().get(Q4, NG).map_or(0, |b| b.len());
                s.push((name.to_owned(), n));
            }
        });
        s.sort();
        sizes.push(s);
    }
    (by_el, local, filters, sizes)
}

/// One static step with a damage gradient imposed on the left edge.
fn solve(m: &mut PhaseFieldModel) -> Vec<Real> {
    {
        let mut b = m.blocked_dofs_mut().unwrap();
        for j in 0..=NY {
            b.as_mut_slice()[j * (NX + 1)] = true;
        }
    }
    {
        let mut d = m.damage_mut().unwrap();
        for j in 0..=NY {
            d.as_mut_slice()[j * (NX + 1)] = 0.1 * (j + 1) as Real;
        }
    }
    m.solve_step(None).unwrap();
    m.damage().unwrap().as_slice().to_vec()
}

#[test]
fn reassigned_phase_field_moves_elements_as_upstream() {
    let ne = NX * NY;
    let (mut moved, threshold) = model(ne);
    let (by_el, _, filters, _) = assignment(&moved);
    assert!(by_el.iter().all(|&l| l == 0));
    assert_eq!(filters[0].len(), ne);
    threshold.store(5, Ordering::SeqCst);
    moved.reassign_phase_field().unwrap();
    let (mut fresh, _) = model(5);
    let got = assignment(&moved);
    assert_eq!(got, assignment(&fresh));
    assert_eq!(got.2[0], (0..5).collect::<Vec<i32>>());
    assert_eq!(got.2[1], (5..12).collect::<Vec<i32>>());
    // upstream `addElements` resizes the internals with their default
    // values (`resizeInternals`): the per-point values the AT2 law sets at
    // initialisation (`damage_energy = g_c l0 I`) are not recomputed for the
    // elements that moved in, so the reassigned model is not the fresh one
    // there; the elements that stayed keep their values
    let de = |m: &PhaseFieldModel, law: usize| -> Vec<Real> {
        let mut out = Vec::new();
        m.physics
            .phasefields
            .law(law)
            .unwrap()
            .for_each_internal(&mut |n, f| {
                if n == "damage_energy"
                    && let InternalFieldRef::Real(f) = f
                {
                    out = f.values().get(Q4, NG).unwrap().as_slice().to_vec();
                }
            });
        out
    };
    assert_eq!(de(&moved, 0), de(&fresh, 0));
    assert!(de(&moved, 1).iter().all(|&v| v == 0.0));
    assert!(de(&fresh, 1).iter().any(|&v| v != 0.0));
    // both models solve
    let (a, b) = (solve(&mut moved), solve(&mut fresh));
    assert_eq!(a.len(), b.len());
    assert!(a.iter().chain(&b).all(|v| v.is_finite()));
}
