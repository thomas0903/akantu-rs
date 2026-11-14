// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_random_generator.hh
// (RandomGenerator<T>), src/common/aka_extern.cc:80-85 (default seeds 5489),
// src/common/aka_common.cc:104-117 (seed policy of akantu::initialize).

//! The process-global generators `RandomGenerator<Idx>` / `RandomGenerator<UInt>`
//! and Akantu's seed policy.

use std::marker::PhantomData;
use std::sync::{Mutex, MutexGuard, PoisonError};

use super::engine::{MinstdRand0, UniformRandomBitGenerator};
use crate::types::Rank;

/// Seed and engine of one global generator (upstream static members
/// `_seed` and `generator`).
#[doc(hidden)]
#[derive(Debug)]
pub struct GlobalEngine {
    seed: i64,
    engine: MinstdRand0,
}

/// Default seed of both generators (`aka_extern.cc`).
const DEFAULT_SEED: i64 = 5489;

static IDX_ENGINE: Mutex<GlobalEngine> = Mutex::new(GlobalEngine {
    seed: DEFAULT_SEED,
    engine: MinstdRand0::with_seed_long(DEFAULT_SEED),
});
static UINT_ENGINE: Mutex<GlobalEngine> = Mutex::new(GlobalEngine {
    seed: DEFAULT_SEED,
    engine: MinstdRand0::with_seed_long(DEFAULT_SEED),
});

mod sealed {
    pub trait Sealed {}
}

/// Selects one of upstream's two independent static generators.
pub trait GeneratorSlot: sealed::Sealed {
    #[doc(hidden)]
    fn cell() -> &'static Mutex<GlobalEngine>;
}

/// `RandomGenerator<Idx>`: the generator used by every `RandomParameter`
/// and seeded by `akantu::initialize` (upstream `Idx` = `Int`, so
/// `RandomGenerator<Int>` is the same object).
#[derive(Debug)]
pub enum IdxSlot {}

/// `RandomGenerator<UInt>`: a second, independent generator (default seed
/// 5489, never seeded by `initialize`; used by a few upstream tests).
#[derive(Debug)]
pub enum UIntSlot {}

impl sealed::Sealed for IdxSlot {}
impl sealed::Sealed for UIntSlot {}
impl GeneratorSlot for IdxSlot {
    fn cell() -> &'static Mutex<GlobalEngine> {
        &IDX_ENGINE
    }
}
impl GeneratorSlot for UIntSlot {
    fn cell() -> &'static Mutex<GlobalEngine> {
        &UINT_ENGINE
    }
}

/// Upstream `akantu::RandomGenerator<T>`: a handle to a process-global
/// `std::default_random_engine` with a static seed (`seed(long)` /
/// `seed()`), shared by all handles of the same slot.
///
/// The engine is protected by a mutex; for reproducible draws, callers must
/// still use it sequentially in upstream order (see the module docs).
#[derive(Debug)]
pub struct RandomGenerator<S: GeneratorSlot = IdxSlot>(PhantomData<S>);

impl<S: GeneratorSlot> Default for RandomGenerator<S> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<S: GeneratorSlot> RandomGenerator<S> {
    /// `min()` of the engine.
    pub const MIN: u32 = 1;
    /// `max()` of the engine (2³¹−2).
    pub const MAX: u32 = 2_147_483_646;

    /// A handle (upstream `RandomGenerator<T> gen;`).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn lock() -> MutexGuard<'static, GlobalEngine> {
        S::cell().lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Static `seed(long s)`: stores `s` and re-seeds the engine with the
    /// implicit `long` → `unsigned long` conversion.
    pub fn seed(s: i64) {
        let mut g = Self::lock();
        g.seed = s;
        g.engine.seed_long(s);
    }

    /// Static `seed()`: the last seed set (5489 by default).
    #[must_use]
    pub fn get_seed() -> i64 {
        Self::lock().seed
    }

    /// `operator()`: next engine value.
    #[must_use]
    pub fn next_u32(&mut self) -> u32 {
        Self::lock().engine.next_u32()
    }

    /// Run `f` with exclusive access to the global engine (one lock for a
    /// whole batch of draws).
    pub fn with_engine<R>(f: impl FnOnce(&mut MinstdRand0) -> R) -> R {
        f(&mut Self::lock().engine)
    }

    /// Copy of the current engine state (for diagnostics and tests).
    #[must_use]
    pub fn engine_snapshot() -> MinstdRand0 {
        Self::lock().engine.clone()
    }

    /// Upstream `printself`: `RandGenerator [seed=<seed>]`.
    #[must_use]
    pub fn describe() -> String {
        format!("RandGenerator [seed={}]", Self::get_seed())
    }
}

impl<S: GeneratorSlot> UniformRandomBitGenerator for RandomGenerator<S> {
    const MIN: u64 = MinstdRand0::MIN;
    const MAX: u64 = MinstdRand0::MAX;

    fn next_value(&mut self) -> u64 {
        u64::from(Self::lock().engine.next_u32())
    }
}

/// C `atol` as implemented by glibc (`strtol(s, NULL, 10)`): skips leading
/// whitespace, optional sign, decimal digits up to the first non-digit;
/// saturates at `LONG_MIN`/`LONG_MAX`; 0 if no digits.
#[must_use]
pub fn atol(s: &str) -> i64 {
    let t = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let mut acc: i64 = 0;
    for b in digits.bytes().take_while(u8::is_ascii_digit) {
        let d = i64::from(b - b'0');
        acc = if neg {
            match acc.checked_mul(10).and_then(|v| v.checked_sub(d)) {
                Some(v) => v,
                None => return i64::MIN,
            }
        } else {
            match acc.checked_mul(10).and_then(|v| v.checked_add(d)) {
                Some(v) => v,
                None => return i64::MAX,
            }
        };
    }
    acc
}

/// C `atoi` as implemented by glibc (`(int) strtol(s, NULL, 10)`): [`atol`]
/// truncated to `int` (GCC defines the out-of-range conversion modulo 2³²).
/// Upstream `initialize` reads `AKA_DEBUG_LEVEL` with it.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // glibc: (int) strtol(...), modulo 2^32
pub fn atoi(s: &str) -> i32 {
    atol(s) as i32
}

/// Seed policy of `akantu::initialize` (`aka_common.cc`):
///
/// 1. environment variable `AKA_SEED` (parsed with `atol`), else
/// 2. command-line option `--aka_seed`, else
/// 3. the input file's global `seed` parameter, else
/// 4. `time(nullptr)`;
///
/// then multiplied by `prank + 1` (C `long` arithmetic; wrapping here where
/// C would overflow). The caller passes the result to
/// [`RandomGenerator::<IdxSlot>::seed`](RandomGenerator::seed).
#[must_use]
pub fn initial_seed(
    env_aka_seed: Option<&str>,
    cli_aka_seed: Option<i64>,
    input_file_seed: Option<i64>,
    time_now: i64,
    prank: Rank,
) -> i64 {
    let seed = match (env_aka_seed, cli_aka_seed, input_file_seed) {
        (Some(env), _, _) => atol(env),
        (None, Some(cli), _) => cli,
        (None, None, Some(file)) => file,
        (None, None, None) => time_now,
    };
    seed.wrapping_mul(i64::from(prank) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atol_semantics() {
        assert_eq!(atol("123"), 123);
        assert_eq!(atol("  -42abc"), -42);
        assert_eq!(atol("+7"), 7);
        assert_eq!(atol("x1"), 0);
        assert_eq!(atol(""), 0);
        assert_eq!(atol("99999999999999999999"), i64::MAX);
        assert_eq!(atol("-99999999999999999999"), i64::MIN);
        assert_eq!(atoi(" 7x"), 7);
        assert_eq!(atoi("4294967297"), 1);
        assert_eq!(atoi("-1"), -1);
    }

    #[test]
    fn seed_policy_priority() {
        assert_eq!(initial_seed(Some("10"), Some(20), Some(30), 40, 0), 10);
        assert_eq!(initial_seed(None, Some(20), Some(30), 40, 1), 40);
        assert_eq!(initial_seed(None, None, Some(30), 40, 2), 90);
        assert_eq!(initial_seed(None, None, None, 40, 3), 160);
    }
}
