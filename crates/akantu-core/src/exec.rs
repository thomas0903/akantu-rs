// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the execution policy of the port's kernels (D-123).

//! How a kernel runs: precision mode, threads, SIMD level (D-123,
//! `docs/ARCHITECTURE.md`, "Execution policy").
//!
//! * [`Precision`]: [`Precision::Bitwise`] (default) computes every result
//!   with the operations of the serial reference path in its order;
//!   [`Precision::Fast`] allows kernels that round differently (FMA, sums in
//!   another order), each validated against the bitwise path within a
//!   written tolerance (D-122).
//! * Threads per process: 1 by default. Every result is identical for every
//!   thread count in [`Precision::Bitwise`]: threaded kernels split only
//!   independent outputs, and sums into shared data keep their serial order
//!   ([`NodeScatter`], [`Exec::sum_in_order`]).
//! * [`SimdLevel`]: detected once per process; a policy only holds a level
//!   the CPU supports ([`SimdToken`]). In [`Precision::Bitwise`] a SIMD
//!   kernel performs per lane the scalar sequence, so every level gives the
//!   same bits.
//!
//! An [`ExecPolicy`] is a plain value. An [`Exec`] is a policy with its
//! thread pool (shared by every `Exec` with the same thread count in the
//! process). A model owns one `Exec`, set once from
//! [`ExecPolicy::process_default`] at creation and changeable through the
//! model's setter between steps; kernels receive it (or read it from their
//! engine) and resolve it once per batch, never per element.
//!
//! The process default is read from the environment on first use
//! (`AKANTU_NUM_THREADS`, `AKANTU_PRECISION`, `AKANTU_SIMD`) unless a
//! program set it before with [`ExecPolicy::set_process_default`]. Tests
//! that need another policy set it on their own model, never on the process
//! default (tests of one binary run in parallel threads of one process).

mod pool;
mod scatter;
pub mod simd;

use std::num::NonZeroUsize;
use std::sync::RwLock;

pub use pool::{Exec, Split, Tuples, TuplesRef};
pub use scatter::NodeScatter;
pub use simd::{SimdLevel, SimdToken};

use crate::error::{Error, Result};

/// Largest accepted thread count.
pub const MAX_THREADS: usize = 1024;

/// Environment variable of the process default thread count.
pub const ENV_THREADS: &str = "AKANTU_NUM_THREADS";
/// Environment variable of the process default precision mode.
pub const ENV_PRECISION: &str = "AKANTU_PRECISION";
/// Environment variable of the process default SIMD level.
pub const ENV_SIMD: &str = "AKANTU_SIMD";

/// Precision mode of the kernels (D-122).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Precision {
    /// Every result bit as the serial reference path (default).
    #[default]
    Bitwise,
    /// Kernels may round differently (FMA, other summation orders,
    /// thread-count-dependent reductions); validated against
    /// [`Precision::Bitwise`] within a written tolerance.
    Fast,
}

impl Precision {
    /// The name used by the environment variable and Python.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bitwise => "bitwise",
            Self::Fast => "fast",
        }
    }
}

impl std::str::FromStr for Precision {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "bitwise" => Ok(Self::Bitwise),
            "fast" => Ok(Self::Fast),
            other => Err(Error::invalid(format!(
                "precision mode `{other}`: expected `bitwise` or `fast`"
            ))),
        }
    }
}

/// Precision mode, thread count and SIMD level of a model's kernels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecPolicy {
    precision: Precision,
    threads: NonZeroUsize,
    simd: SimdToken,
}

impl Default for ExecPolicy {
    /// [`ExecPolicy::serial`].
    fn default() -> Self {
        Self::serial()
    }
}

/// The process default; `None` until set or first read from the
/// environment.
static PROCESS_DEFAULT: RwLock<Option<ExecPolicy>> = RwLock::new(None);

impl ExecPolicy {
    /// Bitwise, one thread, the detected SIMD level.
    #[must_use]
    pub fn serial() -> Self {
        Self {
            precision: Precision::Bitwise,
            threads: NonZeroUsize::MIN,
            simd: SimdToken::detected(),
        }
    }

    /// The policy the environment describes: `AKANTU_NUM_THREADS` (a
    /// positive integer, default 1), `AKANTU_PRECISION` (`bitwise`, the
    /// default, or `fast`), `AKANTU_SIMD` (`auto`, the default, `scalar`,
    /// `avx2`, `avx512`, `neon`; a level the CPU lacks is an error). An
    /// empty variable counts as unset.
    ///
    /// # Errors
    /// A variable with a value outside these.
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// [`ExecPolicy::from_env`] with another source of the variables.
    ///
    /// # Errors
    /// As [`ExecPolicy::from_env`].
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let get = |k: &str| get(k).filter(|v| !v.trim().is_empty());
        let mut p = Self::serial();
        if let Some(v) = get(ENV_THREADS) {
            let n = v.trim().parse::<usize>().map_err(|_| {
                Error::invalid(format!("{ENV_THREADS}=`{v}`: expected a positive integer"))
            })?;
            p = p
                .with_threads(n)
                .map_err(|e| Error::invalid(format!("{ENV_THREADS}=`{v}`: {e}")))?;
        }
        if let Some(v) = get(ENV_PRECISION) {
            p = p.with_precision(
                v.parse()
                    .map_err(|e| Error::invalid(format!("{ENV_PRECISION}: {e}")))?,
            );
        }
        if let Some(v) = get(ENV_SIMD)
            && !v.trim().eq_ignore_ascii_case("auto")
        {
            let level: SimdLevel = v
                .parse()
                .map_err(|e| Error::invalid(format!("{ENV_SIMD}: {e}")))?;
            p = p
                .with_simd(level)
                .map_err(|e| Error::invalid(format!("{ENV_SIMD}: {e}")))?;
        }
        Ok(p)
    }

    /// The policy new models start with: the one given to
    /// [`ExecPolicy::set_process_default`], else [`ExecPolicy::from_env`]
    /// (read once, then kept).
    ///
    /// # Errors
    /// An invalid environment variable (the error repeats on every call).
    pub fn process_default() -> Result<Self> {
        if let Some(p) = *PROCESS_DEFAULT.read().unwrap_or_else(|e| e.into_inner()) {
            return Ok(p);
        }
        let mut slot = PROCESS_DEFAULT.write().unwrap_or_else(|e| e.into_inner());
        if let Some(p) = *slot {
            return Ok(p);
        }
        let p = Self::from_env()?;
        *slot = Some(p);
        Ok(p)
    }

    /// Set the policy of models created from now on (Python
    /// `akantu.set_num_threads` and friends). Models that exist keep theirs.
    /// For programs, not for tests: tests of one binary share the process.
    pub fn set_process_default(policy: Self) {
        *PROCESS_DEFAULT.write().unwrap_or_else(|e| e.into_inner()) = Some(policy);
    }

    /// With another precision mode.
    #[must_use]
    pub const fn with_precision(mut self, precision: Precision) -> Self {
        self.precision = precision;
        self
    }

    /// With `threads` threads (1 ≤ `threads` ≤ [`MAX_THREADS`]).
    ///
    /// # Errors
    /// `threads` outside that range.
    pub fn with_threads(mut self, threads: usize) -> Result<Self> {
        match NonZeroUsize::new(threads) {
            Some(n) if threads <= MAX_THREADS => {
                self.threads = n;
                Ok(self)
            }
            _ => Err(Error::invalid(format!(
                "thread count {threads}: expected 1 to {MAX_THREADS}"
            ))),
        }
    }

    /// With SIMD level `level`.
    ///
    /// # Errors
    /// The CPU does not support `level`.
    pub fn with_simd(mut self, level: SimdLevel) -> Result<Self> {
        self.simd = level.token().ok_or_else(|| {
            Error::invalid(format!(
                "SIMD level `{}` is not supported by this CPU (detected: `{}`)",
                level.name(),
                SimdLevel::detected().name()
            ))
        })?;
        Ok(self)
    }

    /// Precision mode.
    #[must_use]
    pub const fn precision(&self) -> Precision {
        self.precision
    }

    /// Thread count.
    #[must_use]
    pub const fn threads(&self) -> usize {
        self.threads.get()
    }

    /// SIMD level.
    #[must_use]
    pub const fn simd(&self) -> SimdLevel {
        self.simd.level()
    }

    /// The SIMD level as a token (proof that the CPU supports it), for the
    /// dispatch of a multi-versioned kernel.
    #[must_use]
    pub const fn simd_token(&self) -> SimdToken {
        self.simd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_string())
        }
    }

    #[test]
    fn defaults_are_serial_bitwise() {
        let p = ExecPolicy::from_lookup(env(&[])).unwrap();
        assert_eq!(p, ExecPolicy::serial());
        assert_eq!(p.threads(), 1);
        assert_eq!(p.precision(), Precision::Bitwise);
        assert_eq!(p.simd(), SimdLevel::detected());
    }

    #[test]
    fn environment_values() {
        let p = ExecPolicy::from_lookup(env(&[
            (ENV_THREADS, " 4 "),
            (ENV_PRECISION, "Fast"),
            (ENV_SIMD, "scalar"),
        ]))
        .unwrap();
        assert_eq!(p.threads(), 4);
        assert_eq!(p.precision(), Precision::Fast);
        assert_eq!(p.simd(), SimdLevel::Scalar);
        let p = ExecPolicy::from_lookup(env(&[(ENV_THREADS, ""), (ENV_SIMD, "auto")])).unwrap();
        assert_eq!(p, ExecPolicy::serial());
    }

    #[test]
    fn invalid_environment_values_are_errors() {
        for pairs in [
            [(ENV_THREADS, "0")],
            [(ENV_THREADS, "-2")],
            [(ENV_THREADS, "two")],
            [(ENV_THREADS, "100000")],
            [(ENV_PRECISION, "fastest")],
            [(ENV_SIMD, "sse9")],
        ] {
            let e = ExecPolicy::from_lookup(env(&pairs)).unwrap_err();
            assert!(e.to_string().contains(pairs[0].0), "{e}");
        }
    }

    #[test]
    fn unsupported_simd_level_is_refused() {
        for level in SimdLevel::ALL {
            let r = ExecPolicy::serial().with_simd(level);
            assert_eq!(r.is_ok(), level.is_supported(), "{level:?}");
        }
        assert!(SimdLevel::Scalar.is_supported());
    }
}
