// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). `reassignConstitutiveLaw`
// of the diffusion model (D-078; upstream
// src/model/common/constitutive_laws/constitutive_laws_handler_tmpl.hh:337-362,
// ConstitutiveLaw::removeElements / addElements): a reassigned model is the
// model initialised with the final assignment.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use akantu_core::enums::{AnalysisMethod, SparseSolverType};
use akantu_core::{Element, ElementType, GhostType, Real, Result};
use akantu_diffusion::HeatTransferModel;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ConstitutiveLawSelector, SelectorContext};
use akantu_parser::Parser;

const N: usize = 10;

/// Law 0 below element `threshold`, law 1 from it on.
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

fn bar() -> Mesh {
    let mut m = Mesh::new(1, "bar").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for i in 0..=N {
            a.push_node(&[(i as Real / N as Real).powf(1.3)]).unwrap();
        }
        let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        for i in 0..N {
            c.push_tuple(&[i as akantu_core::Idx, (i + 1) as akantu_core::Idx]);
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

const DAT: &str = "model heat_transfer_model [\n\
  constitutive_law heat_diffusion [\n    name = a\n    density = 1\n    capacity = 1\n    \
  conductivity = [[2.0]]\n  ]\n\
  constitutive_law heat_diffusion [\n    name = b\n    density = 3\n    capacity = 2\n    \
  conductivity = [[7.0]]\n  ]\n]\n";

fn model(threshold: usize) -> (HeatTransferModel, Arc<AtomicUsize>) {
    let parser = Parser::parse_str(DAT, "heat.dat").unwrap();
    let mut m =
        HeatTransferModel::with_parser(SharedMesh::new(bar()), None, "heat", parser).unwrap();
    let t = Arc::new(AtomicUsize::new(threshold));
    m.0.set_law_selector(Box::new(Split(Arc::clone(&t))));
    let mut o = ModelOptions::new(AnalysisMethod::Static);
    o.solver_options.sparse_solver_type = Some(SparseSolverType::Eigen);
    m.0.init_full(&o).unwrap();
    (m, t)
}

/// `(law of every element, law-local index of every element, filter of
/// every law)`.
fn assignment(m: &HeatTransferModel) -> (Vec<i32>, Vec<i32>, Vec<Vec<i32>>) {
    let laws = &m.physics.laws;
    let by_el = laws
        .law_index_map()
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    let local = laws
        .local_numbering_map()
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    let filters = (0..laws.nb_laws())
        .map(|i| {
            laws.law(i)
                .unwrap()
                .element_filter()
                .get(ElementType::Segment2, GhostType::NotGhost)
                .map(|a| a.as_slice().to_vec())
                .unwrap_or_default()
        })
        .collect();
    (by_el, local, filters)
}

fn solve(m: &mut HeatTransferModel) -> Vec<Real> {
    m.0.blocked_dofs_mut().unwrap().as_mut_slice()[0] = true;
    m.0.blocked_dofs_mut().unwrap().as_mut_slice()[N] = true;
    m.temperature_mut().unwrap().as_mut_slice()[0] = 40.0;
    m.temperature_mut().unwrap().as_mut_slice()[N] = -12.0;
    m.0.solve_step(None).unwrap();
    m.temperature().unwrap().as_slice().to_vec()
}

#[test]
fn reassigned_model_equals_the_model_built_with_the_final_assignment() {
    let (mut moved, threshold) = model(N);
    let (by_el, _, filters) = assignment(&moved);
    assert!(by_el.iter().all(|&l| l == 0));
    assert_eq!(filters[0].len(), N);
    assert!(filters[1].is_empty());
    threshold.store(4, Ordering::SeqCst);
    moved.0.reassign_law().unwrap();
    let (fresh_model, _) = model(4);
    let got = assignment(&moved);
    let want = assignment(&fresh_model);
    assert_eq!(got, want);
    assert_eq!(got.2[0], vec![0, 1, 2, 3]);
    assert_eq!(got.2[1], vec![4, 5, 6, 7, 8, 9]);
    // every internal is sized for the law's new filter (one point per
    // segment)
    for i in 0..2 {
        let law = moved.physics.laws.law(i).unwrap();
        let n = law
            .element_filter()
            .get(ElementType::Segment2, GhostType::NotGhost)
            .map_or(0, |a| a.len());
        law.for_each_internal(&mut |name, f| {
            if let akantu_model::constitutive_law::InternalFieldRef::Real(f) = f
                && let Ok(b) = f.values().get(ElementType::Segment2, GhostType::NotGhost)
            {
                assert_eq!(b.len(), n, "law {i} internal {name}");
            }
        });
    }
    let mut fresh = fresh_model;
    assert_eq!(solve(&mut moved), solve(&mut fresh));
    // a reassignment that changes nothing is a no-op
    let before = assignment(&moved);
    moved.0.reassign_law().unwrap();
    assert_eq!(assignment(&moved), before);
}
