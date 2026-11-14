// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle probe: registered string parameters
// filled by `Parsable::parseSection` keep every byte of the input file.
// Uses the installed reference library (35a851780d0b9471d258ed53a003ae98aaa95ea0):
// `Parser::parse`, `ParameterRegistry::registerParam` for `std::string`,
// `ID` and `std::set<ID>` members (upstream `ParameterTyped<std::string>` and
// `ParameterTyped<std::set<T>>::setAuto`), and the parser's own
// `getParameterValue<std::string>` / `getParameter(name, default)`.
//
// Usage (from the input's directory): registry_probe <file>
// Prints one JSON object; every string is hex encoded (exact bytes), set
// elements in the set's iteration order.

#include "aka_common.hh"
#include "parsable.hh"
#include "parser.hh"

#include <fstream>
#include <iostream>
#include <iterator>
#include <set>
#include <string>

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

std::string quoted_hex(const std::string & s) { return "\"" + hex(s) + "\""; }

} // namespace

int main(int argc, char ** argv) {
  if (argc != 2) {
    std::cerr << "usage: registry_probe <file>\n";
    return 2;
  }
  std::ifstream in(argv[1], std::ios::binary);
  std::string input((std::istreambuf_iterator<char>(in)),
                    std::istreambuf_iterator<char>());

  akantu::Parser parser;
  parser.parse(argv[1]);

  // A parsable reading the global section, like the review's reproduction.
  akantu::Parsable registry(akantu::ParserType::_global, "probe");
  std::string q;
  akantu::ID id;
  std::set<akantu::ID> names;
  std::string untouched = "default \xff";
  registry.registerParam("q", q, akantu::_pat_parsmod, "std::string");
  registry.registerParam("id", id, akantu::_pat_parsmod, "ID");
  registry.registerParam("names", names, akantu::_pat_parsmod,
                         "std::set<ID>");
  registry.registerParam("untouched", untouched, akantu::_pat_parsmod,
                         "not in the file");
  registry.parseSection(parser);

  std::cout << "{\"file\": \"" << argv[1] << "\", \"input\": "
            << quoted_hex(input) << ",\n";
  std::cout << " \"registry\": {\"q\": " << quoted_hex(q)
            << ", \"id\": " << quoted_hex(id) << ", \"names\": [";
  bool first = true;
  for (const auto & n : names) {
    std::cout << (first ? "" : ", ") << quoted_hex(n);
    first = false;
  }
  std::cout << "], \"untouched\": " << quoted_hex(untouched) << "},\n";

  // The parser-side conversions of the same values.
  std::cout << " \"parameter_value\": {\"q\": "
            << quoted_hex(parser.getParameterValue<std::string>("q"))
            << ", \"id\": "
            << quoted_hex(parser.getParameterValue<std::string>("id"))
            << "},\n";
  std::cout << " \"parameter_or\": {\"q\": "
            << quoted_hex(parser.getParameter("q", std::string("dflt")))
            << ", \"missing\": "
            << quoted_hex(parser.getParameter("missing", std::string("dflt")))
            << "}}\n";
  return 0;
}
