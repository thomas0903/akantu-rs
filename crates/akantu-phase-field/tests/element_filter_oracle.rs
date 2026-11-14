// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/
// phase_field_element_filter.hh (PhaseFieldElementFilter::operator(),
// IsUnbrokenFunctor), on the inputs of upstream's
// test/test_model/test_phase_field_model/ (material_coupling.dat,
// test_four_elements.msh); oracle records of the probe
// oracle/probes/phase_field_filter.

//! PF-011: the element filter against the oracle on
//! elements whose quadrature points straddle the threshold.
//!
//! The probe (`oracle/probes/phase_field_filter/`, provenance in the header
//! of `tests/data/oracle/va4fix_element_filter.txt`) prescribes a nodal
//! damage `{0, 0.25, 1, 0.95, 0.6, 0.999, 0.1, 0.5, 0.75}[n mod 9]` on
//! upstream's four quadrangles and calls `computeAllDrivingForces`, which
//! interpolates it on the quadrature points of the law's `damage` internal.
//! It then prints that internal (IEEE-754 hex) and the verdict of
//! `PhaseFieldElementFilter(model, t)` for six thresholds `t`. The port
//! must reproduce the quadrature damage bit for bit and every verdict:
//! upstream keeps an element if **any** of its points is below `t`. Several
//! (element, threshold) pairs have points on both sides of `t`, where "any"
//! and "all" disagree (a mutant that keeps an element only if all its
//! points are below `t` fails here).

mod common;

use akantu_core::{Element, ElementType, GhostType, Real};
use akantu_phase_field::PhaseFieldElementFilter;
use common::*;

const ORACLE: &str = include_str!("data/oracle/va4fix_element_filter.txt");

fn from_hex(s: &str) -> Real {
    f64::from_bits(u64::from_str_radix(s, 16).unwrap())
}

#[test]
fn element_filter_matches_the_oracle_on_straddling_elements() {
    let records: Vec<Vec<&str>> = ORACLE
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| l.split(' ').collect())
        .collect();
    let mut model = model_from_files("test_four_elements.msh", "material_coupling.dat", 2);
    init_static(&mut model);
    {
        let mut d = model.damage_mut().unwrap();
        let d = d.as_mut_slice();
        let mut nodes = 0;
        for r in records.iter().filter(|r| r[0] == "NODE") {
            d[r[1].parse::<usize>().unwrap()] = from_hex(r[2]);
            nodes += 1;
        }
        assert_eq!(nodes, d.len(), "one NODE record per node");
    }
    model.compute_driving_forces().unwrap();
    let nq = model
        .fe_engine()
        .unwrap()
        .nb_integration_points(ElementType::Quadrangle4, GhostType::NotGhost)
        .unwrap();

    let (mut elements, mut verdicts, mut straddling) = (0, 0, 0);
    let mut points: Vec<Real> = Vec::new();
    for r in &records {
        let ty: ElementType = match r[0] {
            "QDAMAGE" | "FILTER" => r[1].parse().unwrap(),
            _ => continue,
        };
        let element = Element::new(ty, r[2].parse().unwrap(), GhostType::NotGhost);
        if r[0] == "QDAMAGE" {
            // the law's damage at the element's points (upstream
            // `getPhaseField(law).getDamage(type)(local * nq + q)`)
            let law = model.phase_field_by_element().element_ref(element).unwrap()[0];
            let law = akantu_core::decode_idx(law).unwrap().unwrap();
            let local = model
                .phase_field_local_numbering()
                .element(element)
                .unwrap()[0];
            let local = akantu_core::try_to_usize(local).unwrap();
            let internal = model.phase_field(law).unwrap();
            let damage = internal.base().damage.get(ty, GhostType::NotGhost).unwrap();
            points = damage.as_slice()[local * nq..(local + 1) * nq].to_vec();
            let oracle: Vec<Real> = r[3..].iter().map(|h| from_hex(h)).collect();
            assert_eq!(oracle.len(), nq, "{element}: quadrature points");
            for (q, (&mine, &theirs)) in points.iter().zip(&oracle).enumerate() {
                assert!(
                    akantu_validation::exact_eq(mine, theirs),
                    "{element} point {q}: damage {mine:e} vs oracle {theirs:e}"
                );
            }
            elements += 1;
            continue;
        }
        let threshold = from_hex(r[3]);
        let oracle = match r[4] {
            "1" => true,
            "0" => false,
            other => panic!("verdict {other}"),
        };
        let mine = PhaseFieldElementFilter::new(threshold)
            .is_unbroken(&model, element)
            .unwrap();
        assert_eq!(mine, oracle, "{element}, max_damage = {threshold}");
        // the record follows its element's QDAMAGE line
        let below = points.iter().filter(|&&x| x < threshold).count();
        if below > 0 && below < points.len() {
            straddling += 1;
        }
        verdicts += 1;
    }
    eprintln!(
        "element filter vs oracle: {elements} elements, {verdicts} verdicts, {straddling} with \
         points on both sides of the threshold"
    );
    assert_eq!(elements, 4, "upstream's four-element mesh");
    assert_eq!(verdicts, 24, "six thresholds per element");
    assert!(
        straddling >= 4,
        "the records must exercise elements with points on both sides of the threshold"
    );
}
