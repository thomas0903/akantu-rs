<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Clean-room provenance log — `engine.rs`, `distributions_impl.rs` (D-030)

Re-implementation of the behaviour of the oracle's `std::minstd_rand0`,
`std::generate_canonical<double, 53>` and the eleven
`std::*_distribution<double>` used by Akantu (GCC 14.2 / libstdc++ 14,
glibc 2.41, Debian trixie aarch64, `-O2 -ffp-contract=off`), plus
`Distribution::params_string` / `check_parameters` (`parameter.rs`), after the
earlier libstdc++-derived files were removed.

Implementer: a coding agent of the port, 2026-11-09, working from a tree
in which the earlier implementation had been removed.

## 1. Rules followed

**Not read** (at any time during this work): libstdc++ or libc++ sources or
headers (`/usr/include/c++/**` was never opened, listed, grepped or copied, on
the host or in Docker); the removed `engine.rs` / `distributions_impl.rs` and
the removed bodies of `params_string` / `check_parameters` (no `git show`,
`git log -p`, `git diff`, blame or checkout of them, and no look at the
history of the earlier implementation). Compiler diagnostics never displayed
library header text (no compile error inside `<random>` occurred).

**Inputs used** (all permitted by the brief):

* The ISO C++ specification of `<random>` ([rand.req.urng], [rand.req.eng],
  [rand.eng.lcg], [rand.predef], [rand.util.canonical], [rand.dist.*];
  public working draft N4950), from the implementer's knowledge of the
  published wording, and the C++ integral-conversion rules ([conv.integral]).
* Published algorithms: G. Marsaglia, T. A. Bray, *A convenient method for
  generating normal variables*, SIAM Review 6 (1964) (polar method; also
  Knuth, TAOCP vol. 2, §3.4.1 Algorithm P); G. Marsaglia, W. W. Tsang, *A
  simple method for generating gamma variables*, ACM TOMS 26 (2000) (including
  the `alpha < 1` boost `Gamma(alpha + 1) · U^(1/alpha)`); textbook inversion
  formulas for the exponential, Weibull, extreme-value (Gumbel) and Cauchy
  laws and the standard relations lognormal = exp(normal), χ²(n) = 2·Gamma(n/2),
  F = (χ²_m/m)/(χ²_n/n), t = Z/sqrt(χ²_n/n).
* The committed black-box fixtures `tests/fixtures/random/{engine,canonical,
  distributions,random_parameter}.json` (outputs of the oracle's compiled
  library, produced by our probe `oracle/probes/random/dump_random.cc`, which
  only calls the public API).
* Black-box experiments E1–E6 below (programs in
  `oracle/probes/random/cleanroom/`, compiled and run in `akantu-oracle:dev`
  with `g++ -std=c++20 -O2 -ffp-contract=off`), and the V-series variant
  harness (`oracle/probes/random/cleanroom/variants/`).
* Our own code in this module (`random.rs`, `parameter.rs`, `fill.rs`,
  `global.rs`, `fmt_c.rs`, `libm*.rs`), `CLEANROOM_API.md`, and the probe
  `README.md`.

**Disclosure.** As instructed, the implementer read the independent review
of the earlier implementation. Its correctness section and
the module docs of `random.rs` / the probe README (written by the earlier
author) describe some behaviour in prose (e.g. "saves x, returns y", a
below-one clamp in `generate_canonical`, the Cauchy `u == 0.5` retry, an extra
non-zero uniform for gamma `alpha < 1`, the fisher_f / student_t evaluation
order, the list of asserted preconditions). These statements contain no code;
they were treated as hypotheses only. **Every design choice below was
determined or independently confirmed by the standard/papers plus
experiments, and for each operation-order choice the plausible alternatives
were run and shown to fail** (V-series), or shown to be unobservable.

## 2. Experiments

| ID | Program | What is observed |
|---|---|---|
| E1 | `exp_canonical.cc` | `generate_canonical<double,53>` through a scripted generator with the `minstd_rand0` range: number of calls, which call is the low digit, the result for the largest digits, and four candidate formulas compared with the library on 2·10⁷ digit pairs from `minstd_rand0(5489)` |
| E2 | `exp_canonical_k.cc` | Number of calls for generator ranges R = 2^k, 2^k − 1, 2^k + 1 (k = 1…64) |
| E3 | `exp_assertions.cc` | Constructors of all 11 distributions built with `-D_GLIBCXX_ASSERTIONS`, for parameters in {−inf, −1, −min_sub, −0, 0, min_sub, 1, 2, max, inf, NaN} (and all pairs); each in a forked child, abort = asserted |
| E4 | `edges.cc` → `tests/fixtures/random/cleanroom_edges.json` | 70 scripted-generator cases: canonical clamp and digits, u = 0 and clamped u in every inversion formula, normal r2 == 1 / r2 == 0 / r2 > 1, signed zero of the standard normal value (r2 == 1) seen through normal(−0, 1) and student_t, Cauchy u == 0.5, gamma extra u == 0 and v <= 0, gamma log-test equality (from E5), `reset()` and copy of all 11 distributions, 200-draw sequences with non-power-of-two scales, invalid parameters (from E6), `operator<<` of NaN/inf/subnormal |
| E5 | `exp_gamma_boundary.cc` | Gamma(2.5) acceptance when `log(u)` equals the Marsaglia–Tsang threshold exactly (u constructed by search and checked with the library's own `generate_canonical`; normal value taken from the library's own `normal_distribution`), with controls at the neighbouring u |
| E6 | `exp_invalid.cc` | Termination, engine calls and NaN-ness for parameters outside the preconditions (NaN, negative, ±0, ±inf shapes, alpha with alpha + 1 == alpha) |
| V | `variants/run_all.sh` | The four committed fixture files replayed through `distributions_impl.rs` with each alternative operation order switched on |

Key raw results (full outputs: `oracle/probes/random/cleanroom/run.sh` →
`target/cleanroom-random/`):

* **E1**: 2 calls per draw; digits (min+1, min) → `0x1.00000008p-62` = 1/R²,
  (min, min+1) → `0x1.00000004p-31` = 1/R, so the **first call is digit 0**;
  (max, max) → `0x1.fffffffffffffp-1` while the plain quotient is 1.0;
  (min, min) → +0. 2·10⁷ pairs: candidate A (double accumulation, digit 0
  first) **0 mismatches**; B (binary128 accumulation) 7 467 150; C (exact
  integer S, one division) 4 983 158; D (digit order reversed) 20 000 000.
* **E2**: calls = ⌈53 / L⌉ with L = ⌊log₂ R⌋ for every tested range, except
  R = 8 (27 calls), 64 (11) and 2⁵³ (2), which behave as L − 1 (a floating
  logarithm rounding below an exact power of two); for the other powers of
  two L and L − 1 give the same count. R = 1.1·10⁸ (log₂ R = 26.7) takes 3
  calls (⌈53/26⌉), not ⌈53/26.7⌉ = 2.
* **E3**: asserted exactly: uniform `!(a <= b)` (NaN included, no `b − a`
  overflow check: (−inf, inf) accepted); exponential `!(lambda > 0)`; normal
  `!(stddev > 0)` (mean unchecked); gamma `!(alpha > 0)` (beta unchecked);
  chi-squared and Student-t `!(n/2 > 0)` (n = min_sub asserts because n/2
  underflows to 0); Fisher-F `!(m/2 > 0)` or `!(n/2 > 0)`; lognormal, Weibull,
  extreme-value, Cauchy: nothing (0 of 121 pairs).
* **E5**: 12/12 equality cases **accepted** (6 calls); the next smaller u is
  accepted, the next larger u (log u > threshold) rejected (8 calls) in all 12.
* **E6** (`minstd_rand0(5489)`, value/cumulative calls of draws 1–4):
  gamma(NaN, 1) NaN/8, NaN/12, … (terminates; extra uniform drawn);
  gamma(−1, 1), gamma(−5, 1) NaN/8 …; gamma(−1e17, 1) and gamma(−inf, 1)
  NaN/**6**, NaN/8 … (no extra uniform: −1e17 + 1 == −1e17);
  gamma(−2⁵³, 1) NaN/8 …; gamma(0, 1) +0; gamma(−0, 1) +inf; gamma(inf, 1)
  inf/6; weibull(0, 1) 0, 0, inf, 0; uniform(−inf, inf) NaN.

## 3. Design decisions and evidence

### Engine (`engine.rs`)

| Decision | Source | Evidence |
|---|---|---|
| `x ← 16807·x mod (2³¹−1)`, returns the new state; `min() = 1` (c = 0), `max() = m − 1`; default seed 1 | [rand.eng.lcg], [rand.predef] | `engine.json` (22 seeds × 1000 values, `min`/`max`, default state, `default_10000th`); unit test of the standard's 10000th value 1043618065 |
| Seed `s`: state = `s mod m`, or 1 if that is 0 | [rand.eng.lcg] | `engine.json` seeds 0, 2³¹−1, 2³², … |
| C `long` seed → 64-bit `result_type` modulo 2⁶⁴ (`i64::cast_unsigned`) | [conv.integral]; fixture meta `sizeof_uint_fast32_t = 8` | `engine.json` seeds −1, −2, −42, −(2³¹−1), −2³¹, `LONG_MIN`, `LONG_MIN+1` |
| `state()` = `x_i` | [rand.eng.lcg] textual representation | every fixture's `state` / `state_after` field |
| `generate_canonical`: k calls, S = Σ (g_i − min)·Rⁱ accumulated in `double` (`sum += d·t; t *= R`), first call = digit 0, result `sum / t` | [rand.util.canonical] ("arithmetic of type RealType") | E1 (candidate A exact on 2·10⁷ pairs, B/C/D fail); `canonical.json` 4 seeds × 1000 |
| Result ≥ 1 replaced by `0x1.fffffffffffffp-1` | standard's [0, 1) range | E1; E4 cases "canonical: …" |
| k = max(1, ⌈53 / L⌉), L = ⌊log₂ R⌋, L − 1 for R ∈ {8, 64, 2⁵³}; R and digits converted to `double` with one rounding | E2 | E2 table; unit test `canonical_call_counts` (only the `minstd_rand0` range is used by Akantu) |

### Distributions (`distributions_impl.rs`)

All transcendental functions are the glibc ports of `libm.rs`; `sqrt` is
IEEE-754. "V: x fails (a/b seq)" = alternative x breaks a of b fixture
sequences of that distribution (`variants/run_all.sh`, output reproduced in
§4).

| Distribution | Decision | Source | Evidence |
|---|---|---|---|
| uniform | `u·(b − a) + a` | [rand.dist.uni.real] (affine map of u) | V: `a(1−u)+bu` fails 6/15, `a+ub−ua` 6/15; E4 u = 0 with a = −0 gives +0 |
| exponential | `−log(1 − u) / λ` | inversion | V: `−log(u)/λ` 9/9, `−log(1−u)·(1/λ)` 3/9; E4 u = 0 → −0 |
| normal | polar method: x = 2u₁ − 1 then y = 2u₂ − 1 (separate canonicals, 4 calls per attempt), r2 = x² + y² | Marsaglia–Bray | V: `1 − 2u` fails 9/9 |
| | reject iff `r2 > 1` or `r2 == 0` | paper (S ≥ 1 or 0 rejected) refined by E4 | E4: r2 == 1 **accepted** (4 calls, result +0, cache +0), r2 == 0 rejected, r2 = 2 rejected |
| | `mult = sqrt(−2·log(r2) / r2)` | paper | V: `sqrt(−2 log r2)/sqrt(r2)` 6/9, `sqrt(−2/r2·log r2)` 5/9 |
| | cache `x·mult`, return `y·mult`; result `z·stddev + mean` | — | V: returning x and caching y fails 9/9; E4 normal(−0, 1) with standard values −0, +0 gives −0, +0 |
| | `reset()` drops the cache only; `Clone` copies it | — | E4 reset/copy cases; `random_parameter.json` interleave (copy construction/assignment with cache) |
| lognormal | `exp(s·z + m)`, z from an internal N(0, 1) with its own cache | standard relation | printed state (`m s 0 1 flag [z]`) in fixtures; 6 + 1 + interleave sequences |
| gamma | `malpha = alpha < 1 ? alpha + 1 : alpha`; `a1 = malpha − 1/3`; `a2 = 1/sqrt(9·a1)` | Marsaglia–Tsang (d, c) | V: `a1 = (3·malpha − 1)/3` fails 6/18, `a2 = 1/(3·sqrt(a1))` 15/18, `sqrt(1/(9·a1))` 9/18 |
| | normal value n first; redraw only n while `v = 1 + a2·n <= 0` (NaN v ends the loop); `v = v·v·v`; then u | paper (`do … while (v <= 0)`) | V: consuming a uniform on v <= 0 fails 7/18, using \|v\| 7/18; E4 "gamma(1): n ~ −3"; E6 (NaN alpha terminates) |
| | reject iff `u > 1 − 0.0331·(n·n)·(n·n)` **and** `log(u) > n²/2 + a1·(1 − v + log v)` | paper | E5: log(u) == threshold accepted (12/12), so the log test is non-strict (paper writes `<`); V: three other squeeze groupings and three threshold groupings all match every fixture (a different rounding only matters within ~1 ulp of the boundary). Squeeze grouping/comparison are **unobservable**: the squeeze region lies strictly inside the acceptance region, so whenever the squeeze variants disagree the log test accepts anyway; the paper's grouping is used |
| | alpha ≥ 1 branch: `(a1·v)·beta` | — | V: `a1·(v·beta)` fails 6/18 |
| | branch test `alpha == malpha` | — | E6: NaN alpha draws the extra uniform (8 calls); alpha = −1e17 or −inf (alpha + 1 == alpha) does not (6 calls), alpha = −2⁵³ does |
| | alpha < 1: extra uniform after the main loop, redrawn while `u == 0`; result `pow(u, 1/alpha)·a1·v·beta` (left to right) | paper's boost | V: extra uniform before the loop fails 6/18; four other product orders fail 6/18 (+ `random_parameter.json` gamma(0.5, 3)); E4 extra u == 0 rejected (10 calls) |
| | printed `alpha beta <normal>` | — | fixtures |
| chi_squared | `2·Gamma(n/2, 1)` (nested gamma with its normal) | standard relation | printed nested state `n n/2 1 0 1 flag [z]`; 9 + 1 sequences + E4 |
| cauchy | `a + b·tan(π·u)`, u == 0.5 redrawn | inversion | V: `tan(π(u − 1/2))` fails 6/6; E4 u = 0.5 twice → 6 calls |
| fisher_f | `(X·n)/(Y·m)`, X ~ Gamma(m/2, 1) drawn before Y ~ Gamma(n/2, 1) | standard relation | V: Y first 9/9, `(X/m)/(Y/n)` 6/9, `X·n/Y/m` 6/9, `(X/Y)·(n/m)` 6/9 (`(X·n)/(m·Y)` is the same IEEE expression by commutativity) |
| student_t | `Z·sqrt(n / G)`, Z ~ N(0, 1) drawn before G ~ Gamma(n/2, 2); Z is the nested normal's result `z·1 + 0` (not the raw standard value) | standard relation | printed nested state `n 0 1 flag [z] n/2 2 0 1 flag [z]`; V: G first 9/9, `Z/sqrt(G/n)` 9/9, `Z·sqrt(n)/sqrt(G)` 9/9; E4: standard value −0 gives +0 (raw z would give −0) |
| weibull | `b·pow(−log(1 − u), 1/a)` | inversion | V: `−log(u)` 12/12, `b·exp(log(−log(1−u))/a)` 12/12; E4 u = 0 → `pow(−0, 1) = −0` |
| extreme_value | `a − b·log(−log(1 − u))` | inversion | V: `−log(u)` 6/6; E4 u = 0 → +inf |
| all | `operator<<`: numbers in `scientific` with 17 significant digits (`fmt_c::ostream_scientific17`), separated by single spaces; cache flags as `0`/`1`, cached value only when the flag is 1, nested distributions inline | — | `printed` of all 108 fixture sequences, `random_parameter.json` (every chunk), `print_sci`; E4 `-nan inf`, `nan -inf 0`, subnormal |

### `parameter.rs`

| Decision | Evidence |
|---|---|
| `params_string` = the distribution's `write_params` | as above |
| `check_parameters` checks exactly the E3 assertions (with NaN failing), nothing else | E3 |

## 4. Verification summary

* All committed fixtures pass bit-exactly (`tests/random_fixtures.rs`, 9
  tests: 22 engine seeds, 4 canonical seeds, 108 distribution sequences with
  engine end states and `operator<<`, all `RandomParameter` chunk /
  interleave / print fixtures) and the new edge fixture
  (`tests/random_cleanroom_edges.rs`, 70 cases).
* Mutation checks of the boundary rules against `cleanroom_edges.json`:
  `r2 >= 1`, dropping the `r2 == 0` rejection, dropping the Cauchy
  `u == 0.5` or the gamma `u == 0` redraw, a strict gamma log test,
  dropping the canonical clamp, and using the raw standard normal value in
  student_t each make `edge_cases_match_oracle` fail.
* V-series output (`variants/run_all.sh`; "a/b seq" = failing / total
  sequences):

```text
[] ALL MATCH
[uniform=1] FAIL: uniform_real: 6/15 seq   [uniform=2] FAIL: uniform_real: 6/15 seq
[exp=1] FAIL: exponential: 9/9 seq (+ param)   [exp=2] FAIL: exponential: 3/9 seq
[n_flip=1] FAIL: all normal-based: every sequence
[n_mult=1] FAIL: normal 6/9, gamma 18/18, lognormal 6/6, chi_squared 9/9, fisher_f 9/9, student_t 9/9
[n_mult=2] FAIL: normal 5/9, gamma 18/18, lognormal 6/6, chi_squared 9/9, fisher_f 9/9, student_t 9/9
[n_ret=1] FAIL: normal 9/9, gamma 18/18, lognormal 6/6, chi_squared 9/9, fisher_f 9/9, student_t 9/9
[g_a2=1] FAIL: gamma 15/18, fisher_f 6/9   [g_a2=2] FAIL: gamma 9/18, fisher_f 6/9
[g_a1=1] FAIL: gamma 6/18, chi_squared 3/9, fisher_f 3/9, interleave
[g_vloop=1] FAIL: gamma 7/18, chi_squared 4/9, fisher_f 4/9, student_t 3/9
[g_vloop=2] FAIL: gamma 7/18, chi_squared 4/9, fisher_f 4/9, student_t 3/9
[g_pre=1] FAIL: gamma 6/18, chi_squared 3/9, fisher_f 3/9, student_t 3/9, interleave
[g_sq=1..3] ALL MATCH   [g_lt=1..3] ALL MATCH   (unobservable, see §3)
[g_ret=1] FAIL: gamma 6/18
[g_small=1..4] FAIL: gamma 6/18, chi_squared 3/9, fisher_f 3/9, student_t 3/9, interleave, param:gamma
[cauchy=1] FAIL: cauchy 6/6
[f_order=1] FAIL: fisher_f 9/9   [f_expr=1] 6/9   [f_expr=2] 6/9   [f_expr=3] ALL MATCH (same IEEE expression)   [f_expr=4] 6/9
[t_order=1] FAIL: student_t 9/9   [t_expr=1] 9/9   [t_expr=2] 9/9
[weibull=1] FAIL: weibull 12/12   [weibull=2] 12/12
[ev=1] FAIL: extreme_value 6/6
```

## 5. Limits

* Bit compatibility is established for `MinstdRand0` / `RandomGenerator`
  (the only generators Akantu uses) and the scripted test generator with the
  same range; other generator ranges follow E2 for the call count but their
  values were not compared, with one exception recorded below.
* **2026-11-11 / 2026-11-12, 32-bit range.
  ** The test-only `Mt19937` engine of
  `crates/akantu-solid/tests/upstream_material_fixture/mod.rs` (written from
  the C++ standard's specification and the Matsumoto–Nishimura paper; no
  standard-library source was read) feeds `generate_canonical` with a
  32-bit range (2 calls per draw). Evidence that this use is bit compatible
  with the oracle's library (g++ 14.2.0-19, both oracle images): the
  committed 1000-draw fixture `material_gtest_random_rotation`, and the review's
  black-box reproducers
  (`v95_rng_draws.cc`): 18,000,000 engine outputs and draws (6 seeds × 10⁶ ×
  raw / U(0,1) / U(−2.5,7)) and 200,012 scripted 32-bit digit pairs (edges
  and the clamp `(max, max) → 0x3fefffffffffffff`), all bitwise equal.
  Nothing else is claimed for 32-bit-range generators.
* NaN payloads and signs are not reproduced or compared (NaN-ness is).
* The gamma squeeze grouping is not observable and is taken from the paper;
  should a future oracle make it observable, `variants/run_all.sh` has the
  alternatives ready.
