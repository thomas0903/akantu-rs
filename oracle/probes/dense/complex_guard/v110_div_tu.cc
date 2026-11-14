// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: the division under
// test, in its own translation unit so that no operand is known at compile
// time. Black-box only (D-030): the program observes the results; no library
// source or compiled library code is read or inspected.
#include <complex>

std::complex<double> runtime_div(std::complex<double> x, std::complex<double> y) { return x / y; }
