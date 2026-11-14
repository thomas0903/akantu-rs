// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): FE-engine outputs not covered by fe_engine_probe (consistent field
// matrices, lumped elemental arrays, arbitrary-order integration,
// elemental-field interpolation), for crates/akantu-fe/tests/
// fe_engine_extra_matches_oracle.rs.
// Bodies: fe_extra_probe_impl.hh; per-type instantiations:
// fe_extra_probe_*.cc.
//
// Output: fixtures/fe/engine_extra/<type>
// Usage: fe_extra_probe <fixtures_root> <reference_src_root> [type ...]

#include <aka_common.hh>

#include <filesystem>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;

namespace fe_extra {
template <ElementType type>
void probe_extra(const std::filesystem::path & root, const std::filesystem::path & src);
} // namespace fe_extra

namespace {
std::string type_name(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

bool wanted(const std::vector<std::string> & sel, ElementType t) {
  if (sel.empty()) {
    return true;
  }
  for (auto & s : sel) {
    if (s == type_name(t)) {
      return true;
    }
  }
  return false;
}
} // namespace

int main(int argc, char ** argv) {
  if (argc < 3) {
    std::cerr << "usage: " << argv[0] << " <fixtures_root> <reference_src_root> [type ...]\n";
    return 2;
  }
  int fake_argc = 1;
  char ** fake_argv = argv;
  initialize(fake_argc, fake_argv);
  const std::filesystem::path root(argv[1]);
  const std::filesystem::path src(argv[2]);
  std::vector<std::string> sel(argv + 3, argv + argc);
  int failures = 0;
  auto run = [&](ElementType t, auto && f) {
    if (not wanted(sel, t)) {
      return;
    }
    try {
      f();
    } catch (std::exception & e) {
      std::cerr << "fe_extra_probe: " << type_name(t) << " failed: " << e.what() << "\n";
      ++failures;
    }
  };
#define REG(t) run(t, [&] { fe_extra::probe_extra<t>(root, src); })
  REG(_segment_2);
  REG(_segment_3);
  REG(_triangle_3);
  REG(_triangle_6);
  REG(_quadrangle_4);
  REG(_quadrangle_8);
  REG(_tetrahedron_4);
  REG(_tetrahedron_10);
  REG(_pentahedron_6);
  REG(_pentahedron_15);
  REG(_hexahedron_8);
  REG(_hexahedron_20);
#undef REG
  finalize();
  return failures == 0 ? 0 : 1;
}
