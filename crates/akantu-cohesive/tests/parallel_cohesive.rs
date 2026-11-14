// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). tests of
// SolidMechanicsModelCohesive on a distributed mesh, on the in-process local
// world at 1, 2 and 4 ranks: the scenarios of
// oracle/probes/parallel_cohesive against the oracle's per-rank records
// fixtures/parallel_cohesive/<scenario>/np<P>/rank<r>, and independent
// checks.

mod par_coh;

use akantu_cohesive::par_scenarios::{self, NoObserver};
use akantu_comm::{Communicator, run_local};

fn on_ranks(ranks: &[usize], f: impl Fn(Communicator) -> par_coh::Outcome + Sync) {
    for &np in ranks {
        let errors: Vec<String> = run_local(np, par_coh::local_timeout(), &f)
            .into_iter()
            .enumerate()
            .filter_map(|(r, o)| o.err().map(|e| format!("np {np} rank {r}: {e}")))
            .collect();
        assert!(errors.is_empty(), "{}", errors.join("\n"));
    }
}

/// Every scenario runs on 1, 2 and 4 ranks (smoke test of the parallel
/// path before the oracle comparisons).
#[test]
fn scenarios_run_on_one_two_and_four_ranks() {
    for spec in par_scenarios::specs() {
        on_ranks(&[1, 2, 4], |comm| {
            let (_model, _mesh, _dt, recs) = par_coh::run(&spec, &comm, &mut NoObserver)?;
            let ins: usize = recs.iter().map(|r| r.inserted).sum();
            if comm.rank() == 0 {
                eprintln!(
                    "{} np {}: inserted {ins}, final energies {:?}",
                    spec.name,
                    comm.size(),
                    recs.last().map(|r| r.energies)
                );
            }
            Ok(())
        });
    }
}

/// The rank counts the probe recorded for `scenario`, which must be
/// exactly `expected` (1, 2 and 4; 8 for the 2-D scenarios of
/// recorded later): a missing or removed fixture fails here.
fn oracle_ranks(scenario: &str, expected: &[usize]) -> Vec<usize> {
    let counts = par_coh::oracle_rank_counts(scenario);
    assert_eq!(counts, expected, "oracle fixtures of {scenario}");
    counts
}

macro_rules! oracle_test {
    ($name:ident, $scenario:literal, $expected:expr) => {
        #[test]
        fn $name() {
            on_ranks(&oracle_ranks($scenario, &$expected), |comm| {
                par_coh::matches_oracle($scenario, &comm)
            });
        }
    };
}

oracle_test!(
    intrinsic_tri_matches_the_oracle,
    "intrinsic_tri",
    [1, 2, 4, 8]
);
oracle_test!(intrinsic_tet_matches_the_oracle, "intrinsic_tet", [1, 2, 4]);
oracle_test!(
    extrinsic_tri_matches_the_oracle,
    "extrinsic_tri",
    [1, 2, 4, 8]
);
oracle_test!(extrinsic_tet_matches_the_oracle, "extrinsic_tet", [1, 2, 4]);
oracle_test!(
    extrinsic_igtg_tri_matches_the_oracle,
    "extrinsic_igtg_tri",
    [1, 2, 4]
);

/// Independent checks at 1, 2, 3 and 4 ranks with recursive coordinate
/// bisection and at 1, 2 and 4 ranks with the probe's partition rules (see
/// `par_coh::invariants`).
#[test]
fn invariants_hold_for_every_scenario() {
    for spec in par_scenarios::specs() {
        on_ranks(&[1, 2, 3, 4], |comm| {
            par_coh::invariants(&spec, &comm, par_coh::PartKind::Rcb)
        });
        on_ranks(&[2, 4], |comm| {
            par_coh::invariants(&spec, &comm, par_coh::PartKind::Rule)
        });
    }
}

/// One rank equals the serial run bitwise: energies of every
/// step and the final nodal fields of a mesh distributed over one rank
/// against the plain serial model.
#[test]
fn one_rank_equals_the_serial_run() {
    for spec in par_scenarios::specs() {
        on_ranks(&[1], |comm| {
            let (model, _mesh, dt, recs) = par_coh::run(&spec, &comm, &mut NoObserver)?;
            let mut sm = par_coh::ok(akantu_mesh::Mesh::new(spec.dim, "serial"), "mesh")?;
            par_coh::ok(par_scenarios::build_mesh(&spec, &mut sm, true), "build")?;
            let (smodel, sdt, srecs) = par_coh::ok(
                par_scenarios::run(&spec, akantu_mesh::SharedMesh::new(sm), &mut NoObserver),
                "serial",
            )?;
            let bits = |a: &[f64], b: &[f64]| akantu_validation::exact::exact_slices_eq(a, b);
            if !bits(&dt, &sdt) {
                return Err(format!("{}: time steps differ", spec.name));
            }
            for (k, (r, s)) in recs.iter().zip(&srecs).enumerate() {
                if r.inserted != s.inserted || !bits(&r.energies, &s.energies) {
                    return Err(format!(
                        "{}: step {} differs from the serial run",
                        spec.name,
                        k + 1
                    ));
                }
            }
            for (name, a, b) in [
                (
                    "displacement",
                    par_coh::ok(model.smm.displacement(), "u")?
                        .as_slice()
                        .to_vec(),
                    par_coh::ok(smodel.smm.displacement(), "u")?
                        .as_slice()
                        .to_vec(),
                ),
                (
                    "velocity",
                    par_coh::ok(model.smm.velocity(), "v")?.as_slice().to_vec(),
                    par_coh::ok(smodel.smm.velocity(), "v")?.as_slice().to_vec(),
                ),
                (
                    "internal force",
                    par_coh::ok(model.smm.internal_force(), "f")?
                        .as_slice()
                        .to_vec(),
                    par_coh::ok(smodel.smm.internal_force(), "f")?
                        .as_slice()
                        .to_vec(),
                ),
            ] {
                if !bits(&b, &a) {
                    return Err(format!(
                        "{}: final {name} differs from the serial run",
                        spec.name
                    ));
                }
            }
            Ok(())
        });
    }
}

/// Adversarial: with RCB partitions at 4 ranks (cuts at x = 0 and y = 0) the
/// crack of `intrinsic_tri` (x = −0.25) leaves the two right ranks without
/// any local or ghost cohesive element; these zero-work ranks join every
/// collective of the insertion (same event kinds, empty lists) and the
/// invariants hold (`par_coh::invariants`).
#[test]
fn zero_work_ranks_join_the_insertion() {
    for name in ["intrinsic_tri"] {
        let spec = par_scenarios::spec(name).unwrap();
        let counts = run_local(4, par_coh::local_timeout(), |comm| {
            let mesh = par_coh::distributed_with(&spec, &comm, par_coh::PartKind::Rcb)?;
            let (_m, _dt, _r) = par_coh::ok(
                par_scenarios::run(&spec, mesh.clone(), &mut NoObserver),
                name,
            )?;
            let m = par_coh::ok(mesh.read(), "read")?;
            let n: usize = [
                akantu_core::GhostType::NotGhost,
                akantu_core::GhostType::Ghost,
            ]
            .into_iter()
            .flat_map(|g| {
                m.element_types(akantu_core::TypeFilter {
                    spatial_dimension: Some(2),
                    ghost_type: g,
                    kind: Some(akantu_core::ElementKind::Cohesive),
                })
                .map(move |t| (t, g))
                .collect::<Vec<_>>()
            })
            .map(|(t, g)| m.nb_element(t, g))
            .sum();
            Ok::<usize, String>(n)
        });
        let counts: Vec<usize> = counts.into_iter().map(|c| c.unwrap()).collect();
        assert!(
            counts.contains(&0) && counts.iter().any(|&c| c > 0),
            "{name}: cohesive elements per rank {counts:?} (want a zero-work rank)"
        );
        for (r, o) in run_local(4, par_coh::local_timeout(), |comm| {
            par_coh::invariants(&spec, &comm, par_coh::PartKind::Rcb)
        })
        .into_iter()
        .enumerate()
        {
            assert!(o.is_ok(), "{name} rank {r}: {o:?}");
        }
    }
}

// ---------------------------------------------------------------------------
// Facets-only insertion, clustering and fragments on a
// distributed mesh.
// ---------------------------------------------------------------------------

/// Pre-screen / smoke run of the scenarios at 1, 2 and 4
/// ranks: the facets-only runs complete, the fragment stages give 1, 2, 3
/// and 4 fragments, the clustering gives the four clusters of the filter.
#[test]
fn coh2_scenarios_run_on_one_two_and_four_ranks() {
    for spec in par_scenarios::facets_only_specs() {
        on_ranks(&[1, 2, 4], |comm| {
            let mesh = par_coh::distributed(
                &spec,
                &comm,
                akantu_mesh::utils::FacetFlipMode::UpstreamInverse,
            )?;
            if spec.name == "facets_only_shared_tri" && comm.size() > 1 {
                // UD-1043: see facets_only_doubling_a_shared_node_is_refused
                return Ok(());
            }
            let run = par_scenarios::run_facets_only(&spec, mesh.clone(), &mut NoObserver);
            let (_m, _dt, recs) = par_coh::ok(run, spec.name)?;
            let m = par_coh::ok(mesh.read(), "read")?;
            if comm.rank() == 0 {
                eprintln!(
                    "{} np {}: nodes {} (global {}), final energies {:?}",
                    spec.name,
                    comm.size(),
                    m.nb_nodes(),
                    m.nb_global_nodes(),
                    recs.last().map(|r| r.energies)
                );
            }
            Ok(())
        });
    }
    let spec = par_scenarios::fragments_spec();
    on_ranks(&[1, 2, 4], |comm| {
        let mesh = par_coh::distributed(
            &spec,
            &comm,
            akantu_mesh::utils::FacetFlipMode::UpstreamInverse,
        )?;
        struct Obs(Vec<usize>, usize);
        impl par_scenarios::FragmentsObserver for Obs {
            fn after_insertion(
                &mut self,
                _model: &mut akantu_cohesive::SolidMechanicsModelCohesive,
                inserted: usize,
            ) -> akantu_core::Result<()> {
                self.1 = inserted;
                Ok(())
            }
            fn after_stage(
                &mut self,
                _model: &mut akantu_cohesive::SolidMechanicsModelCohesive,
                fm: &akantu_cohesive::FragmentManager,
                _stage: usize,
            ) -> akantu_core::Result<()> {
                self.0.push(fm.nb_fragment());
                Ok(())
            }
        }
        let mut obs = Obs(Vec::new(), 0);
        let _ = par_coh::ok(par_scenarios::run_fragments(mesh, &mut obs), "fragments")?;
        if comm.rank() == 0 {
            eprintln!(
                "fragments_tri np {}: inserted {}, fragments per stage {:?}",
                comm.size(),
                obs.1,
                obs.0
            );
        }
        if obs.0 != [1, 2, 3, 4] {
            return Err(format!("fragments per stage {:?}", obs.0));
        }
        Ok(())
    });
    on_ranks(&[1, 2, 4], |comm| {
        let mut m = par_coh::distributed_mesh(
            &spec,
            &comm,
            akantu_mesh::utils::FacetFlipMode::UpstreamInverse,
        )?;
        let accepted = par_coh::clusters_accepted(&m)?;
        let n = par_coh::ok(
            akantu_mesh::clusters::create_clusters(
                &mut m,
                2,
                "cluster",
                &|el| accepted.contains(&el),
                akantu_mesh::clusters::ClusterFacets::Build,
            ),
            "clusters",
        )?;
        let n_all = par_coh::ok(
            akantu_mesh::clusters::create_clusters(
                &mut m,
                2,
                "all",
                &|_| true,
                akantu_mesh::clusters::ClusterFacets::Build,
            ),
            "clusters",
        )?;
        if comm.rank() == 0 {
            eprintln!(
                "clusters_tri np {}: {n} clusters, {n_all} of the whole mesh",
                comm.size()
            );
        }
        if n != 4 || n_all != 1 {
            return Err(format!(
                "{n} clusters (want 4), {n_all} of the whole mesh (want 1)"
            ));
        }
        Ok(())
    });
}

/// UD-1043 (A): a facets-only insertion on a distributed mesh that doubles
/// a node shared with another rank is refused on every rank (the rank that
/// holds the slave copy names UD-1043, the others report the agreed
/// failure) and rolled back: no rank keeps a doubled node, and a rank
/// without any flagged facet takes part (nothing hangs). Upstream gives the
/// slave copies no global id nor owner and asserts in
/// `NodeSynchronizer::onNodesAdded`.
#[test]
fn facets_only_doubling_a_shared_node_is_refused() {
    let spec = par_scenarios::facets_only_specs()
        .into_iter()
        .find(|s| s.name == "facets_only_shared_tri")
        .unwrap();
    for np in [2, 4] {
        let out = run_local(np, par_coh::local_timeout(), |comm| {
            let mesh = par_coh::distributed(
                &spec,
                &comm,
                akantu_mesh::utils::FacetFlipMode::UpstreamInverse,
            )?;
            let parser = par_coh::ok(
                akantu_parser::Parser::parse_str(&spec.dat, "material.dat"),
                "parse",
            )?;
            let mut model = par_coh::ok(
                akantu_cohesive::SolidMechanicsModelCohesive::with_parser(
                    mesh.clone(),
                    None,
                    akantu_cohesive::model::DEFAULT_ID,
                    parser,
                ),
                "model",
            )?;
            par_coh::ok(
                model.init_full(
                    &akantu_model::ModelOptions::new(
                        akantu_core::enums::AnalysisMethod::ExplicitLumpedMass,
                    ),
                    true,
                ),
                "init_full",
            )?;
            let flagged = par_coh::ok(par_scenarios::flag_facets_only(&spec, &mut model), "flag")?;
            let before = par_coh::ok(mesh.read(), "read")?.nb_nodes();
            let r = model.insert_facets_only();
            let after = par_coh::ok(mesh.read(), "read")?.nb_nodes();
            let named =
                matches!(&r, Err(akantu_core::Error::Unsupported(m)) if m.contains("UD-1043"));
            Ok::<_, String>((r.is_err(), named, before == after, flagged))
        });
        let out: Vec<_> = out.into_iter().map(|o| o.unwrap()).collect();
        assert!(out.iter().all(|o| o.0 && o.2), "np {np}: {out:?}");
        assert!(
            out.iter().any(|o| o.1),
            "np {np}: no rank names UD-1043: {out:?}"
        );
        if np == 4 {
            assert!(
                out.iter().any(|o| o.3 == 0),
                "np 4: want a rank without flagged facet"
            );
        }
    }
}

/// Oracle replays at 1, 2 and 4 ranks (fixtures of
/// oracle/probes/parallel_cohesive v2; classes in `par_coh::coh2`).
macro_rules! coh2_oracle_test {
    ($name:ident, $f:expr) => {
        #[test]
        fn $name() {
            on_ranks(&[1, 2, 4], $f);
        }
    };
}

coh2_oracle_test!(facets_only_tri_matches_the_oracle, |c| {
    par_coh::coh2::facets_only_matches_oracle("facets_only_tri", &c)
});
coh2_oracle_test!(facets_only_tet_matches_the_oracle, |c| {
    par_coh::coh2::facets_only_matches_oracle("facets_only_tet", &c)
});
// Adversarial cases (probe v3): a crack ending on the rank
// boundary (the other ranks double only ghost copies of its facets)…
coh2_oracle_test!(facets_only_pg_tri_matches_the_oracle, |c| {
    par_coh::coh2::facets_only_matches_oracle("facets_only_pg_tri", &c)
});
// …and one facet on the rank boundary doubled without any node (the
// node schemes are not rebuilt when no rank adds a node; fails at 4 ranks
// without the fix)
coh2_oracle_test!(facets_only_bnd_tri_matches_the_oracle, |c| {
    par_coh::coh2::facets_only_matches_oracle("facets_only_bnd_tri", &c)
});
// Checkerboard clustering, a group of the last rank only (the final
// group-name synchronisation of the distributed createClusters)
coh2_oracle_test!(clusters_chk_tri_matches_the_oracle, |c| {
    par_coh::coh2::clusters_chk_matches_oracle(&c)
});
coh2_oracle_test!(clusters_tri_matches_the_oracle, |c| {
    par_coh::coh2::clusters_matches_oracle(&c)
});
coh2_oracle_test!(fragments_tri_matches_the_oracle, |c| {
    par_coh::coh2::fragments_matches_oracle(&c).map(|n| {
        if c.rank() == 0 {
            eprintln!(
                "fragments_tri np {}: order-dependent global entries: {} compared with the \
                 fold of the oracle's contributions, {} not compared",
                c.size(),
                n.folded,
                n.skipped
            );
        }
    })
});

/// The distributed Tri3 square of the probe's `clusters_tri` (2-D rule,
/// `xc = 0`), not shared.
fn clusters_square(comm: &akantu_comm::Communicator) -> Result<akantu_mesh::Mesh, String> {
    par_coh::distributed_mesh(
        &par_scenarios::fragments_spec(),
        comm,
        akantu_mesh::utils::FacetFlipMode::UpstreamInverse,
    )
}

/// UD-1040 (A): `createClusters` of a dimension other than the element
/// synchronizer's (here `createBoundaryGroupFromGeometry`, dimension 1, on
/// the distributed triangles) is refused on every rank before any
/// exchange (upstream throws from `ClusterSynchronizer::packData` in the
/// middle of the exchange); no group is created.
#[test]
fn clusters_of_another_dimension_are_refused_on_every_rank() {
    for np in [2, 4] {
        let out = run_local(np, par_coh::local_timeout(), |comm| {
            let mut m = clusters_square(&comm)?;
            let before = m.groups().element_groups().count();
            let r = akantu_mesh::clusters::create_boundary_group_from_geometry(&mut m);
            let named =
                matches!(&r, Err(akantu_core::Error::Unsupported(s)) if s.contains("UD-1040"));
            Ok::<_, String>((
                r.is_err(),
                named,
                m.groups().element_groups().count() == before,
                format!("{r:?}"),
            ))
        });
        for (r, o) in out.into_iter().enumerate() {
            let o = o.unwrap();
            assert!(o.0 && o.2, "np {np} rank {r}: {o:?}");
            assert!(
                o.1,
                "np {np} rank {r}: the refusal does not name UD-1040: {}",
                o.3
            );
        }
    }
}

/// UD-1041 (A): a filter that rejects an owner's element but accepts its
/// ghost copy is refused on every rank (upstream pairs the ghost's cluster
/// with `starting_index − 1` of the owner, i.e. another rank's cluster, or
/// −1 on rank 0); no cluster group is created.
#[test]
fn clusters_with_a_filter_inconsistent_across_ranks_are_refused() {
    for np in [2, 4] {
        let out = run_local(np, par_coh::local_timeout(), |comm| {
            let mut m = clusters_square(&comm)?;
            let accepted = par_coh::clusters_accepted(&m)?;
            // owners reject the filter's removed cells, ghost copies accept
            // everything
            let filter = |el: akantu_core::Element| {
                el.ghost_type() == akantu_core::GhostType::Ghost || accepted.contains(&el)
            };
            let r = akantu_mesh::clusters::create_clusters(
                &mut m,
                2,
                "cluster",
                &filter,
                akantu_mesh::clusters::ClusterFacets::Build,
            );
            let named = matches!(&r, Err(e) if e.to_string().contains("UD-1041"));
            let groups = m
                .groups()
                .element_groups()
                .filter(|g| g.name().starts_with("cluster_"))
                .count();
            Ok::<_, String>((r.is_err(), named, groups))
        });
        let out: Vec<_> = out.into_iter().map(|o| o.unwrap()).collect();
        assert!(out.iter().all(|o| o.0 && o.2 == 0), "np {np}: {out:?}");
        assert!(
            out.iter().any(|o| o.1),
            "np {np}: no rank names UD-1041: {out:?}"
        );
    }
}

/// UD-1042 (C): the distributed clustering creates no temporary group, so
/// a user group named `tmp_<prefix>_<i>` survives (upstream replaces, then
/// destroys it); the result is otherwise the oracle's (`clusters_tri`).
#[test]
fn distributed_clustering_keeps_a_group_named_like_upstreams_temporaries() {
    on_ranks(&[2], |comm| {
        let mut m = clusters_square(&comm)?;
        par_coh::ok(
            m.groups_mut()
                .create_element_group("tmp_cluster_0", 2)
                .map(|_| ()),
            "group",
        )?;
        let n = par_coh::ok(
            akantu_mesh::clusters::create_clusters(
                &mut m,
                2,
                "cluster",
                &|_| true,
                akantu_mesh::clusters::ClusterFacets::Build,
            ),
            "clusters",
        )?;
        if n != 1 || !m.groups().element_group_exists("tmp_cluster_0") {
            return Err(format!("{n} clusters; tmp_cluster_0 kept: false"));
        }
        Ok(())
    });
}
