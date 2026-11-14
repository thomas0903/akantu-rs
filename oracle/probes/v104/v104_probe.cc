// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle probe (not part of the main probe build). Upstream behaviour, printed as text:
//  1. G4-04: AKANTU_ENUM_INPUT_STREAM / AKANTU_CLASS_ENUM_INPUT_STREAM
//     (src/common/aka_enum_macros.hh:89-126) of ElementType, ElementKind and
//     RandomDistributionType on chosen spellings: accepted (value, unread
//     rest) or refused;
//  2. G4-05 / G4-07: Array<T>::printself (aka_array_tmpl.hh:730-757) and
//     CircularArray<T>::printself (aka_circular_array_inline_impl.hh:76-88)
//     per debug level and module filter, called from this user translation
//     unit (AKANTU_MODULE undefined here, so AKANTU_DEBUG_MODULE is the
//     literal "AKANTU_MODULE", as oracle/probes/core/core_probe.cc notes);
//  3. D-065 / CORE-019: eigh / eig(values, vectors, sort = true)
//     (aka_types.hh:440-560) of diagonal matrices with exactly tied
//     eigenvalues, sizes 16..33: black-box record of the column order
//     (row of the non-zero entry of each eigenvector column, with sign).
// Addresses are masked as ADDR.
#include <aka_array.hh>
#include <aka_circular_array.hh>
#include <aka_common.hh>
#include <aka_random_generator.hh>
#include <aka_types.hh>

#include <exception>
#include <iostream>
#include <limits>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;

namespace {
std::string esc(const std::string & s) {
  std::string o;
  for (char c : s) {
    if (c == '\t') {
      o += "\\t";
    } else if (c == '\n') {
      o += "\\n";
    } else {
      o += c;
    }
  }
  return o;
}

template <class T> void parse_one(const char * tname, const std::string & s) {
  std::stringstream ss(s);
  T t{};
  std::cout << "ENUM " << tname << " [" << esc(s) << "] -> ";
  try {
    ss >> t;
    std::string rest;
    std::getline(ss, rest, '\0');
    std::cout << "accept " << std::to_string(t) << " rest=[" << esc(rest) << "]\n";
  } catch (std::exception &) {
    std::cout << "refuse\n";
  }
}

std::string mask(std::string t) {
  const std::string key = " + address        : ";
  std::size_t p = 0;
  while ((p = t.find(key, p)) != std::string::npos) {
    auto b = p + key.size();
    t.replace(b, t.find('\n', b) - b, "ADDR");
    p = b;
  }
  return t;
}

template <bool sym>
void sort_case(int n, const std::vector<std::pair<int, int>> & ties, const std::string & label) {
  std::vector<double> d(n);
  for (int i = 0; i < n; ++i) {
    d[i] = double((i * 7) % n + 1);
  }
  for (auto && [a, b] : ties) {
    d[b] = d[a];
  }
  Matrix<Real> A(n, n);
  A.setZero();
  for (int i = 0; i < n; ++i) {
    A(i, i) = d[i];
  }
  Vector<Real> values(n);
  Matrix<Real> vectors(n, n);
  if constexpr (sym) {
    A.eigh(values, vectors, true);
  } else {
    A.eig(values, vectors, true);
  }
  std::cout << "SORT " << (sym ? "eigh" : "eig") << " " << label << " n=" << n << " values=";
  for (int k = 0; k < n; ++k) {
    std::cout << values(k) << (k + 1 < n ? "," : "");
  }
  std::cout << " perm=";
  for (int k = 0; k < n; ++k) {
    int r = -1;
    int nz = 0;
    for (int i = 0; i < n; ++i) {
      if (vectors(i, k) != 0.) {
        r = i;
        ++nz;
      }
    }
    std::cout << (nz == 1 ? "" : "?") << r << (r >= 0 and vectors(r, k) < 0 ? "-" : "")
              << (k + 1 < n ? "," : "");
  }
  std::cout << "\n";
}
} // namespace

int main(int argc, char * argv[]) {
  initialize(argc, argv);
  auto & dbg = debug::debugger;
  const DebugLevel saved = debug::getDebugLevel();

  // 1. enum operator>>
  const std::vector<std::string> et = {
      "_triangle_3", "triangle_3", " _triangle_3", "_triangle_3 ", "\t_triangle_3\n",
      "_triangle_3 _quadrangle_4", "_TRIANGLE_3", "__triangle_3", "_not_defined",
      "_max_element_type", "", "_bernoulli_beam_2", "_cohesive_2d_4", "_igfem_triangle_4"};
  for (auto & s : et) {
    parse_one<ElementType>("ElementType", s);
  }
  const std::vector<std::string> ek = {"_ek_regular", "ek_regular",      " _ek_cohesive",
                                       "_ek_cohesive\n", "_ek_not_defined", "_ek_structural",
                                       "_ek_igfem",   "_EK_REGULAR",     "",
                                       "_ek_regular x"};
  for (auto & s : ek) {
    parse_one<ElementKind>("ElementKind", s);
  }
  const std::vector<std::string> rd = {"uniform", "_uniform",    " weibull",   "weibull ",
                                       "not_defined", "_not_defined", "Uniform", "normal lognormal",
                                       "",        "student_t",   "\tcauchy"};
  for (auto & s : rd) {
    parse_one<RandomDistributionType>("RandomDistributionType", s);
  }

  // 2. printself per level x module filter (user translation unit)
  Array<Real> real(3, 2, "real");
  const std::vector<double> rv = {1. / 3., -0., 2.5e-7, 1e20, 123456789., 1e-5};
  for (Int i = 0; i < 3; ++i) {
    for (Int j = 0; j < 2; ++j) {
      real(i, j) = rv[2 * i + j];
    }
  }
  Array<Int> ints(2, 1, "ints");
  ints(0) = 3;
  ints(1) = -4;
  CircularArray<Real> ring(3, 2, 0., "ring");
  for (Int i = 0; i < 3; ++i) {
    for (Int j = 0; j < 2; ++j) {
      ring(i, j) = 10. * i + j;
    }
  }
  ring.makeStep();
  CircularArray<Int> iring(2, 1, 7, "");
  iring(1) = -1;

  const std::vector<std::pair<std::string, std::vector<std::string>>> filters = {
      {"none", {}},
      {"core", {"core"}},
      {"mesh", {"mesh"}},
      {"AKANTU_MODULE", {"AKANTU_MODULE"}},
      {"core+AKANTU_MODULE", {"core", "AKANTU_MODULE"}}};
  const std::vector<DebugLevel> levels = {dblWarning, dblDump, dblTest};
  for (auto && [fname, mods] : filters) {
    for (auto & m : mods) {
      dbg.addModuleToDebug(m);
    }
    for (auto level : levels) {
      for (int indent : {0, 2}) {
        auto rec = [&](const std::string & name, const auto & arr) {
          debug::setDebugLevel(level);
          std::stringstream ss;
          arr.printself(ss, indent);
          debug::setDebugLevel(saved);
          std::cout << "BEGIN " << name << " level=" << int(level) << " filter=" << fname
                    << " indent=" << indent << "\n"
                    << mask(ss.str()) << "END\n";
        };
        rec("real", real);
        rec("ints", ints);
        rec("ring", ring);
        rec("iring", iring);
      }
    }
    for (auto & m : mods) {
      dbg.removeModuleToDebug(m);
    }
  }

  // 3. sort ties (D-065)
  for (int n : {16, 17, 18, 20, 24, 33}) {
    const std::vector<std::pair<std::string, std::vector<std::pair<int, int>>>> tcs = {
        {"none", {}},
        {"pair01", {{0, 1}}},
        {"pair2last", {{2, n - 1}}},
        {"triple", {{0, 5}, {0, 9}}},
        {"twopairs", {{1, 3}, {4, n - 2}}},
        {"pairmid", {{n / 2, n / 2 + 1}}}};
    for (auto && [label, ties] : tcs) {
      sort_case<true>(n, ties, label);
      sort_case<false>(n, ties, label);
    }
  }
  finalize();
  return 0;
}
