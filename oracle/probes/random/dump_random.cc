// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs. Oracle probe for the random-number facilities ported from
// Akantu (EPFL/LSMS): src/common/aka_random_generator.hh, src/common/aka_extern.cc
// (default seed), src/common/aka_common.cc (seed policy).
//
// Stand-alone: needs only the C++ standard library (not Akantu). It prints the
// raw output of `std::default_random_engine`, `std::generate_canonical`, every
// standard distribution wrapped by Akantu, and Akantu's `RandomParameter`
// semantics (re-implemented verbatim below) as JSON fixtures, with every
// double written as its IEEE-754 bit pattern (16 hex digits).
//
// Build and run (see run.sh):
//   g++ -O2 -std=c++20 -ffp-contract=off -o dump_random dump_random.cc
//   ./dump_random <output-dir>

#include <bit>
#include <cfloat>
#include <climits>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <functional>
#include <gnu/libc-version.h>
#include <iomanip>
#include <iostream>
#include <memory>
#include <random>
#include <sstream>
#include <string>
#include <type_traits>
#include <vector>

#ifndef PROBE_FLAGS
#define PROBE_FLAGS "unknown"
#endif

static_assert(std::is_same_v<std::default_random_engine, std::minstd_rand0>,
              "probe assumes libstdc++ (default_random_engine == minstd_rand0)");

namespace {

constexpr std::size_t N = 1000;

std::string hex(double x) {
  char buf[32];
  std::snprintf(buf, sizeof(buf), "\"%016llx\"",
                static_cast<unsigned long long>(std::bit_cast<std::uint64_t>(x)));
  return buf;
}

std::string json_str(const std::string & s) {
  std::string out = "\"";
  for (char c : s) {
    if (c == '"' || c == '\\') {
      out += '\\';
    }
    out += c;
  }
  out += '"';
  return out;
}

template <class Engine> unsigned long long engine_state(const Engine & e) {
  // libstdc++ prints the LCG state `_M_x` with operator<<.
  std::ostringstream os;
  os << e;
  return std::stoull(os.str());
}

template <class Dist> std::string dist_str(const Dist & d) {
  std::ostringstream os;
  os << d;
  return os.str();
}

std::string meta_json() {
  std::ostringstream os;
  os << "{\"compiler\":" << json_str(__VERSION__)
     << ",\"glibcxx\":" << __GLIBCXX__
     << ",\"glibcxx_release\":" << _GLIBCXX_RELEASE
     << ",\"glibc\":" << json_str(gnu_get_libc_version())
#if defined(__aarch64__)
     << ",\"arch\":\"aarch64\""
#elif defined(__x86_64__)
     << ",\"arch\":\"x86_64\""
#else
     << ",\"arch\":\"other\""
#endif
     << ",\"flags\":" << json_str(PROBE_FLAGS)
#if defined(__FP_FAST_FMA)
     << ",\"fp_fast_fma\":true"
#else
     << ",\"fp_fast_fma\":false"
#endif
     << ",\"flt_eval_method\":" << FLT_EVAL_METHOD
     << ",\"sizeof_long\":" << sizeof(long)
     << ",\"sizeof_uint_fast32_t\":" << sizeof(std::uint_fast32_t)
     << ",\"long_double_digits\":" << LDBL_MANT_DIG << "}";
  return os.str();
}

/* -------------------------------------------------------------------------- */
/* Verbatim re-implementation of akantu/src/common/aka_random_generator.hh     */
/* (35a851780d), minus Akantu's Array/enum machinery.                          */
/* -------------------------------------------------------------------------- */
enum class RandomDistributionType {
  _uniform,
  _exponential,
  _gamma,
  _weibull,
  _extreme_value,
  _normal,
  _lognormal,
  _chi_squared,
  _cauchy,
  _fisher_f,
  _student_t,
  _not_defined
};

const char * to_string(RandomDistributionType t) {
  switch (t) {
  case RandomDistributionType::_uniform: return "uniform";
  case RandomDistributionType::_exponential: return "exponential";
  case RandomDistributionType::_gamma: return "gamma";
  case RandomDistributionType::_weibull: return "weibull";
  case RandomDistributionType::_extreme_value: return "extreme_value";
  case RandomDistributionType::_normal: return "normal";
  case RandomDistributionType::_lognormal: return "lognormal";
  case RandomDistributionType::_chi_squared: return "chi_squared";
  case RandomDistributionType::_cauchy: return "cauchy";
  case RandomDistributionType::_fisher_f: return "fisher_f";
  case RandomDistributionType::_student_t: return "student_t";
  case RandomDistributionType::_not_defined: return "not_defined";
  }
  return "?";
}

using Real = double;
using Idx = int;
using UInt = unsigned int;

template <typename T> class RandomGenerator {
private:
  static long int _seed;
  static std::default_random_engine generator;

public:
  inline T operator()() { return generator(); }
  std::default_random_engine & getGenerator() { return generator; }
  static void seed(long int s) {
    _seed = s;
    generator.seed(_seed);
  }
  static long int seed() { return _seed; }
  static constexpr T min() { return std::default_random_engine::min(); }
  static constexpr T max() { return std::default_random_engine::max(); }
};

// aka_extern.cc:81-85
template <> long int RandomGenerator<Idx>::_seed = 5489;
template <> long int RandomGenerator<UInt>::_seed = 5489U;
template <> std::default_random_engine RandomGenerator<Idx>::generator(5489);
template <> std::default_random_engine RandomGenerator<UInt>::generator(5489U);

template <typename T, class Distribution> struct RandomDistributionTypeHelper {
  static constexpr RandomDistributionType value =
      RandomDistributionType::_not_defined;
};
#define HELPER(DIST, TYPE)                                                     \
  template <> struct RandomDistributionTypeHelper<Real, std::DIST<Real>> {     \
    static constexpr RandomDistributionType value =                            \
        RandomDistributionType::TYPE;                                          \
  };
HELPER(uniform_real_distribution, _uniform)
HELPER(exponential_distribution, _exponential)
HELPER(gamma_distribution, _gamma)
HELPER(weibull_distribution, _weibull)
HELPER(extreme_value_distribution, _extreme_value)
HELPER(normal_distribution, _normal)
HELPER(lognormal_distribution, _lognormal)
HELPER(chi_squared_distribution, _chi_squared)
HELPER(cauchy_distribution, _cauchy)
HELPER(fisher_f_distribution, _fisher_f)
HELPER(student_t_distribution, _student_t)
#undef HELPER

template <class T> class RandomDistribution {
public:
  virtual ~RandomDistribution() = default;
  virtual T operator()(RandomGenerator<Idx> & gen) = 0;
  virtual std::unique_ptr<RandomDistribution<T>> make_unique() const = 0;
  virtual void printself(std::ostream & stream, int = 0) const = 0;
};

template <class T, class Distribution>
class RandomDistributionProxy : public RandomDistribution<T> {
public:
  explicit RandomDistributionProxy(Distribution dist)
      : distribution(std::move(dist)) {}
  T operator()(RandomGenerator<Idx> & gen) override {
    return this->distribution(gen.getGenerator());
  }
  std::unique_ptr<RandomDistribution<T>> make_unique() const override {
    return std::make_unique<RandomDistributionProxy<T, Distribution>>(
        distribution);
  }
  void printself(std::ostream & stream, int = 0) const override {
    stream << to_string(RandomDistributionTypeHelper<T, Distribution>::value);
    stream << " [ " << distribution << " ]";
  }

private:
  Distribution distribution;
};

template <typename T> class RandomParameter {
public:
  template <class Distribution>
  explicit RandomParameter(T base_value, Distribution dist)
      : base_value(base_value),
        type(RandomDistributionTypeHelper<T, Distribution>::value),
        distribution_proxy(
            std::make_unique<RandomDistributionProxy<T, Distribution>>(
                std::move(dist))) {}

  explicit RandomParameter(T base_value)
      : base_value(base_value), type(RandomDistributionType::_uniform),
        distribution_proxy(
            std::make_unique<
                RandomDistributionProxy<T, std::uniform_real_distribution<T>>>(
                std::uniform_real_distribution<T>(0., 0.))) {}

  RandomParameter(const RandomParameter & other)
      : base_value(other.base_value), type(other.type),
        distribution_proxy(other.distribution_proxy->make_unique()) {}

  RandomParameter & operator=(const RandomParameter & other) {
    distribution_proxy = other.distribution_proxy->make_unique();
    base_value = other.base_value;
    type = other.type;
    return *this;
  }

  RandomParameter(RandomParameter && other) noexcept = default;
  RandomParameter & operator=(RandomParameter && other) noexcept = default;

  void setBaseValue(const T & value) { this->base_value = value; }
  T getBaseValue() const { return this->base_value; }

  template <class iterator> void setValues(iterator it, iterator end) {
    RandomGenerator<Idx> gen;
    for (; it != end; ++it) {
      *it = this->base_value + (*distribution_proxy)(gen);
    }
  }

  void printself(std::ostream & stream, int = 0) const {
    stream << base_value;
    stream << " + ";
    distribution_proxy->printself(stream);
  }

private:
  T base_value;
  RandomDistributionType type;
  std::unique_ptr<RandomDistribution<T>> distribution_proxy;
};

template <typename T>
RandomParameter<T>
make_random_parameter(T base,
                      RandomDistributionType distribution =
                          RandomDistributionType::_uniform,
                      T a = T(), T b = T()) {
  switch (distribution) {
  case RandomDistributionType::_not_defined:
    return RandomParameter<T>(base, std::uniform_real_distribution<T>(0., 0.));
  case RandomDistributionType::_uniform:
    return RandomParameter<T>(base, std::uniform_real_distribution<T>(a, b));
  case RandomDistributionType::_exponential:
    return RandomParameter<T>(base, std::exponential_distribution<T>(a));
  case RandomDistributionType::_gamma:
    return RandomParameter<T>(base, std::gamma_distribution<T>(a, b));
  case RandomDistributionType::_weibull:
    return RandomParameter<T>(base, std::weibull_distribution<T>(b, a));
  case RandomDistributionType::_extreme_value:
    return RandomParameter<T>(base, std::extreme_value_distribution<T>(a, b));
  case RandomDistributionType::_normal:
    return RandomParameter<T>(base, std::normal_distribution<T>(a, b));
  case RandomDistributionType::_lognormal:
    return RandomParameter<T>(base, std::lognormal_distribution<T>(a, b));
  case RandomDistributionType::_chi_squared:
    return RandomParameter<T>(base, std::chi_squared_distribution<T>(a));
  case RandomDistributionType::_cauchy:
    return RandomParameter<T>(base, std::cauchy_distribution<T>(a, b));
  case RandomDistributionType::_fisher_f:
    return RandomParameter<T>(base, std::fisher_f_distribution<T>(a, b));
  case RandomDistributionType::_student_t:
    return RandomParameter<T>(base, std::student_t_distribution<T>(a));
  }
  return RandomParameter<T>(base, std::uniform_real_distribution<T>(0., 0.));
}

std::string print_param(const RandomParameter<Real> & p) {
  std::ostringstream os;
  p.printself(os);
  return os.str();
}

/* -------------------------------------------------------------------------- */
/* Output helpers                                                             */
/* -------------------------------------------------------------------------- */
struct Out {
  std::string dir;
  FILE * open(const std::string & name) const {
    auto path = dir + "/" + name;
    FILE * f = std::fopen(path.c_str(), "w");
    if (f == nullptr) {
      std::perror(path.c_str());
      std::exit(1);
    }
    return f;
  }
};

void put_values(FILE * f, const std::vector<double> & v) {
  std::fputc('[', f);
  for (std::size_t i = 0; i < v.size(); ++i) {
    std::fputs(hex(v[i]).c_str(), f);
    if (i + 1 != v.size()) {
      std::fputc(',', f);
    }
  }
  std::fputc(']', f);
}

const std::vector<long int> ENGINE_SEEDS = {
    0L,          1L,          2L,          42L,
    5489L,       123456L,     1496137735L, 2992275470L,
    2147483646L, 2147483647L, 2147483648L, 4294967295L,
    4294967296L, 4294967297L, -1L,         -2L,
    -42L,        -2147483647L, -2147483648L, LONG_MAX,
    LONG_MIN,    LONG_MIN + 1};

/* -------------------------------------------------------------------------- */
void dump_engine(const Out & out) {
  FILE * f = out.open("engine.json");
  std::fprintf(f, "{\"meta\":%s,\n\"min\":%llu,\"max\":%llu,\n",
               meta_json().c_str(),
               (unsigned long long)std::default_random_engine::min(),
               (unsigned long long)std::default_random_engine::max());
  {
    std::default_random_engine e;
    std::fprintf(f, "\"default_constructed_state\":%llu,\n", engine_state(e));
    e.discard(9999);
    std::fprintf(f, "\"default_10000th\":%llu,\n", (unsigned long long)e());
  }
  std::fputs("\"seeds\":[\n", f);
  for (std::size_t s = 0; s < ENGINE_SEEDS.size(); ++s) {
    long int seed = ENGINE_SEEDS[s];
    // Exactly what RandomGenerator<Idx>::seed(long int) does.
    RandomGenerator<Idx>::seed(seed);
    RandomGenerator<Idx> gen;
    auto state = engine_state(gen.getGenerator());
    std::fprintf(f, "{\"seed\":\"%ld\",\"state\":%llu,\"values\":[", seed,
                 state);
    for (std::size_t i = 0; i < N; ++i) {
      Idx v = gen();
      std::fprintf(f, "%d%s", v, i + 1 == N ? "" : ",");
    }
    std::fprintf(f, "]}%s\n", s + 1 == ENGINE_SEEDS.size() ? "" : ",");
  }
  std::fputs("]}\n", f);
  std::fclose(f);
}

/* -------------------------------------------------------------------------- */
void dump_canonical(const Out & out) {
  FILE * f = out.open("canonical.json");
  std::fprintf(f, "{\"meta\":%s,\n\"cases\":[\n", meta_json().c_str());
  const std::vector<long int> seeds = {5489L, 1L, 42L, -3L};
  for (std::size_t s = 0; s < seeds.size(); ++s) {
    std::default_random_engine e;
    e.seed(seeds[s]);
    std::vector<double> v;
    for (std::size_t i = 0; i < N; ++i) {
      v.push_back(std::generate_canonical<double, 53>(e));
    }
    std::fprintf(f, "{\"seed\":\"%ld\",\"state_after\":%llu,\"values\":",
                 seeds[s], engine_state(e));
    put_values(f, v);
    std::fprintf(f, "}%s\n", s + 1 == seeds.size() ? "" : ",");
  }
  // generate_canonical boundary: the largest sum must map below 1.
  std::fputs("]}\n", f);
  std::fclose(f);
}

/* -------------------------------------------------------------------------- */
struct DistCase {
  std::string name;
  std::vector<double> params;
  // Draws n values with a fresh distribution object, returns values and the
  // printed distribution (operator<<) after the draws.
  std::function<std::pair<std::vector<double>, std::string>(
      std::default_random_engine &, std::size_t)>
      run;
};

template <class D>
DistCase make_case(const std::string & name, std::vector<double> params,
                   std::function<D()> factory) {
  return DistCase{name, std::move(params),
                  [factory](std::default_random_engine & e, std::size_t n) {
                    D d = factory();
                    std::vector<double> v;
                    v.reserve(n);
                    for (std::size_t i = 0; i < n; ++i) {
                      v.push_back(d(e));
                    }
                    return std::make_pair(v, dist_str(d));
                  }};
}

std::vector<DistCase> dist_cases() {
  using namespace std;
  vector<DistCase> c;
  auto uni = [&](double a, double b) {
    c.push_back(make_case<uniform_real_distribution<double>>(
        "uniform_real", {a, b},
        [a, b] { return uniform_real_distribution<double>(a, b); }));
  };
  uni(0., 1.);
  uni(0., 0.);
  uni(-2.5, 7.25);
  uni(0., 9.4e3);
  uni(1e-3, 1e5);

  auto expo = [&](double l) {
    c.push_back(make_case<exponential_distribution<double>>(
        "exponential", {l}, [l] { return exponential_distribution<double>(l); }));
  };
  expo(1.);
  expo(0.5);
  expo(3.7);

  auto gam = [&](double a, double b) {
    c.push_back(make_case<gamma_distribution<double>>(
        "gamma", {a, b}, [a, b] { return gamma_distribution<double>(a, b); }));
  };
  gam(1., 1.);
  gam(0.5, 2.);
  gam(0.1, 1.);
  gam(2.5, 1.5);
  gam(9., 0.5);
  gam(100., 0.01);

  auto wei = [&](double a, double b) {
    c.push_back(make_case<weibull_distribution<double>>(
        "weibull", {a, b}, [a, b] { return weibull_distribution<double>(a, b); }));
  };
  wei(1., 1.);
  wei(2., 3.5);
  wei(0.5, 1.);
  wei(10., 2e6);

  auto ev = [&](double a, double b) {
    c.push_back(make_case<extreme_value_distribution<double>>(
        "extreme_value", {a, b},
        [a, b] { return extreme_value_distribution<double>(a, b); }));
  };
  ev(0., 1.);
  ev(-1.5, 2.5);

  auto nor = [&](double m, double s) {
    c.push_back(make_case<normal_distribution<double>>(
        "normal", {m, s}, [m, s] { return normal_distribution<double>(m, s); }));
  };
  nor(0., 1.);
  nor(10., 0.5);
  nor(-3., 1e-3);

  auto lnor = [&](double m, double s) {
    c.push_back(make_case<lognormal_distribution<double>>(
        "lognormal", {m, s},
        [m, s] { return lognormal_distribution<double>(m, s); }));
  };
  lnor(0., 1.);
  lnor(1.5, 0.25);

  auto chi = [&](double n) {
    c.push_back(make_case<chi_squared_distribution<double>>(
        "chi_squared", {n}, [n] { return chi_squared_distribution<double>(n); }));
  };
  chi(1.);
  chi(2.5);
  chi(10.);

  auto cau = [&](double a, double b) {
    c.push_back(make_case<cauchy_distribution<double>>(
        "cauchy", {a, b}, [a, b] { return cauchy_distribution<double>(a, b); }));
  };
  cau(0., 1.);
  cau(2., 0.5);

  auto fis = [&](double m, double n) {
    c.push_back(make_case<fisher_f_distribution<double>>(
        "fisher_f", {m, n}, [m, n] { return fisher_f_distribution<double>(m, n); }));
  };
  fis(1., 1.);
  fis(5., 2.);
  fis(10.5, 20.);

  auto stu = [&](double n) {
    c.push_back(make_case<student_t_distribution<double>>(
        "student_t", {n}, [n] { return student_t_distribution<double>(n); }));
  };
  stu(1.);
  stu(3.);
  stu(30.);
  return c;
}

void dump_distributions(const Out & out) {
  const std::vector<long int> seeds = {5489L, 42L, 123456L};
  auto cases = dist_cases();
  FILE * f = out.open("distributions.json");
  std::fprintf(f, "{\"meta\":%s,\n\"cases\":[\n", meta_json().c_str());
  bool first = true;
  for (auto & dc : cases) {
    for (auto seed : seeds) {
      std::default_random_engine e;
      e.seed(seed);
      auto [values, printed] = dc.run(e, N);
      if (not first) {
        std::fputs(",\n", f);
      }
      first = false;
      std::fprintf(f, "{\"dist\":\"%s\",\"params\":[", dc.name.c_str());
      for (std::size_t i = 0; i < dc.params.size(); ++i) {
        std::fprintf(f, "%s%s", hex(dc.params[i]).c_str(),
                     i + 1 == dc.params.size() ? "" : ",");
      }
      std::fprintf(f,
                   "],\"seed\":\"%ld\",\"state_after\":%llu,\"printed\":%s,"
                   "\"values\":",
                   seed, engine_state(e), json_str(printed).c_str());
      put_values(f, values);
      std::fputs("}", f);
    }
  }
  std::fputs("\n]}\n", f);
  std::fclose(f);
}

/* -------------------------------------------------------------------------- */
/* Akantu RandomParameter scenarios                                           */
/* -------------------------------------------------------------------------- */
struct Writer {
  FILE * f;
  bool first = true;
  void event(const std::string & json) {
    std::fprintf(f, "%s%s", first ? "" : ",\n", json.c_str());
    first = false;
  }
};

std::string values_json(const std::vector<double> & v) {
  std::string s = "[";
  for (std::size_t i = 0; i < v.size(); ++i) {
    s += hex(v[i]);
    if (i + 1 != v.size()) {
      s += ",";
    }
  }
  return s + "]";
}

std::string gen_state() {
  RandomGenerator<Idx> g;
  return std::to_string(engine_state(g.getGenerator()));
}

void dump_parameters(const Out & out) {
  FILE * f = out.open("random_parameter.json");
  std::fprintf(f, "{\"meta\":%s,\n", meta_json().c_str());

  // 1. make_random_parameter for every type: printed form + draws through
  //    setValues in chunks (global engine shared, distribution state kept).
  struct MakeCase {
    double base;
    RandomDistributionType type;
    int nparams;
    double a, b;
  };
  using RDT = RandomDistributionType;
  const std::vector<MakeCase> make_cases = {
      {0., RDT::_not_defined, 0, 0., 0.},
      {94e3, RDT::_uniform, 0, 0., 0.},
      {94e3, RDT::_uniform, 2, 0., 9.4e3},
      {1.5, RDT::_exponential, 1, 2., 0.},
      {-1., RDT::_gamma, 2, 0.5, 3.},
      {300e6, RDT::_weibull, 2, 30e6, 5.},
      {0., RDT::_weibull, 2, 1., 0.5},
      {2., RDT::_extreme_value, 2, 0., 1.},
      {1e-3, RDT::_normal, 2, 0., 1e-4},
      {0.1, RDT::_lognormal, 2, 0., 0.5},
      {0., RDT::_chi_squared, 1, 3., 0.},
      {5., RDT::_cauchy, 2, 0., 2.},
      {0., RDT::_fisher_f, 2, 4., 6.},
      {0., RDT::_student_t, 1, 2.5, 0.},
  };
  const std::vector<std::size_t> chunks = {3, 5, 1, 4, 7};

  std::fputs("\"make\":[\n", f);
  for (std::size_t c = 0; c < make_cases.size(); ++c) {
    const auto & mc = make_cases[c];
    RandomGenerator<Idx>::seed(1);
    RandomParameter<Real> p =
        mc.nparams == 0   ? make_random_parameter<Real>(mc.base, mc.type)
        : mc.nparams == 1 ? make_random_parameter<Real>(mc.base, mc.type, mc.a)
                          : make_random_parameter<Real>(mc.base, mc.type, mc.a,
                                                        mc.b);
    std::fprintf(f, "{\"base\":%s,\"type\":\"%s\",\"nparams\":%d,\"a\":%s,"
                    "\"b\":%s,\"printed_initial\":%s,\"chunks\":[",
                 hex(mc.base).c_str(), to_string(mc.type), mc.nparams,
                 hex(mc.a).c_str(), hex(mc.b).c_str(),
                 json_str(print_param(p)).c_str());
    for (std::size_t k = 0; k < chunks.size(); ++k) {
      std::vector<double> v(chunks[k]);
      p.setValues(v.begin(), v.end());
      std::fprintf(f, "{\"values\":%s,\"state\":%s,\"printed\":%s}%s",
                   values_json(v).c_str(), gen_state().c_str(),
                   json_str(print_param(p)).c_str(),
                   k + 1 == chunks.size() ? "" : ",");
    }
    std::fprintf(f, "]}%s\n", c + 1 == make_cases.size() ? "" : ",");
  }
  std::fputs("],\n", f);

  // 2. Interleaving and copy semantics (copies carry the cached normal value).
  std::fputs("\"interleave\":[\n", f);
  {
    Writer w{f};
    RandomGenerator<Idx>::seed(2024);
    auto pn = make_random_parameter<Real>(10., RDT::_normal, 0., 1.);
    auto pg = make_random_parameter<Real>(0., RDT::_gamma, 0.7, 2.);
    auto pu = RandomParameter<Real>(3.); // default: uniform(0, 0)
    auto pl = make_random_parameter<Real>(0., RDT::_lognormal, 0., 0.3);
    auto draw = [&](const char * name, RandomParameter<Real> & p,
                    std::size_t n) {
      std::vector<double> v(n);
      p.setValues(v.begin(), v.end());
      w.event(std::string("{\"param\":\"") + name +
              "\",\"n\":" + std::to_string(n) +
              ",\"values\":" + values_json(v) + ",\"state\":" + gen_state() +
              ",\"printed\":" + json_str(print_param(p)) + "}");
    };
    draw("n", pn, 3);
    draw("g", pg, 2);
    draw("u", pu, 2);
    RandomParameter<Real> pn_copy = pn; // copy constructor: cached value copied
    draw("n", pn, 1);
    draw("n_copy", pn_copy, 1);
    draw("l", pl, 3);
    RandomParameter<Real> pl_copy(pl);
    draw("l_copy", pl_copy, 2);
    draw("l", pl, 2);
    pn = pg; // copy assignment: n now a gamma with g's normal cache
    draw("n", pn, 3);
    draw("g", pg, 3);
    draw("u", pu, 1);
  }
  std::fputs("\n],\n", f);

  // 3. Printing of base values (std::ostream default formatting).
  std::fputs("\"print_base\":[\n", f);
  {
    const std::vector<double> bases = {
        0.,        -0.,      1.,       94e3,      300e6,      1e-7,
        0.1,       123456.,  1234567., 123456789., 2.5e-310,  -1.5,
        1e100,     0.0001,   0.00001,  9.9999995, 999999.5,   1.0e6,
        3.14159265358979, 2.5,    -2.5e-5,   INFINITY,   -INFINITY, NAN};
    for (std::size_t i = 0; i < bases.size(); ++i) {
      std::ostringstream os;
      os << bases[i];
      std::fprintf(f, "{\"x\":%s,\"s\":%s}%s", hex(bases[i]).c_str(),
                   json_str(os.str()).c_str(), i + 1 == bases.size() ? "" : ",\n");
    }
  }
  std::fputs("],\n", f);

  // 4. Printing with max_digits10 scientific of special values.
  std::fputs("\"print_sci\":[\n", f);
  {
    const std::vector<double> xs = {0.,    -0.,      1.,    0.1,   1e-300,
                                    5e-324, DBL_MAX, 9.4e3, 1.0 / 3.0, -2.5,
                                    INFINITY, -INFINITY, NAN, 0.5, 123456789012345678.};
    for (std::size_t i = 0; i < xs.size(); ++i) {
      std::ostringstream os;
      os.flags(std::ios_base::scientific | std::ios_base::left);
      os.precision(std::numeric_limits<double>::max_digits10);
      os << xs[i];
      std::fprintf(f, "{\"x\":%s,\"s\":%s}%s", hex(xs[i]).c_str(),
                   json_str(os.str()).c_str(), i + 1 == xs.size() ? "" : ",\n");
    }
  }
  std::fputs("],\n", f);

  // 5. RandomGenerator<Idx> and RandomGenerator<UInt> are independent statics.
  {
    RandomGenerator<UInt> gu;
    RandomGenerator<Idx>::seed(77);
    RandomGenerator<Idx> gi;
    std::vector<long long> vu, vi;
    for (int i = 0; i < 5; ++i) {
      vu.push_back(gu());
      vi.push_back(gi());
    }
    std::fprintf(f, "\"uint_generator_first5\":[%lld,%lld,%lld,%lld,%lld],\n",
                 vu[0], vu[1], vu[2], vu[3], vu[4]);
    std::fprintf(f, "\"idx_seed77_first5\":[%lld,%lld,%lld,%lld,%lld],\n", vi[0],
                 vi[1], vi[2], vi[3], vi[4]);
    std::fprintf(f, "\"idx_seed\":%ld,\"uint_seed\":%ld\n",
                 RandomGenerator<Idx>::seed(), RandomGenerator<UInt>::seed());
  }
  std::fputs("}\n", f);
  std::fclose(f);
}

} // namespace

int main(int argc, char ** argv) {
  if (argc != 2) {
    std::fprintf(stderr, "usage: %s <output-dir>\n", argv[0]);
    return 2;
  }
  Out out{argv[1]};
  // Order matters only for the global RandomGenerator<UInt> check (never
  // seeded, so its first values are those of seed 5489).
  dump_parameters(out);
  dump_engine(out);
  dump_canonical(out);
  dump_distributions(out);
  return 0;
}
