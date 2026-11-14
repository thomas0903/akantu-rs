# Clean-room provenance log — `akantu-parser` stream conversions (D-030)

Scope: `src/stream.rs` (all items of `CLEANROOM_API.md`) and the
distribution printing of `src/param.rs` (`impl Display for RandomParameter`,
`RandomDistribution::parameters_text`), plus the CLI floating-point
conversion that replaced `cli.rs`'s former `stream_double`.

## Implementer and boundary

- Implementer: a coding agent of the port, 2026-11-09, starting from a
  stub commit of these files.
- **Not read** at any time: libstdc++ or libc++ sources or headers
  (`/usr/include/c++/**` in the oracle image or on the host, GCC/LLVM source
  trees); the removed implementation of `stream.rs`/`param.rs` (no
  `git log -p`, `git show`, `git diff`, `git blame` of these files at commits
  before the stub; no look at earlier versions of them).
- Read (permitted): the stubbed files (signatures and doc
  comments only, plus the unit test `random_parameter_printing` that was left
  in `param.rs` — behavioural expectations, not implementation); the rest of
  the crate (grammar, expressions, sections, CLI, registry, integration
  tests and committed oracle fixtures). The pre-existing `cli.rs` contained a
  helper `stream_double`; it was not part of the stripped set, and it has been
  replaced by the clean-room `stream::extract_double` below.
- Third-party code read: upstream Akantu (LGPL-3.0-or-later) —
  `src/io/parser/parser_tmpl.hh` (how `operator T()`, `operator bool()`,
  `operator std::vector<std::string>()` call the streams/regex),
  `src/common/aka_random_generator.hh` (`RandomParameter::printself`,
  `RandomDistributionProxy::printself`, `make_random_parameter`),
  `src/io/parser/cppargparse/` (`checkType`, `addValue`, `parse`),
  `src/common/aka_common.cc` (option registration). Rust standard library
  documentation (MIT/Apache-2.0) for `str::parse::<f64>` and `{:.N}`/`{:.Ne}`
  formatting.

## Specifications used

- ISO C++ (as known to the implementer; no copy of an implementation was
  consulted): `[istream::sentry]` (white-space skipping, `failbit|eofbit` on
  exhausted input, no extraction), `[istream.formatted.arithmetic]` (`short`
  and `int` read through `long` then clamped with `failbit`; character
  extractors read one character), `[facet.num.get.virtuals]` stage 1
  (`%d`/`%u`/`%g` for the default `dec` base), stage 2 (accumulation of
  atoms "allowed as the next character of the field"), stage 3 (conversion as
  by `strtoll`/`strtoull`/`strtod`: zero + `failbit` if the field is not
  completely converted, bound + `failbit` on overflow) and the `bool` rules
  (`boolalpha`: incremental matching of `truename()`/`falsename()`, iterator
  left after the last matched character, `false` stored on failure;
  `noboolalpha`: `long`, `0`/`1` → `false`/`true`, otherwise `true` +
  `failbit`), `[facet.num.put.virtuals]` / `[ostream.inserters.arithmetic]`
  (default floatfield ⇒ `%g` with the stream precision, `scientific` ⇒ `%e`).
- ISO C: `isspace` in the "C" locale, `strtod` correct rounding, `printf`
  `%e` (≥ 2 exponent digits) and `%g` (choice of style from the exponent X
  after rounding to P digits, P = max(precision, 1), trailing zeros removed).
- ECMAScript regex semantics as used by upstream (`[[:space:]]` in the "C"
  locale; `[^,]+` maximal runs).

## Black-box experiments

All run in `akantu-oracle:dev` (GCC 14.2.0, glibc 2.41, aarch64, image
`sha256:6683233d0a3b…`), `--memory 4g`, oracle volume read-only, built with
the oracle flags (`oracle/probes/parser/CMakeLists.txt`). Only the compiled
programs' behaviour was observed.

| Probe | Observes | Fixture |
|---|---|---|
| `oracle/probes/parser/stream_blackbox.cc` | `ss >> T` for `short`, `int`, `long`, `long long`, the unsigned types, `signed char`, `unsigned char`, `double`, `std::string`, `bool` (`boolalpha`, `noboolalpha`, Akantu's two-step conversion): stored value (sentinels detect "not written"), `fail`/`eof`/`bad`, consumed characters, over 1328 inputs (curated + 1150 deterministic random strings); Akantu's regex list conversion; `os << double` default, `scientific` and `setprecision` at 11–12 precisions for 700 doubles (special values, ties, random bit patterns); `os << distribution` for the 11 distributions × 24 parameter pairs as built by `make_random_parameter` | `tests/fixtures/parser/stream_blackbox.jsonl` |
| `oracle/probes/parser/cli_probe.cc` + `cli_cases.py` | Akantu `cppargparse` on 115 command lines × 4 parse flags (7 argument sets): exit status, message, help, typed values, remaining argv, `printself` | `tests/fixtures/parser/cli_blackbox.jsonl` |

Regenerate with `oracle/probes/parser/run.sh`.

## Observations → implementation

| Item | Observation (fixture) | Implementation |
|---|---|---|
| `is_c_space`, `trim_c_space` | ` \t\n\v\f\r` skipped by the sentry; `0xa0`, `0xff` not | byte test; trimming both ends |
| sentry | blank/empty input: nothing stored, `fail+eof`, all consumed | `value: None`, `fail`, `eof` |
| integer stage 2 | `[+-]?[0-9]*` accumulated, stops at the first other byte (`+-1` consumes 1, `0x1A` consumes 1, `1e5` consumes 1, `- 1` consumes 1); `eof` iff the end was reached | state machine in `extract_int` |
| integer stage 3 | no digit → `0` + `fail`; signed: exact value or the bound on its side + `fail` (also `short`/`int`); unsigned: magnitude > MAX → MAX + `fail` regardless of sign, else magnitude negated modulo 2ⁿ for `-` (`-1` → MAX, `-4294967295` → 1 for `unsigned`) | `StreamInt::{MAX_U64, NEG_MIN_U64, from_magnitude}` and the range test in `extract_int` |
| character types | `signed char`/`unsigned char` read one byte, never fail unless blank, no `eof` | `StreamInt::CHARACTER`, `from_byte` |
| `double` stage 2 | `[+-]? D* (. D*)?` then `[eE][+-]?D*` only if a mantissa digit was seen (`+.e1` consumes 2, `7e.` consumes 2, `1e5.5` consumes 3, `1..2` consumes 2) | `extract_double` |
| `double` stage 3 | incomplete field (`1e`, `1e+`, `.`, `-`) → `0` + `fail`; overflow (`1e400`, `2e308`, `1.7976931348623159e308`) → `±DBL_MAX` + `fail`; underflow (`1e-400`, `2.4e-324`) → correctly rounded 0/subnormal, no `fail`; `-0` → `-0.0` | Rust's correctly rounded `str::parse::<f64>` on the validated field, infinity mapped to `±f64::MAX` + `fail` |
| `bool` `boolalpha` | `true`/`false` matched case-sensitively; matched prefix consumed (`trUe` consumes 2, `ftrue` 1); full match stops without `eof`; end of input → `fail+eof` | `extract_bool(.., true)` |
| `bool` `noboolalpha` | `long` extraction; 0 → false, 1 → true, other (incl. overflow bounds) → true + `fail`; no digits → false + `fail` | `extract_bool(.., false)` |
| Akantu `operator bool()` | retry from the position after the `boolalpha` attempt (`1` → true, `tru` → false, `  42` → true) | `parse_bool` |
| `std::string` | first white-space-delimited word; blank → untouched + `fail` | `extract_word`, `word_bytes`, `parse_word` |
| string list/set | white space and brackets removed byte-wise (high bytes kept), maximal non-`,` runs | `string_list_bytes`, `parse_string_list`, `parse_string_set` |
| `%e` | `d.ddd` + `e±XX` (≥ 2 digits), precision 0 has no point, ties to even (`1.5`→`2e+00`, `2.5`→`2e+00`), exact digits at precision 40; `inf`, `-inf`, `nan`, `-nan` | `format_scientific` (Rust exact `{:.Ne}` + C exponent) |
| `%g` | C rule on X after rounding; trailing zeros stripped; default precision 6 | `format_general`, `format_double` |
| distributions | `%.17e` reals separated by spaces; `normal` appends `0`; distributions with an internal normal append `0 1 0`; gamma-based ones embed gamma(`a/2`, 1) (chi², Fisher F) or gamma(`a/2`, 2) (Student t); `weibull` prints its constructor order (b, a) | `RandomDistribution::parameters_text`, `Display for RandomParameter` |
| CLI `checkType`/`addValue` | `_float` rejects `1e400`, `2e308`, `1e`, `.`, `nan`, `inf`, `-`, empty; accepts `1e-400` (→ 0), `0x10` (→ 0), `1e5x`, `1,5` (→ 1) | `cli.rs` uses `stream::extract_double` (`fail` → rejected) |

## Verification

`cargo test -p akantu-parser --test stream_blackbox` compares every
observation exactly (values by bit pattern, flags, consumed counts, text).
`tests/cli_oracle.rs` does the same for the CLI fixture. The parser oracle
test, `test_parser.verified` byte equality and the hand-derived value tests
pass unchanged. Results (2026-11-09): all observations reproduced on
aarch64-apple-darwin (macOS 27.2) and on Linux aarch64 in
`akantu-rust-test:trixie-arm64` (glibc 2.41-12+deb13u4, Rust 1.99.1):
1 328 inputs, 700 doubles, 264 distributions, 460 command lines, 0
mismatches. The Rust standard library's decimal conversions (correctly
rounded parsing; exact, ties-to-even formatting) agree with glibc on every
recorded case, including ties and 40-digit expansions.

## Compliance statement

The implementer of the items above did not read libstdc++/libc++ source or
headers, nor the removed implementation, at any time during this work
package; the only C++ standard-library knowledge used is the public
specification and the observations listed here.

## Integration note (2026-11-10)

At the integration, `impl Display for RandomParameter` and
`RandomDistribution::parameters_text` were changed mechanically to
**delegate** to the clean-room printing of `akantu_core::random`
(`Display for akantu_core::random::RandomParameter`,
`Distribution::params_string`; provenance
`crates/akantu-core/src/random/CLEANROOM_LOG.md`), through the bridge
`RandomParameter::to_core` (`docs/ARCHITECTURE.md`), so the port has one
distribution printer. Nothing was re-derived. The former bodies recorded
above are kept verbatim as the differential reference of
`tests/random_bridge.rs` (≈22 000 cases, 0 differences), and the
observations above (264 distribution texts) plus the oracle corpus
`printself` records pass unchanged through the delegated printing. The
integrator did not read libstdc++/libc++ sources or headers.
