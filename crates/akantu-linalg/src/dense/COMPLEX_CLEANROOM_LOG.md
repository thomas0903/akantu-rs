# Complex division: clean-room provenance log (A3c, 2026-11-10)

Subject: `Complex::quotient` in `complex.rs`, which must reproduce bit for bit
the oracle's `std::complex<double>` division (g++ 14.2.0-19, Debian trixie
13.7, aarch64, image `akantu-oracle:dev`). Contract: `COMPLEX_CLEANROOM.md`
(D-030 and its amendment). Implementer: a coding agent of the port that
worked under these rules from a stub of the function.

## 1. Rules followed

* No GCC / libgcc / libstdc++ source code was read or searched for.
* No compiled library code (libgcc, libstdc++, libakantu, or any binary that
  statically contains them, including our own probe executables) was
  disassembled, dumped or otherwise inspected. No `objdump`, `gdb`, `nm`,
  `readelf`, `ltrace` or similar was run on any binary. The only thing
  observed is the numerical result printed by our own programs.
* The removed implementation was not read: no `git log -p`, `git show`,
  `git diff` or `git blame` of `dense/complex.rs` at any commit before
  an earlier commit, on any branch.
* GCC documentation was not consulted either; the design below follows only
  from the published sources in §2 and the experiments in §4.

**Disclosure 1 (stub residue).** The stub of an earlier commit, which the brief told me
to edit, still contained five `const` items and a module doc paragraph left
over from the removed implementation. I saw them when I opened the file.
They were **not** used as a derivation input. Every threshold, factor and
FMA placement below was measured independently by the experiment cited for
it. Thresholds that could not be measured (§5) were chosen by the argument
given there. All leftover text was deleted, and the new code uses its own
names and comments. Reviewers may want to weigh this exposure.

**Disclosure 2 (training data).** The implementer is a language model whose
training data may contain public GCC sources. No such source was recalled or
used as a reference here. Each choice is backed by the experiment cited for
it, and every probe and result file is committed so the derivation can be
re-run.

## 2. Published sources used

* [S] R. L. Smith, "Algorithm 116: Complex division", *CACM* 5(8):435, 1962:
  `r = d/c`, `den = c + d·r`, `x = (a + b·r)/den`, `y = (b − a·r)/den` for
  `|d| <= |c|`, and the mirrored formulas otherwise.
* [BS] M. Baudin, R. L. Smith, "A robust complex division in Scilab", 2012
  (arXiv:1210.4539). Two ideas taken from it: when the ratio `r` is zero, use
  the numerators `a + d·(b/c)` and `b − d·(a/c)`; and rescale the operands by
  powers of two near the ends of the exponent range. The thresholds and
  factors themselves were measured (§4), not taken from the paper.
* [G] ISO/IEC 9899:2011 Annex G, G.5.1 paragraph 4 and EXAMPLE 2 (`_Cdivd`):
  recovery of infinities and zeros that compute as NaN + iNaN (zero
  denominator; infinite numerator with finite denominator; finite numerator
  with infinite denominator).
* [I] IEEE 754-2008: fused multiply-add rounds once; round-to-nearest-even.

## 3. Resulting algorithm and where each choice comes from

| # | Choice in `complex.rs` | Evidence |
|---|---|---|
| 1 | Smith's algorithm with true divisions (no reciprocal) | [S]; E1 (0 / 2 000 000 mismatches; every "multiply by 1/den" variant ≥ 7302 mismatches) |
| 2 | Uses `r = c/d` iff `\|c\| < \|d\|` (strict). A tie uses `r = d/c` | E0 (`(1+i)/(1−i) = +0 + i`, the other branch gives −0); E1 (`<=` variant: 10 448 mismatches) |
| 3 | Denominator and both numerators are single-rounding FMAs (`f64::mul_add`); the `r = c/d` imaginary numerator is `fma(b, r, −a)` (not `−fma(−b, r, a)`) | E1: the all-fused variant is the only one with 0 mismatches; unfusing any one term gives ≥ 29 176; the negated form gives 7302 |
| 4 | Baudin & Smith numerators `fma(d, b/c, a)`, `fma(−d, a/c, b)` (mirrored `fma(c, a/d, b)`, `fma(c, b/d, −a)`) **iff `\|r\| <= DBL_MIN`** | [BS]; E3 (best `r == 0` variant still 56 602 mismatches); E3b (`\|r\| < DBL_MIN` + fused: 0 / 1 000 000); E3c (exact switch: `\|r\| <= 2^-1022`: 0 / 2 000 000 incl. 500 000 at `\|r\| = DBL_MIN`; `<`: 94 862) |
| 5 | `max(\|c\|,\|d\|) < 2^-52` → multiply all four operands by `2^52` | E5b/E6: switch exactly between `c = 0x1.fffffffffffffp-53` (scaled) and `2^-52` (not); spurious overflow of `a` from exactly `a = 2^972` → factor `2^52`; E7: same factor for `q ∈ [−110, −53]`; E13: scaling all four (rather than the numerator only, with the quotient rescaled) explains the double-rounding cases |
| 6 | Otherwise, if `\|a\| < DBL_MIN` or `\|b\| < DBL_MIN` (±0 included) and no overflow is possible → multiply all four by `2^52` | E10–E13 (numerator-only scaling 2956/3000, all-four scaling explains the rest: 0 "neither"); E14 (clean `A` bands); E15 (holds for `max(\|a\|,\|b\|)` up to 2^64, 0 U); E16 (sharp: `min(\|a\|,\|b\|)` ilogb −1023 → 2871 A / 0 U, ilogb −1022 → 0 A / 2995 U, both orientations); E17 (zero triggers: +0 7597 A / 0 U, −0 7641 / 0; `\|a\| = DBL_MIN` → 7235 U / 0 A, predecessor → 0 U / 7319 A); a subnormal `c`/`d` does not trigger it (E13 U-only cases) |
| 7 | Rules 5 and 6 are exclusive (else-if) | E21: 524 358 exclusive-only, 0 sequential-only |
| 8 | Guard of rule 6: `max(\|a\|,\|b\|) < 2^971` and `max(\|c\|,\|d\|) < 2^971` | E18 (no spurious overflow for `b ≥ 2^972` or a huge denominator); E20: any numerator guard in `[2^60, 2^972]` and any denominator guard in `[2^60, 2^971]` gives 0 / 20 000 000 mismatches, larger guards fail. 2^971 is the natural bound (`2·CD·2^52 < 2^1024`), see §5. **Corrected in §9:** the guard is `< 0x1.fffffffffffffp+970` for both maxima (the x86_64 runtime; unobservable on aarch64) |
| 9 | `max(\|c\|,\|d\|) >= DBL_MAX/2` → multiply all four by `1/2` | E19: switch exactly between `0x1.ffffffffffffep+1022` and `0x1.fffffffffffffp+1022` = `DBL_MAX/2`, for every denominator orientation; the numerator is scaled too (a numerator that would overflow stays finite); huge numerators alone are **not** scaled (`(1.5·2^1023)(1+i)/(2+i)` gives `inf` in the oracle) |
| 10 | Maxima formed as `\|x\| < \|y\| ? \|y\| : \|x\|` (a NaN `c` disables the scalings) | E23: this form 0 mismatches on the 104 976-point special grid; `fmax` 56, the other comparison 112. The numerator maximum's form is unobservable (only used by the guard) |
| 11 | Annex G recovery when both parts are NaN, on the **scaled** operands, zero test `c == 0 && d == 0`, unfused inner products | [G]; E22 (no recovery 6752 mismatches; original operands 1464; scaled operands 56, all explained by row 10); E25 (0 / 20 000 000 random special/finite mix + 0 / 104 976 grid) |
| 12 | Caller compilation flags are irrelevant (the division is an out-of-line runtime routine) | E26: oracle in its own TU at `-O0`, `-O2`, `-O3`, strict oracle flags, `-ffp-contract=fast`: 0 / 5 000 000 each |

`f64::mul_add` is required by row 3 and justified by E1 (AGENTS.md, "Rules for numerical code"
exception). It is correctly rounded on every Rust target, so the port's
results do not depend on the host.

## 4. Experiments

All programs live in `oracle/probes/dense/complex_cleanroom/`. They were
compiled with `g++ -O2 -std=c++20 -ffp-contract=off` inside
`akantu-oracle:dev` and run there. The division under test is always a
`noinline` function applied to run-time operands, so it cannot be
constant-folded. `run.sh` rebuilds and reruns everything and writes the
outputs to `results/` (committed). Candidate models are our own C++ code
(`smith_core.h`, `model_v1.h` … `model_v5.h`) written from §2 and the
experiments before them. Each `model_vN` adds one rule.

* **E0** `e0_basic.cc`: hand-picked divisions (`results/e0.txt`).
  `(1+i)/(1−i) = (+0, 1)` shows that a tie `|c| = |d|` takes the `r = d/c`
  branch. `(1+2i)/(1+2i) = (1, +0)` shows the `r = c/d` imaginary numerator
  is `b·r − a`, not the Baudin & Smith form `−(a − b·r)`. Also checked:
  `1/0 → (inf, inf)` and `inf/(1+i)`.
* **E1** `e1_smith_variants.cc`: 64 Smith variants (tie rule × fused/unfused
  den, x, y × divide/reciprocal × imaginary-numerator form) on 2 000 000
  moderate-range inputs: the fixture's `uni·10^[−12,12]`, `2^[−4,4]` random
  significands, small integers (ties, signed zeros) and exact ties. The only
  variant with 0 mismatches is: strict `<`, all fused, divisions,
  `fma(b, r, −a)`. The runner-up has 7302 (`results/e1.txt`).
* **E2** `e2_wide_range.cc`: the E1 winner on the whole double range, 4e6
  inputs. 120 055 mismatches (signed zeros and lost precision when `r` is tiny,
  spurious overflow), which motivated E3–E7.
* **E3/E3b/E3c**: the ratio `r` underflows while all operands stay far from
  the range limits. Candidate numerators were compared with the oracle. The
  result is the `|r| <= DBL_MIN` switch with fused `d·(b/c)` grouping (row 4).
  E3c concentrates `|r|` on `{DBL_MIN, pred, succ}` and `2^[−1024,−1021]`.
* **E4** `e4_residual.cc`: residual analysis of each model on 4e6–4e7 finite
  inputs (uniform bit patterns, uniform exponents, top and bottom of the
  range). Mismatches by model: model_v1 102 468 / 4e6; model_v2 165 / 4e6;
  model_v3 334 / 2e7 (all with `max(|c|,|d|) ≈ 2^1023`); model_v4
  **0 / 4e7**.
* **E5/E5b** `e5_numerator_upscale.cc`, `e5b_scan.cc`: a spurious infinite
  imaginary part for `(1.5·2^999)/(1.25·2^-266 + 1.75·2^-967 i)` reveals a
  numerator up-scaling that depends on the denominator size (`c` scan: it
  happens for `c ≤ 2^-53`).
* **E6** `e6_bisect.cc`: bit-pattern bisections. The scaling starts below
  `c = 2^-52` (strict). The first `a` that overflows is exactly `2^972`, so
  the factor is exactly `2^52` (rows 5).
* **E7** `e7_upscale_map.cc`: the same factor `2^52` for every `q` where it
  is observable (`[−110, −53]`).
* **E8–E11** `e8_explain.cc`, `e9_scan_exact.cc`, `e10_tiny_candidates.cc`,
  `e11_trigger_map.cc`: after model_v2 the remaining mismatches are results
  near the subnormal range that the oracle rounds correctly. E10 shows that
  scaling the numerator by ≥ 2^52 explains 2956 of 3000 such cases, and E11
  maps where scaling matters.
* **E12/E13** `e12_trigger_dump.cc`, `e13_all4.cc`: of 64 813 cases where
  scaling matters, scaling **all four** operands by 2^52 (with no rescaling of
  the quotient) leaves 0 "neither" cases in the band `max(|c|,|d|) ≥ 2^-52`.
  The few "unscaled only" cases there have `min(|a|,|b|)` normal, or a
  subnormal `c`/`d` only.
* **E14** `e14_scan_a52.cc`: scans of two base cases. The oracle always
  equals either the all-four-scaled model or both models, except where
  scaling would overflow (an operand ≥ 2^972), where it is unscaled.
* **E15** `e15_large_ab.cc`: the scaling also happens for a large numerator
  component (up to 2^64, the observable limit): 0 unscaled.
* **E16** `e16_ab_threshold.cc`: cancellations `fma(b, r, a)` to subnormal
  values with both `a` and `b` normal or not. The trigger is exactly
  `min(|a|,|b|) < DBL_MIN`, in both branch orientations.
* **E17** `e17_trigger_details.cc`: ±0 triggers it, `|a| = DBL_MIN` does not
  and its predecessor does. With `max(|c|,|d|) < 2^-52` and a zero part
  there is no double scaling.
* **E18** `e18_guards.cc`: overflow detectors at the large end. No spurious
  overflow from scaling a numerator ≥ 2^972. No scaling for denominators
  near 2^971. The only switch, at `X ≈ 2^1023`, is the huge-denominator rule
  (G5 shows it without the small-numerator trigger).
* **E19** `e19_downscale.cc`: bisection of the huge-denominator rule: exactly
  `max(|c|,|d|) >= DBL_MAX/2` (row 9).
* **E20** `e20_guard_sensitivity.cc`: 2e7 inputs with the trigger and large
  other operands, for 11 guard values (row 8).
* **E21** `e21_exclusive.cc`: rules 5 and 6 are exclusive (row 7).
* **E22/E23** `e22_special.cc`, `e23_special_max.cc`: the exhaustive grid of
  {±0, ±2^-1074, ±DBL_MIN, ±1, ±3, ±2^1000, ±DBL_MAX, ±inf, ±NaN}^4 (104 976
  divisions) against recovery and max variants (rows 10, 11).
* **E24** `e24_nan_sign.cc`: the oracle's NaN sign bits for one-NaN operands
  (§6).
* **E25** `e25_special_random.cc`: model_v5 against 2e7 random mixes of special,
  boundary (thresholds and their neighbours) and finite values, plus the
  E22 grid: 0 mismatches.
* **E26** `e26_flags.cc` + `e26_oracle_tu.cc`: flag independence (row 12).
* **E27** `e27_bulk.cc`: 6e6 bulk oracle records for the Rust-side check
  (§7). model_v5 disagrees with 0 of them (`results/e27.txt`).
* `gen_fixture.cc`: the committed fixture (4500 records, 11 tags, one per
  rule and boundary). model_v5 disagrees with 0 of them
  (`results/gen_fixture_stats.txt`).

## 5. Parameters the oracle's outputs do not determine

* **Factor of the huge-denominator scaling (row 9).** Every power of two
  `f ≥ 2^-970` gives the same results. A component that loses bits when
  multiplied by `f` contributes less than the smallest subnormal to a
  quotient whose denominator is at least `DBL_MAX/2·f`. I chose `1/2`, as in
  [BS].
* **Guard of rule 6 (row 8).** When the guard is large, the trigger value
  (a subnormal or zero part) cannot affect the result unless the denominator
  is tiny, and then the other numerator part is small as well. Up to the
  overflow point (2^972 for the numerator part, 2^971 for the denominator,
  where the scaled `2·CD·2^52` would overflow) the guard only switches
  between bit-identical results. E20 confirms this for 11 guard values. I
  chose `2^971` for both, the largest value that is safe for the denominator.
* **Form of the numerator maximum.** It only enters the guard comparison:
  identical results (E23).

## 6. Not reproduced: NaN sign bits and payloads

The oracle's NaN outputs carry sign bits that follow its own instruction-level
NaN propagation (E24; e.g. `(NaN + 0i)/(1 + 0.5i) = (NaN, −NaN)`). Neither C++
nor Rust specifies NaN signs or payloads for arithmetic, and the dense parity
comparator treats any two NaNs as equal. The port reproduces which parts are
NaN, not their bits. On the committed fixture, 153 of the 312 records with a
NaN part also agree in NaN sign bits on aarch64-apple-darwin (reported by the
test, not asserted).

## 7. Rust verification

* `cargo test -p akantu-linalg` and `--release`: all pass, including
  `dense_parity` (603 `cdiv` records of `dense_strict.txt` plus the complex
  eigen-solver records that go through `quotient`) and
  `complex_div_cleanroom::complex_division_matches_oracle_bit_for_bit`
  (4500 records, 0 mismatches).
* Opt-in bulk check: `CDIV_BULK_OUT=/abs/file.bin oracle/probes/dense/complex_cleanroom/run.sh`
  (or E27 directly), then
  `AKANTU_CDIV_BULK=/abs/file.bin cargo test -p akantu-linalg --release --test complex_div_cleanroom`.
  6 000 000 records, 0 mismatches (debug and release, 2026-11-10).
* `cargo fmt --check`; `cargo clippy -p akantu-linalg --all-targets -- -D warnings`.

## 8. The x86_64 runtime (2026-11-11; D-043)

Subject: `ComplexDivision::Unfused` (`Complex::quotient_with`), the
`std::complex<double>` division of the **x86_64** oracle image
(`akantu-oracle:dev` `sha256:45122c21…`, g++ 14.2.0-19, Debian 13.7,
x86_64), found when the dense records for `13 ≤ n ≤ 48`,
generated on x86_64, differed from the port in 1,238
complex-eigenvector entries of the two general records with complex pairs
and nowhere else. Implementer: The rules of §1
were followed again: no GCC / libgcc / glibc source, documentation or
compiled code was read, searched for or inspected; only the results printed
by our own programs were observed.

**Hypothesis.** The same algorithm as §3 (scaling, strict `|c| < |d|`
branch, true divisions, Baudin & Smith numerators when `|r| <= DBL_MIN`,
Annex G recovery) with every fused multiply-add replaced by a rounded product
followed by a rounded sum, the x86_64 baseline ISA having no FMA instruction.
The Baudin & Smith numerators then have two candidate unfused forms,
`c·(a/d) + b` (BS1) and `(c·a)/d + b` (BS2). Model `model_v6.h` (BS1 by
default, BS2 selectable).

**Experiments** (`oracle/probes/dense/complex_cleanroom/run_x86_64.sh`,
an x86_64 sandbox, results in `results_x86_64/`):

| ID | Probe | Result |
|---|---|---|
| E1 (x86_64) | the 64 Smith variants of E1 (branch rule, fused or not per operation, reciprocal, sign form), 2,000,000 cases | only `le=0 fden=0 fx=0 fy=0 recip=0 negy=0` (all unfused) has 0 mismatches; the next best has 8,529; the aarch64 winner (all fused) fails on 98,250+ |
| E4 (x86_64) | whole-range residual (E4's four distributions), 40,000,000 cases | model v6: **0** mismatches; model v5 (fused): 662,216 |
| E28 | E27's mix (whole range, special/finite mix, small-numerator cancellations, `\|r\|` around `DBL_MIN`) plus the dense-fixture range, 7,000,000 cases | model v6 BS1: **0** in every class; BS2: 137,985 (differs from BS1 only where BS1 is right); model v5: 549,409 |
| fixture | `gen_fixture.cc` on x86_64 → `tests/fixtures/dense/complex_div_cleanroom_x86_64.txt` (4,500 records; 1,746 inputs differ from the aarch64 fixture's because the generator's constructions are evaluated on the host) | `ComplexDivision::Unfused` 4,500 / 4,500 bit for bit (`complex_division_matches_x86_64_oracle_bit_for_bit`); the fused runtime fails on 374 of them, and the unfused one on 373 of the aarch64 records (`runtimes_are_told_apart_by_the_fixtures`) |

**Rust.** `smith` evaluates each multiply-add through `mad(division, x, y,
z)`: `x.mul_add(y, z)` for `Fused` (unchanged), `x * y + z` for `Unfused`.
The scaling and the Annex G recovery are shared (they contain no
multiply-add). `quotient` and `/` remain the fused (aarch64) runtime, whose
fixtures are authoritative for merges until the x86_64 corpus exists
(D-043(5)); `dense::eig_vectors_complex_with(a, storage, division)` selects
the runtime for the eigenvector computation, and `dense_parity.rs`
replays the x86_64 records `dense_eig_large_strict.txt` with `Unfused` (88
comparisons bit for bit; with `Fused`, exactly the two complex-vector fields
differ). Which runtime the port uses by default once the x86_64 corpus is
authoritative (D-043(3)) is a project decision.

## 9. The up-scaling guard, corrected (2026-11-12)

Subject: `UP_SCALE_GUARD` (rule 6 of §3), shared by both runtimes. Implementer:
Only black-box results were used, as in §1;
no GCC / libgcc / glibc source, documentation or compiled code was read.

**Finding.** §8's "0 mismatches" held only because
E1, E4 and E28 never sample `max(|a|,|b|)` or `max(|c|,|d|)` equal to
`0x1.fffffffffffffp+970` (`DBL_MAX·2^-53`, the predecessor of `2^971`). At
that magnitude the x86_64 runtime does **not** up-scale a small numerator
part, while the guard `< 2^971` did. Evidence (all machine divisions of our
own programs in the x86_64 image `sha256:45122c21…`):

- `ComplexDivision::Unfused` with `2^971` disagreed on **996 of 24,261,021**
  replayed records, all at that magnitude, all in the sign of a zero part
  (E25's stream: 126 of 2·10⁷, the residuals already seen in the x86
  analysis; the reviewer's generator: 870 of 3.6·10⁷).
- Bisection (`ab_bisect.txt`, `guard_scan*.txt`): up-scaled at
  `0x1.ffffffffffffep+970`, not at `0x1.fffffffffffffp+970`, for **both**
  maxima (the denominator-only correction still leaves 16 grid cases).
- The x86 analysis found the denominator half independently on E25's exact inputs
  (`oracle/results/x86_64/cdiv/cdiv-compare.txt`):
  0 mismatches with `< 0x1.fffffffffffffp+970`, 126 with `< 2^971`, 1 with
  `< 2^970`.

**Change.** `UP_SCALE_GUARD = 0x1.fffffffffffffp+970` for both maxima and both
variants. For the aarch64 runtime the guard is unobservable anywhere in
`[2^60, 2^971]` (E18, E20), and every committed aarch64 record
(`complex_div_cleanroom.txt`, `dense_strict.txt`, the `eig` records) still
replays bitwise with `Fused`; a re-check measured 0 / 24,261,021 with `Unfused`
and 0 / 5.6·10⁷ for the C++ model with this guard.

**Regression records.** `tests/fixtures/dense/complex_div_guard_x86_64.txt`:
162 machine results at the guard magnitude (the 125 distinct inputs of E25's
126 residuals, identical in the re-check's `e25_v6.txt` and in the x86
analysis; 25 denominator-guard and 12 numerator-guard cases of the
re-check's generator).
`complex_div_cleanroom::complex_division_matches_x86_64_guard_records`:
162 / 162 bitwise with `Unfused`; with the former guard all 162 differ.
