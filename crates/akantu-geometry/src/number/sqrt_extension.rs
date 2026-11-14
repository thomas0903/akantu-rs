// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/mesh_geom_common.hh (the
// `Root_of_2` type of `cgal::Spherical`, used by mesh_sphere_intersector).
// Derived from CGAL 5.6.2, LGPL-3.0-or-later (CGAL's Number_types package):
//   Number_types/include/CGAL/Sqrt_extension/Sqrt_extension_type.h and
//   Sqrt_extension/Real_embeddable_traits.h
//     Copyright (c) 2006-2008 Max-Planck-Institute Saarbruecken (Germany).
//     Author(s): Michael Hemmer
//   Number_types/include/CGAL/Root_of_traits.h (`Root_of_traits_helper<FT,
//   Field_tag>`: `Root_of_2 = Sqrt_extension<FT, FT, Tag_true, Tag_true>`)
//     Copyright (c) 2005,2006 INRIA Sophia-Antipolis (France).
//     Author(s): Sylvain Pion, Monique Teillaud, Athanasios Kakargias,
//     Michael Hemmer
// No GPL-licensed CGAL code is used in this file (D-042 (3), D-022).

//! `Root_of_2` of `Quotient<MP_Float>`: `Sqrt_extension<FT, FT, Tag_true,
//! Tag_true>`, numbers `a0 + a1·√root` (CGAL 5.6.2).
//!
//! Only the operations used by the spherical-kernel line/sphere solver and
//! the lexicographic comparison of circular-arc points are provided. The
//! `is_extended` flag follows CGAL exactly (it decides `to_double`). The
//! interval filter of CGAL's `sign`/`compare` (`FP_TAG = Tag_true`) is
//! omitted: it only short-cuts the exact computation and cannot change its
//! result.

use std::cmp::Ordering;

use akantu_core::Result;

use super::quotient::Quotient;

/// CGAL `Sqrt_extension<Quotient<MP_Float>, Quotient<MP_Float>, Tag_true,
/// Tag_true>` (the spherical kernel's `Root_of_2`).
#[derive(Clone, Debug)]
pub struct RootOf2 {
    a0: Quotient,
    a1: Quotient,
    root: Quotient,
    is_extended: bool,
}

impl RootOf2 {
    /// `Sqrt_extension(const NT&)`: a rational number (not extended).
    #[must_use]
    pub fn from_rational(a0: Quotient) -> Self {
        Self {
            a0,
            a1: Quotient::from_i32(0),
            root: Quotient::from_i32(0),
            is_extended: false,
        }
    }

    /// `Sqrt_extension(a0, a1, root)` = `make_root_of_2(a0, a1, root)`
    /// (3-argument form): extended (`ACDE_TAG = Tag_true` allows a zero
    /// root).
    #[must_use]
    pub fn new(a0: Quotient, a1: Quotient, root: Quotient) -> Self {
        Self {
            a0,
            a1,
            root,
            is_extended: true,
        }
    }

    /// `a0`.
    #[must_use]
    pub fn a0(&self) -> &Quotient {
        &self.a0
    }

    /// `a1`.
    #[must_use]
    pub fn a1(&self) -> &Quotient {
        &self.a1
    }

    /// `root`.
    #[must_use]
    pub fn root(&self) -> &Quotient {
        &self.root
    }

    /// `is_extended()`.
    #[must_use]
    pub fn is_extended(&self) -> bool {
        self.is_extended
    }

    /// `n · x` (Boost: copy `x`, then `operator*=(const NT&)`:
    /// `a0 *= n; a1 *= n`; the flag is kept).
    ///
    /// # Errors
    /// Exponent overflow.
    pub fn mul_rational(&self, n: &Quotient) -> Result<Self> {
        Ok(Self {
            a0: self.a0.mul(n)?,
            a1: self.a1.mul(n)?,
            root: self.root.clone(),
            is_extended: self.is_extended,
        })
    }

    /// `x + n` (`operator+=(const NT&)`: `a0 += n`).
    ///
    /// # Errors
    /// Exponent overflow.
    pub fn add_rational(&self, n: &Quotient) -> Result<Self> {
        Ok(Self {
            a0: self.a0.add(n)?,
            a1: self.a1.clone(),
            root: self.root.clone(),
            is_extended: self.is_extended,
        })
    }

    /// CGAL `sign_()` (exact sign of an extended number).
    fn sign_ext(a0: &Quotient, a1: &Quotient, root: &Quotient) -> Result<Ordering> {
        let s0 = a0.sign();
        let s1 = a1.sign();
        if s0 == s1 {
            return Ok(s0);
        }
        if s0 == Ordering::Equal {
            return Ok(s1);
        }
        if s1 == Ordering::Equal {
            return Ok(s0);
        }
        let r = a1.mul(a1)?.mul(root)?.sub(&a0.mul(a0)?)?;
        Ok(if s1 == Ordering::Greater {
            r.sign()
        } else {
            r.sign().reverse()
        })
    }

    /// Exact sign.
    ///
    /// # Errors
    /// Exponent overflow.
    pub fn sign(&self) -> Result<Ordering> {
        if !self.is_extended {
            return Ok(self.a0.sign());
        }
        Self::sign_ext(&self.a0, &self.a1, &self.root)
    }

    /// CGAL `compare(const NT&)` (exact).
    ///
    /// # Errors
    /// Exponent overflow.
    pub fn compare_rational(&self, num: &Quotient) -> Result<Ordering> {
        if !self.is_extended {
            return self.a0.compare(num);
        }
        Self::sign_ext(&self.a0.sub(num)?, &self.a1, &self.root)
    }

    /// CGAL `compare(const Self&, in_same_extension = false)` (exact; the
    /// general algorithm for possibly different roots).
    ///
    /// # Errors
    /// Exponent overflow.
    pub fn compare(&self, y: &Self) -> Result<Ordering> {
        if !self.is_extended {
            return Ok(y.compare_rational(&self.a0)?.reverse());
        }
        if !y.is_extended {
            return self.compare_rational(&y.a0);
        }
        let diff_a0 = self.a0.sub(&y.a0)?;
        let sign_left = diff_a0.sign();
        let x_sqr = self.a1.mul(&self.a1)?.mul(&self.root)?;
        let y_sqr = y.a1.mul(&y.a1)?.mul(&y.root)?;
        let right_res = y_sqr.compare(&x_sqr)?;
        let sign_right = match right_res {
            Ordering::Greater => y.a1.sign(),
            Ordering::Less => self.a1.sign().reverse(),
            Ordering::Equal => {
                let s = y.a1.sign();
                if s == self.a1.sign() {
                    Ordering::Equal
                } else {
                    s
                }
            }
        };
        if sign_left == Ordering::Equal {
            return Ok(sign_right.reverse());
        }
        if sign_right == Ordering::Equal {
            return Ok(sign_left);
        }
        if sign_left != sign_right {
            return Ok(sign_left);
        }
        let a = diff_a0.mul(&diff_a0)?.sub(&x_sqr.add(&y_sqr)?)?;
        let b = self.a1.mul_i32(2)?.mul(&y.a1)?;
        let c = self.root.mul(&y.root)?;
        let sgn = Self::sign_ext(&a, &b, &c)?;
        let swap = sign_left == Ordering::Less;
        Ok(match sgn {
            Ordering::Greater if swap => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
            Ordering::Less if swap => Ordering::Greater,
            Ordering::Less => Ordering::Less,
            Ordering::Equal => Ordering::Equal,
        })
    }

    /// `CGAL::to_double`: `to_double(a0) + to_double(a1) · sqrt(to_double(root))`
    /// when extended, else `to_double(a0)`.
    ///
    /// # Errors
    /// Exponent overflow.
    pub fn to_double(&self) -> Result<f64> {
        if self.is_extended {
            let a0 = self.a0.to_double()?;
            let a1 = self.a1.to_double()?;
            let r = self.root.to_double()?;
            Ok(a0 + a1 * r.sqrt())
        } else {
            self.a0.to_double()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(d: f64) -> Quotient {
        Quotient::try_from_f64(d).unwrap()
    }

    #[test]
    fn comparisons_are_exact() {
        // 1 + √2 vs 2.4142135623730951 (the double nearest 1 + √2 is below).
        let x = RootOf2::new(q(1.0), q(1.0), q(2.0));
        let r = RootOf2::from_rational(q(2.414_213_562_373_095));
        assert_eq!(x.compare(&r).unwrap(), Ordering::Greater);
        assert_eq!(r.compare(&x).unwrap(), Ordering::Less);
        // 1 - √2 < 0, -1 + √2 > 0.
        assert_eq!(
            RootOf2::new(q(1.0), q(-1.0), q(2.0)).sign().unwrap(),
            Ordering::Less
        );
        assert_eq!(
            RootOf2::new(q(-1.0), q(1.0), q(2.0)).sign().unwrap(),
            Ordering::Greater
        );
        // √8 = 2·√2 (different roots, equal values).
        let a = RootOf2::new(q(0.0), q(1.0), q(8.0));
        let b = RootOf2::new(q(0.0), q(2.0), q(2.0));
        assert_eq!(a.compare(&b).unwrap(), Ordering::Equal);
        // 3 - √2 vs 1 + √3.
        let c = RootOf2::new(q(3.0), q(-1.0), q(2.0));
        let d = RootOf2::new(q(1.0), q(1.0), q(3.0));
        assert_eq!(c.compare(&d).unwrap(), Ordering::Less);
        assert_eq!(d.compare(&c).unwrap(), Ordering::Greater);
    }

    #[test]
    fn to_double_formula() {
        let x = RootOf2::new(q(1.0), q(1.0), q(2.0));
        assert_eq!(x.to_double().unwrap(), 1.0 + 2.0_f64.sqrt());
        assert_eq!(RootOf2::from_rational(q(0.5)).to_double().unwrap(), 0.5);
    }
}
