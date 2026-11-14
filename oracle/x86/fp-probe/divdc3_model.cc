// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: root-cause probe (clean room, D-030).
//
// Defines __divdc3 (the libgcc entry point GCC calls for std::complex<double>
// division) from the project's clean-room model of the aarch64 division,
// oracle/probes/dense/complex_cleanroom/model_v5.h (black-box derived, fused
// Smith core). Linked into a probe executable, it replaces libgcc's x86_64
// division for the probe's own code, so x86_64 output that then matches the
// aarch64 fixture is attributable to __divdc3. No libgcc source is read.
#include "model_v5.h"

extern "C" _Complex double __divdc3(double a, double b, double c, double d) {
  const C z = model_v5(a, b, c, d);
  _Complex double r;
  __real__ r = z.real();
  __imag__ r = z.imag();
  return r;
}
