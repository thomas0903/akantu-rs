// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/ntn_contact/{ntn_initiation_function.hh, ntn_initiation_function.cc},
// the registrations of friction_laws/ntn_friclaw_rate_and_state.hh and
// friction_laws/ntn_friclaw_rate_and_state_flashheating.hh.

//! Friction factory (upstream `NTNFactory`, `registerFriction`,
//! `initializeNTNFriction`; TASN-008, TASN-031…033).
//!
//! Registered law names: `coulomb`, `linear_slip_weakening`,
//! `linear_slip_weakening_no_healing`, `linear_cohesive`,
//! `rate_and_state:<coefficient>:<state>` for the five coefficients
//! ([`RsCoefficient`]) and three state laws ([`RsState`]), the legacy keys
//! `rate_and_state_n_shaped_{ageing,slip,ageing_regularized}`, and
//! `rate_and_state:flashheating`. The friction type follows the contact
//! (`NTNFriction` for a node-to-node contact, `NTRFFriction` otherwise).
//!
//! **UD-039 (B, selector):** upstream allocates the Rubin–Ampuero
//! regularisation for any option other than `simplified_prakash_clifton`,
//! including the default `no_regularisation` and misspellings. The port
//! honours the option and rejects unknown names; with
//! [`crate::TasnUpstreamCompat::regularisation_fallback_rubin_ampuero`] it
//! reproduces the fallback (results agree with `no_regularisation` while
//! `t_star = 0`, its default, but the parameter `t_star` is then accepted and
//! restarted).
//!
//! **UD-399 (C):** the overload `initializeNTNFriction(contact, law, reg)` is
//! declared upstream but never defined (link error); the port provides it as
//! [`initialize_ntn_friction_with`].

use akantu_core::{Error, Result};
use akantu_parser::{Parser, ParserSection, ParserType};

use crate::contact::NtnBaseContact;
use crate::friction::{FrictionLaw, NtnFriction, Regularisation, RsCoefficient, RsState};

/// Identifier of the friction objects created by the factory (upstream
/// default id of `NTNFriction` / `NTRFFriction`).
pub const FRICTION_ID: &str = "friction";

/// Every registered law name, in upstream registration order.
#[must_use]
pub fn registered_friction_laws() -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for c in RsCoefficient::ALL {
        for s in RsState::ALL {
            v.push(format!("rate_and_state:{}:{}", c.name(), s.name()));
        }
    }
    for s in RsState::ALL {
        v.push(format!("rate_and_state_n_shaped_{}", s.name()));
    }
    v.push("rate_and_state:flashheating".into());
    v.extend(
        [
            "coulomb",
            "linear_slip_weakening",
            "linear_slip_weakening_no_healing",
            "linear_cohesive",
        ]
        .map(String::from),
    );
    v
}

/// The law of a registered name (`None` if not registered).
#[must_use]
pub fn friction_law_from_name(name: &str) -> Option<FrictionLaw> {
    match name {
        "coulomb" => return Some(FrictionLaw::Coulomb),
        "linear_slip_weakening" => return Some(FrictionLaw::LinearSlipWeakening),
        "linear_slip_weakening_no_healing" => {
            return Some(FrictionLaw::LinearSlipWeakeningNoHealing);
        }
        "linear_cohesive" => return Some(FrictionLaw::LinearCohesive),
        "rate_and_state:flashheating" => return Some(FrictionLaw::RateAndStateFlashHeating),
        _ => {}
    }
    // legacy keys (TASN-031…033)
    if let Some(rest) = name.strip_prefix("rate_and_state_n_shaped_") {
        let state = RsState::ALL.into_iter().find(|s| s.name() == rest)?;
        return Some(FrictionLaw::RateAndState {
            coefficient: RsCoefficient::NShaped,
            state,
        });
    }
    let rest = name.strip_prefix("rate_and_state:")?;
    let (c, s) = rest.split_once(':')?;
    let coefficient = RsCoefficient::ALL.into_iter().find(|x| x.name() == c)?;
    let state = RsState::ALL.into_iter().find(|x| x.name() == s)?;
    Some(FrictionLaw::RateAndState { coefficient, state })
}

/// The regularisation of a factory option (UD-039, see the module docs).
///
/// # Errors
/// Unknown option (corrected mode).
pub fn regularisation_from_option(option: &str, upstream_fallback: bool) -> Result<Regularisation> {
    match option {
        "rubin_ampuero" => Ok(Regularisation::RubinAmpuero),
        "simplified_prakash_clifton" => Ok(Regularisation::SimplifiedPrakashClifton),
        _ if upstream_fallback => Ok(Regularisation::RubinAmpuero),
        "no_regularisation" => Ok(Regularisation::NoRegularisation),
        other => Err(Error::invalid(format!(
            "unknown friction regularisation '{other}' (no_regularisation, rubin_ampuero, \
             simplified_prakash_clifton; UD-039)"
        ))),
    }
}

/// Upstream `NTNFactory::allocate(law, regularisation, contact)`: the
/// friction object for a law name and regularisation option, with
/// default-valued parameters. `nb_nodes`: number of mesh nodes.
///
/// # Errors
/// Unknown law or regularisation.
pub fn allocate_friction(
    contact: &NtnBaseContact,
    nb_nodes: usize,
    friction_law: &str,
    friction_reg: &str,
) -> Result<NtnFriction> {
    let law = friction_law_from_name(friction_law).ok_or_else(|| {
        Error::not_found(
            "friction law",
            format!(
                "{friction_law} (registered: {})",
                registered_friction_laws().join(", ")
            ),
        )
    })?;
    let reg = regularisation_from_option(
        friction_reg,
        contact.compat().regularisation_fallback_rubin_ampuero,
    )?;
    NtnFriction::new(contact, nb_nodes, law, reg, FRICTION_ID)
}

/// Upstream `initializeNTNFriction(contact)`: the **first** `friction`
/// section of the input file (`friction <law> [<regularisation>] [ … ]`,
/// regularisation `no_regularisation` by default) allocates and
/// parameterises the friction; further friction sections are ignored with a
/// warning.
///
/// # Errors
/// No friction section (upstream dereferences an empty range), unknown law
/// or regularisation, invalid parameters.
pub fn initialize_ntn_friction(
    contact: &NtnBaseContact,
    nb_nodes: usize,
    parser: &Parser,
) -> Result<NtnFriction> {
    let mut sections = parser.root.sub_sections_of(ParserType::Friction);
    let section: &ParserSection = sections.next().ok_or_else(|| {
        Error::not_found("friction section", "the input file has no friction section")
    })?;
    let law = section.name.clone();
    let reg = section.option_or("no_regularisation").to_owned();
    let mut friction = allocate_friction(contact, nb_nodes, &law, &reg)?;
    friction.parse_friction_section(section, parser.is_permissive())?;
    if sections.next().is_some() {
        log::warn!(
            "There were several friction sections in input file. Only first one was used and all others ignored."
        );
    }
    Ok(friction)
}

/// The upstream-declared overload `initializeNTNFriction(contact,
/// friction_law, friction_reg)` (UD-399): allocation without parsing.
///
/// # Errors
/// Unknown law or regularisation.
pub fn initialize_ntn_friction_with(
    contact: &NtnBaseContact,
    nb_nodes: usize,
    friction_law: &str,
    friction_reg: &str,
) -> Result<NtnFriction> {
    allocate_friction(contact, nb_nodes, friction_law, friction_reg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_name_resolves() {
        let names = registered_friction_laws();
        assert_eq!(names.len(), 15 + 3 + 1 + 4);
        let mut laws = std::collections::HashSet::new();
        for n in &names {
            let l = friction_law_from_name(n).unwrap_or_else(|| panic!("{n}"));
            laws.insert(l);
        }
        // the three legacy keys alias rate_and_state:n_shaped:*
        assert_eq!(laws.len(), names.len() - 3);
        assert_eq!(
            friction_law_from_name("rate_and_state_n_shaped_slip"),
            friction_law_from_name("rate_and_state:n_shaped:slip")
        );
        assert!(friction_law_from_name("rate_and_state:dieterich").is_none());
        assert!(friction_law_from_name("rate_and_state:foo:ageing").is_none());
        assert!(friction_law_from_name("rate_and_state_revised_n_shaped_slip").is_none());
    }

    #[test]
    fn regularisation_selector() {
        use Regularisation::*;
        assert_eq!(
            regularisation_from_option("no_regularisation", false).unwrap(),
            NoRegularisation
        );
        assert_eq!(
            regularisation_from_option("rubin_ampuero", false).unwrap(),
            RubinAmpuero
        );
        assert_eq!(
            regularisation_from_option("simplified_prakash_clifton", false).unwrap(),
            SimplifiedPrakashClifton
        );
        assert!(regularisation_from_option("rubin", false).is_err());
        // upstream fallback
        assert_eq!(
            regularisation_from_option("no_regularisation", true).unwrap(),
            RubinAmpuero
        );
        assert_eq!(
            regularisation_from_option("anything", true).unwrap(),
            RubinAmpuero
        );
        assert_eq!(
            regularisation_from_option("simplified_prakash_clifton", true).unwrap(),
            SimplifiedPrakashClifton
        );
    }
}
