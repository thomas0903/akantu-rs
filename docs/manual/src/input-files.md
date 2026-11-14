<!-- availability: available akantu::parser -->
# Input files

Simulation parameters, materials and solver settings are read from text
input files (conventionally `*.dat`), with the grammar of upstream's
parser (`io.rst`, *Input file*; implementation
`src/io/parser/input_file_parser.hh`, `algebraic_parser*.{hh,cc}`). The
port reproduces that grammar exactly, including its quirks
(PARSE-001…007 in [`FEATURE_PARITY.md`](../../FEATURE_PARITY.md)).

## Structure

A file is a list of parameters and sections. A section is
`type [name] [option] [ … ]`; sections nest. A parameter is
`name = value` on one line.

```dat
# a comment runs to the end of the line
spatial_dimension = 2
lactostatic_p = 30e3

user parameters [
    mesh_file     = swiss_cheese.msh
    stress        = [[lactostatic_p, 0            ], \
                     [0,             lactostatic_p]]
    max_nb_iterations = 100
    precision     = 1e-9
]

material elastic [
    name = cheese
    rho  = 1.2e3
    E    = 2*lactostatic_p   # parameters of enclosing sections are visible
    nu   = 0.3
]
```

Rules (all reproduced from upstream):

* Section header: the type is one of the keywords below (matched
  case-insensitively); the name is `[a-zA-Z_][a-zA-Z0-9_:]*`; an optional
  option word follows the name (`material elastic opt [`). A section
  without a name is called `anonymous_<n>`.
* Parameter names are `[a-zA-Z_][a-zA-Z_0-9]*` and must be unique in a
  section (a duplicate is a parse error).
* Values run to the end of the line, or to the first `#`, `;`, `=` or
  `\`. A trailing `\` continues the value on the next line (used above for
  the matrix). Upstream has no statement separator: a `;` ends the value
  and the rest of the line is a syntax error.
* Comments start with `#` and run to the end of the line.
* There is no `include` directive.

Section types:

<!-- BEGIN GENERATED: section-types -->
`cohesive_inserter`, `contact`, `embedded_interface`, `friction`, `global`, `heat`, `integration_scheme`, `material`, `phasefield`, `constitutive_law`, `mesh`, `model`, `model_solver`, `neighborhood`, `neighborhoods`, `non_linear_solver`, `non_local`, `rules`, `solver`, `time_step_solver`, `user`, `weight_function`, `contact_detector`, `contact_resolution`
<!-- END GENERATED: section-types -->

A value that spans several lines needs the `\` continuation. The example
of the upstream manual (`io.rst`) splits the `stress` matrix over two
lines without it; upstream rejects that file as the port does (upstream's
own example input, `examples/c++/solid_mechanics_model/io/parser/
input_file.dat`, writes the matrix on one line):

```dat,fails
user parameters [
    stress = [[lactostatic_p, 0            ],
              [0,             lactostatic_p]]
]
```

A `;` is not a separator:

```dat,fails
material elastic [
    E = 1; nu = 0.3
]
```

## Reading parameters

`Parser::parse_file` (or `parse_str` for text) returns the parsed tree,
whose root is the `global` section. Values are kept as text and converted
on request, so an expression is evaluated in the scope of the section that
holds it.

```rust
use akantu::parser::{Parser, ParserType, SearchScope};

let text = "\
user parameters [
    mass = 10.5
    area = 2
    density = mass / area
    n = [1, 2, 2*area]
    stress = [[1, 0], [0, 2]]
    mesh = swiss_cheese.msh
]";
let parser = Parser::parse_str(text, "input.dat")?;

// upstream: const auto & usersect = getUserParser();
let user = parser.user_section()?;
assert_eq!(user.section_type, ParserType::User);
assert_eq!(user.name, "parameters");

// upstream: Real mass = usersect.getParameter("mass");
let mass: f64 = user.get_parameter_value("mass", SearchScope::Current)?;
assert_eq!(mass, 10.5);
let density: f64 = user.get_parameter_value("density", SearchScope::Current)?;
assert_eq!(density, 5.25);
let n: Vec<f64> = user.get_parameter_value("n", SearchScope::Current)?;
assert_eq!(n, [1.0, 2.0, 4.0]);
let stress = user.get_parameter("stress", SearchScope::Current)?.to_matrix()?;
assert_eq!((stress.rows(), stress.at(1, 1)), (2, 2.0));
let mesh: String = user.get_parameter_value("mesh", SearchScope::Current)?;
assert_eq!(mesh, "swiss_cheese.msh");

// a default when the parameter is absent (upstream getParameter(name, default))
let iterations: f64 = user.get_parameter_or("max_iterations", 100.0, SearchScope::Current)?;
assert_eq!(iterations, 100.0);
# Ok::<(), akantu::core::Error>(())
```

`SearchScope::Current` looks only in the section, `SearchScope::Parent`
in its ancestors and `SearchScope::CurrentAndParent` in both (upstream
`_ppsc_current_scope`, `_ppsc_parent_scope`,
`_ppsc_current_and_parent_scope`).

## Algebraic expressions

Scalar values are expressions with the operators `+ - * /` and `**`
(power, left-associative), unary `-`/`+`, parentheses, the constants `pi`
and `e`, the functions

* unary: `abs acos asin atan ceil cos cosh exp floor log10 log sin sinh
  sqrt tan tanh acosh asinh atanh exp2 expm1 log1p log2 erf erfc lgamma
  tgamma trunc round`,
* binary: `pow min max atan2 fmod hypot`,

and references to other parameters of the current or the enclosing
sections. The power operator is `**`: the upstream manual mentions `^`,
but the upstream grammar (`algebraic_parser_x3.cc`) only knows `**`, and
`^` is a conversion error in both.

A parameter whose name starts with a function or constant name
cannot be referenced in a scalar expression: the grammar tries the
functions and the constants before parameter names, and they match a
prefix (`algebraic_parser_x3.cc:156-164`). `eta`, `e1` and `pi_x` are read
as the constant `e` or `pi` followed by stray characters, `cost` and
`expo` as the functions `cos` and `exp` without their parenthesis; all
are conversion errors, upstream and in the port (checked by
`doc001_names_starting_with_a_function_or_constant`). Defining such a
parameter is fine; only referencing it fails, so rename the parameters
that other expressions use (`eta` → `my_eta`):

```dat
eta = 0.5
damping = 2 * eta     # parses, but reading `damping` fails: `e` + "ta"
my_eta = 0.5
damping_ok = 2 * my_eta
```

```rust
use akantu::parser::{Parser, SearchScope};

let parser = Parser::parse_str(
    "general = 50\n\
     toto = 2*pi + max(2, general)\n\
     cube = 2**3**2\n\
     neg = -2**2\n\
     hat = 2^3\n",
    "input.dat",
)?;
let root = &parser.root;
let get = |name: &str| root.get_parameter_value::<f64>(name, SearchScope::Current);
assert_eq!(get("toto")?, 2.0 * std::f64::consts::PI + 50.0);
assert_eq!(get("cube")?, 64.0); // (2**3)**2: left-associative
assert_eq!(get("neg")?, 4.0); // the sign belongs to the number
assert!(get("hat").is_err()); // `^` is not an operator
# Ok::<(), akantu::core::Error>(())
```

Inside vectors and matrices (`[a, b, c]`, `[[a, b], [c, d]]`) and random
parameters, upstream uses a second expression grammar (UD-055) whose
function names are case-insensitive and which tries constants before
functions: `exp(1)` and `erf(1)` are rejected there (the constant `e`
matches first) and so is `atan2(…)` (`atan` matches first). The port
reproduces both grammars; use scalar parameters for such expressions:

```rust
use akantu::parser::{Parser, SearchScope};

let parser = Parser::parse_str("x = exp(1)\nv = [x, 2]\nw = [exp(1), 2]\n", "in.dat")?;
let root = &parser.root;
assert_eq!(root.get_parameter_value::<Vec<f64>>("v", SearchScope::Current)?, [std::f64::consts::E, 2.0]);
assert!(root.get_parameter_value::<Vec<f64>>("w", SearchScope::Current).is_err());
# Ok::<(), akantu::core::Error>(())
```

A parameter that refers to itself, directly or through others
(`yop = yop`), is a conversion error in the port; upstream recurses until
its stack overflows (UD-080).

A referenced variable is evaluated again at each reference, with no cache,
as upstream does. A chain of k definitions in which each one references the
previous one twice (`a1 = a0 + a0`, `a2 = a1 + a1`, …) therefore takes 2^k
evaluations: 4.7 s for k = 24 in a release build on an Apple M4 Pro, and each further line
doubles the time. Input files written by hand never come close; parse an
untrusted file only with a time limit (`SECURITY.md`).

## Random parameters

A value may be a base value followed by a distribution and its parameters
(`base [distribution [p1, p2]]`); objects that register a parameter as
random (upstream `RandomParameter`) draw one value per quadrature point.
The laws of this release with random parameters (generated from the
code):

<!-- BEGIN GENERATED: random-parameters -->
| Section | Law | Random parameters |
|---|---|---|
| `material` | `cohesive_bilinear` | `sigma_c` |
| `material` | `cohesive_exponential` | `sigma_c` |
| `material` | `cohesive_linear` | `sigma_c` |
| `material` | `cohesive_linear_fatigue` | `sigma_c` |
| `material` | `cohesive_linear_friction` | `sigma_c` |
| `material` | `cohesive_linear_uncoupled` | `sigma_c` |
| `material` | `marigo` | `Yd` |
| `material` | `marigo_non_local` | `Yd` |
| `material` | `mazars` | `K0` |
| `material` | `mazars_non_local` | `K0` |
| `material` | `plastic_mazars` | `K0` |
| `material` | `plastic_mazars_non_local` | `K0` |
| `phasefield` | `linear` | `gc` |
| `phasefield` | `quadratic` | `gc` |
<!-- END GENERATED: random-parameters -->

Such a parameter is given a distribution in its law's section (e.g.
`gc = 5e-3 uniform [0, 1e-3]` in a `phasefield` section); in other
sections (here `user parameters`) the parser stores random values and
programs read them with the parser API (example below):

```dat
user parameters [
    strength = 10 weibull [0.2, 0.5]  # 10 + Weibull(scale 0.2, shape 0.5)
    noise    = 0 normal [0, 1e-3]
    fixed    = 10                     # no distribution: deterministic
]
```

Distributions: `uniform [a, b]`, `exponential [λ]`, `gamma [α, β]`,
`weibull [a, b]` (scale `a`, shape `b`; C++
`std::weibull_distribution(b, a)`), `extreme_value [a, b]`,
`normal [mean, σ]`, `lognormal [m, s]`, `chi_squared [n]`,
`cauchy [a, b]`, `fisher_f [m, n]`, `student_t [n]`. For a given seed the
draws are bit-identical to those of upstream's strict build (libstdc++
distributions, glibc 2.41 on aarch64; decision D-006), on every platform
the port was measured on; an upstream build with its default flags
(contraction) or on x86_64 glibc differs in some draws
(`docs/NUMERICAL_VALIDATION.md`). The seed is set as described in
[Command-line and environment options](command-line.md#random-seed).

```rust
use akantu::parser::{Parser, RandomDistribution, SearchScope};

let parser = Parser::parse_str("rand1 = 10 uniform [0.2, 0.5]\nrand3 = 10\n", "in.dat")?;
let r = parser.root.get_parameter("rand1", SearchScope::Current)?.to_random()?;
assert_eq!(r.base_value, 10.0);
assert_eq!(r.distribution, Some(RandomDistribution::Uniform(0.2, 0.5)));
let d = parser.root.get_parameter("rand3", SearchScope::Current)?.to_random()?;
assert_eq!(d.distribution, None);
# Ok::<(), akantu::core::Error>(())
```

## Global parameters

| Parameter | Effect |
|---|---|
| `permissive_parser = true` | unknown parameters and sub-sections of parsed objects (materials, solvers) are reported as warnings instead of errors |
| `seed = <n>` | seed of the random generator when neither `AKA_SEED` nor `--aka_seed` is given ([Command-line options](command-line.md#random-seed)) |

```rust
use akantu::parser::Parser;

let strict = Parser::parse_str("a = 1\n", "in.dat")?;
assert!(!strict.is_permissive());
let permissive = Parser::parse_str("permissive_parser = true\na = 1\n", "in.dat")?;
assert!(permissive.is_permissive());
# Ok::<(), akantu::core::Error>(())
```

The upstream `debug_level` entry of the parser test input
(`test/test_io/test_parser/input_file.dat`) is an ordinary parameter:
upstream does not read it (the debug level comes from the command line,
see [Command-line options](command-line.md)).
