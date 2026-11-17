<!-- availability: available akantu::solver -->
<!-- availability: available akantu::linalg -->
# Solver configuration

This chapter corresponds to *Model solver configuration* of the upstream
manual (`io.rst`). Solvers are configured in a `model` section of the input
file whose name is the model type; the model reads it when it is
constructed (upstream `initDOFManager(section)`), so the solvers it
defines exist before `init_full`.

```dat
model solid_mechanics_model [
    default_solver = static          # the time_step_solver to activate

    time_step_solver static [
        sparse_solver_type = eigen
        non_linear_solver newton_raphson [
            threshold        = 1e-10
            max_iterations   = 10
            convergence_type = solution
        ]
    ]

    material elastic [
        name = steel
        rho  = 7800
        E    = 210e9
        nu   = 0.3
    ]
]
```

## The `model` section

| Parameter | Default | Meaning |
|---|---|---|
| `name` | the model's id | selects the section when several models of the same type exist: a section with `name = foo` is used only by the model whose id is `foo`; a section without `name` is used by every model of that type |
| `dof_manager_type` | `default` | DOF manager back-end (upstream `initDOFManager`): `default`, or `petsc` with the opt-in `petsc` feature (the solid, phase-field, diffusion and structural models, and the cohesive model through its `solid_mechanics_model_cohesive` section; [The PETSc back-end](petsc.md)); without the feature `petsc` is refused ("To use the solver petsc you will have to code it…", as an upstream build without PETSc) |
| `default_solver` | the solver created by `init_full` | name of a `time_step_solver` of the section to make the default solver; an unknown name is an error |

Materials may be placed in this section (preferred) or at the top level of
the file ([Materials](materials.md)).

## Keywords

The accepted values (generated from the port's enumerations, which use
the upstream spellings):

<!-- BEGIN GENERATED: solver-keywords -->
| Keyword position | Accepted values |
|---|---|
| `time_step_solver <type>` | `static`, `dynamic`, `dynamic_lumped` |
| `non_linear_solver <type>` | `linear`, `newton_raphson`, `newton_raphson_modified`, `lumped`, `newton_raphson_contact`, `petsc_snes`, `petsc_tao`, `auto` |
| `integration_scheme <type>` | `pseudo_time`, `forward_euler`, `trapezoidal_rule_1`, `backward_euler`, `central_difference`, `fox_goodwin`, `trapezoidal_rule_2`, `linear_acceleration`, `newmark_beta`, `generalized_trapezoidal` |
| `sparse_solver_type = …` | `mumps`, `eigen`, `petsc`, `auto`, `faer` |
| `convergence_type = …` | `residual`, `solution`, `residual_mass_wgh` |
<!-- END GENERATED: solver-keywords -->

`petsc_snes`, `petsc_tao` and `petsc` are accepted by the parser; they
need a model on the PETSc DOF manager (`dof_manager_type = petsc` or
`init_dof_manager("petsc")`, opt-in feature `petsc`, [The PETSc
back-end](petsc.md)), and selecting them on the default DOF manager is an
error (test `manual_claims_2.rs::doc_solver_petsc_types_are_refused`).
`newton_raphson_contact` is the Newton–Raphson solver under upstream's
contact name (the default non-linear solver of the contact mechanics
model's `static` time step solver, upstream and port): one implementation,
as upstream, and it solves like `newton_raphson` (tests
`manual_claims_2.rs::doc_solver_newton_raphson_contact_is_selectable` and
`::doc012_contact_model_is_available`). `mumps` needs a build with the
feature `mumps` ([The MUMPS sparse solver](mumps.md)); without it,
selecting it is an error. `faer` is an extension of the port: a pure-Rust
sparse LU, selectable explicitly, without a bitwise claim (decision
D-040). `auto` selects MUMPS when it is compiled in and the run uses an MPI
communicator, else `eigen` (never `faer`; upstream selects MUMPS whenever
it is compiled in, see [Selecting it](mumps.md#selecting-it)).

## `time_step_solver <type> [ … ]`

| Parameter | Default | Meaning |
|---|---|---|
| `name` | the type | identifier of the solver (`default_solver`, `solve_step(Some(id))`) |
| `sparse_solver_type` | `auto` | sparse direct solver of the linear systems |

A `time_step_solver` holds at most one `non_linear_solver` (more is an
error) and any number of `integration_scheme` sections. Settings that are
not given come from the model's defaults for that solver type:

| Solver type | Non-linear solver | Integration scheme of `displacement` | Solution type |
|---|---|---|---|
| `static` | `newton_raphson` | `pseudo_time` | — |
| `dynamic` (implicit) | `newton_raphson` | `trapezoidal_rule_2` | `displacement` |
| `dynamic` (with `ExplicitConsistentMass`) | `newton_raphson` | `central_difference` | `acceleration` |
| `dynamic_lumped` | `lumped` | `central_difference` | `acceleration` |

## `non_linear_solver <type> [ … ]`

Parameters of the Newton–Raphson family (`newton_raphson`,
`newton_raphson_modified`, `linear`), with upstream's defaults and access
rights (`p` = may be set in the input file, `w` = may be set from code,
`r` = readable):

| Parameter | Default | Access | Meaning |
|---|---|---|---|
| `threshold` | `1e-10` | rwp | convergence threshold |
| `max_iterations` | `10` | rwp | iteration limit; not converging is an error (`Error::NotConverged`) |
| `convergence_type` | `solution` | rwp | `solution` (norm of the increment), `residual` (norm of the residual, the threshold becoming relative after the first test) |
| `force_linear_recompute` | `true` | rw- | re-assemble the Jacobian of the `linear` solver at the next solve (without effect on the solver that `non_linear_solver linear` creates, which assembles the Jacobian at every solve, as upstream's `NonLinearSolverLinear`) |
| `verbose_iteration` | `false` | rw- | print the error at every iteration (rank 0) |
| `error`, `nb_iterations`, `converged` | — | r-- | results of the last solve |

The PETSc solvers `petsc_snes` and `petsc_tao` take PETSc options instead:
any other parameter `n = v` of their section (or `set_param`) becomes the
option `-n v` ([The PETSc back-end](petsc.md)). Akantu's names are
refused, in the input file and from code, with the PETSc option to use
instead (upstream `ParsablePETSc::exceptIfWrongParameter` and the table of
its manual):

| Parameter | `petsc_snes` | `petsc_tao` |
|---|---|---|
| `max_iterations` | `snes_max_it` | `tao_max_it` |
| `threshold`, `convergence_type` | `snes_atol`, `snes_rtol` or `snes_stol` | `tao_gatol`, `tao_grtol` or `tao_gttol` |
| `absolute_threshold` | — | `tao_gatol` |
| `relative_threshold` | — | `tao_grtol` |

(tests `petsc_options.rs::akantu_names_are_refused_with_the_petsc_option`
and `::tao_readable_parameters`, which need no PETSc; `max_iterations`,
`threshold` and `convergence_type` stay readable on `petsc_tao`).

Unknown names in a `non_linear_solver` section are errors, or warnings
when the file sets `permissive_parser = true` (upstream's
`Parsable::parseParam`, `parsable.cc:49-62`; the other solver sections are
not parsed as parameter sets).

`force_linear_recompute` and `verbose_iteration` are not input-file
parameters: upstream registers them `_pat_modifiable` only
(`non_linear_solver_newton_raphson.cc:80-84`), so a `non_linear_solver`
section that sets them is rejected with "not parsable" by upstream and by
the port, also with `permissive_parser` (which only tolerates *unknown*
names). The upstream manual lists them in the input-file table; set them
from code instead:

```dat
model solid_mechanics_model [
    time_step_solver static [
        non_linear_solver newton_raphson [
            verbose_iteration = true     # rejected when the model is built
        ]
    ]
]
```

```rust
use akantu::core::enums::{AnalysisMethod, MeshIoType};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::{ParamValue, Parser};
use akantu::solid::SolidMechanicsModel;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let mesh = || -> akantu::core::Result<SharedMesh> {
#     let mut m = Mesh::new(2, "mesh")?;
#     akantu::io::read_mesh(&mut m, &mesh_file, MeshIoType::Auto)?;
#     Ok(SharedMesh::new(m))
# };
let material = "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n";

// in the input file: "not parsable"
let bad = Parser::parse_str(
    "model solid_mechanics_model [\n time_step_solver static [\n \
     non_linear_solver newton_raphson [\n verbose_iteration = true\n ]\n ]\n]\n",
    "bad.dat",
)?;
assert!(SolidMechanicsModel::with_parser(mesh()?, None, "solid_mechanics_model", bad).is_err());

// from code: allowed
let parser = Parser::parse_str(material, "material.dat")?;
let mut model = SolidMechanicsModel::with_parser(mesh()?, None, "solid_mechanics_model", parser)?;
model.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
let newton = model.solver.non_linear_solver_mut(None)?;
assert_eq!(newton.get_param("threshold")?, ParamValue::Real(1e-10));
assert_eq!(newton.get_param("max_iterations")?, ParamValue::Int(10));
newton.set_param("verbose_iteration", ParamValue::Bool(true))?;
# Ok::<(), akantu::core::Error>(())
```

`convergence_type = residual_mass_wgh` is accepted by upstream's parser but
was never implemented: upstream silently runs the `solution` criterion
without refreshing the residual (UD-194). The port refuses it with
`Error::NotImplemented` by default; `akantu::solver::UpstreamCompat`
(`residual_mass_wgh_unrefreshed`) reproduces upstream for comparisons.

The PETSc solvers (`petsc_snes`, `petsc_tao`) and the forwarding of PETSc
options (`snes_*`, `ksp_*`, `pc_*`, `tao_*`) described by the upstream
manual belong to the PETSc back-end (opt-in feature `petsc`), which a
model uses once it runs on the PETSc DOF manager
([The PETSc back-end](petsc.md)).

### Iterations and convergence criteria

At iteration $k$ the solver assembles the residual
$\boldsymbol{r}^{(k)}$ of the model (and, depending on the variant, the
Jacobian $\boldsymbol{J}$ of the integration scheme), solves

$$
\boldsymbol{J}\,\delta\boldsymbol{x}^{(k)} = \boldsymbol{r}^{(k)}
$$

and passes the increment $\delta\boldsymbol{x}^{(k)}$ to the corrector of
the integration scheme. `newton_raphson` assembles $\boldsymbol{J}$ at
every iteration and `newton_raphson_modified` once at the start of each
solve; `linear` makes a single linear solve per step and tests no
criterion (its `error`, `nb_iterations` and `converged` keep their
values). There is no line search.

Both criteria use the Euclidean norm over the free degrees of freedom,
each counted once over all processes,

$$
\lVert \boldsymbol{x} \rVert = \sqrt{\sum_{i \in \mathcal{F}} x_i^2},
$$

where $\mathcal{F}$ holds the degrees of freedom that are not blocked
and whose node the process owns (local or master nodes; slave, pure
ghost and periodic slave nodes are left out); the sums of the processes
are added into one. With $\varepsilon$ the `threshold`:

* `solution`: the solve stops after the first iteration $k$ with
  $\lVert \delta\boldsymbol{x}^{(k)} \rVert < \varepsilon$;
* `residual`: the residual of the predictor is tested first, and the
  solve does no iteration when
  $\lVert \boldsymbol{r}^{(0)} \rVert < \varepsilon$. Otherwise the
  threshold becomes relative, and the solve stops after the first
  iteration with
  $\lVert \boldsymbol{r}^{(k)} \rVert < \varepsilon\,\lVert \boldsymbol{r}^{(0)} \rVert$,
  $\boldsymbol{r}^{(k)}$ being reassembled after the corrector.

The value $\lVert \cdot \rVert$ of the last test is the parameter
`error`; a solve that reaches `max_iterations` iterations without
passing the test ends with `Error::NotConverged`.

## `integration_scheme <type> [ … ]`

| Parameter | Default | Meaning |
|---|---|---|
| `name` | *(required)* | DOF family the scheme integrates (`displacement` for the solid mechanics model); a missing name is an error |
| `solution_type` | the model's default for that family (else `displacement`) | variable solved for: `displacement`, `velocity` or `acceleration` |

The schemes are those of upstream: `pseudo_time` (static),
`forward_euler`, `trapezoidal_rule_1`, `backward_euler` and
`generalized_trapezoidal` (first order), `central_difference`,
`fox_goodwin`, `trapezoidal_rule_2`, `linear_acceleration` and
`newmark_beta` (second order). The Newmark parameters keep upstream's
names and meaning, which are not the textbook ones (UD-017): with
$\alpha$ (`alpha`) and $\beta$ (`beta`) Akantu's parameters, the textbook
$\beta_\mathrm{N}$ is Akantu's $\alpha\beta$ (`alpha · beta`) and the
textbook $\gamma_\mathrm{N}$ is Akantu's $\beta$ (`beta`), i.e.

$$
\begin{aligned}
\boldsymbol{u}_{n+1} &= \boldsymbol{u}_n + \Delta t\,\dot{\boldsymbol{u}}_n + \Delta t^2 \left[ \left(\tfrac{1}{2} - \alpha\beta\right) \ddot{\boldsymbol{u}}_n + \alpha\beta\, \ddot{\boldsymbol{u}}_{n+1} \right], \\
\dot{\boldsymbol{u}}_{n+1} &= \dot{\boldsymbol{u}}_n + \Delta t \left[ (1 - \beta)\, \ddot{\boldsymbol{u}}_n + \beta\, \ddot{\boldsymbol{u}}_{n+1} \right]
\end{aligned}
$$

(upstream `newmark-beta.cc`: the predictor is
$\tilde{\boldsymbol{u}} = \boldsymbol{u}_n + \Delta t\,\dot{\boldsymbol{u}}_n + \tfrac{1}{2}\Delta t^2\,\ddot{\boldsymbol{u}}_n$,
$\tilde{\dot{\boldsymbol{u}}} = \dot{\boldsymbol{u}}_n + \Delta t\,\ddot{\boldsymbol{u}}_n$,
$\tilde{\ddot{\boldsymbol{u}}} = \ddot{\boldsymbol{u}}_n$; an acceleration
increment $\delta\ddot{\boldsymbol{u}}$ then changes $\dot{\boldsymbol{u}}$ by
$\beta\,\Delta t\,\delta\ddot{\boldsymbol{u}}$ and $\boldsymbol{u}$ by
$\alpha\beta\,\Delta t^2\,\delta\ddot{\boldsymbol{u}}$). The presets:

| Scheme | Akantu `alpha`, `beta` | Textbook $\beta_\mathrm{N}$, $\gamma_\mathrm{N}$ |
|---|---|---|
| `central_difference` | $0$, $\tfrac{1}{2}$ | $0$, $\tfrac{1}{2}$ |
| `fox_goodwin` | $\tfrac{1}{6}$, $\tfrac{1}{2}$ | $\tfrac{1}{12}$, $\tfrac{1}{2}$ |
| `trapezoidal_rule_2` (average acceleration) | $\tfrac{1}{2}$, $\tfrac{1}{2}$ | $\tfrac{1}{4}$, $\tfrac{1}{2}$ |
| `linear_acceleration` | $\tfrac{1}{3}$, $\tfrac{1}{2}$ | $\tfrac{1}{6}$, $\tfrac{1}{2}$ |
| `newmark_beta` (generic) | 0, 0 (set them) | $\alpha\beta$, $\beta$ |

The `solution_type` of the section chooses the unknown
$\delta\boldsymbol{x}$ of the linear systems: an increment of
acceleration, of velocity or of displacement. The corrector adds
$c\,\delta\boldsymbol{x}$ to $\ddot{\boldsymbol{u}}$,
$d\,\delta\boldsymbol{x}$ to $\dot{\boldsymbol{u}}$ and
$e\,\delta\boldsymbol{x}$ to $\boldsymbol{u}$ at the free degrees of
freedom, and the Jacobian is

$$
\boldsymbol{J} = c\,\boldsymbol{M} + d\,\boldsymbol{C} + e\,\boldsymbol{K}
$$

with the coefficients (the damping matrix $\boldsymbol{C}$ only when the
model has one, and $\boldsymbol{K}$ not read when $e = 0$, as for
`central_difference` with `solution_type = acceleration`):

| `solution_type` | $c$ | $d$ | $e$ |
|---|---|---|---|
| `acceleration` | $1$ | $\beta\,\Delta t$ | $\alpha\beta\,\Delta t^2$ |
| `velocity` | $\dfrac{1}{\beta\,\Delta t}$ | $1$ | $\alpha\,\Delta t$ |
| `displacement` | $\dfrac{1}{\alpha\beta\,\Delta t^2}$ | $\dfrac{1}{\alpha\,\Delta t}$ | $1$ |

A `displacement` unknown therefore needs $\alpha \neq 0$ and
$\beta \neq 0$, a `velocity` unknown $\beta \neq 0$:
`central_difference` ($\alpha = 0$) is used with `acceleration` or
`velocity`.

The first-order schemes integrate a variable $u$ and its rate $\dot{u}$
(for example the temperature of the heat model)
with the generalised trapezoidal rule of parameter $\alpha$ (`alpha`):

$$
u_{n+1} = u_n + \Delta t \left[ (1 - \alpha)\, \dot{u}_n + \alpha\, \dot{u}_{n+1} \right],
$$

with $\alpha = 0$ for `forward_euler`, $\tfrac{1}{2}$ for
`trapezoidal_rule_1`, $1$ for `backward_euler`, and $0$ by default for
`generalized_trapezoidal`. The predictor is
$\tilde{u} = u_n + (1 - \alpha)\,\Delta t\,\dot{u}_n$; for
$\alpha \neq 0$ the port then resets the free rates to $0$, so that the
corrector (which adds $e\,\delta x$ to $u$ and $d\,\delta x$ to
$\dot{u}$, with $e = 1$, $d = 1/(\alpha\,\Delta t)$ for a
`displacement` unknown and $e = \alpha\,\Delta t$, $d = 1$ for a
`velocity` unknown) gives the rule above. The Jacobian is
$\boldsymbol{J} = d\,\boldsymbol{M} + e\,\boldsymbol{K}$ (for forward
Euler with a rate unknown, $e = 0$ and $\boldsymbol{K}$ is not read).
Upstream keeps the rate in the predictor, which for $\alpha > 0$ solves
$\dot{u}_{n+1} = \dot{u}_n + (u_{n+1} - \tilde{u})/(\alpha\,\Delta t)$
instead (UD-287, UD-195; `akantu::solver::UpstreamCompat`
`generalized_trapezoidal_keeps_rate_in_predictor` and
`generalized_trapezoidal_reads_k_when_e_is_zero` reproduce upstream).

The doc comment of upstream's `NewmarkBeta` (`newmark-beta.hh:36-40`)
states a different update; the code, which both libraries follow, is the
one above. Checked by `doc002_newmark_parameters_are_akantus`
(`crates/akantu/tests/manual_claims.rs`: the presets' `alpha`/`beta` and
one step of `trapezoidal_rule_2` on a one-degree-of-freedom oscillator
against the closed-form Newmark update with $\beta_\mathrm{N} = \tfrac{1}{4}$,
$\gamma_\mathrm{N} = \tfrac{1}{2}$).

```rust
use akantu::core::enums::{AnalysisMethod, IntegrationSchemeType, MeshIoType, TimeStepSolverType};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::Parser;
use akantu::solid::SolidMechanicsModel;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let mut mesh = Mesh::new(2, "mesh")?;
# akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;

let input = "\
model solid_mechanics_model [
    default_solver = newmark
    time_step_solver dynamic [
        name = newmark
        integration_scheme fox_goodwin [
            name = displacement
            solution_type = acceleration
        ]
    ]
    material elastic [
        name = steel
        rho = 7800
        E = 210e9
        nu = 0.3
    ]
]";
let parser = Parser::parse_str(input, "dynamic.dat")?;
let mut model =
    SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
// the section's solver is the default one: init_full creates no other solver
model.init_full(&ModelOptions::new(AnalysisMethod::ImplicitDynamic))?;
let solver = model.solver.solver(None)?;
assert_eq!((solver.id.as_str(), solver.kind), ("newmark", TimeStepSolverType::Dynamic));
assert_eq!(
    solver.integration_scheme("displacement")?.kind(),
    IntegrationSchemeType::FoxGoodwin
);
assert!(!model.solver.has_solver("implicit"));
# Ok::<(), akantu::core::Error>(())
```

## Choosing solvers from code

Without an input file, `init_full` creates the model's default solver for
the analysis method (`static`, `implicit`, `explicit_lumped` or
`explicit`). Solver options can be passed in `ModelOptions`, and further
solvers can be created with `get_new_solver_of_type` and selected per
step with `solve_step(Some(id))`:

```rust
use akantu::core::enums::{AnalysisMethod, MeshIoType, SparseSolverType};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::Parser;
use akantu::solid::SolidMechanicsModel;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let mut mesh = Mesh::new(2, "mesh")?;
# akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
let parser = Parser::parse_str(
    "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
    "material.dat",
)?;
let mut model =
    SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;

let mut options = ModelOptions::new(AnalysisMethod::Static);
options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
model.init_full(&options)?;
assert!(model.solver.has_solver("static"));
# Ok::<(), akantu::core::Error>(())
```

Integration schemes passed in `ModelOptions::solver_options` are replaced
by the model defaults when `init_full` creates the default solver, as
upstream (UD-009, preserved for compatibility), and
`set_integration_scheme` refuses a DOF family that already has a scheme
("Their DOFs … have already an integration scheme associated", as
upstream). Choose the scheme in the input file (previous section), or
create a further solver from code and give it its scheme, as upstream
programs do with `getNewSolver` + `setIntegrationScheme`:

```rust
use akantu::core::enums::{
    AnalysisMethod, IntegrationSchemeType, MeshIoType, NonLinearSolverType, TimeStepSolverType,
};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::Parser;
use akantu::solid::SolidMechanicsModel;
use akantu::solver::SolutionType;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let mut mesh = Mesh::new(2, "mesh")?;
# akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
# let parser = Parser::parse_str(
#     "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#     "material.dat",
# )?;
# let mut model =
#     SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
model.init_full(&ModelOptions::new(AnalysisMethod::ImplicitDynamic))?;
let default = model.solver.solver(Some("implicit"))?.integration_scheme("displacement")?.kind();
assert_eq!(default, IntegrationSchemeType::TrapezoidalRule2);

// the family already has a scheme in this solver
assert!(model
    .set_integration_scheme("implicit", "displacement", IntegrationSchemeType::BackwardEuler, SolutionType::Displacement)
    .is_err());

// upstream: model.getNewSolver("euler", _dynamic, _newton_raphson);
//           model.setIntegrationScheme("euler", "displacement", _backward_euler);
model.get_new_solver_of_type("euler", TimeStepSolverType::Dynamic, Some(NonLinearSolverType::NewtonRaphson))?;
model.set_integration_scheme("euler", "displacement", IntegrationSchemeType::BackwardEuler, SolutionType::Displacement)?;
let chosen = model.solver.solver(Some("euler"))?.integration_scheme("displacement")?.kind();
assert_eq!(chosen, IntegrationSchemeType::BackwardEuler);
// model.solve_step(Some("euler")) now advances with backward Euler
# Ok::<(), akantu::core::Error>(())
```

## Releasing the static solver's memory

A sparse direct solver keeps its analysis and factors after a solve, so
that the next solve with the same matrix only back-substitutes; upstream
keeps them for the model's lifetime, and so does the library by default.
When no solve with that operator follows, for example after a static
preload before an explicit loop, `SolidMechanicsModel::release_static_solver`
(Python: `model.releaseStaticSolver()`) releases them. Freed: the analysis
and factors of `J`'s sparse solver (MUMPS: its instance, destroyed
collectively; the SparseLU translation and faer: their factors), all of `J`,
and the values and lookup map of `K`. Kept: `K`'s coordinates in their
storage order (16 bytes per stored entry) and the record of its profile. A
later solve assembles `K` again into the kept profile, so every entry keeps
its position (also when the profile grew after the first assembly, as with
contact, or when `K` was promoted to unsymmetric storage), rebuilds `J` from
it, analyses and factorises again, and gives the results of a run without
the release, bit for bit. It is collective (every rank, the same program
point, outside any other solver operation); the PETSc back-end returns a
"not supported" error. Do not call it between the steps of an implicit run:
each step would then pay a new assembly and factorisation.

The cohesive benchmark driver (`cohesive_extrinsic`) does this after its
static preload with `rust_release_static_solver = true` (default false),
after reading the solver statistics; `rss_after_release` in its memory
records shows the effect.

```rust
use akantu::core::enums::{AnalysisMethod, MeshIoType};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::Parser;
use akantu::solid::SolidMechanicsModel;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let mut mesh = Mesh::new(2, "mesh")?;
# akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
# let parser = Parser::parse_str(
#     "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#     "material.dat",
# )?;
# let mut model =
#     SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
model.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
// clamp the left edge, pull the right edge down
let x: Vec<f64> = model.physics.base.mesh.read()?.nodes().as_slice().to_vec();
let x_max = x.iter().step_by(2).fold(f64::MIN, |m, &v| m.max(v));
for (n, b) in model.blocked_dofs_mut()?.as_mut_slice().chunks_mut(2).enumerate() {
    b.fill(x[2 * n] == 0.0);
}
for (n, f) in model.external_force_mut()?.as_mut_slice().chunks_mut(2).enumerate() {
    if x[2 * n] == x_max {
        f[1] = -1.0;
    }
}
model.solve_step(None)?;
let u = model.displacement()?.as_slice().to_vec();

model.release_static_solver()?;
let k = model.dof_manager().matrix("K")?;
// values and lookup map freed, the coordinates kept
assert!(k.is_released() && k.nnz() == 0 && !k.coo().0.is_empty());

// the same load again: assembles and factorises again, and the
// equilibrium is unchanged
model.solve_step(None)?;
let scale = u.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
assert!(scale > 0.0);
for (a, b) in model.displacement()?.as_slice().iter().zip(&u) {
    assert!((a - b).abs() <= 1e-12 * scale);
}
# Ok::<(), akantu::core::Error>(())
```
