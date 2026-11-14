// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle probe: runs Akantu's `cppargparse`
// (reference 35a851780d0b9471d258ed53a003ae98aaa95ea0, installed oracle
// library) on one command line and prints what it parsed.
//
// Usage: cli_probe <scenario> <flags> [args...]
//   scenario: one of the argument sets registered in `setup` below (the Rust
//             test crates/akantu-parser/tests/cli_oracle.rs registers the
//             same sets);
//   flags:    the cppargparse ParseFlags as an integer (0..3);
//   args:     the command line after the program name ("prog").
// On success prints one JSON object (remaining argv, per argument `has` and
// typed values, `printself` text); on error cppargparse prints the message
// and the help text to stderr and exits (status recorded by cli_cases.py).

#include "cppargparse.hh"

#include <bit>
#include <cstdint>
#include <cstdio>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

using cppargparse::_any;
using cppargparse::_at_least_one;
using cppargparse::_boolean;
using cppargparse::_float;
using cppargparse::_integer;
using cppargparse::_one_if_possible;
using cppargparse::_string;
using cppargparse::ArgumentParser;
using cppargparse::ArgumentType;

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

struct Arg {
  std::string name;
  ArgumentType type;
};

std::vector<Arg> setup(ArgumentParser & p, const std::string & scenario) {
  if (scenario == "float") {
    p.addArgument("--val", "value", 1, _float);
    return {{"val", _float}};
  }
  if (scenario == "float_def") {
    p.addArgument("--val", "value", 1, _float, 2.5);
    p.addArgument("--opt", "optional", _one_if_possible, _float, 1.5, 3.25);
    return {{"val", _float}, {"opt", _float}};
  }
  if (scenario == "float_pos") {
    p.addArgument("x", "first", 1, _float);
    p.addArgument("y", "second", _one_if_possible, _float);
    return {{"x", _float}, {"y", _float}};
  }
  if (scenario == "int") {
    p.addArgument("-n;--number", "number", 1, _integer);
    p.addArgument("--many", "many", _any, _integer);
    return {{"number", _integer}, {"many", _integer}};
  }
  if (scenario == "bool") {
    p.addArgument("--b", "boolean", 1, _boolean);
    p.addArgument("--flag", "flag", 0, _boolean, false, true);
    return {{"b", _boolean}, {"flag", _boolean}};
  }
  if (scenario == "mixed") {
    p.addArgument("files", "files", _at_least_one, _string);
    p.addArgument("--val", "value", 1, _float);
    p.addArgument("-n", "number", 1, _integer);
    p.addArgument("--opt", "option", _one_if_possible, _string,
                  std::string("d"), std::string("c"));
    return {{"files", _string}, {"val", _float}, {"n", _integer},
            {"opt", _string}};
  }
  if (scenario == "akantu") {
    // src/common/aka_common.cc initialize()
    p.addArgument("--aka_input_file", "Akantu's input file", 1, _string,
                  std::string());
    p.addArgument(
        "--aka_debug_level",
        std::string("Akantu's overall debug level") +
            std::string(" (0: error, 1: exceptions, 4: warnings, 5: info, ..., "
                        "100: dump") +
            std::string(" more info on levels can be found in aka_error.hh)"),
        1, _integer, long(4));
    p.addArgument("--aka_print_backtrace",
                  "Should Akantu print a backtrace in case of error", 0,
                  _boolean, false, true);
    p.addArgument("--aka_seed", "The seed to use on prank 0", 1, _integer);
    return {{"aka_input_file", _string},
            {"aka_debug_level", _integer},
            {"aka_print_backtrace", _boolean},
            {"aka_seed", _integer}};
  }
  std::cerr << "unknown scenario " << scenario << "\n";
  std::exit(64);
}

/// The single typed value (`T v = p[name]`, which requires exactly one
/// value; upstream's vector cast does not compile), or "many" otherwise (the
/// values are then visible in the printself text).
template <class T, class F>
std::string values(const ArgumentParser & p, const std::string & name, F f) {
  try {
    T v = p[name];
    return "[\"" + f(v) + "\"]";
  } catch (std::length_error &) {
    return "\"many\"";
  }
}

} // namespace

int main(int argc, char ** argv) {
  if (argc < 3) {
    std::cerr << "usage: cli_probe <scenario> <flags> [args...]\n";
    return 64;
  }
  ArgumentParser p;
  auto args = setup(p, argv[1]);
  int flags = std::stoi(argv[2]);
  std::vector<char *> av;
  std::string prog = "prog";
  av.push_back(prog.data());
  for (int i = 3; i < argc; ++i) {
    av.push_back(argv[i]);
  }
  int ac = static_cast<int>(av.size());
  char ** avp = av.data();
  p.parse(ac, avp, flags);

  std::cout << "{\"argv\":[";
  for (int i = 0; i < ac; ++i) {
    std::cout << (i ? "," : "") << "\"" << hex(avp[i]) << "\"";
  }
  std::cout << "],\"args\":{";
  bool first = true;
  for (auto & a : args) {
    std::cout << (first ? "" : ",") << "\"" << a.name << "\":";
    first = false;
    if (!p.has(a.name)) {
      std::cout << "null";
      continue;
    }
    switch (a.type) {
    case _string:
      std::cout << values<std::string>(p, a.name,
                                       [](const std::string & s) { return hex(s); });
      break;
    case _float:
      std::cout << values<double>(p, a.name, [](double x) { return bits(x); });
      break;
    case _integer:
      std::cout << values<long>(p, a.name,
                                [](long l) { return std::to_string(l); });
      break;
    case _boolean:
      std::cout << values<bool>(p, a.name,
                                [](bool b) { return std::string(b ? "1" : "0"); });
      break;
    }
  }
  std::ostringstream os;
  os << p;
  std::cout << "},\"print\":\"" << hex(os.str()) << "\"}\n";
  return 0;
}
