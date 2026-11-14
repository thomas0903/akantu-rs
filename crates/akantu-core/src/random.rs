// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_random_generator.hh
// (RandomGenerator, RandomDistributionType, RandomDistribution(Proxy),
// RandomParameter, make_random_parameter), src/common/aka_extern.cc (default
// seeds), src/common/aka_common.cc (seed policy) and the draw order of
// src/model/common/constitutive_laws/{internal_field_tmpl.hh,
// random_internal_field_tmpl.hh}. The engine and distributions reproduce the
// behaviour of the oracle's C++ standard library (GCC 14 libstdc++) by a
// clean-room implementation (D-030) from the C++ standard, published
// algorithms and black-box experiments: random/CLEANROOM_LOG.md.

//! Bit-compatible random numbers (decision D-006).
//!
//! Akantu samples random material parameters (`RandomParameter`, e.g.
//! `sigma_c = 300e6 weibull [30e6, 5]`) with `std::default_random_engine` and
//! the standard distributions of the C++ library it is compiled with. The
//! oracle is built with GCC/libstdc++ on Debian trixie, where
//! `default_random_engine` is `minstd_rand0`. This module reproduces that
//! stack exactly:
//!
//! | Upstream | Rust |
//! |---|---|
//! | `std::minstd_rand0` (`std::default_random_engine`) | [`MinstdRand0`] |
//! | `std::generate_canonical<double, 53>` | [`generate_canonical`] |
//! | `std::uniform_real_distribution<double>` … `std::student_t_distribution<double>` | [`distributions`] |
//! | `RandomDistributionType` | [`RandomDistributionType`] |
//! | `RandomDistributionProxy<Real, D>` (type-erased distribution with state) | [`Distribution`] |
//! | `RandomParameter<Real>`, `make_random_parameter` | [`RandomParameter`], [`RandomParameter::make`] |
//! | `RandomGenerator<Idx>`, `RandomGenerator<UInt>` (process-global statics) | [`RandomGenerator`] |
//! | seed policy of `akantu::initialize` | [`initial_seed`] |
//! | `InternalField::reset/resize` + `RandomInternalField::setArrayValues` | [`fill_element_type_map_array`] |
//! | glibc `log`, `exp`, `pow`, `tan` (called by libstdc++) | `libm` (private, pure-Rust ports) |
//!
//! # Bit compatibility
//!
//! * The engine, `generate_canonical`, `uniform_real_distribution` and all
//!   `RandomParameter` semantics are plain integer/IEEE-754 arithmetic and are
//!   bit-identical on every platform.
//! * The other distributions call libm. libm results are *not* specified by
//!   IEEE-754 and differ between glibc (oracle), macOS and even glibc on
//!   aarch64 vs x86_64 (GCC contracts glibc's sources into fused
//!   multiply-adds on aarch64). The private `libm` module is therefore a
//!   pure-Rust port of the exact instruction sequences of the oracle's
//!   Debian glibc 2.41 aarch64 `log`, `exp`, `pow` and `tan` (using
//!   [`f64::mul_add`], which is IEEE-754 `fusedMultiplyAdd` and hence
//!   identical everywhere, exactly where the oracle's binary uses `fmadd`).
//!   The Rust results never depend on the host libm.
//! * The reference arithmetic is that of the oracle's `strict` configuration
//!   (`-ffp-contract=off`): libstdc++'s own expressions are evaluated without
//!   contraction. A GCC build with the default `-ffp-contract=fast` on
//!   aarch64 fuses e.g. `sum += x * tmp` in `generate_canonical` and differs
//!   by 1 ulp in about 26 % of the canonical draws (see
//!   `oracle/probes/random/README.md`).
//!
//! # Draw-order contract (for constitutive laws and internal fields)
//!
//! All random parameters of a process share **one** engine,
//! `RandomGenerator<Idx>` (here [`RandomGenerator::<IdxSlot>`]), seeded once by
//! `akantu::initialize` with `seed * (prank + 1)` ([`initial_seed`]). Field
//! values are therefore reproducible only if fields are filled in exactly the
//! upstream order:
//!
//! 1. A `RandomInternalField` draws in `InternalField::internalInitialize`
//!    (via `reset()`), in `resize()` (only the *new* tail of each array, e.g.
//!    after elements were added) and again on `setDefaultValue` /
//!    `setRandomDistribution` (both call `reset()`, which redraws the *whole*
//!    field). The history field `previous_<id>` is a plain `InternalField` and
//!    never draws.
//! 2. Within one `reset()`/`resize()`: ghost types in the order
//!    `_not_ghost`, `_ghost`; within a ghost type, element types in enum order
//!    (only types allocated in the field, regardless of dimension and kind);
//!    within an array, the flat storage order — element-major, then
//!    quadrature point, then component — one draw per scalar
//!    ([`fill_element_type_map_array`] implements this).
//! 3. One draw is `base_value + distribution(engine)`. **A parameter without
//!    a distribution is `uniform [0, 0]` and still consumes two engine calls
//!    per scalar** (`generate_canonical` needs two `minstd_rand0` outputs for
//!    53 bits). Every `RandomInternalField` therefore advances the shared
//!    engine, even when it is effectively deterministic.
//! 4. Engine calls per draw: uniform, exponential, weibull, extreme_value: 2;
//!    cauchy: 2 per attempt (`u == 0.5` is rejected); normal/lognormal: 4 per
//!    polar attempt for every *other* draw (the second value of a pair is
//!    cached in the distribution); gamma/chi_squared/student_t/fisher_f: data
//!    dependent (Marsaglia–Tsang rejection, internal normal cache, plus one
//!    extra canonical draw when alpha < 1).
//! 5. Distribution state (the cached normal value of `normal`, `lognormal`,
//!    `gamma`, …) lives in the parameter object. Upstream copies a
//!    `RandomParameter` *with* that state (`make_unique()` copy-constructs
//!    the std distribution); [`RandomParameter`]'s `Clone` does the same. Each
//!    internal field owns its own copy (made by `setRandomDistribution`).
//! 6. Values are drawn per process (no synchronisation): ghost values differ
//!    from the owner's values unless a material re-synchronises them
//!    explicitly (Marigo synchronises `Yd` with tag `_clh_init_cl`).
//! 7. Upstream is single-threaded: the global engine is behind a mutex here,
//!    but fills must still happen sequentially in the order above for
//!    reproducibility.

mod distributions_impl;
mod engine;
mod fill;
pub(crate) mod fmt_c;
mod global;
pub(crate) mod libm;
mod libm_tables;
mod parameter;

pub use engine::{DefaultRandomEngine, MinstdRand0, UniformRandomBitGenerator, generate_canonical};
pub use fill::{fill_element_type_map_array, fill_slice};
pub use global::{GeneratorSlot, IdxSlot, RandomGenerator, UIntSlot, atoi, atol, initial_seed};
pub use parameter::{Distribution, RandomDistributionType, RandomParameter};

/// The oracle's standard distributions (`std::*_distribution<double>`), with their
/// internal state, as used by Akantu.
pub mod distributions {
    pub use super::distributions_impl::{
        CauchyDistribution, ChiSquaredDistribution, ExponentialDistribution,
        ExtremeValueDistribution, FisherFDistribution, GammaDistribution, LognormalDistribution,
        NormalDistribution, StudentTDistribution, UniformRealDistribution, WeibullDistribution,
    };
}

/// Former path of [`crate::glibc_libm`] (the stable, documented module),
/// kept as a hidden alias for existing callers.
#[doc(hidden)]
pub mod glibc_libm {
    pub use crate::glibc_libm::*;
}
