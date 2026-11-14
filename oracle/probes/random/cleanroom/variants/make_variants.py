#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs. Clean-room variant harness (D-030): copies
# crates/akantu-core/src/random/distributions_impl.rs to src/dist.rs and makes
# each operation-order decision switchable (`vv("key") == k` selects
# alternative k; 0 = the committed implementation). Fails loudly if the
# implementation no longer contains a patched expression.
import pathlib
import sys

here = pathlib.Path(__file__).resolve().parent
src = here.parents[4] / "crates/akantu-core/src/random/distributions_impl.rs"
s = src.read_text()

REPLACEMENTS = [
    # uniform: 1 = a(1-u) + bu, 2 = a + ub - ua
    ("        let u = generate_canonical(g);\n        u * (self.b - self.a) + self.a",
     "        let u = generate_canonical(g);\n        match vv(\"uniform\") { 1 => self.a * (1.0 - u) + self.b * u, "
     "2 => self.a + u * self.b - u * self.a, _ => u * (self.b - self.a) + self.a }"),
    # exponential: 1 = -log(u)/l, 2 = -log(1-u) * (1/l)
    ("        -log(1.0 - u) / self.lambda",
     "        match vv(\"exp\") { 1 => -log(u) / self.lambda, 2 => -log(1.0 - u) * (1.0 / self.lambda), "
     "_ => -log(1.0 - u) / self.lambda }"),
    # normal: n_flip 1 = 1 - 2u; n_mult 1 = sqrt(-2 log r2)/sqrt(r2), 2 = sqrt(-2/r2 log r2);
    # n_ret 1 = return the first value, cache the second
    ("            let x = 2.0 * generate_canonical(g) - 1.0;\n            let y = 2.0 * generate_canonical(g) - 1.0;",
     "            let (x, y) = if vv(\"n_flip\") == 1 { (1.0 - 2.0 * generate_canonical(g), 1.0 - 2.0 * generate_canonical(g)) } "
     "else { (2.0 * generate_canonical(g) - 1.0, 2.0 * generate_canonical(g) - 1.0) };"),
    ("            let mult = (-2.0 * log(r2) / r2).sqrt();\n            self.saved = x * mult;\n            self.saved_available = true;\n            return y * mult;",
     "            let mult = match vv(\"n_mult\") { 1 => (-2.0 * log(r2)).sqrt() / r2.sqrt(), 2 => (-2.0 / r2 * log(r2)).sqrt(), "
     "_ => (-2.0 * log(r2) / r2).sqrt() };\n            self.saved_available = true;\n"
     "            if vv(\"n_ret\") == 1 { self.saved = y * mult; return x * mult; }\n"
     "            self.saved = x * mult;\n            return y * mult;"),
    # gamma constants: g_a2 1 = 1/(3 sqrt(a1)), 2 = sqrt(1/(9 a1)); g_a1 1 = (3 malpha - 1)/3
    ("        let a2 = 1.0 / (9.0 * (malpha - 1.0 / 3.0)).sqrt();",
     "        let a2 = match vv(\"g_a2\") { 1 => 1.0 / (3.0 * (malpha - 1.0 / 3.0).sqrt()), "
     "2 => (1.0 / (9.0 * (malpha - 1.0 / 3.0))).sqrt(), _ => 1.0 / (9.0 * (malpha - 1.0 / 3.0)).sqrt() };"),
    ("        let a1 = self.malpha - 1.0 / 3.0;",
     "        let a1 = if vv(\"g_a1\") == 1 { (3.0 * self.malpha - 1.0) / 3.0 } else { self.malpha - 1.0 / 3.0 };"),
    # gamma v <= 0 handling: 1 = also consume a uniform, 2 = use |v|
    ("                if !(v <= 0.0) {\n                    break;\n                }",
     "                if !(v <= 0.0) {\n                    break;\n                }\n"
     "                if vv(\"g_vloop\") == 1 { let _ = generate_canonical(g); }\n"
     "                if vv(\"g_vloop\") == 2 { v = v.abs(); break; }"),
    # gamma squeeze (g_sq) and logarithmic test (g_lt) groupings
    ("            if !(u > 1.0 - 0.0331 * (n * n) * (n * n)\n                && log(u) > 0.5 * n * n + a1 * (1.0 - v + log(v)))",
     "            let sq = match vv(\"g_sq\") { 1 => 1.0 - 0.0331 * n * n * n * n, 2 => 1.0 - 0.0331 * ((n * n) * (n * n)), "
     "3 => 1.0 - n * n * n * n * 0.0331, _ => 1.0 - 0.0331 * (n * n) * (n * n) };\n"
     "            let lt = match vv(\"g_lt\") { 1 => 0.5 * n * n + a1 * (log(v) - v + 1.0), 2 => 0.5 * n * n + (a1 - a1 * v + a1 * log(v)), "
     "3 => 0.5 * n * n + a1 * (1.0 - v) + a1 * log(v), _ => 0.5 * n * n + a1 * (1.0 - v + log(v)) };\n"
     "            if !(u > sq && log(u) > lt)"),
    # gamma result: g_ret 1 = a1 (v beta); g_small 1..4 other product orders for alpha < 1
    ("            a1 * v * self.beta\n",
     "            if vv(\"g_ret\") == 1 { a1 * (v * self.beta) } else { a1 * v * self.beta }\n"),
    ("            pow(u, 1.0 / self.alpha) * a1 * v * self.beta",
     "            let p = pow(u, 1.0 / self.alpha);\n            match vv(\"g_small\") { 1 => a1 * v * p * self.beta, "
     "2 => a1 * v * self.beta * p, 3 => p * (a1 * v) * self.beta, 4 => p * (a1 * v * self.beta), _ => p * a1 * v * self.beta }"),
    # cauchy: 1 = tan(pi (u - 1/2))
    ("        self.a + self.b * tan(PI * u)",
     "        if vv(\"cauchy\") == 1 { self.a + self.b * tan(PI * (u - 0.5)) } else { self.a + self.b * tan(PI * u) }"),
    # fisher_f: f_order 1 = denominator gamma first; f_expr alternatives
    ("        let x = self.gd_x.sample(g);\n        let y = self.gd_y.sample(g);\n        (x * self.n) / (y * self.m)",
     "        let (x, y) = if vv(\"f_order\") == 1 { let y = self.gd_y.sample(g); (self.gd_x.sample(g), y) } "
     "else { let x = self.gd_x.sample(g); (x, self.gd_y.sample(g)) };\n"
     "        match vv(\"f_expr\") { 1 => (x / self.m) / (y / self.n), 2 => x * self.n / y / self.m, "
     "3 => (x * self.n) / (self.m * y), 4 => (x / y) * (self.n / self.m), _ => (x * self.n) / (y * self.m) }"),
    # student_t: t_order 1 = gamma first; t_expr alternatives
    ("        let z = self.nd.sample(g);\n        let gamma = self.gd.sample(g);\n        z * (self.n / gamma).sqrt()",
     "        let (z, gamma) = if vv(\"t_order\") == 1 { let gm = self.gd.sample(g); (self.nd.sample(g), gm) } "
     "else { let z = self.nd.sample(g); (z, self.gd.sample(g)) };\n"
     "        match vv(\"t_expr\") { 1 => z / (gamma / self.n).sqrt(), 2 => z * self.n.sqrt() / gamma.sqrt(), "
     "_ => z * (self.n / gamma).sqrt() }"),
    # weibull: 1 = -log(u), 2 = exp(log(-log(1-u))/a)
    ("        self.b * pow(-log(1.0 - u), 1.0 / self.a)",
     "        match vv(\"weibull\") { 1 => self.b * pow(-log(u), 1.0 / self.a), "
     "2 => self.b * exp(log(-log(1.0 - u)) / self.a), _ => self.b * pow(-log(1.0 - u), 1.0 / self.a) }"),
    # extreme value: 1 = -log(u)
    ("        self.a - self.b * log(-log(1.0 - u))",
     "        if vv(\"ev\") == 1 { self.a - self.b * log(-log(u)) } else { self.a - self.b * log(-log(1.0 - u)) }"),
]

for old, new in REPLACEMENTS:
    if s.count(old) != 1:
        sys.exit(f"make_variants.py: expression not found exactly once:\n{old}")
    s = s.replace(old, new)

# gamma: g_pre 1 = draw the alpha < 1 extra uniform before the main loop
old = "        let (mut u, mut n, mut v);\n        loop {"
new = ("        let (mut u, mut n, mut v);\n        let mut pre = 0.0;\n"
       "        if vv(\"g_pre\") == 1 && self.alpha != self.malpha { loop { pre = generate_canonical(g); if pre != 0.0 { break; } } }\n"
       "        loop {")
assert s.count(old) == 1, old
s = s.replace(old, new)
old = ("            loop {\n                u = generate_canonical(g);\n                if u != 0.0 {\n                    break;\n                }\n            }\n            let p")
new = ("            if vv(\"g_pre\") == 1 { u = pre; } else { loop {\n                u = generate_canonical(g);\n"
       "                if u != 0.0 {\n                    break;\n                }\n            } }\n            let p")
assert s.count(old) == 1, old
s = s.replace(old, new)

s = s.replace("use super::libm::{exp, log, pow, tan};", "use super::libm::{exp, log, pow, tan};\nuse crate::v as vv;")
(here / "src/dist.rs").write_text(s)
print("src/dist.rs written")
