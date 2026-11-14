#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generates crates/akantu-parser/tests/fixtures/extra/
# expressions.dat, an input file exercising number literals, operators,
# functions, constants, variables, vectors, matrices and random parameters.
# Every parameter is converted with every conversion by dump_parser.cc
# (oracle) and by the Rust port (tests/oracle.rs). Deterministic output.
#
#   python3 oracle/probes/parser/gen_expressions.py > \
#       crates/akantu-parser/tests/fixtures/extra/expressions.dat
import random

rng = random.Random(20261109)

literals = [
    "0", "-0", "+0", "3.", ".5", "-.5", "1.e5", "10", "-100.0", "+1e10",
    "0.1", "0.2", "0.3", "0.7", "1.1", "2.675", "1e23", "1e22", "1e-5",
    "123456789", "9007199254740993", "9007199254740995",
    "3.14159265358979323846", "2.7182818284590452354",
    "3.8328782866680342684", "9.7874197348334701168", "3.6702824005625512938",
    "0.33333333333333333333", "0.00024999999968750003", "0.323687421261617",
    "0.05162663834780683", "1.6465129043044597", "-9.47874286e-11",
    "123456789012345678901", "12345678901234567890.123",
    "1234567890123456789012345678901234567890",
    "0.00000000000000000000000000001234567890123456789",
    "000000000000000001234", "00000000000000000000.5", "0000000000000000.5",
    "2.2250738585072014e-308", "4.9406564584124654e-324", "2.5e-324",
    "1.7976931348623157e308", "1.7976931348623159e308", "8.98846567431158e307",
    "1e308", "10e308", "1e-307", "1e-308", "1e-320", "1e-400", "1e-700",
    "0e999", "1e400", "1e400 + 1", "1e400-2", "2e", "2e+3", "2E-3", "1e+0005",
    "1e2147483647", "1e2147483648", "1e-2147483648", "1e-2147483649",
    "inf", "-inf", "Infinity", "INF", "nan", "-nan", "NaN", "nan(123)",
    "nan(1, 2)", "nan(", "info", ".", "-", "+", "e", "pi", "-pi", "E",
]
# random literals: 1..25 significant digits, decimal point anywhere,
# exponents -40..40
for _ in range(150):
    nd = rng.randint(1, 25)
    digits = "".join(rng.choice("0123456789") for _ in range(nd))
    digits = rng.choice("123456789") + digits[1:]
    p = rng.randint(0, nd)
    lit = digits[:p] + "." + digits[p:] if rng.random() < 0.8 else digits
    if rng.random() < 0.6:
        lit += "e" + str(rng.randint(-40, 40))
    literals.append(lit)

expressions = [
    "3. + 10.", "10 - 100", "-1 - -1", "1e10 + 4e5 - 1e9 + 1e10", "3. * 10.",
    "10 * 100", "-1 / -1", "1e10 / 4e5", "3. ** 10.", "-2**2", "- 2**2",
    "2**3**2", "2**-1", "2 - - 3", "1 - 2 - 3", "8 / 4 / 2", "1/3", "2/3*3",
    "0.1+0.2", "(0.1+0.2)+0.3", "0.1+(0.2+0.3)", "-(2)**2", "+-1", "--1",
    "1 2", "(1 + 2", "2 * * 3", "1 +", "", "()", "( 1 )", "2*(3+4)*5",
    "10. * (cos(3 * pi) + sin(5 * pi / 2)) ** 2", "pi * 0.05**2 / 4",
    "2*pi + max(2, 50)", "e**2", "pi**0.5", "1e-3*pi",
    "pow(3., 10.)", "min(3., 10.)", "max(3., 10.)", "atan2(3.1415, 2.)",
    "fmod(10., 3.)", "fmod(-10., 3.)", "hypot(3., 10.)", "abs(-3.)",
    "acos(.5)", "asin(.5)", "atan(.5)", "ceil(3.2)", "cos(3.)", "cosh(3.)",
    "exp(3.)", "floor(-3.5)", "log10(3.)", "log(3.)", "sin(3.)", "sinh(3.)",
    "sqrt(3.)", "tan(3.)", "tanh(3.)", "acosh(3.)", "asinh(3.)", "atanh(.5)",
    "exp2(3.)", "expm1(3.)", "log1p(3.)", "log2(3.)", "erf(3.)", "erfc(3.)",
    "lgamma(3.)", "tgamma(3.)", "trunc(3.14)", "round(3.14)", "round(2.5)",
    "round(-2.5)", "lgamma(-0.5)", "tgamma(0.5)", "erf(0.3)", "erfc(0.3)",
    "acosh(1.5)", "asinh(-2.5)", "atanh(-0.3)", "sin(1e22)", "cos(1e22)",
    "exp(-745.5)", "exp(709.8)", "pow(2, 0.5)", "pow(-8, 1./3.)",
    "min(nan, 1)", "min(1, nan)", "max(nan, 1)", "max(1, nan)",
    "COS(0)", "Cos(0)", "EXP(0)", "exp(0)", "cosine", "exponent", "eta",
    "sqrt (4)", "sqrt", "sqrt(", "pow(2)", "pow(2 3)", "atan2(1, 2)",
    "log 2", "a_var * 2", "b_var", "c_var", "undefined_var", "E_var + 1",
    "vec_a", "cyclic", "self_ref", "bad_var", "x_space", "pi2", "e_2",
]

vectors = [
    "[1, 23+2, 5, a_var]", "[ 1 ]", "[]", "[1, ]", "[1 2]", "1, 2", "[1,2] x",
    "[exp(1)]", "[EXP(1)]", "[erf(0)]", "[atan2(1, 2)]", "[atan(1)]",
    "[COS(0), Sin(0)]", "[pi, e, -pi]", "[nan(1, 2), 1]", "[a_var, b_var]",
    "[cosine]", "[undefined_var]", "[vec_a]", "[1e400 + 1]", "[(1, 2)]",
    "[min(1, 2), max(1, 2)]", "[pow(2, 3)]", "[0.1 + 0.2]",
]

matrices = [
    "[[1, 2], [3, 4]]", "[[1, 2], [3]]", "[[1], [2, 3, 4]]", "[vec_a, vec_b]",
    "[vec_a, [5, 6]]", "[vec _a, v ec_b]", "[nope]", "[]", "[[]]",
    "[[1, 2] [3, 4]]", "[[1, 2],]", "[vec_bad]", "[vec_trailing]",
    "[[a_var, 0], [0, a_var]]", "[vec_c, vec_d]", "[vec_e, vec_c, [1]]",
    "[ve c_c]", "[vec_c vec_d]", "[[1, 2], [3, 4]] x", "[[1,2],[3,4]]",
]

randoms = [
    "10 uniform [0.2, 0.5 ]", "10 weibull [0.2, 0.5 ]", "10", "1 + 2",
    "10 uniform [0, 0]", "10 uniform [0.2]", "10 uniform [1, 2, 3]",
    "1 exponential [2]", "1 exponential [2, 3]", "1 gamma [2, 3]",
    "1 extreme_value [2, 3]", "1 normal [0, 1]", "1 lognormal [0, 1]",
    "1 chi_squared [3]", "1 cauchy [0, 1]", "1 fisher_f [2, 3]",
    "1 student_t [4]", "2exponential [1]", "a_var uniform [0, 0.1 * a_var]",
    "10 uniform [0.2, ]", "10 uniform [0.2 0.5]", "10 normalx [1, 2]",
    "10 Uniform [1, 2]", "10 uniform", "10 uniform []", "uniform [1, 2]",
    "26.4e6 weibull [6.6e6, 5.]", "300.0e6 uniform [-1e6, 1e6]",
]

words = [
    "true", "false", "1", "0", "2", "-1", "tr1", "True", "TRUE", "yes", "t",
    "1.5", "00", "abc", "truex", "3.5", "1e3", "0x10", "2147483647",
    "2147483648", "-2147483649", "4294967296", "-4294967295", "+7", "007",
    "steel plate", "[coh1, coh2, coh3,coh4]", "a b, ,c", "\"quoted name\"",
]

print("# akantu-rs expression corpus (generated by")
print("# oracle/probes/parser/gen_expressions.py; do not edit)")
print("permissive_parser = true")
print("a_var = 4")
print("b_var = a_var * 2")
print("c_var = 2*pi + max(2, b_var)")
print("E_var = 3")
print("vec_a = [1, 2]")
print("vec_b = [3, 4, a_var]")
print("vec_bad = [1, ]")
print("vec_c = [5,6]")
print("vec_d = [7,a_var*2]x")
print("vec_e = [1,2] trailing words")
print("vec_trailing = [7, 8] garbage")
# Cyclic references crash upstream (stack overflow; the probe records it).
print("cyclic = cyclic + 1")
print("self_ref = other_ref")
print("other_ref = self_ref")
print("bad_var = 1 +")
print("x_space = 1 2")
print("cosine = 1")
print("exponent = 2")
print("eta = 3")
print("pi2 = 4")
print("e_2 = 5")
n = 0
for kind, items in [("lit", literals), ("expr", expressions)]:
    for e in items:
        n += 1
        if e.strip():
            print(f"s{n:04d} = {e}")
            print(f"v{n:04d} = [{e}]")
for kind, items in [("vec", vectors), ("mat", matrices), ("rnd", randoms), ("word", words)]:
    for e in items:
        n += 1
        if e.strip():
            print(f"{kind}{n:04d} = {e}")
print("user scope [")
print("  a_var = 100")
print("  local = a_var + b_var")
print("  lvec = [a_var, b_var]")
print("  lmat = [vec_a, lvec]")
print("  lvec2 = [a_var,b_var]")
print("  lmat2 = [vec_c, lvec2]")
print("  lrand = a_var uniform [b_var, c_var]")
print("  material nested [")
print("    deep = local * 2 + a_var")
print("  ]")
print("]")
