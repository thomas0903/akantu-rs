// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): records what the installed reference library does with parameter
// sub-registries and registered sub-sections
// (rows PARSE-008/PARSE-009):
//
//  * ParameterRegistry::registerSubRegistry (std::map by id), get/set with
//    the recursive sub-registry search (src/io/parser/parameter_registry.cc,
//    parameter_registry_tmpl.hh: `consisder_sub`, true for plain
//    registries), hasParameter / listParameters / listSubRegisteries,
//    printself;
//  * Parsable::registerSubSection (std::map by (type, name), also a
//    sub-registry), parseSection / parseSubSection dispatch, the
//    non-recursive get/set of Parsable (`consisder_sub = false`,
//    src/io/parser/parsable.cc), permissive and strict unknown sub-sections.
//
// Every operation is logged in order with its result (value as a decimal
// string with 17 significant digits, or the exception message), together
// with the order of parseParam / parseSubSection / updateInternalParameters
// calls. Output: fixtures/parser/subregistry/<case> (parameters JSON).
// Usage: parsable_probe <fixtures_root> <work_dir>

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <parameter_registry.hh>
#include <parsable.hh>
#include <parser.hh>

#include <filesystem>
#include <fstream>
#include <functional>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;
namespace ap = akantu_probe;

namespace {

constexpr const char * kProbe = "parsable_probe";
constexpr const char * kProbeVersion = "1";

std::vector<std::string> g_events;

std::string real_str(Real r) {
  std::ostringstream o;
  o << std::setprecision(17) << r;
  return o.str();
}

/// Plain registry exposing the protected parts used by the probe.
class Reg : public ParameterRegistry {
public:
  explicit Reg(std::string name) : name(std::move(name)) {}
  using ParameterRegistry::registerSubRegistry;
  void updateInternalParameters() override { g_events.push_back("update:" + name); }
  std::string print() const {
    std::ostringstream o;
    this->printself(o, 0);
    return o.str();
  }
  std::string name;
};

/// Parsable logging its parse calls.
class Pars : public Parsable {
public:
  Pars(ParserType type, std::string name) : Parsable(type, name), name(std::move(name)) {}
  void updateInternalParameters() override { g_events.push_back("update:" + name); }
  void parseParam(const ParserParameter & in_param) override {
    g_events.push_back("param:" + name + ":" + in_param.getName());
    Parsable::parseParam(in_param);
  }
  void parseSubSection(const ParserSection & section) override {
    g_events.push_back("sub:" + name + ":" + section.getName());
    Parsable::parseSubSection(section);
  }
  std::string print() const {
    std::ostringstream o;
    this->printself(o, 0);
    return o.str();
  }
  std::string name;
};

struct Log {
  ap::Json ops = ap::Json::array();

  void record(const std::string & op, const std::function<std::string()> & f) {
    ap::Json j = ap::Json::object();
    j["op"] = op;
    g_events.clear();
    try {
      j["result"] = f();
    } catch (debug::Exception & e) {
      j["error"] = std::string(e.info());
    } catch (std::exception & e) {
      j["error"] = std::string("std::exception: ") + e.what();
    }
    j["events"] = ap::Json::array_of(g_events);
    g_events.clear();
    ops.push(std::move(j));
  }
};

std::string get_real(ParameterRegistry & r, const std::string & name) {
  Real v = r.get(name);
  return real_str(v);
}

std::string join(const std::vector<ID> & v) {
  std::string s;
  for (const auto & x : v) {
    s += (s.empty() ? "" : ",") + x;
  }
  return s;
}

void write_file(const std::filesystem::path & p, const std::string & text) {
  std::ofstream f(p, std::ios::binary);
  f << text;
}

/* -------------------------------------------------------------------------- */
void registries(const std::filesystem::path & root) {
  Reg r("R"), a("A"), b("B"), d("D");
  Real r_r = 1.0, b_x = 2.0, b_shared = 3.0, a_y = 4.0, a_shared = 5.0, d_z = 6.0,
       b_ro = 8.0;
  r.registerParam("r", r_r, _pat_parsmod, "root parameter");
  b.registerParam("x", b_x, _pat_parsmod, "in B");
  b.registerParam("shared", b_shared, _pat_parsmod, "in A and B");
  b.registerParam("ro", b_ro, _pat_readable, "read only, in B");
  a.registerParam("y", a_y, _pat_parsmod, "in A");
  a.registerParam("shared", a_shared, _pat_parsmod, "in A and B");
  d.registerParam("z", d_z, _pat_parsmod, "in D (sub-registry of A)");
  // registration order b, a: the std::map iterates a_sub before b_sub
  r.registerSubRegistry("b_sub", b);
  r.registerSubRegistry("a_sub", a);
  a.registerSubRegistry("deep", d);

  Log log;
  log.record("get R r", [&] { return get_real(r, "r"); });
  log.record("get R y", [&] { return get_real(r, "y"); });
  log.record("get R x", [&] { return get_real(r, "x"); });
  log.record("get R shared", [&] { return get_real(r, "shared"); });
  log.record("get R z", [&] { return get_real(r, "z"); });
  log.record("get R nope", [&] { return get_real(r, "nope"); });
  log.record("get B z", [&] { return get_real(b, "z"); });
  log.record("set R z 7", [&] {
    r.set("z", Real(7.));
    return std::string("ok");
  });
  log.record("set R shared 9", [&] {
    r.set("shared", Real(9.));
    return std::string("ok");
  });
  log.record("set R x 11", [&] {
    r.set("x", Real(11.));
    return std::string("ok");
  });
  log.record("set R nope 1", [&] {
    r.set("nope", Real(1.));
    return std::string("ok");
  });
  log.record("set R ro 1", [&] {
    r.set("ro", Real(1.));
    return std::string("ok");
  });
  log.record("set D nope 1", [&] {
    d.set("nope", Real(1.));
    return std::string("ok");
  });
  log.record("get R z", [&] { return get_real(r, "z"); });
  log.record("get R shared", [&] { return get_real(r, "shared"); });
  log.record("get B shared", [&] { return get_real(b, "shared"); });
  log.record("get R x", [&] { return get_real(r, "x"); });
  log.record("get R ro", [&] { return get_real(r, "ro"); });
  log.record("has R y", [&] { return std::string(r.hasParameter("y") ? "true" : "false"); });
  log.record("has A y", [&] { return std::string(a.hasParameter("y") ? "true" : "false"); });
  log.record("list R params", [&] { return join(r.listParameters()); });
  log.record("list R subs", [&] { return join(r.listSubRegisteries()); });
  log.record("list A subs", [&] { return join(a.listSubRegisteries()); });
  log.record("print R", [&] { return r.print(); });

  ap::FixtureWriter w(root, "parser/subregistry/registry_tree",
                      "ParameterRegistry::registerSubRegistry: recursive get/set over a "
                      "two-level sub-registry tree (std::map order, consisder_sub = true), "
                      "access rights, listing and printing",
                      kProbe, kProbeVersion);
  w.add_tag("parser");
  w.add_tag("wp-verify-1");
  w.parameters()["ops"] = log.ops;
  std::vector<double> final_values{r_r, b_x, b_shared, a_y, a_shared, d_z, b_ro};
  w.add("final_values", {final_values.size()}, {"entry"}, "table", final_values)
      .description("members after all operations: R.r, B.x, B.shared, A.y, A.shared, D.z, "
                   "B.ro")
      .tolerance_class("exact");
  w.finish();
  std::cout << "  parser/subregistry/registry_tree\n";
}

/* -------------------------------------------------------------------------- */
void parsables(const std::filesystem::path & root, const std::filesystem::path & work) {
  Pars p(ParserType::_material, "parent");
  Pars wf(ParserType::_weight_function, "weight");
  Pars inner(ParserType::_rules, "inner");
  Pars rule_a1(ParserType::_rules, "rule_a_first");
  Pars rule_a2(ParserType::_rules, "rule_a_second");
  Real E = 1.0, nu = 0.3, radius = 1.0, k = 0.0, va1 = 0.0, va2 = 0.0;
  Int update_rate = 1;
  p.registerParam("E", E, _pat_parsmod, "Young's modulus");
  p.registerParam("nu", nu, _pat_parsmod, "Poisson's ratio");
  wf.registerParam("radius", radius, _pat_parsmod, "radius");
  wf.registerParam("update_rate", update_rate, _pat_parsmod, "update rate");
  inner.registerParam("k", k, _pat_parsmod, "inner value");
  rule_a1.registerParam("value", va1, _pat_parsmod, "value");
  rule_a2.registerParam("value", va2, _pat_parsmod, "value");
  p.registerSubSection(ParserType::_weight_function, "weight_parameter", wf);
  wf.registerSubSection(ParserType::_rules, "inner", inner);
  // insert_or_assign: the second registration of the same key wins
  p.registerSubSection(ParserType::_rules, "rule_a", rule_a1);
  p.registerSubSection(ParserType::_rules, "rule_a", rule_a2);

  const auto ok = work / "parsable_ok.dat";
  write_file(ok, "material elastic [\n"
                 "  nu = 0.25\n"
                 "  E = 210\n"
                 "  rules rule_a [\n"
                 "    value = 2.5\n"
                 "  ]\n"
                 "  weight_function weight_parameter [\n"
                 "    update_rate = 4\n"
                 "    radius = 3.5\n"
                 "    rules inner [\n"
                 "      k = -1.25\n"
                 "    ]\n"
                 "  ]\n"
                 "]\n");
  const auto unknown = work / "parsable_unknown.dat";
  write_file(unknown, "material elastic [\n"
                      "  E = 7\n"
                      "  rules rule_b [\n"
                      "    value = 1\n"
                      "  ]\n"
                      "]\n");
  const auto wrong_type = work / "parsable_wrong_type.dat";
  write_file(wrong_type, "material elastic [\n"
                         "  E = 8\n"
                         "  rules weight_parameter [\n"
                         "    radius = 1\n"
                         "  ]\n"
                         "]\n");
  const auto permissive = work / "parsable_permissive.dat";
  write_file(permissive, "permissive_parser = true\n"
                         "material elastic [\n"
                         "  E = 9\n"
                         "  rules rule_b [\n"
                         "    value = 1\n"
                         "  ]\n"
                         "  weight_function weight_parameter [\n"
                         "    radius = 4.5\n"
                         "  ]\n"
                         "]\n");

  auto parse = [&](const std::filesystem::path & file) {
    Parser parser;
    parser.parse(file.string());
    const auto & section = *parser.getSubSections(ParserType::_material).first;
    p.parseSection(section);
    return std::string("ok");
  };

  Log log;
  log.record("get parent radius", [&] { return get_real(p, "radius"); });
  log.record("set parent radius 2", [&] {
    p.set("radius", Real(2.));
    return std::string("ok");
  });
  log.record("set parent E 2", [&] {
    p.set("E", Real(2.));
    return std::string("ok");
  });
  log.record("list parent subs", [&] { return join(p.listSubRegisteries()); });
  log.record("list weight subs", [&] { return join(wf.listSubRegisteries()); });
  log.record("parse ok", [&] { return parse(ok); });
  log.record("get parent E", [&] { return get_real(p, "E"); });
  log.record("get parent nu", [&] { return get_real(p, "nu"); });
  log.record("get weight radius", [&] { return get_real(wf, "radius"); });
  log.record("get weight update_rate",
             [&] { return std::to_string(Int(wf.get("update_rate"))); });
  log.record("get inner k", [&] { return get_real(inner, "k"); });
  log.record("get rule_a_first value", [&] { return get_real(rule_a1, "value"); });
  log.record("get rule_a_second value", [&] { return get_real(rule_a2, "value"); });
  log.record("parse unknown", [&] { return parse(unknown); });
  log.record("get parent E", [&] { return get_real(p, "E"); });
  log.record("parse wrong_type", [&] { return parse(wrong_type); });
  log.record("get parent E", [&] { return get_real(p, "E"); });
  log.record("parse permissive", [&] { return parse(permissive); });
  log.record("get parent E", [&] { return get_real(p, "E"); });
  log.record("get weight radius", [&] { return get_real(wf, "radius"); });
  log.record("print parent", [&] { return p.print(); });
  Parser::setPermissive(false);

  ap::FixtureWriter w(root, "parser/subregistry/parsable_tree",
                      "Parsable::registerSubSection: parseSection dispatch of sub-sections "
                      "by (type, name) over three levels, insert_or_assign re-registration, "
                      "strict/permissive unknown sub-sections, non-recursive get/set of a "
                      "Parsable (consisder_sub = false), call order",
                      kProbe, kProbeVersion);
  w.add_tag("parser");
  w.add_tag("wp-verify-1");
  w.parameters()["ops"] = log.ops;
  w.parameters()["inputs"] = ap::Json::object();
  for (const auto & f : {ok, unknown, wrong_type, permissive}) {
    std::ifstream in(f, std::ios::binary);
    std::stringstream ss;
    ss << in.rdbuf();
    w.parameters()["inputs"][f.filename().string()] = ss.str();
  }
  std::vector<double> final_values{E, nu, radius, double(update_rate), k, va1, va2};
  w.add("final_values", {final_values.size()}, {"entry"}, "table", final_values)
      .description("members after all operations: parent.E, parent.nu, weight.radius, "
                   "weight.update_rate, inner.k, rule_a_first.value, rule_a_second.value")
      .tolerance_class("exact");
  w.finish();
  std::cout << "  parser/subregistry/parsable_tree\n";
}

} // namespace

int main(int argc, char ** argv) {
  if (argc != 3) {
    std::cerr << "usage: parsable_probe <fixtures_root> <work_dir>\n";
    return 2;
  }
  int fake_argc = 1;
  char ** fake_argv = argv;
  initialize(fake_argc, fake_argv);
  const std::filesystem::path root(argv[1]);
  const std::filesystem::path work(argv[2]);
  std::filesystem::create_directories(work);
  int failures = 0;
  try {
    registries(root);
    parsables(root, work);
  } catch (std::exception & e) {
    std::cerr << "parsable_probe failed: " << e.what() << "\n";
    ++failures;
  }
  finalize();
  return failures == 0 ? 0 : 1;
}
