# Constitutive laws

This chapter is the catalogue of the constitutive laws of the solid
mechanics model. It corresponds to *Constitutive laws*
(`constitutive-laws.rst`) of the upstream manual (parity row DOC-010);
the text is written for the port, its equations are those of the port's
code (`crates/akantu-solid/src/materials/`), which follows upstream's
source, and every difference from upstream is named with its entry in
[`docs/UPSTREAM_DEFECTS.md`](../../UPSTREAM_DEFECTS.md).

[Materials](materials.md) explains the `material` sections, the
assignment of laws to elements and the elastic family (`elastic`,
`elastic_orthotropic`, `elastic_anisotropic`). The other laws are
described family by family:

| Family | Laws (input-file keyword) | Page |
|---|---|---|
| finite-strain hyperelasticity | `neohookean` | [Neo-Hookean hyperelasticity](laws/neohookean.md) |
| viscoelasticity | `sls_deviatoric`, `viscoelastic_maxwell` | [Viscoelastic laws](laws/viscoelastic.md) |
| plasticity | `plastic_linear_isotropic_hardening`, `plastic_drucker_prager` | [Plasticity](laws/plasticity.md) |
| damage | `marigo`, `mazars`, `plastic_mazars`, `anisotropic_damage` | [Damage](laws/damage.md) |
| non-local damage | `marigo_non_local`, `mazars_non_local`, `plastic_mazars_non_local` and the `non_local` section | [Non-local laws](laws/non-local.md) |
| phase-field fracture (solid side) | `phasefield` | [The phase-field material](laws/phasefield.md) |
| cohesive laws (cohesive model) | `cohesive_linear`, `cohesive_bilinear`, `cohesive_exponential`, `cohesive_linear_fatigue`, `cohesive_linear_friction`, `cohesive_linear_uncoupled` | [Cohesive elements](models/cohesive.md#cohesive-laws) |
| extra materials (feature `extra-materials`) | `damage_iterative`, `vreepeerlings`, … | [Extra materials](models/extra-materials.md) |

The list of every registered name is generated from the factories in
[Materials](materials.md#registered-laws).

## How a law is evaluated

A constitutive law computes the stress of an element from its strain. In
the finite-element discretisation it is evaluated at every quadrature
point of every element assigned to it: the model hands the law the
displacement gradient $\nabla \boldsymbol{u}$ of each point, and the law
returns the stress $\boldsymbol{\sigma}$ (method `compute_stress` of the
trait `akantu::solid::material::Material`). An implicit analysis also
needs the tangent $\mathbb{C} = \partial \boldsymbol{\sigma} / \partial \boldsymbol{\varepsilon}$
of every point (`compute_tangent_moduli`), assembled into the stiffness
matrix; the tangents of the port are Voigt matrices on engineering shear
strains. Quantities a law keeps per quadrature point (plastic strain,
damage, the stresses of viscous arms) are internal fields
(`akantu::model::InternalField`), which follow the element types of the
law, can keep the value of the previous converged step, and are resized
when elements are added or removed.

With `finite_deformation = true` the strain measure becomes the
Green–Lagrange strain
$\boldsymbol{E} = \frac{1}{2}(\boldsymbol{F}^T \boldsymbol{F} - \boldsymbol{I})$
of the deformation gradient $\boldsymbol{F} = \boldsymbol{I} + \nabla \boldsymbol{u}$,
and the stress the law returns is the second Piola–Kirchhoff stress
$\boldsymbol{S}$, for the laws that support it (each law page says
which).

A parameter of some laws can be random: the input file adds a
distribution to its base value (`Yd = 50 uniform [0, 10]`), and each
quadrature point draws its own value at `init_full` (details, the list of
these parameters and the seed in [Input files](input-files.md#random-parameters)).
The uniform distribution draws in $[a, b)$; the Weibull distribution of
scale $\lambda$ and shape $m$ (`weibull [a, b]` with $a = \lambda$ and
$b = m$) has the cumulative distribution function

$$
F(x) = 1 - e^{-(x/\lambda)^m}.
$$

## Reading the keyword tables

Every law has a table of the keywords its `material` section accepts,
with four columns:

* Keyword: the name as the law registers it. Names are
  case-sensitive, in the port as upstream (`Plane_Stress`, not
  `Plane_stress`; `Eta`, not `eta`): an unknown name is an error, a
  warning with `permissive_parser = true`
  ([Materials](materials.md#material-sections)).
* Meaning.
* Unit: Akantu has no unit system: the input must use one
  consistent set of units. The column gives the physical dimension:
  *stress* (pascal in SI units), *length*, *time*, *energy/area*,
  *stress·time* (a viscosity), *mass/volume*, *stress/length*, *volume*,
  *1* (dimensionless), *—* (no dimension), *degree* (an angle
  in degrees), *bool*.
* Default: the value the law holds when the section does not set
  the keyword.

The tables are checked against the laws by
`crates/akantu/tests/manual.rs::law_keyword_tables_match_the_registered_parameters`:
every keyword of a table must be registered by the law and parsable,
every parsable keyword of the law must be in its table (apart from the
common keywords below, which the table states it inherits), and every
default must equal the law's value, read through the law's parameters,
or, for the keywords that the law registers as parsable only (upstream
`_pat_parsable`, not readable back), from the fields of a newly
constructed law. Every `material` section of the manual's input-file
snippets is checked the same way
(`manual.rs::manual_material_snippets_use_registered_keywords`), and so is
every `non_local` section with its `weight_function` sub-section, parsed
by the non-local neighbourhood and weight function themselves
(`manual.rs::manual_non_local_snippets_use_registered_keywords`), so a
keyword that a law does not register fails a test. Keywords in the tables
and in `material` / `non_local` sections are checked; the prose (and
`text` blocks such as formulas) is not. The internal fields
listed for each law are checked against the law after `init_full`
(`manual.rs::law_internal_lists_match_the_laws`).

## Common keywords

Every law of the solid mechanics model registers:

| Keyword | Meaning | Unit | Default |
|---|---|---|---|
| `name` | name of the material (required; selectors and `material_by_name` use it) | — | none: a section without it is an error |
| `rho` | density | mass/volume | `0` |
| `finite_deformation` | evaluate the law on the Green–Lagrange strain and return the second Piola–Kirchhoff stress; the model then assembles the total-Lagrangian forms ([Finite deformation](solid-mechanics.md#finite-deformation)) | bool | `false` |
| `eigen_grad_u` | prescribed displacement-gradient eigenstrain $\boldsymbol{\varepsilon}^0$, subtracted from $\nabla \boldsymbol{u}$ before the law is evaluated (a matrix, `[[a, b], [c, d]]` in 2-D) | 1 | zero matrix |

The laws built on the elastic law (every law of this chapter except
`neohookean`) also register the keywords of `elastic` (meaning and
defaults in [Materials](materials.md#parameters-of-the-elastic-family)):
`E` (Young's modulus), `nu` (Poisson's ratio), `alpha` (thermal expansion
coefficient), `delta_T` (uniform temperature change) and, in 2-D,
`Plane_Stress`; the derived `lambda`, `mu` and `kapa` are readable only.
A keyword table that says *inherits `elastic`* lists only the law's own
keywords.

Every law stores the internal fields `stress` (the Cauchy stress, or the
second Piola–Kirchhoff stress with `finite_deformation`), `grad_u`,
`eigen_grad_u` and `potential_energy` (per quadrature point), and in 2-D
`third_axis_deformation` (upstream's plane-stress toolbox); the laws that
have the thermal fields `delta_T`, `sigma_th` and `epsilon_th` do not list
them. The internal-field lists of the law pages give the others (all
checked by `manual.rs::law_internal_lists_match_the_laws`).

## Energies

`model.energy(id)` sums the energy `id` over the laws that define it
([Solid mechanics model](solid-mechanics.md#energies)):

| Id | Laws | Meaning |
|---|---|---|
| `potential` | every law | the stored energy, law by law below |
| `dissipated` | `marigo`, `mazars`, `plastic_mazars`, the three non-local laws, `phasefield`, `sls_deviatoric`, `viscoelastic_maxwell` | the energy dissipated so far (damage, viscous) |
| `plastic` | `plastic_linear_isotropic_hardening`, `plastic_drucker_prager`, `plastic_mazars`, `plastic_mazars_non_local` | the plastic work so far |
| `work` | `viscoelastic_maxwell` | the mechanical work so far |

The dissipated, plastic and mechanical works are accumulated by
upstream's `updateEnergies`.

## Upstream compatibility switches

Where upstream's code is wrong, the port's default is the corrected
behaviour (category B of decision D-003), and a field of `LawOptions`
(`akantu::solid::material::LawOptions`, set on `model.physics.law_options`
before `init_full`, see [Materials](materials.md#upstream-compatibility-switches))
reproduces upstream for comparisons with the C++ library;
`LawOptions::upstream()` sets every field at once. The fields that concern
the laws of this chapter are named in their sections.
