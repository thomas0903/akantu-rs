// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room black-box probe (D-030): observes the
// behaviour of the C++ standard library *as compiled in the oracle image*
// (GCC 14.2 / glibc 2.41) through its public interface only. No library
// source or header is read; the program only calls standard APIs and prints
// what it observes. The output (JSON lines) is the expected-behaviour fixture
// crates/akantu-parser/tests/fixtures/parser/stream_blackbox.jsonl used by
// crates/akantu-parser/tests/stream_blackbox.rs.
//
// Observed operations (all on a fresh std::stringstream in the "C" locale):
//   * `ss >> T` for short, int, long, long long, unsigned short, unsigned,
//     unsigned long, unsigned long long, signed char, unsigned char, double,
//     std::string, bool (boolalpha and noboolalpha), and the two-step bool
//     conversion of Akantu `ParserParameter::operator bool()`;
//     recorded: value (or "-" when the target was not written, detected with
//     two different sentinels), failbit, eofbit, badbit and the number of
//     characters consumed (`tellg` after `clear`).
//   * `os << double` with default flags, `std::scientific` and default
//     floatfield at several precisions.
//   * `os << distribution` for freshly constructed <random> distributions
//     (the text Akantu's RandomParameter::printself embeds).
//
// Build/run (see README.md): g++ -std=c++20 -O2 stream_blackbox.cc && ./a.out
//
// `akantu_bool` and `akantu_list` copy the conversion code of Akantu
// src/io/parser/parser_tmpl.hh: Copyright (©) 2013-2023 EPFL (Ecole
// Polytechnique Fédérale de Lausanne) Laboratory (LSMS - Laboratoire de
// Simulation en Mécanique des Solides), LGPL-3.0-or-later.

#include <bit>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <iomanip>
#include <iostream>
#include <limits>
#include <random>
#include <regex>
#include <set>
#include <sstream>
#include <string>
#include <vector>

namespace {

std::string hex(const std::string & s) {
  static const char * d = "0123456789abcdef";
  std::string o;
  for (unsigned char c : s) {
    o += d[c >> 4];
    o += d[c & 15];
  }
  return o;
}

std::string bits(double x) {
  char buf[32];
  std::snprintf(buf, sizeof(buf), "%016llx",
                static_cast<unsigned long long>(std::bit_cast<std::uint64_t>(x)));
  return buf;
}

std::string state(std::stringstream & ss) {
  std::string s;
  s += ss.fail() ? '1' : '0';
  s += ss.eof() ? '1' : '0';
  s += ss.bad() ? '1' : '0';
  ss.clear();
  auto p = ss.tellg();
  s += ',' + std::to_string(static_cast<long long>(p));
  return s;
}

template <class T> std::string show(T v) {
  if constexpr (std::is_same_v<T, double>) {
    return bits(v);
  } else if constexpr (std::is_same_v<T, bool>) {
    return v ? "1" : "0";
  } else if constexpr (std::is_signed_v<T>) {
    return std::to_string(static_cast<long long>(v));
  } else {
    return std::to_string(static_cast<unsigned long long>(v));
  }
}

/// "<value or ->,<fail><eof><bad>,<consumed>"
template <class T> std::string extract(const std::string & in, T s1, T s2) {
  T a = s1;
  std::stringstream ss1(in);
  ss1 >> a;
  std::string st = state(ss1);
  T b = s2;
  std::stringstream ss2(in);
  ss2 >> b;
  (void)state(ss2);
  std::string v = (a == s1 && b == s2) ? std::string("-") : show(a);
  return v + "," + st;
}

std::string extract_bool(const std::string & in, bool alpha) {
  bool a = false;
  std::stringstream ss1(in);
  ss1 >> (alpha ? std::boolalpha : std::noboolalpha) >> a;
  std::string st = state(ss1);
  bool b = true;
  std::stringstream ss2(in);
  ss2 >> (alpha ? std::boolalpha : std::noboolalpha) >> b;
  (void)state(ss2);
  std::string v = (a == false && b == true) ? std::string("-") : show(a);
  return v + "," + st;
}

/// Akantu ParserParameter::operator bool() (src/io/parser/parser_tmpl.hh).
bool akantu_bool(const std::string & value, bool sentinel) {
  bool b = sentinel;
  std::stringstream sstr(value);
  sstr >> std::boolalpha >> b;
  if (sstr.fail()) {
    sstr.clear();
    sstr >> std::noboolalpha >> b;
  }
  return b;
}

std::string extract_word(const std::string & in) {
  std::string w = "SENTINEL";
  std::stringstream ss(in);
  ss >> w;
  std::string st = state(ss);
  return hex(w) + "," + st;
}

/// Akantu ParserParameter::operator std::vector<std::string>()
/// (src/io/parser/parser_tmpl.hh, LGPL-3.0-or-later), elements hex-encoded
/// and separated by '.'.
std::string akantu_list(const std::string & value) {
  std::vector<std::string> tmp;
  auto string =
      std::regex_replace(value, std::regex("[[:space:]]|\\[|\\]"), "");
  std::smatch sm;
  while (std::regex_search(string, sm, std::regex("[^,]+"))) {
    tmp.push_back(sm.str());
    string = sm.suffix();
  }
  std::string o;
  for (std::size_t i = 0; i < tmp.size(); ++i) {
    o += (i ? "." : "") + hex(tmp[i]);
  }
  return o;
}

void print_input(const std::string & in) {
  std::cout << "{\"k\":\"in\",\"in\":\"" << hex(in) << "\""
            << ",\"short\":\"" << extract<short>(in, 77, 99) << "\""
            << ",\"int\":\"" << extract<int>(in, 77, 99) << "\""
            << ",\"long\":\"" << extract<long>(in, 77, 99) << "\""
            << ",\"llong\":\"" << extract<long long>(in, 77, 99) << "\""
            << ",\"ushort\":\"" << extract<unsigned short>(in, 77, 99) << "\""
            << ",\"uint\":\"" << extract<unsigned>(in, 77, 99) << "\""
            << ",\"ulong\":\"" << extract<unsigned long>(in, 77, 99) << "\""
            << ",\"ullong\":\"" << extract<unsigned long long>(in, 77, 99)
            << "\""
            << ",\"schar\":\"" << extract<signed char>(in, 77, 99) << "\""
            << ",\"uchar\":\"" << extract<unsigned char>(in, 77, 99) << "\""
            << ",\"double\":\"" << extract<double>(in, 77.0, 99.0) << "\""
            << ",\"bool_alpha\":\"" << extract_bool(in, true) << "\""
            << ",\"bool_num\":\"" << extract_bool(in, false) << "\""
            << ",\"akantu_bool\":\""
            << ((akantu_bool(in, false) == false && akantu_bool(in, true))
                    ? std::string("-")
                    : show(akantu_bool(in, false)))
            << "\""
            << ",\"word\":\"" << extract_word(in) << "\""
            << ",\"list\":\"" << akantu_list(in) << "\"}\n";
}

std::string fmt_default(double x) {
  std::ostringstream os;
  os << x;
  return os.str();
}

std::string fmt_sci(double x, int p) {
  std::ostringstream os;
  os << std::scientific << std::setprecision(p) << x;
  return os.str();
}

std::string fmt_gen(double x, int p) {
  std::ostringstream os;
  os << std::setprecision(p) << x;
  return os.str();
}

const int sci_prec[] = {0, 1, 2, 3, 5, 6, 10, 16, 17, 20, 40};
const int gen_prec[] = {0, 1, 2, 3, 5, 6, 10, 15, 16, 17, 20, 40};

void print_double(double x) {
  std::cout << "{\"k\":\"fmt\",\"x\":\"" << bits(x) << "\",\"def\":\""
            << fmt_default(x) << "\",\"sci\":[";
  bool first = true;
  for (int p : sci_prec) {
    std::cout << (first ? "" : ",") << "\"" << fmt_sci(x, p) << "\"";
    first = false;
  }
  std::cout << "],\"gen\":[";
  first = true;
  for (int p : gen_prec) {
    std::cout << (first ? "" : ",") << "\"" << fmt_gen(x, p) << "\"";
    first = false;
  }
  std::cout << "]}\n";
}

template <class D> std::string dist_text(const D & d) {
  std::ostringstream os;
  os << d;
  return os.str();
}

/// One line per (distribution, a, b): constructed exactly like Akantu's
/// make_random_parameter (src/common/aka_random_generator.hh).
void print_dist(double a, double b) {
  auto line = [&](const char * name, const std::string & text) {
    std::cout << "{\"k\":\"dist\",\"d\":\"" << name << "\",\"a\":\"" << bits(a)
              << "\",\"b\":\"" << bits(b) << "\",\"t\":\"" << text << "\"}\n";
  };
  line("uniform", dist_text(std::uniform_real_distribution<double>(a, b)));
  line("exponential", dist_text(std::exponential_distribution<double>(a)));
  line("gamma", dist_text(std::gamma_distribution<double>(a, b)));
  line("weibull", dist_text(std::weibull_distribution<double>(b, a)));
  line("extreme_value",
       dist_text(std::extreme_value_distribution<double>(a, b)));
  line("normal", dist_text(std::normal_distribution<double>(a, b)));
  line("lognormal", dist_text(std::lognormal_distribution<double>(a, b)));
  line("chi_squared", dist_text(std::chi_squared_distribution<double>(a)));
  line("cauchy", dist_text(std::cauchy_distribution<double>(a, b)));
  line("fisher_f", dist_text(std::fisher_f_distribution<double>(a, b)));
  line("student_t", dist_text(std::student_t_distribution<double>(a)));
}

/// Deterministic 64-bit generator (splitmix64), independent of <random>.
struct Mix {
  std::uint64_t s;
  std::uint64_t next() {
    std::uint64_t z = (s += 0x9e3779b97f4a7c15ULL);
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
    return z ^ (z >> 31);
  }
};

} // namespace

int main() {
  // ---------------------------------------------------------------- inputs
  std::vector<std::string> inputs = {
      "", " ", "\t\n", "\v\f\r ", "0", "1", "-1", "+1", "2", "42", "  42",
      "42  ", "42abc", "abc", "-", "+", "+-1", "--1", "-+1", "- 1", "+ 1",
      "0x1A", "0X1a", "0x", "010", "08", "09", "1e5", "3.5", "-3.5", ".5",
      "5.", "1,000", "1 000", "1'000", "2147483647", "2147483648",
      "-2147483648", "-2147483649", "4294967295", "4294967296",
      "-4294967295", "-4294967296", "9223372036854775807",
      "9223372036854775808", "-9223372036854775808", "-9223372036854775809",
      "18446744073709551615", "18446744073709551616",
      "-18446744073709551615", "-18446744073709551616",
      "99999999999999999999999", "-99999999999999999999999",
      "000000000000000000000000000000000000001", "-0000000000000000000002",
      "32767", "32768", "-32768", "-32769", "65535", "65536", "-65535",
      "-65536", "127", "128", "-128", "-129", "255", "256", "-255", "-256",
      "true", "false", "True", "TRUE", "False", "tru", "t", "f", "fals",
      "falsex", "truex", "true1", " true", "true false", "1true", "0true",
      "trUe", "ftrue", "tfalse", "00", "01", "10", "-0", "+0", "\v5", "\f5",
      "\r5", "\xa0" "5", "\xff", "5\xff", "\xff" "5", "nan", "inf",
      "-inf", "infinity", "NAN", "nan(12)", "x", "1 2", "0b1", "1_000",
      "1e", "1e+", "1e-", "1E5", "1e+5", "1e-5", "1.5e3x", "-.5", ".", "-.",
      "+.e1", "1e400", "-1e400", "1e-400", "-1e-400", "1e308", "2e308",
      "4.9e-324", "2.4e-324", "2.5e-324", "1.7976931348623157e308",
      "1.7976931348623158e308", "1.7976931348623159e308", "0x1p3",
      "0x1.8p1", "1.5e3", "123456789012345678901234567890", "0.1", "0.2",
      "0.30000000000000004", "1e-310", "2.2250738585072011e-308",
      "2.2250738585072014e-308", "  -12  ", "\n-12\n", "12\n34", "+12",
      "1.", "1..2", "1.2.3", "1e5.5", "e5", "E", "-e1", "1.0e0010",
      "1e0000000000000000000000001", "1e-0000000000000000000000001",
      "0e999999999", "1e999999999999999999999", "1e-999999999999999999999",
      "abc def", "  word  ", "a\tb", "[1,2]", "#c", "steel  ",
      "[a, b ,c]", "a,,b", ",a,", "[ ]", "[,]", "a b\tc\nd", "x\xa0y,z",
      "[[a]],[b]", "\xff,\xfe", "solid_mechanics_model, heat_transfer",
  };
  // Random strings over the characters that matter to number parsing.
  const std::string alphabet =
      "0123456789+-xXaAbBcCdDeEfF. \t\n,ntruels[]\v\f\r\xa0\xff";
  Mix rng{20261109};
  for (int i = 0; i < 900; ++i) {
    int len = static_cast<int>(rng.next() % 13);
    std::string s;
    for (int j = 0; j < len; ++j) {
      s += alphabet[rng.next() % alphabet.size()];
    }
    inputs.push_back(s);
  }
  // Random decimal integers of all lengths with random signs.
  for (int i = 0; i < 250; ++i) {
    int len = 1 + static_cast<int>(rng.next() % 25);
    std::string s;
    switch (rng.next() % 4) {
    case 0:
      s = "-";
      break;
    case 1:
      s = "+";
      break;
    default:
      break;
    }
    for (int j = 0; j < len; ++j) {
      s += static_cast<char>('0' + rng.next() % 10);
    }
    inputs.push_back(s);
  }
  for (auto & in : inputs) {
    print_input(in);
  }

  // ---------------------------------------------------------- formatting
  std::vector<double> xs = {
      0.0, -0.0, 1.0, -1.0, 0.5, 1.5, 2.5, 3.5, 0.125, 0.375, 10.0, 100.0,
      1e5, 1e6, 123456.0, 1234567.0, 999999.0, 999999.5, 9999995.0, 0.0001,
      0.00001, 0.000123456, 0.0001234565, 1e-5, 9.9999e-5, 9.99995e-5,
      0.1, 0.2, 0.3, 1.0 / 3.0, 2.0 / 3.0, 3.14159265358979323846,
      2.7182818284590452354, 1e21, 1e22, 1e23, 1e100, 1e-100, 1e300,
      1e-300, 1e308, 4.9406564584124654e-324, 2.2250738585072014e-308,
      2.2250738585072009e-308, 1.7976931348623157e308,
      std::numeric_limits<double>::infinity(),
      -std::numeric_limits<double>::infinity(),
      std::numeric_limits<double>::quiet_NaN(),
      -std::numeric_limits<double>::quiet_NaN(), 5e-324, 0.15, 0.25, 0.35,
      0.45, 1.25, 1.35, 2.675, 1.005, 9.5, 95.0, 950000.0, 9500000.0,
      0.95, 0.095, 0.0095, 0.00095, 0.000095, 8.0, 14.2831853071795862,
      6.6e6, 2.64e7, 2.9e6, 2.4e6, 9.4e3, 1e-6, -2.14748e9, 4294967295.0,
      9007199254740993.0, 123456789012345678.0, 0.30000000000000004,
  };
  for (int i = 0; i < 250; ++i) { // random bit patterns
    xs.push_back(std::bit_cast<double>(rng.next()));
  }
  for (int i = 0; i < 250; ++i) { // random "decimal-looking" values
    double m = static_cast<double>(rng.next() % 1000000) /
               static_cast<double>(1 + rng.next() % 1000);
    int e = static_cast<int>(rng.next() % 41) - 20;
    xs.push_back(m * std::pow(10.0, e));
  }
  for (int i = 0; i < 120; ++i) { // exact binary ties for small precisions
    double m = static_cast<double>(rng.next() % 2000) + 0.5;
    int e = static_cast<int>(rng.next() % 9) - 4;
    xs.push_back(std::ldexp(m, e));
  }
  for (double x : xs) {
    print_double(x);
  }

  // -------------------------------------------------------- distributions
  const double pairs[][2] = {
      {0.0, 1.0},   {0.2, 0.5},   {2.0, 3.0},  {3.0, 0.0},   {4.0, 0.0},
      {1.0, 2.0},   {0.5, 0.25},  {5.0, 7.0},  {0.3, 0.7},   {1e-3, 1e3},
      {-1.0, 1.0},  {7.0, 2.5},   {0.1, 0.1},  {1.5, 0.75},  {9.0, 11.0},
      {6.6e6, 5.0}, {2.0, 1e-7},  {1.0, 1.0},  {13.0, 0.5},  {0.75, 3.0},
      {std::numeric_limits<double>::infinity(),
       -std::numeric_limits<double>::infinity()},
      {std::numeric_limits<double>::quiet_NaN(),
       -std::numeric_limits<double>::quiet_NaN()},
      {5e-324, 1.7976931348623157e308},
      {-0.0, 1e-300},
  };
  for (auto & p : pairs) {
    print_dist(p[0], p[1]);
  }
  return 0;
}
