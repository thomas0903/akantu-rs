// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/test_material_fixtures.hh (FriendMaterial::getRandomVector,
// getRandomRotation, applyRotation, reverseRotation, getDeviatoricStrain,
// getHydrostaticStrain, getComposedStrain).
//! Upstream's `test_material_gtest` random inputs, reproduced:
//! every `FriendMaterial` seeds a `std::mt19937` with
//! `::testing::GTEST_FLAG(random_seed)` (0 unless `--gtest_random_seed` is
//! given; the CMake registration passes none) and draws
//! `std::uniform_real_distribution<Real>()` values. Each typed test builds a
//! fresh material, so each test's rotation is the **first**
//! `getRandomRotation()` of a generator seeded with 0.
//!
//! Clean room (D-030): [`Mt19937`] is written from the published algorithm
//! (M. Matsumoto, T. Nishimura, "Mersenne Twister: a 623-dimensionally
//! equidistributed uniform pseudo-random number generator", ACM TOMACS 8,
//! 1998) with the parameters and the seeding recurrence of the C++ standard
//! (`[rand.eng.mers]`, `[rand.predef]`: `mt19937` = `mersenne_twister_engine
//! <uint_fast32_t, 32, 624, 397, 31, 0x9908b0df, 11, 0xffffffff, 7,
//! 0x9d2c5680, 15, 0xefc60000, 18, 1812433253>`, whose 10000th output of a
//! default-constructed engine is 4123659995); no standard-library source was
//! read. The canonical draw and the uniform distribution are
//! `akantu_core::random::{generate_canonical,
//! distributions::UniformRealDistribution}` (the project's clean-room
//! implementation, `crates/akantu-core/src/random/CLEANROOM_LOG.md`). The
//! engine output, the uniform draws and both rotations are compared with
//! the oracle (black-box data of `oracle/probes/upstream_tests_2b`, case
//! `fixtures/model/upstream_tests_2b/material_gtest_random_rotation`).
//!
//! The rotation transcription keeps upstream's operation order (Eigen's
//! `normalized()` divides by `√(Σ v_i²)` summed in index order, `dot` and
//! `cross` component-wise), so it is reproduced bitwise.

#![allow(dead_code)] // each test binary uses a subset

use akantu_core::Real;
use akantu_core::random::UniformRandomBitGenerator;
use akantu_core::random::distributions::UniformRealDistribution;

/// `std::mt19937` (32-bit Mersenne Twister, see the module docs).
#[derive(Clone, Debug)]
pub struct Mt19937 {
    state: [u32; 624],
    index: usize,
}

impl Mt19937 {
    const N: usize = 624;
    const M: usize = 397;
    const MATRIX_A: u32 = 0x9908_b0df;
    const UPPER: u32 = 0x8000_0000;
    const LOWER: u32 = 0x7fff_ffff;

    /// Engine after `seed(value)` (`[rand.eng.mers]`: `x₀ = value mod 2³²`,
    /// `xᵢ = f·(xᵢ₋₁ ⊕ (xᵢ₋₁ ≫ 30)) + i mod 2³²`).
    #[must_use]
    pub fn new(value: u32) -> Self {
        let mut state = [0u32; 624];
        state[0] = value;
        for i in 1..Self::N {
            let prev = state[i - 1];
            state[i] = 1_812_433_253u32
                .wrapping_mul(prev ^ (prev >> 30))
                .wrapping_add(i as u32);
        }
        Self {
            state,
            index: Self::N,
        }
    }

    fn twist(&mut self) {
        for i in 0..Self::N {
            let y = (self.state[i] & Self::UPPER) | (self.state[(i + 1) % Self::N] & Self::LOWER);
            let mut x = self.state[(i + Self::M) % Self::N] ^ (y >> 1);
            if y & 1 != 0 {
                x ^= Self::MATRIX_A;
            }
            self.state[i] = x;
        }
        self.index = 0;
    }

    /// Next tempered output (`g()`).
    pub fn next_u32(&mut self) -> u32 {
        if self.index >= Self::N {
            self.twist();
        }
        let mut z = self.state[self.index];
        self.index += 1;
        z ^= z >> 11;
        z ^= (z << 7) & 0x9d2c_5680;
        z ^= (z << 15) & 0xefc6_0000;
        z ^= z >> 18;
        z
    }
}

impl UniformRandomBitGenerator for Mt19937 {
    const MIN: u64 = 0;
    const MAX: u64 = u32::MAX as u64;
    fn next_value(&mut self) -> u64 {
        u64::from(self.next_u32())
    }
}

/// The generator of a fresh `FriendMaterial` (`engine.seed(random_seed = 0)`).
#[must_use]
pub fn friend_material_generator() -> Mt19937 {
    Mt19937::new(0)
}

/// Column-major `D × D` (`m[col][row]`).
pub type Mat<const D: usize> = [[Real; D]; D];

fn norm(v: &[Real]) -> Real {
    let mut s = 0.;
    for x in v {
        s += x * x;
    }
    s.sqrt()
}

/// Upstream `getRandomVector()`.
pub fn random_vector(engine: &mut Mt19937, dim: usize) -> Vec<Real> {
    let mut dis = UniformRealDistribution::new(0., 1.);
    let mut v = vec![0.; dim];
    while norm(&v) < 1e-9 {
        for x in v.iter_mut() {
            *x = dis.sample(engine);
        }
    }
    v
}

fn normalized(v: &[Real]) -> Vec<Real> {
    let n = norm(v);
    v.iter().map(|x| x / n).collect()
}

fn cross(a: [Real; 3], b: [Real; 3]) -> [Real; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Upstream `getRandomRotation()` (column-major): the columns `v1` (random,
/// normalised), `v2` (2-D: `e3 × v1`; 3-D: a second random vector
/// orthogonalised against `v1` and normalised) and `v3 = v1 × v2`, then
/// `transposeInPlace()` (so the rows are `v1, v2, v3`).
pub fn random_rotation<const D: usize>(engine: &mut Mt19937) -> Mat<D> {
    let v1 = normalized(&random_vector(engine, D));
    let mut cols = vec![v1.clone()];
    if D == 2 {
        let v2 = cross([0., 0., 1.], [v1[0], v1[1], 0.]);
        cols.push(vec![v2[0], v2[1]]);
    }
    if D == 3 {
        let v2 = random_vector(engine, 3);
        let d = v2[0] * v1[0] + v2[1] * v1[1] + v2[2] * v1[2];
        let w: Vec<Real> = (0..3).map(|i| v2[i] - d * v1[i]).collect();
        let v2 = normalized(&w);
        let v3 = cross([v1[0], v1[1], v1[2]], [v2[0], v2[1], v2[2]]);
        cols.push(v2);
        cols.push(v3.to_vec());
    }
    // rotation(i, j) = cols[j][i]; transposeInPlace → rotation(i, j) = cols[i][j];
    // column-major storage m[col][row] = rotation(row, col) = cols[row][col]
    std::array::from_fn(|c| std::array::from_fn(|r| cols[r][c]))
}

/// `A·B` (column-major).
pub fn matmul<const D: usize>(a: &Mat<D>, b: &Mat<D>) -> Mat<D> {
    std::array::from_fn(|c| {
        std::array::from_fn(|r| {
            let mut s = 0.;
            for k in 0..D {
                s += a[k][r] * b[c][k];
            }
            s
        })
    })
}

/// `Aᵀ`.
pub fn transpose<const D: usize>(a: &Mat<D>) -> Mat<D> {
    std::array::from_fn(|c| std::array::from_fn(|r| a[r][c]))
}

/// Upstream `applyRotation(A, R) = R A Rᵀ`.
pub fn apply_rotation<const D: usize>(a: &Mat<D>, r: &Mat<D>) -> Mat<D> {
    matmul(&matmul(r, a), &transpose(r))
}

/// Upstream `reverseRotation(A, R) = Rᵀ A R`.
pub fn reverse_rotation<const D: usize>(a: &Mat<D>, r: &Mat<D>) -> Mat<D> {
    matmul(&matmul(&transpose(r), a), r)
}

/// `‖A‖∞` (Eigen `lpNorm<Infinity>`: largest absolute entry).
pub fn lp_inf<const D: usize>(a: &Mat<D>) -> Real {
    a.iter().flatten().fold(0.0, |m: Real, v| m.max(v.abs()))
}

/// `A − B`.
pub fn sub<const D: usize>(a: &Mat<D>, b: &Mat<D>) -> Mat<D> {
    std::array::from_fn(|c| std::array::from_fn(|r| a[c][r] - b[c][r]))
}

/// Upstream `getComposedStrain(intensity)` (`(hydro + dev)·intensity`),
/// top-left `D × D` block, column-major.
pub fn composed_strain<const D: usize>(intensity: Real) -> Mat<D> {
    let dev = [[0., 1., 2.], [1., 0., 3.], [2., 3., 0.]];
    std::array::from_fn(|c| {
        std::array::from_fn(|r| {
            let h = if r == c { intensity } else { 0. };
            (h + dev[r][c] * intensity) * intensity
        })
    })
}

/// Upstream `getDeviatoricStrain(intensity)` (3 × 3).
pub fn deviatoric_strain(intensity: Real) -> Mat<3> {
    let dev = [[0., 1., 2.], [1., 0., 3.], [2., 3., 0.]];
    std::array::from_fn(|c| std::array::from_fn(|r| dev[r][c] * intensity))
}

/// Upstream `getHydrostaticStrain(intensity)` (3 × 3).
pub fn hydrostatic_strain(intensity: Real) -> Mat<3> {
    std::array::from_fn(|c| std::array::from_fn(|r| if r == c { intensity } else { 0. }))
}
