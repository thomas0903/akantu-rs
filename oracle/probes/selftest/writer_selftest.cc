// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: self-test of oracle/probes/include/akantu_probe/fixture_writer.hh.
// Writes fixtures/selftest/cpp_writer_roundtrip, which the Rust test
// crates/akantu-validation/tests/selftest_fixtures.rs reads back bit-exactly.
//
// Build & run (any C++17 compiler, no Akantu needed):
//   c++ -std=c++17 -O1 -ffp-contract=off -I oracle/probes/include \
//       oracle/probes/selftest/writer_selftest.cc -o /tmp/writer_selftest
//   /tmp/writer_selftest fixtures
// No Eigen in this program: the D-050 Eigen pin check does not apply.
#define AKANTU_PROBE_NO_EIGEN
#include "akantu_probe/fixture_writer.hh"

#include <cmath>
#include <cstdint>
#include <iostream>
#include <limits>
#include <vector>

int main(int argc, char ** argv) {
  if (argc != 2) {
    std::cerr << "usage: " << argv[0] << " <fixtures_root>\n";
    return 2;
  }
  akantu_probe::FixtureWriter w(
      argv[1], "selftest/cpp_writer_roundtrip",
      "Self-test of the header-only C++ fixture writer: special doubles, "
      "integer types, booleans, empty and 0-d arrays, ragged data",
      "writer_selftest", "1");
  w.set_command("oracle/probes/selftest/writer_selftest fixtures");
  w.set_status("golden");
  w.parameters()["note"] = "values are fixed by construction";
  w.parameters()["pi"] = 3.141592653589793;

  // Special doubles: signed zeros, subnormals, extremes, NaN, infinities.
  std::vector<double> special = {0.0,
                                 -0.0,
                                 1.0,
                                 0.1,
                                 -1.0 / 3.0,
                                 std::numeric_limits<double>::denorm_min(),
                                 std::numeric_limits<double>::min(),
                                 std::numeric_limits<double>::max(),
                                 std::numeric_limits<double>::epsilon(),
                                 std::nextafter(1.0, 2.0),
                                 std::numeric_limits<double>::quiet_NaN(),
                                 std::numeric_limits<double>::infinity(),
                                 -std::numeric_limits<double>::infinity()};
  w.add("special", {special.size()}, {"entry"}, "table", special)
      .description("special IEEE-754 values")
      .tolerance_class("exact");

  // 2-d nodal field with component labels.
  std::vector<double> u;
  for (int n = 0; n < 4; ++n) {
    for (int c = 0; c < 2; ++c) {
      u.push_back(n + 0.25 * c);
    }
  }
  w.add("u", {4, 2}, {"node", "component"}, "nodal", u)
      .components({"x", "y"})
      .ordering("node", "global_id", "node_ids");
  std::vector<std::int64_t> ids = {10, 11, 12, 13};
  w.add("node_ids", {4}, {"node"}, "nodal", ids);

  std::vector<std::int32_t> i32 = {-2147483647 - 1, -1, 0, 2147483647};
  w.add("i32", {2, 2}, {"row", "col"}, "table", i32);
  std::vector<std::uint8_t> u8 = {0, 255};
  w.add("u8", {2}, {"entry"}, "table", u8);
  w.add_bool("flags", {3}, {"entry"}, "table", {true, false, true});

  std::vector<double> empty;
  w.add("empty", {0, 3}, {"entry", "component"}, "table", empty);
  std::vector<double> scalar = {42.5};
  w.add("scalar", {}, {}, "scalar", scalar);

  // Ragged: 3 rows of lengths 2, 0, 1.
  std::vector<double> rv = {1.0, 0.5, 0.25};
  std::vector<std::int64_t> ro = {0, 2, 2, 3};
  w.add("residuals/offsets", {ro.size()}, {"entry"}, "ragged_offsets", ro);
  w.add("residuals", {rv.size()}, {"iteration"}, "ragged_values", rv)
      .ragged("residuals/offsets", "step");

  w.add_tag("selftest");
  w.finish();
  std::cout << "wrote " << w.dir() << "\n";
  return 0;
}
