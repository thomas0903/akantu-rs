# Clean-room reimplementation contract (D-030)

**Status (2026-11-09):** implemented; provenance of every design choice in
`CLEANROOM_LOG.md`.

`engine.rs` and `distributions_impl.rs` were removed because they were
translated from GPL-3.0-licensed libstdc++ sources.
They must be
re-implemented **without reading libstdc++ sources or the removed files**
(do not inspect them in git history either). Allowed inputs: the ISO C++
standard text for `<random>` ([rand.req], [rand.eng.lcg], [rand.util.canonical],
[rand.dist.*]), published algorithm papers (e.g. Marsaglia & Bray 1964 polar
method; Marsaglia & Tsang 2000 gamma method), the committed black-box
fixtures in `crates/akantu-core/tests/fixtures/random/`, and black-box
experiments compiling small programs against the oracle's libstdc++ in the
`akantu-oracle:dev` Docker image (running them is allowed; reading the
library headers is not).

The rest of the module (`random.rs`, `fill.rs`, `global.rs`, `parameter.rs`,
`fmt_c.rs`, `libm*.rs`) is our own code or derived from LGPL sources and
expects these public items (signatures only, as previously defined):

## `engine.rs`
- `pub trait UniformRandomBitGenerator { fn next_value(&mut self) -> u64; /* + min/max as needed */ }`
- `pub struct MinstdRand0` (LCG a = 16807, c = 0, m = 2^31 − 1);
  `pub type DefaultRandomEngine = MinstdRand0;`
- consts `MULTIPLIER`, `INCREMENT`, `MODULUS`, `DEFAULT_SEED = 1`
- `pub const fn new() -> Self`, `pub const fn with_seed(s: u64) -> Self`,
  `pub const fn with_seed_long(s: i64) -> Self` (C `long` seed semantics),
  `pub fn seed(&mut self, s: u64)`, `pub fn seed_long(&mut self, s: i64)`,
  `pub const fn state(&self) -> u64`, `pub fn next_u32(&mut self) -> u32`,
  `pub fn discard(&mut self, z: u64)`; `Default`; `UniformRandomBitGenerator`.
- `pub fn generate_canonical<G: UniformRandomBitGenerator>(g: &mut G) -> f64`
  (`std::generate_canonical<double, 53>` semantics).

## `distributions_impl.rs`
For each of `UniformRealDistribution(a, b)`, `ExponentialDistribution(lambda)`,
`NormalDistribution(mean, stddev)` (+ `standard()`, `saved_available()`),
`LognormalDistribution(m, s)`, `GammaDistribution(alpha, beta)`,
`ChiSquaredDistribution(n)`, `CauchyDistribution(a, b)`,
`FisherFDistribution(m, n)`, `StudentTDistribution(n)`,
`WeibullDistribution(a, b)`, `ExtremeValueDistribution(a, b)`:
- constructor `new(...)` (const where possible), parameter getters,
- `pub fn reset(&mut self)`,
- `pub fn sample<G: UniformRandomBitGenerator>(&mut self, g: &mut G) -> f64`,
- `pub fn write_params(&self, out: &mut String)` — the text the oracle's
  `operator<<` writes for the distribution (parameters and internal state),
  to be determined from black-box fixtures (`params_string` in
  `parameter.rs`), using `fmt_c.rs` for C-style number formatting.
- `Clone` copies internal state (e.g. cached normal values).

## `parameter.rs`
`Distribution::params_string` and `Distribution::check_parameters` bodies were
removed (stubbed with `todo!`); re-implement from the standard's documented
preconditions and black-box output.
