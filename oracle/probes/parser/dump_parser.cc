// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle probe: loads an input file with Akantu's
// Parser (reference 35a851780d0b9471d258ed53a003ae98aaa95ea0) and prints the
// whole tree as JSON, together with the result of every typed conversion of
// every parameter (the operators of src/io/parser/parser_tmpl.hh).
//
// Usage: dump_parser <input file>   (run in the file's directory with the base
//                                    name, so that the debug file name matches
//                                    the upstream tests)
//
// Output format (one JSON object): see crates/akantu-parser/tests/oracle.rs.
// Doubles are printed as their IEEE-754 bit pattern ("0x%016llx").

#include "aka_common.hh"
#include "aka_random_generator.hh"
#include "parser.hh"

#include <sys/wait.h>
#include <unistd.h>

#include <bit>
#include <cstdint>
#include <cstdio>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;

namespace {

std::string json_string(const std::string & s) {
  std::string out = "\"";
  for (unsigned char c : s) {
    switch (c) {
    case '"':
      out += "\\\"";
      break;
    case '\\':
      out += "\\\\";
      break;
    case '\n':
      out += "\\n";
      break;
    case '\r':
      out += "\\r";
      break;
    case '\t':
      out += "\\t";
      break;
    default:
      if (c < 0x20 || c >= 0x7f) {
        char buf[8];
        std::snprintf(buf, sizeof(buf), "\\u%04x", unsigned(c));
        out += buf;
      } else {
        out += char(c);
      }
    }
  }
  return out + "\"";
}

std::string bits(double x) {
  char buf[32];
  std::snprintf(buf, sizeof(buf), "\"0x%016llx\"",
                static_cast<unsigned long long>(std::bit_cast<std::uint64_t>(x)));
  return buf;
}

template <class F> std::string conversion_in_process(F && f) {
  try {
    return "{\"ok\": " + f() + "}";
  } catch (debug::Exception & e) {
    return "{\"error\": \"akantu\"}";
  } catch (std::exception & e) {
    // Boost.Spirit X3 expectation_failure and other std exceptions are not
    // caught by ParserSection::getParameter(name, default).
    return "{\"error\": \"std\"}";
  } catch (...) {
    return "{\"error\": \"other\"}";
  }
}

/// Each conversion runs in a forked child: cyclic variable references
/// (`a = a`) make upstream recurse until the stack overflows, which is
/// recorded as {"error": "crash"}.
template <class F> std::string conversion(F && f) {
  int fd[2];
  if (pipe(fd) != 0) {
    return "{\"error\": \"probe\"}";
  }
  std::cout.flush();
  pid_t pid = fork();
  if (pid == 0) {
    close(fd[0]);
    std::string r = conversion_in_process(f);
    const char * p = r.c_str();
    std::size_t left = r.size();
    while (left > 0) {
      ssize_t n = write(fd[1], p, left);
      if (n <= 0) {
        _exit(3);
      }
      p += n;
      left -= std::size_t(n);
    }
    close(fd[1]);
    _exit(0);
  }
  close(fd[1]);
  std::string r;
  char buf[4096];
  ssize_t n;
  while ((n = read(fd[0], buf, sizeof(buf))) > 0) {
    r.append(buf, std::size_t(n));
  }
  close(fd[0]);
  int status = 0;
  waitpid(pid, &status, 0);
  if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
    return "{\"error\": \"crash\"}";
  }
  return r;
}

std::string dump_parameter(const ParserParameter & p) {
  std::stringstream dbg;
  p.printself(dbg);
  std::string d = dbg.str();
  // "name: value (file:line:col)" -> line, col
  auto open = d.rfind(" (");
  std::string pos = d.substr(open + 2, d.size() - open - 3);
  auto c2 = pos.rfind(':');
  auto c1 = pos.rfind(':', c2 - 1);
  std::string line = pos.substr(c1 + 1, c2 - c1 - 1);
  std::string col = pos.substr(c2 + 1);

  std::stringstream o;
  o << "{\"name\": " << json_string(p.getName())
    << ", \"value\": " << json_string(p.getValue()) << ", \"line\": " << line
    << ", \"column\": " << col;
  o << ", \"real\": " << conversion([&] {
    Real r = p;
    return bits(r);
  });
  o << ", \"vector\": " << conversion([&] {
    Vector<Real> v = p;
    std::string s = "[";
    for (Int i = 0; i < v.size(); ++i) {
      s += (i ? ", " : "") + bits(v(i));
    }
    return s + "]";
  });
  o << ", \"matrix\": " << conversion([&] {
    Matrix<Real> m = p;
    std::string s = "[";
    for (Int i = 0; i < m.rows(); ++i) {
      s += (i ? ", [" : "[");
      for (Int j = 0; j < m.cols(); ++j) {
        s += (j ? ", " : "") + bits(m(i, j));
      }
      s += "]";
    }
    return s + "]";
  });
  o << ", \"random\": " << conversion([&] {
    RandomParameter<Real> r = p;
    std::stringstream sr;
    sr << r;
    return "{\"base\": " + bits(r.getBaseValue()) +
           ", \"print\": " + json_string(sr.str()) + "}";
  });
  o << ", \"bool\": " << conversion([&] {
    bool b = p;
    return std::string(b ? "true" : "false");
  });
  o << ", \"int\": " << conversion([&] {
    Int i = p;
    return std::to_string(i);
  });
  o << ", \"uint\": " << conversion([&] {
    UInt u = p;
    return std::to_string(u);
  });
  o << ", \"word\": " << conversion([&] {
    std::string w = p;
    return json_string(w);
  });
  o << ", \"string_list\": " << conversion([&] {
    std::vector<std::string> l = p;
    std::string s = "[";
    for (std::size_t i = 0; i < l.size(); ++i) {
      s += (i ? ", " : "") + json_string(l[i]);
    }
    return s + "]";
  });
  o << "}";
  return o.str();
}

std::string dump_section(const ParserSection & s, int indent) {
  std::string pad(indent, ' ');
  std::stringstream t;
  t << s.getType();
  std::stringstream o;
  o << "{\"type\": " << json_string(t.str())
    << ", \"name\": " << json_string(s.getName())
    << ", \"option\": " << json_string(s.getOption()) << ",\n"
    << pad << " \"parameters\": [";
  auto params = s.getParameters();
  bool first = true;
  for (auto it = params.first; it != params.second; ++it) {
    o << (first ? "\n" : ",\n") << pad << "  " << dump_parameter(*it);
    first = false;
  }
  o << "],\n" << pad << " \"sections\": [";
  first = true;
  auto subs = s.getSubSections();
  for (auto && sub : subs) {
    o << (first ? "\n" : ",\n") << pad << "  " << dump_section(sub, indent + 2);
    first = false;
  }
  o << "]}";
  return o.str();
}

int dump_file(const std::string & file) {
  Parser parser;
  try {
    parser.parse(file);
  } catch (debug::Exception & e) {
    std::cout << "{\"file\": " << json_string(file)
              << ", \"parse_error\": \"akantu\"}\n";
    return 0;
  } catch (std::exception & e) {
    std::cout << "{\"file\": " << json_string(file)
              << ", \"parse_error\": \"std\"}\n";
    return 0;
  }
  std::cout << "{\"file\": " << json_string(file) << ", \"permissive\": "
            << (Parser::isPermissive() ? "true" : "false")
            << ",\n \"root\": " << dump_section(parser, 1) << "}\n";
  return 0;
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc != 2) {
    std::cerr << "usage: dump_parser <input file>\n";
    return 2;
  }
  return dump_file(argv[1]);
}
