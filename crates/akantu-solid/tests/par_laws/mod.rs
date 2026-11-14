// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/constitutive_laws/
// constitutive_laws_handler_tmpl.hh (initConstitutiveLaws:
// synchronize(_clh_init_cl)), src/model/solid_mechanics/materials/
// material_damage/material_marigo_inline_impl.hh (getNbData/packData/
// unpackData of `Yd`), src/synchronizer/synchronizer_registry.cc
// (synchronize(tag) over the registered synchronizers); akantu-rs
// scenarios (no direct upstream test): law-specific exchanges of the solid
// model on distributed meshes, shared by the local-world tests
// (`smm_parallel_laws.rs`) and the real-MPI binary (`mpi_smm.rs`).

//! Law-specific exchanges of the solid model on distributed meshes
//! (MPI-041 `_clh_init_cl`, MPI-053 tag liveness and user extension tags).
//!
//! Ghost values are checked against an **independent** channel: every rank
//! all-gathers `(barycentre, values)` of its not-ghost elements and each
//! ghost element looks its owner's values up by barycentre (the Kuhn cube's
//! barycentres are distinct), so an exchange that writes consistent but
//! wrong values is caught.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use akantu_comm::Communicator;
use akantu_core::enums::{AnalysisMethod, SynchronizationTag};
use akantu_core::{Element, ElementType, GhostType, Real, Result};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{InternalFieldMut, InternalFieldRef, LawSpec, ModelServices};
use akantu_parser::Parser;
use akantu_solid::{
    ForeignLaw, ForeignLawState, ForeignMaterial, ForeignMaterialMethods, Material,
    SolidMechanicsModel,
};

use super::par_smm::{self, Outcome, Part, Run, ok};

const TET: ElementType = ElementType::Tetrahedron4;

/// Distributed-vs-serial guard of the Marigo run and of the `degrade_mass`
/// lumped mass, relative to the largest serial value: the D-082 second-check
/// bound (the value of `par_phase_field::ENVELOPE` in the couplers' tests,
/// was 1e-12). A regression guard, not an error bound:
/// measured (local world, aarch64) Marigo 8.7e-19 of `max|u|` 6.3e-4
/// (1.4e-15 relative, ×7.3 below), lumped mass 6.9e-18 of `max m` 4.6e-2
/// (1.5e-16 relative, ×66 below); NUMERICAL_VALIDATION.
pub const VS_SERIAL: Real = 1e-14;

/// Marigo with a deterministic threshold (distributed vs serial).
pub const DAT_MARIGO: &str = "material marigo [\n  name = m\n  rho = 1\n  E = 1\n  nu = 0.3\n  \
    Yd = 2e-8\n  Sd = 1e-5\n]\n";

/// Marigo with a random threshold (`_clh_init_cl` carries the draws).
pub const DAT_MARIGO_RANDOM: &str = "material marigo [\n  name = m\n  rho = 1\n  E = 1\n  \
    nu = 0.3\n  Yd = 1e-7 uniform [0, 1e-7]\n  Sd = 1e-5\n]\n";

fn model_on(
    comm: &Communicator,
    n: usize,
    dat: &str,
    configure: impl FnOnce(&mut SolidMechanicsModel) -> std::result::Result<(), String>,
) -> std::result::Result<(SolidMechanicsModel, SharedMesh), String> {
    let m = par_smm::distributed_cube(comm, n, &Part::Rcb)?;
    let shared = SharedMesh::new(m);
    let parser = ok(Parser::parse_str(dat, "material.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(shared.clone(), None, "smm", parser),
        "model",
    )?;
    configure(&mut model)?;
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init_full",
    )?;
    Ok((model, shared))
}

/// Values of the `Real` internal `name` of law 0 per `(type, ghost)` block
/// (law-local order) and the law's element filter.
fn law_values(
    model: &SolidMechanicsModel,
    name: &str,
    ghost: GhostType,
) -> std::result::Result<(Vec<i32>, Vec<Real>, usize), String> {
    let law = ok(model.physics.materials.law(0), "law")?;
    let filter: Vec<i32> = law
        .base()
        .element_filter
        .get(TET, ghost)
        .map(|a| a.as_slice().to_vec())
        .unwrap_or_default();
    let mut vals: Option<std::result::Result<(Vec<Real>, usize), String>> = None;
    law.for_each_internal(&mut |n, f| {
        if n == name
            && vals.is_none()
            && let InternalFieldRef::Real(f) = f
        {
            // a block the law never allocated (no ghost element) is empty
            vals = Some(Ok(f
                .get(TET, ghost)
                .map(|a| (a.as_slice().to_vec(), a.nb_component()))
                .unwrap_or_default()));
        }
    });
    let (v, nbc) = vals.unwrap_or_else(|| Err(format!("no internal {name}")))?;
    let per = if filter.is_empty() {
        0
    } else {
        v.len() / filter.len()
    };
    let _ = nbc;
    Ok((filter, v, per))
}

/// Overwrite the `Real` internal `name` of law 0 on `ghost` with `f(element
/// barycentre, value index)`.
fn set_law_values(
    model: &mut SolidMechanicsModel,
    shared: &SharedMesh,
    name: &str,
    ghost: GhostType,
    f: impl Fn([Real; 3], usize) -> Real,
) -> Outcome {
    let mesh = ok(shared.read(), "read")?;
    let law = ok(model.physics.materials.law_mut(0), "law")?;
    let filter: Vec<i32> = law
        .base()
        .element_filter
        .get(TET, ghost)
        .map(|a| a.as_slice().to_vec())
        .unwrap_or_default();
    ok(
        law.for_each_internal_mut(&mut |n, fld| {
            let InternalFieldMut::Real(fld) = fld else {
                return Ok(());
            };
            if n != name || filter.is_empty() {
                return Ok(());
            }
            let mut a = fld.get_mut(TET, ghost)?;
            let per = if filter.is_empty() {
                0
            } else {
                a.len() * a.nb_component() / filter.len()
            };
            let s = a.as_mut_slice();
            for (l, &e) in filter.iter().enumerate() {
                let b = mesh.barycenter(Element::new(TET, e, ghost))?;
                for k in 0..per {
                    s[l * per + k] = f([b[0], b[1], b[2]], k);
                }
            }
            Ok(())
        }),
        "set values",
    )
}

type Key = [u64; 3];

fn key(b: &[Real]) -> Key {
    [b[0].to_bits(), b[1].to_bits(), b[2].to_bits()]
}

/// Every rank's not-ghost `(barycentre, values)` of the internal `name`,
/// all-gathered: barycentre → values of the owner.
fn owners_values(
    model: &SolidMechanicsModel,
    shared: &SharedMesh,
    name: &str,
) -> std::result::Result<BTreeMap<Key, Vec<Real>>, String> {
    let (filter, vals, per) = law_values(model, name, GhostType::NotGhost)?;
    let mesh = ok(shared.read(), "read")?;
    let mut mine: Vec<Real> = Vec::new();
    for (l, &e) in filter.iter().enumerate() {
        let b = ok(
            mesh.barycenter(Element::new(TET, e, GhostType::NotGhost)),
            "barycentre",
        )?;
        mine.extend_from_slice(&b[..3]);
        mine.extend_from_slice(&vals[l * per..(l + 1) * per]);
    }
    let comm = mesh.communicator().clone();
    let per_bytes = [u64::try_from(per).unwrap_or(0)];
    let pers = ok(comm.all_gather(&per_bytes), "all_gather per")?;
    let all = ok(comm.all_gather_v(&mine), "all_gather_v")?;
    let mut out = BTreeMap::new();
    for (r, chunk) in all.iter().enumerate() {
        let p = usize::try_from(pers[r]).unwrap_or(0);
        if p == 0 {
            continue;
        }
        for rec in chunk.chunks(3 + p) {
            out.insert(key(&rec[..3]), rec[3..].to_vec());
        }
    }
    Ok(out)
}

/// `(ghost elements, of which equal bitwise to their owner's values)`.
/// D-106 (2): a check of
/// the scenario whose outcome may differ between ranks is **agreed before
/// the next collective call** — every rank fails there, the failing ones
/// with their own message — instead of returning on the failing ranks
/// only, which would leave the others waiting in the next collective until
/// the deadline (and, under real MPI, break the communicator for every
/// later scenario of the binary). The pattern of
/// `node_removal_common::agreed` (`tests/node_removal_common/mod.rs`;
/// a copy per module until one shared helper replaces
/// them, D-107).
fn agreed(comm: &Communicator, local: Outcome) -> Outcome {
    agreed_value(comm, local)
}

/// [`agreed`] for a check that also yields a value.
fn agreed_value<T>(
    comm: &Communicator,
    local: std::result::Result<T, String>,
) -> std::result::Result<T, String> {
    let mut c = [u64::from(local.is_err())];
    ok(
        comm.all_reduce(&mut c, akantu_comm::ReduceOp::Sum),
        "agree a check of the scenario",
    )?;
    match local {
        Err(e) => Err(e),
        Ok(_) if c[0] > 0 => Err(format!("a check failed on {} other rank(s)", c[0])),
        Ok(v) => Ok(v),
    }
}

/// The sum over the ranks of a per-rank count (an expectation on "the
/// ghosts of the distributed mesh" holds for the whole mesh, not for every
/// rank: a rank may hold none at a high rank count; D-106 (2)).
fn summed(comm: &Communicator, local: usize) -> std::result::Result<u64, String> {
    let mut c = [u64::try_from(local).map_err(|e| e.to_string())?];
    ok(comm.all_reduce(&mut c, akantu_comm::ReduceOp::Sum), "sum")?;
    Ok(c[0])
}

/// [`ghosts_vs_owners`] agreed on every rank, with the agreed checks of
/// the scenarios: on every rank each ghost holds its owner's value, and the
/// distributed mesh has ghosts somewhere (a sum over the ranks).
fn agreed_ghosts_hold_owners(
    comm: &Communicator,
    model: &SolidMechanicsModel,
    shared: &SharedMesh,
    name: &str,
    what: &str,
) -> Outcome {
    let (nb_ghost, equal) = agreed_value(comm, ghosts_vs_owners(model, shared, name))?;
    agreed(
        comm,
        if equal == nb_ghost {
            Ok(())
        } else {
            Err(format!(
                "{what}: {equal}/{nb_ghost} ghost elements hold their owner's {name}"
            ))
        },
    )?;
    if comm.size() > 1 && summed(comm, nb_ghost)? == 0 {
        return Err(format!("{what}: no ghost element on a distributed cube"));
    }
    Ok(())
}

fn ghosts_vs_owners(
    model: &SolidMechanicsModel,
    shared: &SharedMesh,
    name: &str,
) -> std::result::Result<(usize, usize), String> {
    let owners = owners_values(model, shared, name)?;
    let (filter, vals, per) = law_values(model, name, GhostType::Ghost)?;
    let mesh = ok(shared.read(), "read")?;
    let mut equal = 0;
    for (l, &e) in filter.iter().enumerate() {
        let b = ok(
            mesh.barycenter(Element::new(TET, e, GhostType::Ghost)),
            "barycentre",
        )?;
        let want = owners
            .get(&key(&b))
            .ok_or_else(|| format!("ghost {e}: no owner with barycentre {b:?}"))?;
        let have = &vals[l * per..(l + 1) * per];
        if have.len() == want.len()
            && have
                .iter()
                .zip(want)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        {
            equal += 1;
        }
    }
    Ok((filter.len(), equal))
}

/// MPI-041: Marigo on a distributed cube (deterministic `Yd`), explicit
/// run with damage, against the serial run per global node within
/// [`VS_SERIAL`] (summation order at interface nodes).
pub fn marigo_distributed_vs_serial(comm: Communicator) -> Outcome {
    let rank = comm.rank();
    let size = comm.size();
    let m = par_smm::distributed_cube(&comm, 4, &Part::Rcb)?;
    let par = par_smm::run_with(m, Run::Explicit, 40, DAT_MARIGO)?;
    if rank != 0 {
        return Ok(());
    }
    let par = par.ok_or("no gathered result")?;
    let serial_mesh = par_smm::cube_mesh(Communicator::serial(), 4, true)?;
    let ser =
        par_smm::run_with(serial_mesh, Run::Explicit, 40, DAT_MARIGO)?.ok_or("no serial result")?;
    let (du, mu, eu) = par_smm::diff(&par.u, &ser.u);
    println!(
        "INFO marigo explicit ranks={size}: |du|max={du:.3e} (|u|max={mu:.3e}, {eu}/{} bitwise), \
         Ep {:.17e} vs {:.17e}",
        par.u.len(),
        par.potential,
        ser.potential
    );
    if du > VS_SERIAL * mu.max(Real::MIN_POSITIVE) {
        return Err(format!(
            "marigo at {size} ranks differs from serial: du {du:e}"
        ));
    }
    Ok(())
}

/// The serial Marigo run damages the cube (so the vs-serial scenario
/// exercises the damage branch).
pub fn marigo_run_damages(comm: Communicator) -> Outcome {
    if comm.rank() != 0 {
        return Ok(());
    }
    let m = par_smm::cube_mesh(Communicator::serial(), 4, true)?;
    let ser = par_smm::run_with(m, Run::Explicit, 40, DAT_MARIGO)?.ok_or("no serial result")?;
    let elastic_mesh = par_smm::cube_mesh(Communicator::serial(), 4, true)?;
    let el = par_smm::run_with(elastic_mesh, Run::Explicit, 40, par_smm::DAT)?
        .ok_or("no elastic result")?;
    let (du, _, _) = par_smm::diff(&ser.u, &el.u);
    if du == 0.0 {
        return Err("the Marigo run equals the elastic run: no damage".into());
    }
    Ok(())
}

/// MPI-041: `_clh_init_cl` at `init_full` gives every ghost element its
/// owner's random `Yd` (independent check by barycentre); a second
/// exchange changes nothing. With
/// `SmmUpstreamCompat::clh_init_cl_not_exchanged` (UD-900, upstream) the
/// ghosts keep the local draws, `synchronize(_clh_init_cl)` is refused as
/// an unregistered tag (UD-553), a no-op with `unregistered_synchronization_tag_is_a_noop`,
/// and exchanges the owners' values once the switch is off again.
pub fn clh_init_cl_exchange(comm: Communicator) -> Outcome {
    let size = comm.size();
    // corrected default
    let (mut model, shared) = model_on(&comm, 3, DAT_MARIGO_RANDOM, |_| Ok(()))?;
    // D-106 (2): agreed checks; the ghosts are counted over the whole mesh
    agreed_ghosts_hold_owners(&comm, &model, &shared, "Yd", "default")?;
    let before = agreed_value(&comm, law_values(&model, "Yd", GhostType::Ghost))?.1;
    let before_ng = agreed_value(&comm, law_values(&model, "Yd", GhostType::NotGhost))?.1;
    ok(
        model.synchronize(SynchronizationTag::ClhInitCl),
        "synchronize",
    )?;
    let unchanged = (|| {
        let after = law_values(&model, "Yd", GhostType::Ghost)?.1;
        let after_ng = law_values(&model, "Yd", GhostType::NotGhost)?.1;
        if !akantu_validation::exact::exact_slices_eq(&before, &after)
            || !akantu_validation::exact::exact_slices_eq(&before_ng, &after_ng)
        {
            return Err("a second _clh_init_cl exchange changed Yd".to_string());
        }
        Ok(())
    })();
    agreed(&comm, unchanged)?;
    // upstream: nothing exchanged
    let (mut model, shared) = model_on(&comm, 3, DAT_MARIGO_RANDOM, |m| {
        m.physics.upstream_compat.clh_init_cl_not_exchanged = true;
        Ok(())
    })?;
    let (nb_ghost, equal) = agreed_value(&comm, ghosts_vs_owners(&model, &shared, "Yd"))?;
    let mut any_differ = [u64::from(nb_ghost > 0 && equal < nb_ghost)];
    ok(
        comm.all_reduce(&mut any_differ, akantu_comm::ReduceOp::Sum),
        "all_reduce",
    )?;
    if size > 1 && any_differ[0] == 0 {
        return Err("compat: every ghost Yd equals its owner's draw (no local draws?)".into());
    }
    let local = agreed_value(&comm, law_values(&model, "Yd", GhostType::Ghost))?.1;
    let r = model.synchronize(SynchronizationTag::ClhInitCl);
    agreed(
        &comm,
        match &r {
            Err(e) if e.to_string().contains("not registered") => Ok(()),
            _ => Err(format!(
                "compat: synchronize(_clh_init_cl) at {size} ranks: {r:?}"
            )),
        },
    )?;
    model
        .physics
        .upstream_compat
        .unregistered_synchronization_tag_is_a_noop = true;
    ok(model.synchronize(SynchronizationTag::ClhInitCl), "no-op")?;
    let noop = law_values(&model, "Yd", GhostType::Ghost).and_then(|(_, after, _)| {
        if akantu_validation::exact::exact_slices_eq(&local, &after) {
            Ok(())
        } else {
            Err("the UD-553 no-op changed the ghost Yd".to_string())
        }
    });
    agreed(&comm, noop)?;
    model.physics.upstream_compat = akantu_solid::SmmUpstreamCompat::default();
    ok(model.synchronize(SynchronizationTag::ClhInitCl), "exchange")?;
    agreed_ghosts_hold_owners(&comm, &model, &shared, "Yd", "explicit exchange")
}

/// A user law whose internal `tag_data` is exchanged for `_user_1`.
#[derive(Debug, Default)]
struct TaggedElastic;

/// Value of `tag_data` written by the owner (barycentre, index).
fn tag_value(b: [Real; 3], k: usize) -> Real {
    1.0 + b[0] + 10.0 * b[1] + 100.0 * b[2] + 0.5 * k as Real
}

impl ForeignMaterialMethods for TaggedElastic {
    fn compute_stress(
        &mut self,
        law: &mut ForeignLaw<'_, '_>,
        ty: ElementType,
        g: GhostType,
    ) -> Result<()> {
        law.state
            .base
            .stress
            .get_mut(ty, g)?
            .as_mut_slice()
            .fill(0.0);
        Ok(())
    }
    fn push_wave_speed(&self, _: &ForeignLawState, _: Element) -> Result<Real> {
        Ok(1.0)
    }
}

const DAT_TAGGED: &str = "material tagged [\n  name = t\n  rho = 1\n]\n";

/// MPI-053: user extension tags. `synchronize(_user_1)` is refused before
/// the user registers the tag (UD-553), registering a model or reserved tag
/// is refused, and once `_user_1` is registered the user law's `tag_data`
/// travels owner → ghost (independent check by barycentre); `_user_2`,
/// registered by nobody, stays refused.
pub fn extension_tag_dispatch(comm: Communicator) -> Outcome {
    let size = comm.size();
    let (mut model, shared) = model_on(&comm, 3, DAT_TAGGED, |m| {
        ok(
            m.physics.factory.register(
                "tagged",
                Arc::new(
                    |spec: &LawSpec, _: &dyn ModelServices| -> Result<Box<dyn Material>> {
                        let mut m = ForeignMaterial::new(spec, "tagged", Box::new(TaggedElastic))?;
                        m.register_internal_real("tag_data", 2)?;
                        m.state
                            .exchanged
                            .push((SynchronizationTag::User1, "tag_data".into()));
                        Ok(Box::new(m))
                    },
                ),
            ),
            "register law",
        )
    })?;
    set_law_values(
        &mut model,
        &shared,
        "tag_data",
        GhostType::NotGhost,
        tag_value,
    )?;
    set_law_values(&mut model, &shared, "tag_data", GhostType::Ghost, |_, _| {
        Real::NAN
    })?;
    // the mesh carries an element synchronizer at every rank count (one
    // rank: empty schemes), so the unregistered tag is refused everywhere
    if model.synchronize(SynchronizationTag::User1).is_ok() {
        return Err("synchronize(_user_1) before its registration succeeded".into());
    }
    for t in [
        SynchronizationTag::SmmStress,
        SynchronizationTag::Update,
        SynchronizationTag::HtmPhi,
        SynchronizationTag::SolverSolution,
    ] {
        if model.register_synchronization_tag(t).is_ok() {
            return Err(format!("register_synchronization_tag({t}) accepted"));
        }
    }
    ok(
        model.register_synchronization_tag(SynchronizationTag::User1),
        "register _user_1",
    )?;
    ok(
        model.register_synchronization_tag(SynchronizationTag::User1),
        "register _user_1 twice",
    )?;
    ok(
        model.synchronize(SynchronizationTag::User1),
        "synchronize _user_1",
    )?;
    // D-106 (2): agreed, the ghosts counted over the whole mesh
    agreed_ghosts_hold_owners(&comm, &model, &shared, "tag_data", "_user_1")?;
    if model.synchronize(SynchronizationTag::User2).is_ok() {
        return Err("synchronize(_user_2), never registered, succeeded".into());
    }
    // `_user_2` registered on the last rank only — the
    // registration is agreed first, so every rank refuses with the same
    // error, promptly (upstream: the registered rank waits in the exchange),
    // and the communicator stays usable; registered everywhere, it runs
    if size > 1 {
        if comm.rank() == size as i32 - 1 {
            ok(
                model.register_synchronization_tag(SynchronizationTag::User2),
                "register _user_2 on the last rank",
            )?;
        }
        let t0 = std::time::Instant::now();
        let r = model.synchronize(SynchronizationTag::User2);
        let want = format!("registered on 1 of {size} ranks");
        match &r {
            Err(e)
                if e.to_string().contains(&want)
                    && !matches!(e, akantu_core::Error::Fatal { .. }) => {}
            _ => return Err(format!("partial registration of _user_2: {r:?}")),
        }
        if t0.elapsed() > std::time::Duration::from_secs(5) {
            return Err(format!(
                "partial registration refused after {:?}",
                t0.elapsed()
            ));
        }
        let mut x = [1u64];
        ok(
            comm.all_reduce(&mut x, akantu_comm::ReduceOp::Sum),
            "all_reduce after the refusal",
        )?;
        ok(
            model.register_synchronization_tag(SynchronizationTag::User2),
            "register _user_2 everywhere",
        )?;
        ok(
            model.synchronize(SynchronizationTag::User2),
            "synchronize _user_2 once registered everywhere",
        )?;
    }
    Ok(())
}

/// A law declaring internals for a tag that carries no
/// law data (`_smm_stress`, a generic exchange of the residual; `_diffusion`,
/// a tag the solid model does not handle) is refused at `init_materials`,
/// on every rank and on a serial mesh alike, instead of seeing its data
/// silently dropped.
pub fn law_data_for_a_generic_tag_is_refused(comm: Communicator) -> Outcome {
    // no early return: the next case builds a distributed model (collective)
    let mut failure: Option<String> = None;
    for tag in [SynchronizationTag::SmmStress, SynchronizationTag::Diffusion] {
        for distributed in [true, false] {
            let c = if distributed {
                comm.clone()
            } else {
                Communicator::serial()
            };
            if !distributed && comm.rank() != 0 {
                continue;
            }
            let r = model_on(&c, 2, DAT_TAGGED, |m| {
                ok(
                    m.physics.factory.register(
                        "tagged",
                        Arc::new(
                            move |spec: &LawSpec,
                                  _: &dyn ModelServices|
                                  -> Result<Box<dyn Material>> {
                                let mut m =
                                    ForeignMaterial::new(spec, "tagged", Box::new(TaggedElastic))?;
                                m.register_internal_real("tag_data", 2)?;
                                m.state.exchanged.push((tag, "tag_data".into()));
                                Ok(Box::new(m))
                            },
                        ),
                    ),
                    "register law",
                )
            });
            match &r {
                Err(e) if e.contains("law-specific data for the model tag") => {}
                Err(e) => {
                    failure.get_or_insert(format!("{tag} (distributed {distributed}): {e}"));
                }
                Ok(_) => {
                    failure.get_or_insert(format!(
                        "{tag} (distributed {distributed}): law data for the tag accepted"
                    ));
                }
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

/// Check a collective failure caused by the last rank: every rank failed
/// (the last one with an error containing `own`, the others with "failed
/// on another rank" or their own exchange error), `fatal` = agreed fatal,
/// promptly, the communicator still usable.
fn check_agreed_failure(
    comm: &Communicator,
    r: Result<()>,
    own: &str,
    fatal: bool,
    elapsed: std::time::Duration,
) -> Outcome {
    let last = comm.size() as i32 - 1;
    let local = (|| {
        let e = match r {
            Ok(()) => return Err("the operation must fail on every rank".to_string()),
            Err(e) => e,
        };
        let msg = e.to_string();
        let is_fatal = matches!(e, akantu_core::Error::Fatal { .. });
        if is_fatal != fatal {
            return Err(format!("fatal = {fatal} expected, got: {msg}"));
        }
        if is_fatal && e.fatal_kind() != Some(akantu_core::FatalKind::Agreed) {
            return Err(format!("an agreed fatal error expected, got: {msg}"));
        }
        let want = if comm.rank() == last {
            own
        } else {
            "failed on another rank"
        };
        if !msg.contains(want) {
            return Err(format!("rank {}: '{want}' expected in: {msg}", comm.rank()));
        }
        if elapsed > std::time::Duration::from_secs(5) {
            return Err(format!("returned after {elapsed:?}"));
        }
        Ok(())
    })();
    // the communicator is still usable (every rank reaches this reduction)
    let mut x = [1u64];
    ok(
        comm.all_reduce(&mut x, akantu_comm::ReduceOp::Sum),
        "all_reduce after the failure",
    )?;
    if x[0] != comm.size() as u64 {
        return Err(format!("all_reduce after the failure: {}", x[0]));
    }
    local
}

/// Failure injection: the `_clh_init_cl` exchange of
/// `init_full` fails on the last rank only (its user law names a missing
/// internal for the tag: its message size is unknown, the preflight refuses
/// it before any message) → agreed
/// **fatal** on every rank (the laws hold their elements, as the
/// `_constitutive_law_id` failure), promptly.
pub fn clh_init_cl_failure_is_agreed_fatal(comm: Communicator) -> Outcome {
    let last = comm.size() as i32 - 1;
    let m = par_smm::distributed_cube(&comm, 2, &Part::Rcb)?;
    let shared = SharedMesh::new(m);
    let parser = ok(Parser::parse_str(DAT_TAGGED, "material.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(shared, None, "smm", parser),
        "model",
    )?;
    let bad = comm.rank() == last;
    ok(
        model.physics.factory.register(
            "tagged",
            Arc::new(
                move |spec: &LawSpec, _: &dyn ModelServices| -> Result<Box<dyn Material>> {
                    let mut m = ForeignMaterial::new(spec, "tagged", Box::new(TaggedElastic))?;
                    m.register_internal_real("tag_data", 2)?;
                    if bad {
                        m.state
                            .exchanged
                            .push((SynchronizationTag::ClhInitCl, "missing".into()));
                    }
                    Ok(Box::new(m))
                },
            ),
        ),
        "register law",
    )?;
    let t0 = std::time::Instant::now();
    let r = model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass));
    // the accessor's size of the bad law is unknown (usize::MAX): the
    // synchronizer's preflight refuses the message for `clh_init_cl` before
    // any communication
    check_agreed_failure(&comm, r, "clh_init_cl", true, t0.elapsed())
}

/// Phase-field solid law with `degrade_mass` (MPI-036).
pub const DAT_PF_SOLID: &str = "material phasefield [\n  name = plate\n  rho = 2.\n  E = 210.0\n  \
    nu = 0.3\n  eta = 0.01\n  degrade_mass = true\n]\n";

/// Damage written on the not-ghost points (barycentre, point index).
fn pf_damage(b: [Real; 3], k: usize) -> Real {
    (0.1 + 0.3 * b[0] + 0.2 * b[1] * b[2] + 0.01 * k as Real).min(0.9)
}

fn lumped_mass_with_damage(
    comm: &Communicator,
    distributed: bool,
) -> std::result::Result<(Option<Vec<Real>>, SolidMechanicsModel, SharedMesh), String> {
    let (mut model, shared) = if distributed {
        model_on(comm, 3, DAT_PF_SOLID, |_| Ok(()))?
    } else {
        model_on(&Communicator::serial(), 3, DAT_PF_SOLID, |_| Ok(()))?
    };
    set_law_values(
        &mut model,
        &shared,
        "damage",
        GhostType::NotGhost,
        pf_damage,
    )?;
    set_law_values(&mut model, &shared, "damage", GhostType::Ghost, |_, _| 0.0)?;
    ok(model.assemble_mass_lumped(), "assemble_mass_lumped")?;
    let mass = ok(model.mass(), "mass")?.as_slice().to_vec();
    let m = ok(shared.read(), "read")?;
    let parallel = m.synchronizers().node.is_some() && m.communicator().size() > 1;
    let g = if parallel {
        let ns = m.synchronizers().node.as_ref().ok_or("no node sync")?;
        let e = akantu_mesh::synchronizers::NodeEntities { mesh: &m };
        ok(ns.gather(&e, &mass, 3), "gather")?
    } else if m.communicator().rank() == 0 {
        let mut g = vec![0.0; mass.len()];
        for n in 0..m.nb_nodes() {
            let gid = usize::try_from(m.node_global_id(n as akantu_core::Idx)).unwrap_or(0);
            g[3 * gid..3 * gid + 3].copy_from_slice(&mass[3 * n..3 * n + 3]);
        }
        Some(g)
    } else {
        None
    };
    drop(m);
    Ok((g, model, shared))
}

/// MPI-036: the lumped mass of the phase-field solid law with
/// `degrade_mass` on a distributed cube equals the serial one per global
/// node within [`VS_SERIAL`]: the ghost elements' contributions use their
/// owner's damage (`_smm_density`; without it they would use the ghost
/// default 0).
pub fn phasefield_degrade_mass_lumped_vs_serial(comm: Communicator) -> Outcome {
    let (par, _, _) = lumped_mass_with_damage(&comm, true)?;
    if comm.rank() != 0 {
        return Ok(());
    }
    let par = par.ok_or("no gathered mass")?;
    let (ser, _, _) = lumped_mass_with_damage(&comm, false)?;
    let ser = ser.ok_or("no serial mass")?;
    let (d, mag, eq) = par_smm::diff(&par, &ser);
    println!(
        "INFO degrade_mass lumped mass ranks={}: |dm|max={d:.3e} (|m|max={mag:.3e}, {eq}/{} bitwise)",
        comm.size(),
        par.len()
    );
    if d > VS_SERIAL * mag {
        return Err(format!(
            "lumped mass differs from serial: {d:e} (|m| {mag:e})"
        ));
    }
    Ok(())
}

/// MPI-036 (mutant 4 of a mutation check): `_smm_density` of the
/// phase-field solid law with `degrade_mass` carries `damage` **and**
/// `grad_u` (upstream `MaterialPhaseField::packData`,
/// `material_phasefield_inline_impl.hh:137-186`): with distinct values on
/// the not-ghost points and NaN on the ghost points, both
/// `synchronize(_smm_density)` and the lumped-mass assembly (which runs the
/// exchange before lumping) give every ghost element its owner's `damage`
/// and `grad_u`, bit for bit (independent check by barycentre).
pub fn smm_density_carries_damage_and_grad_u(comm: Communicator) -> Outcome {
    let (mut model, shared) = model_on(&comm, 3, DAT_PF_SOLID, |_| Ok(()))?;
    // no early return between the collectives (owners_values all-gathers):
    // the first failure is kept and returned at the end
    let mut failure: Option<String> = None;
    let grad = |b: [Real; 3], k: usize| 1e-3 * (k as Real + 1.0) * (b[0] + 2.0 * b[1] - b[2]);
    for path in ["synchronize", "assemble_mass_lumped"] {
        set_law_values(
            &mut model,
            &shared,
            "damage",
            GhostType::NotGhost,
            pf_damage,
        )?;
        set_law_values(&mut model, &shared, "grad_u", GhostType::NotGhost, grad)?;
        for name in ["damage", "grad_u"] {
            set_law_values(&mut model, &shared, name, GhostType::Ghost, |_, _| {
                Real::NAN
            })?;
        }
        if path == "synchronize" {
            ok(model.synchronize(SynchronizationTag::SmmDensity), path)?;
        } else {
            model.force_mass_reassembly();
            ok(model.assemble_mass_lumped(), path)?;
        }
        for name in ["damage", "grad_u"] {
            // D-106 (2): agreed, the ghosts counted over the whole mesh
            if let Err(e) = agreed_ghosts_hold_owners(&comm, &model, &shared, name, path) {
                failure.get_or_insert(e);
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

/// Failure injection: the `_smm_density` exchange of the
/// lumped-mass assembly fails on the last rank only (a law-local index of
/// one of its sent elements is out of range: the packing fails before any
/// message) → the assembly fails on every rank (agreed, recoverable),
/// promptly, and nothing changed on any rank (mass, reassembly needed).
pub fn smm_density_failure_is_agreed(comm: Communicator) -> Outcome {
    if comm.size() < 2 {
        return Ok(());
    }
    let (_, mut model, shared) = lumped_mass_with_damage(&comm, true)?;
    let mass0 = ok(model.mass(), "mass")?.as_slice().to_vec();
    let last = comm.size() as i32 - 1;
    if comm.rank() == last {
        let mesh = ok(shared.read(), "read")?;
        let es = mesh
            .synchronizers()
            .element
            .as_ref()
            .ok_or("no element sync")?;
        let e = es
            .communications
            .send
            .values()
            .flatten()
            .next()
            .copied()
            .ok_or("the last rank sends no element")?;
        drop(mesh);
        ok(
            model
                .physics
                .materials
                .local_numbering_map_mut()
                .element_mut(e),
            "local numbering",
        )?[0] = 1_000_000;
    }
    model.force_mass_reassembly();
    let t0 = std::time::Instant::now();
    let r = model.assemble_mass_lumped();
    check_agreed_failure(&comm, r, "has no values", false, t0.elapsed())?;
    if !akantu_validation::exact::exact_slices_eq(&mass0, ok(model.mass(), "mass")?.as_slice()) {
        return Err("the failed assembly changed the lumped mass".into());
    }
    Ok(())
}

/// Serialises the tests that draw from the process-global random generator
/// (`Yd = … uniform …`): concurrent tests of one test binary would
/// interleave their draws. Held around every `run_local` of such a test.
pub static RNG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fixture(
    scenario: &str,
    np: usize,
    rank: usize,
) -> std::result::Result<akantu_validation::Fixture, String> {
    akantu_validation::Fixture::open_by_id(&format!("parallel/{scenario}/np{np}/rank{rank}"))
        .map_err(|e| format!("fixture {scenario} np{np} rank{rank}: {e}"))
}

fn bits_eq(have: &[Real], want: &[Real]) -> bool {
    akantu_validation::exact::exact_slices_eq(want, have)
}

/// MPI-041 against the oracle (`fixtures/parallel/clh_marigo_cube_n3`,
/// probe v5, 3 identical pinned runs): Marigo with the random threshold
/// `Yd = 1e-7 uniform [0, 1e-7]` on the oracle's Scotch partition, the
/// generator of rank r seeded with `4242 (r + 1)` as the probe.
///
/// * With `SmmUpstreamCompat::clh_init_cl_not_exchanged` (upstream: no
///   synchronizer for `_clh_init_cl`, UD-900): law filters (ghost in
///   canonical rank order), `Yd` of both ghost types (the ghost values are
///   the rank's own draws), the time step, and after 20 explicit steps the
///   displacement, velocity and damage — all bitwise.
/// * Default (exchanged): every ghost element's `Yd` equals the owner's
///   `Yd` recorded by the oracle **on the owner's rank** (receive scheme
///   entry k from p ↔ entry k of p's send scheme to this rank), and the
///   run is bitwise the same (no stress reads a ghost `Yd`).
///
/// Every rank must have its own generator: one process per rank (real
/// MPI), or one rank in the local world.
pub fn clh_marigo_matches_oracle(comm: Communicator) -> Outcome {
    let scenario = "clh_marigo_cube_n3";
    let (np, rank) = (comm.size(), comm.rank() as usize);
    let fx = fixture(scenario, np, rank)?;
    let lf = |name: &str| {
        fx.load_f64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    let li = |name: &str| {
        fx.load_i64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    for upstream in [true, false] {
        let part = if np > 1 {
            Part::Prescribed(if rank == 0 {
                super::par_common::oracle_partition(scenario, np)?
            } else {
                Vec::new()
            })
        } else {
            Part::Rcb
        };
        let m = par_smm::distributed_cube(&comm, 3, &part)?;
        // D-106 (2): every rank-local comparison is agreed before the next
        // collective (the model's construction, the steps, the next cube)
        agreed(&comm, super::par_common::compare_with_oracle(&m, &fx))?;
        let shared = SharedMesh::new(m);
        let parser = ok(
            Parser::parse_str(DAT_MARIGO_RANDOM, "material.dat"),
            "parser",
        )?;
        akantu_core::random::RandomGenerator::<akantu_core::random::IdxSlot>::seed(
            4242 * (comm.rank() as i64 + 1),
        );
        let mut model = ok(
            SolidMechanicsModel::with_parser(shared.clone(), None, "smm", parser),
            "model",
        )?;
        model.physics.upstream_compat.clh_init_cl_not_exchanged = upstream;
        ok(
            model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
            "init_full",
        )?;
        let initial = (|| {
            for ghost in GhostType::ALL {
                let g = if ghost == GhostType::Ghost {
                    "ghost"
                } else {
                    "not_ghost"
                };
                let (filter, yd, _) = law_values(&model, "Yd", ghost)?;
                let want_f = li(&format!("law0/filter/_tetrahedron_4/{g}")).unwrap_or_default();
                let have_f: Vec<i64> = filter.iter().map(|&v| i64::from(v)).collect();
                if have_f != want_f {
                    return Err(format!("law 0 {g} filter differs from the oracle"));
                }
                if upstream || ghost == GhostType::NotGhost {
                    let want = lf(&format!("law0/Yd/_tetrahedron_4/{g}")).unwrap_or_default();
                    if !bits_eq(&yd, &want) {
                        return Err(format!(
                            "upstream = {upstream}: Yd {g} differs from the oracle"
                        ));
                    }
                }
            }
            Ok(())
        })();
        agreed(&comm, initial)?;
        if !upstream {
            // owners' values from their own oracle records; the number of
            // ghosts checked is summed over the ranks (D-106 (2): a rank may
            // hold no ghost)
            let checked_here = (|| -> std::result::Result<usize, String> {
                let mesh = ok(shared.read(), "read")?;
                let es = mesh
                    .synchronizers()
                    .element
                    .as_ref()
                    .ok_or("no element sync")?;
                let (gfilter, gyd, per) = law_values(&model, "Yd", GhostType::Ghost)?;
                let mut checked = 0usize;
                for (p, scheme) in &es.communications.recv {
                    let p = usize::try_from(*p).map_err(|e| e.to_string())?;
                    let owner = fixture(scenario, np, p)?;
                    let send = owner
                        .load_i64(&format!("element_scheme/send/{rank}"))
                        .map_err(|e| e.to_string())?
                        .data;
                    let oyd = owner
                        .load_f64("law0/Yd/_tetrahedron_4/not_ghost")
                        .map_err(|e| e.to_string())?
                        .data;
                    let ofilter = owner
                        .load_i64("law0/filter/_tetrahedron_4/not_ghost")
                        .map_err(|e| e.to_string())?
                        .data;
                    for (k, e) in scheme.iter().enumerate() {
                        let oe = send.get(3 * k + 1).copied().ok_or("short send scheme")?;
                        let ol = ofilter
                            .iter()
                            .position(|&x| x == oe)
                            .ok_or("owner element not in law 0")?;
                        let gl = gfilter
                            .iter()
                            .position(|&x| x == e.index())
                            .ok_or("ghost element not in law 0")?;
                        if !bits_eq(
                            &gyd[gl * per..(gl + 1) * per],
                            &oyd[ol * per..(ol + 1) * per],
                        ) {
                            return Err(format!("ghost {e}: Yd is not the owner's (rank {p})"));
                        }
                        checked += 1;
                    }
                }
                Ok(checked)
            })();
            let checked = agreed_value(&comm, checked_here)?;
            if np > 1 && summed(&comm, checked)? == 0 {
                return Err("no ghost Yd checked on any rank".into());
            }
        }
        // the run (upstream: setup_cube_bc with the bending mode)
        let (coords, nb) = {
            let mm = ok(shared.read(), "read")?;
            (mm.nodes().as_slice().to_vec(), mm.nb_nodes())
        };
        {
            let mut b = ok(model.blocked_dofs_mut(), "blocked")?;
            let b = b.as_mut_slice();
            for n in 0..nb {
                if coords[3 * n + 2] == 0.0 {
                    b[3 * n] = true;
                    b[3 * n + 1] = true;
                    b[3 * n + 2] = true;
                }
            }
        }
        {
            let mut u = ok(model.displacement_mut(), "u")?;
            let u = u.as_mut_slice();
            for n in 0..nb {
                let (x, y, z) = (coords[3 * n], coords[3 * n + 1], coords[3 * n + 2]);
                u[3 * n] = 1e-3 * z * z * (1.0 + 0.3 * y);
                u[3 * n + 1] = 5e-4 * z * (0.5 - x);
            }
        }
        let dt = 0.8 * ok(model.stable_time_step(), "stable")?;
        if !bits_eq(&[dt], &lf("model/time_step")?) {
            return Err(format!("time step {dt:e} differs from the oracle"));
        }
        ok(model.set_time_step(dt, None), "time step")?;
        for s in 0..20 {
            ok(model.solve_step(None), &format!("step {s}"))?;
        }
        let last = (|| {
            if !bits_eq(
                ok(model.displacement(), "u")?.as_slice(),
                &lf("model/displacement")?,
            ) {
                return Err(format!(
                    "upstream = {upstream}: displacement differs from the oracle"
                ));
            }
            if !bits_eq(
                ok(model.velocity(), "v")?.as_slice(),
                &lf("model/velocity")?,
            ) {
                return Err(format!(
                    "upstream = {upstream}: velocity differs from the oracle"
                ));
            }
            for (ghost, g) in [
                (GhostType::NotGhost, "not_ghost"),
                (GhostType::Ghost, "ghost"),
            ] {
                let d = law_values(&model, "damage", ghost)?.1;
                let want = lf(&format!("law0/damage/_tetrahedron_4/{g}")).unwrap_or_default();
                if !bits_eq(&d, &want) {
                    return Err(format!(
                        "upstream = {upstream}: damage {g} differs from the oracle"
                    ));
                }
            }
            Ok(())
        })();
        // before the next iteration's cube (collective)
        agreed(&comm, last)?;
    }
    Ok(())
}
