<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Random-number oracle probes

Stand-alone C++ programs (standard library only, no Akantu build needed) that
record the behaviour of Akantu's random-number stack on the oracle toolchain,
and the generator of the Rust libm tables.

| File | Purpose |
|---|---|
| `dump_random.cc` | Engine (`std::default_random_engine` = `minstd_rand0`), `generate_canonical<double,53>`, all 11 distributions wrapped by Akantu, and a verbatim re-implementation of Akantu's `RandomGenerator` / `RandomParameter` / `make_random_parameter` (`src/common/aka_random_generator.hh`, `aka_extern.cc`). Writes `engine.json`, `canonical.json`, `distributions.json`, `random_parameter.json`. |
| `libm_vectors.cc` | Reference hashes of glibc `log`, `exp`, `pow`, `tan` over deterministic input sets (splitmix64), plus special inputs → `libm.json`. |
| `libm_special.cc` | glibc `pow` on the Cartesian product of 58 special values (signed zeros, integers, subnormals, exponent transitions, infinities, quiet/signalling NaNs of both signs) and `log`/`exp`/`tan` on each → `libm_special_cartesian.json`. |
| `cleanroom/` | Clean-room black-box experiments behind `engine.rs` / `distributions_impl.rs` (D-030) and the edge fixture `cleanroom_edges.json`; see `cleanroom/README.md` and `crates/akantu-core/src/random/CLEANROOM_LOG.md`. |
| `run.sh` | Builds and runs both probes in Docker in three variants (`strict` = `-O2 -ffp-contract=off`, `default` = `-O2`, `O0`), reports differences and refreshes `crates/akantu-core/tests/fixtures/random/` (from `strict`). `--platform linux/amd64 --image <img>` runs elsewhere without touching the fixtures. |
| `gen_libm_tables.py` | Generates `crates/akantu-core/src/random/libm_tables.rs` from the glibc sources (reproducing their upstream copyright notices) and verifies every table byte-for-byte in the oracle's `libm.so.6`. |

Doubles are stored as 16-hex-digit IEEE-754 bit patterns; seeds as decimal
strings (C `long int`).

## Fixture provenance

* Image `akantu-oracle:dev` (Debian trixie, **aarch64**), `g++ (Debian 14.2.0-19) 14.2.0`,
  libstdc++ `_GLIBCXX_RELEASE` 14 (`__GLIBCXX__` 20250315), glibc
  `2.41-12+deb13u4`, `libm.so.6` sha256
  `a8afb2d1f4a0c5e14c0fc82f794ec839eafe29738907aa92895410f57630bbe2`.
* Flags `-std=c++20 -O2 -ffp-contract=off` (the oracle's `strict`
  configuration, `docs/ORACLE_BUILD.md`). `sizeof(long)` =
  `sizeof(uint_fast32_t)` = 8, `FLT_EVAL_METHOD` 0, `__FP_FAST_FMA` defined.
* N = 1000 values per sequence; engine seeds 0, 1, 2, 42, 5489, 123456,
  1496137735, 2992275470, 2³¹−2, 2³¹−1, 2³¹, 2³²−1, 2³², 2³²+1, −1, −2, −42,
  −(2³¹−1), −2³¹, `LONG_MAX`, `LONG_MIN`, `LONG_MIN+1`; distributions with
  seeds 5489, 42, 123456 and 2–6 parameter sets each.

## Findings

1. **Contraction (`-ffp-contract`) matters.** With GCC's default
   `-ffp-contract=fast` on aarch64, libstdc++'s own arithmetic is contracted
   into FMAs (e.g. `sum += x * tmp` in `generate_canonical`,
   `u * (b - a) + a`, `ret * stddev + mean`). Compared with `strict`:
   `generate_canonical` differs by 1 ulp in ≈26 % of the draws; every
   distribution differs in 4–72 % of the draws (up to ~2.4·10³ ulp, 4.9·10⁴ ulp
   for one Student-t value, where cancellation amplifies the canonical
   difference). The number of engine calls was unchanged in all 108 sequences.
   `-O0` equals `strict`. On x86_64 (no FMA in the default ISA) `default` =
   `strict`. → Golden data with random fields must come from the `strict`
   oracle, or be compared with a tolerance.
2. **glibc libm differs between architectures.** Engine, canonical, uniform
   and all `RandomParameter` scenarios are bit-identical on aarch64 and
   x86_64 (Debian trixie amd64 under Rosetta, `strict`). Distributions that call
   libm differ in rare draws (10 of 108 sequences had 1–2 differing values out
   of 1000, all 1–2 ulp: gamma, weibull, lognormal, fisher_f), and all
   2²²-input libm hashes of `libm.json` differ (two of the 2¹⁶ hashes
   coincide): glibc's generic C code is compiled with GCC's default
   contraction on aarch64 (`fmadd`), while the x86_64 build selects, via
   ifunc, a non-FMA variant on CPUs without AVX2+FMA (as under Rosetta) and
   an FMA variant (`__ieee754_log_fma`, `__ieee754_exp_fma`,
   `__ieee754_pow_fma`, `__tan_fma`) otherwise.
   *Static analysis* of those x86_64 FMA variants (Debian `libc6-dbg`
   symbols) shows the same fused operations as the aarch64 build in `log`,
   `pow` (log and exp parts) and `tan` (|x| ≤ 25 paths). The one structural
   difference is the exp argument reduction: aarch64 (`TOINT_INTRINSICS`)
   rounds the rounded product `x·N/ln2` half-away (`frinta`), x86_64 computes
   `fma(x, N/ln2, 0x1.8p52) − 0x1.8p52` (exact product, ties to even); the two
   `k` agreed on 4·10⁸ random inputs. So an x86_64 oracle on AVX2 hardware
   (e.g. cloud instances) is *expected* to match aarch64, but this was not
   verified by execution (no AVX2 host available locally); an x86_64 oracle
   without AVX2 does not match.
3. **Evaluation order.** `fisher_f` evaluates `gd_x` before `gd_y`, and
   `student_t` evaluates the normal before the gamma (both unsequenced in
   C++; GCC 14 order, confirmed by the fixtures).
4. A `RandomParameter` without distribution is `uniform [0, 0]` and still
   consumes two engine calls per value.

The Rust port (`crates/akantu-core/src/random/`) reproduces the aarch64
`strict` oracle bit-for-bit on every platform: libm is a pure-Rust port of the
oracle's aarch64 glibc instruction sequences (`f64::mul_add` exactly where the
binary uses `fmadd`), verified against glibc on 1.4·10⁹ inputs on Linux
aarch64 and against the committed hashes everywhere.

## Regenerating

```sh
oracle/probes/random/run.sh                      # fixtures (aarch64 oracle image)
# libm tables: needs the Debian glibc source and the oracle's libm.so.6
docker run --rm -v "$PWD/tmp":/o akantu-oracle:dev cp -L /usr/lib/aarch64-linux-gnu/libm.so.6 /o/
#   apt-get source glibc=2.41-12+deb13u4 (debian:trixie with deb-src), then apply
#   debian/patches/git-updates.diff to the orig tree
python3 -I oracle/probes/random/gen_libm_tables.py --glibc-src glibc-2.41 \
  --libm tmp/libm.so.6 > crates/akantu-core/src/random/libm_tables.rs
```
