// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): records what `akantu::initialize(input_file, argc, argv)` of the
// installed reference library does to the process-global random generator
// (src/common/aka_common.cc:62-125: seed policy AKA_SEED > --aka_seed >
// input-file `seed` > time(nullptr), times prank + 1;
// RandomGenerator<Idx>::seed) on every rank
// (row CORE-031). Run under mpirun by
// oracle/probes/python/init_cases.py, one invocation per scenario and rank
// count.
//
// Records per rank: the seed (RandomGenerator<Idx>::seed()), the next raw
// engine values of RandomGenerator<Idx> and of the independent
// RandomGenerator<UInt>, the argument vector left after initialize removed
// the parsed --aka_* options, the parsed `--aka_debug_level`, and
// time(nullptr) just before and after initialize (bounds of the time
// fallback).
//
// Output: fixtures/core/initialize/<scenario>_np<size>_r<rank>
// Usage: init_probe <fixtures_root> <scenario> <input_file|-> [args...]

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_random_generator.hh>
#include <communicator.hh>

#include <cppargparse.hh>

#include <cstdint>
#include <cstdlib>
#include <ctime>
#include <filesystem>
#include <iostream>
#include <string>
#include <vector>

using namespace akantu;
namespace ap = akantu_probe;

int main(int argc, char ** argv) {
  if (argc < 4) {
    std::cerr << "usage: init_probe <fixtures_root> <scenario> <input_file|-> [args...]\n";
    return 2;
  }
  const std::filesystem::path root(argv[1]);
  const std::string scenario(argv[2]);
  const std::string input_file = std::string(argv[3]) == "-" ? std::string() : argv[3];

  // argv seen by initialize: a fixed program name (deterministic fixtures)
  // followed by the extra arguments
  std::vector<std::string> given{"init_probe"};
  for (int i = 4; i < argc; ++i) {
    given.emplace_back(argv[i]);
  }
  std::vector<char *> args;
  for (auto & s : given) {
    args.push_back(s.data());
  }
  args.push_back(nullptr);
  int a_argc = static_cast<int>(given.size());
  char ** a_argv = args.data();

  const auto t_before = static_cast<long long>(std::time(nullptr));
  initialize(input_file, a_argc, a_argv);
  const auto t_after = static_cast<long long>(std::time(nullptr));

  auto & comm = Communicator::getStaticCommunicator();
  const auto rank = comm.whoAmI();
  const auto size = comm.getNbProc();

  std::vector<std::string> remaining;
  for (int i = 0; i < a_argc; ++i) {
    remaining.emplace_back(a_argv[i]);
  }

  const long seed = RandomGenerator<Idx>::seed();
  RandomGenerator<Idx> gen;
  std::vector<std::int64_t> draws;
  for (int k = 0; k < 6; ++k) {
    draws.push_back(static_cast<std::int64_t>(gen()));
  }
  RandomGenerator<UInt> gen_u;
  std::vector<std::int64_t> draws_u;
  for (int k = 0; k < 2; ++k) {
    draws_u.push_back(static_cast<std::int64_t>(gen_u()));
  }
  const long debug_level = getStaticArgumentParser()["aka_debug_level"];

  const std::string case_id = "core/initialize/" + scenario + "_np" + std::to_string(size) +
                              "_r" + std::to_string(rank);
  ap::FixtureWriter w(root, case_id,
                      "akantu::initialize(input_file, argc, argv) seed policy on rank " +
                          std::to_string(rank) + " of " + std::to_string(size) +
                          ": seed, first engine values of RandomGenerator<Idx> and "
                          "RandomGenerator<UInt>, remaining arguments",
                      "init_probe", "1");
  w.add_tag("core");
  w.add_tag("wp-verify-1");
  w.set_parallel(size, rank);
  auto & p = w.parameters();
  p["scenario"] = scenario;
  p["input_file"] = input_file;
  const char * env_seed = std::getenv("AKA_SEED");
  if (env_seed != nullptr) {
    p["env_aka_seed"] = std::string(env_seed);
  }
  p["argv"] = ap::Json::array_of(given);
  p["remaining_argv"] = ap::Json::array_of(remaining);
  p["rank"] = static_cast<long long>(rank);
  p["size"] = static_cast<long long>(size);
  p["time_before"] = t_before;
  p["time_after"] = t_after;
  p["debug_level"] = static_cast<long long>(debug_level);
  std::vector<std::int64_t> s{static_cast<std::int64_t>(seed)};
  w.add("seed", {1}, {"entry"}, "scalar", s)
      .description("RandomGenerator<Idx>::seed() after initialize")
      .tolerance_class("exact");
  w.add("draws/idx", {draws.size()}, {"entry"}, "table", draws)
      .description("next raw values of RandomGenerator<Idx> (std::default_random_engine)")
      .tolerance_class("exact");
  w.add("draws/uint", {draws_u.size()}, {"entry"}, "table", draws_u)
      .description("next raw values of RandomGenerator<UInt> (never seeded by initialize)")
      .tolerance_class("exact");
  if (not input_file.empty() and std::filesystem::exists(input_file)) {
    w.add_input_file("input_file", input_file);
  }
  w.finish();
  std::cout << "  " << case_id << "\n";
  finalize();
  return 0;
}
