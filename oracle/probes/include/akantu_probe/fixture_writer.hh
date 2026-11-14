// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct Akantu counterpart: it is the C++ writer of the golden fixture
// format specified in docs/NUMERICAL_VALIDATION.md (Rust reader:
// crates/akantu-validation). Header-only, C++17, standard library only.
//
// Usage:
//   akantu_probe::FixtureWriter w(root, "fe/element_class/_triangle_3",
//                                 "description", "element_probe", "1");
//   w.add("shapes/order_1", {nq, nn}, {"quad", "node_local"}, "quadrature",
//         data_ptr).element_type("_triangle_3").tolerance_class("kernel");
//   w.finish();
//
// Provenance is taken from the environment (set by
// oracle/scripts/generate-fixtures.sh):
//   AKANTU_PROBE_BUILD_INFO   path of a JSON object written verbatim as
//                             manifest "build" (config, compiler, library
//                             flags, and the probe's actual compile command
//                             and flags: run-in-container.sh writes one per
//                             probe from compile_commands.json)
//   AKANTU_PROBE_REFERENCE_SHA / AKANTU_PROBE_REFERENCE_DESCRIBE
//   AKANTU_PROBE_PATCHES      comma-separated patch names
//   AKANTU_PROBE_CREATED      creation timestamp (RFC 3339)
//   AKANTU_PROBE_COMMAND      command line reproducing the case
// Without AKANTU_PROBE_BUILD_INFO a build object is synthesised from
// compile-time macros (AKANTU_PROBE_CXX_FLAGS, AKANTU_PROBE_CONFIG, ...).
//
// Eigen cache-size pin (D-050, D-073): on x86_64 a probe
// must include "akantu_probe/eigen_pin.hh" after its Akantu headers. The
// writer then refuses to write a fixture unless the EFFECTIVE Eigen cache
// sizes of the process are 16384 / 524288 / 524288 and records them as
// `build.libraries.eigen_cache_sizes_measured`. A probe without Eigen (the
// writer self-test) defines AKANTU_PROBE_NO_EIGEN before including this
// header. Diagnostics that force other sizes on purpose set the environment
// variable AKANTU_PROBE_ALLOW_UNPINNED_EIGEN=1 (the measured values are still
// recorded). Nothing changes on other architectures.

#ifndef AKANTU_PROBE_FIXTURE_WRITER_HH_
#define AKANTU_PROBE_FIXTURE_WRITER_HH_

#include <algorithm>
#include <array>
#include <cctype>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace akantu_probe {

/* -------------------------------------------------------------------------- */
/* SHA-256 (FIPS 180-4)                                                       */
/* -------------------------------------------------------------------------- */
class Sha256 {
public:
  Sha256() { reset(); }

  void reset() {
    h_ = {0x6a09e667U, 0xbb67ae85U, 0x3c6ef372U, 0xa54ff53aU,
          0x510e527fU, 0x9b05688cU, 0x1f83d9abU, 0x5be0cd19U};
    len_ = 0;
    buf_len_ = 0;
  }

  void update(const void * data, std::size_t n) {
    const auto * p = static_cast<const std::uint8_t *>(data);
    len_ += n;
    while (n > 0) {
      std::size_t take = std::min<std::size_t>(64 - buf_len_, n);
      std::memcpy(buf_.data() + buf_len_, p, take);
      buf_len_ += take;
      p += take;
      n -= take;
      if (buf_len_ == 64) {
        block(buf_.data());
        buf_len_ = 0;
      }
    }
  }

  std::string hex() {
    std::uint64_t bits = len_ * 8;
    std::uint8_t pad = 0x80;
    update(&pad, 1);
    std::uint8_t zero = 0;
    while (buf_len_ != 56) {
      update(&zero, 1);
    }
    std::array<std::uint8_t, 8> l{};
    for (int i = 0; i < 8; ++i) {
      l[7 - i] = static_cast<std::uint8_t>(bits >> (8 * i));
    }
    update(l.data(), 8);
    static const char * digits = "0123456789abcdef";
    std::string out;
    out.reserve(64);
    for (auto w : h_) {
      for (int i = 3; i >= 0; --i) {
        auto b = static_cast<std::uint8_t>(w >> (8 * i));
        out += digits[b >> 4];
        out += digits[b & 0xF];
      }
    }
    reset();
    return out;
  }

  static std::string of(const std::string & bytes) {
    Sha256 s;
    s.update(bytes.data(), bytes.size());
    return s.hex();
  }

private:
  static std::uint32_t rotr(std::uint32_t x, int n) {
    return (x >> n) | (x << (32 - n));
  }

  void block(const std::uint8_t * p) {
    static const std::uint32_t k[64] = {
        0x428a2f98U, 0x71374491U, 0xb5c0fbcfU, 0xe9b5dba5U, 0x3956c25bU,
        0x59f111f1U, 0x923f82a4U, 0xab1c5ed5U, 0xd807aa98U, 0x12835b01U,
        0x243185beU, 0x550c7dc3U, 0x72be5d74U, 0x80deb1feU, 0x9bdc06a7U,
        0xc19bf174U, 0xe49b69c1U, 0xefbe4786U, 0x0fc19dc6U, 0x240ca1ccU,
        0x2de92c6fU, 0x4a7484aaU, 0x5cb0a9dcU, 0x76f988daU, 0x983e5152U,
        0xa831c66dU, 0xb00327c8U, 0xbf597fc7U, 0xc6e00bf3U, 0xd5a79147U,
        0x06ca6351U, 0x14292967U, 0x27b70a85U, 0x2e1b2138U, 0x4d2c6dfcU,
        0x53380d13U, 0x650a7354U, 0x766a0abbU, 0x81c2c92eU, 0x92722c85U,
        0xa2bfe8a1U, 0xa81a664bU, 0xc24b8b70U, 0xc76c51a3U, 0xd192e819U,
        0xd6990624U, 0xf40e3585U, 0x106aa070U, 0x19a4c116U, 0x1e376c08U,
        0x2748774cU, 0x34b0bcb5U, 0x391c0cb3U, 0x4ed8aa4aU, 0x5b9cca4fU,
        0x682e6ff3U, 0x748f82eeU, 0x78a5636fU, 0x84c87814U, 0x8cc70208U,
        0x90befffaU, 0xa4506cebU, 0xbef9a3f7U, 0xc67178f2U};
    std::uint32_t w[64];
    for (int i = 0; i < 16; ++i) {
      w[i] = (std::uint32_t(p[4 * i]) << 24) |
             (std::uint32_t(p[4 * i + 1]) << 16) |
             (std::uint32_t(p[4 * i + 2]) << 8) | std::uint32_t(p[4 * i + 3]);
    }
    for (int i = 16; i < 64; ++i) {
      std::uint32_t s0 =
          rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >> 3);
      std::uint32_t s1 =
          rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >> 10);
      w[i] = w[i - 16] + s0 + w[i - 7] + s1;
    }
    auto a = h_[0], b = h_[1], c = h_[2], d = h_[3];
    auto e = h_[4], f = h_[5], g = h_[6], h = h_[7];
    for (int i = 0; i < 64; ++i) {
      std::uint32_t S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      std::uint32_t ch = (e & f) ^ (~e & g);
      std::uint32_t t1 = h + S1 + ch + k[i] + w[i];
      std::uint32_t S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      std::uint32_t mj = (a & b) ^ (a & c) ^ (b & c);
      std::uint32_t t2 = S0 + mj;
      h = g;
      g = f;
      f = e;
      e = d + t1;
      d = c;
      c = b;
      b = a;
      a = t1 + t2;
    }
    h_[0] += a;
    h_[1] += b;
    h_[2] += c;
    h_[3] += d;
    h_[4] += e;
    h_[5] += f;
    h_[6] += g;
    h_[7] += h;
  }

  std::array<std::uint32_t, 8> h_{};
  std::array<std::uint8_t, 64> buf_{};
  std::uint64_t len_{0};
  std::size_t buf_len_{0};
};

/// Hash `n` doubles for an oracle-compared digest under decision D-046
/// (docs/DECISIONS.md; akantu_validation::exact::canonical_le_bytes on the
/// Rust side): a default quiet NaN of either sign (`0x7ff8000000000000`
/// aarch64, `0xfff8000000000000` x86_64 SSE) is hashed as
/// `0x7ff8000000000000`; every other value as its own little-endian bytes.
/// Digests of aarch64 runs are therefore unchanged, and x86_64 runs give
/// the same digests.
inline void update_doubles_d046(Sha256 & h, const double * v, std::size_t n) {
  constexpr std::uint64_t kDefaultNan = 0x7ff8000000000000ULL;
  constexpr std::uint64_t kSign = 0x8000000000000000ULL;
  for (std::size_t i = 0; i < n; ++i) {
    std::uint64_t b = 0;
    std::memcpy(&b, v + i, sizeof b);
    if ((b & ~kSign) == kDefaultNan) {
      b = kDefaultNan;
    }
    unsigned char le[8];
    for (int k = 0; k < 8; ++k) {
      le[k] = static_cast<unsigned char>(b >> (8 * k));
    }
    h.update(le, sizeof le);
  }
}

/* -------------------------------------------------------------------------- */
/* Minimal JSON value (writer only)                                           */
/* -------------------------------------------------------------------------- */
class Json {
public:
  enum class Type { Null, Bool, Int, Real, String, Array, Object, Raw };

  Json() = default;
  Json(std::nullptr_t) {}
  Json(bool b) : type_(Type::Bool), b_(b) {}
  Json(int i) : type_(Type::Int), i_(i) {}
  Json(long i) : type_(Type::Int), i_(i) {}
  Json(long long i) : type_(Type::Int), i_(i) {}
  Json(unsigned i) : type_(Type::Int), i_(i) {}
  Json(unsigned long i) : type_(Type::Int), i_(static_cast<long long>(i)) {}
  Json(unsigned long long i)
      : type_(Type::Int), i_(static_cast<long long>(i)) {}
  Json(double r) : type_(Type::Real), r_(r) {
    if (not std::isfinite(r)) {
      throw std::runtime_error("JSON cannot represent non-finite numbers");
    }
  }
  Json(const char * s) : type_(Type::String), s_(s) {}
  Json(std::string s) : type_(Type::String), s_(std::move(s)) {}

  static Json array() {
    Json j;
    j.type_ = Type::Array;
    return j;
  }
  static Json object() {
    Json j;
    j.type_ = Type::Object;
    return j;
  }
  /// Pre-serialised JSON text inserted verbatim (must be valid JSON).
  static Json raw(std::string text) {
    Json j;
    j.type_ = Type::Raw;
    j.s_ = std::move(text);
    return j;
  }
  template <class T> static Json array_of(const std::vector<T> & v) {
    Json j = array();
    for (const auto & x : v) {
      j.push(Json(x));
    }
    return j;
  }

  Json & push(Json v) {
    if (type_ == Type::Null) {
      type_ = Type::Array;
    }
    if (type_ != Type::Array) {
      throw std::runtime_error("JSON push on a non-array");
    }
    arr_.push_back(std::move(v));
    return arr_.back();
  }

  /// Object member access (insertion order is preserved).
  Json & operator[](const std::string & key) {
    if (type_ == Type::Null) {
      type_ = Type::Object;
    }
    if (type_ != Type::Object) {
      throw std::runtime_error("JSON member access on a non-object");
    }
    for (auto & kv : obj_) {
      if (kv.first == key) {
        return kv.second;
      }
    }
    obj_.emplace_back(key, Json());
    return obj_.back().second;
  }

  bool has(const std::string & key) const {
    for (const auto & kv : obj_) {
      if (kv.first == key) {
        return true;
      }
    }
    return false;
  }

  Type type() const { return type_; }
  bool is_null() const { return type_ == Type::Null; }

  std::string dump(int indent = 2) const {
    std::string out;
    write(out, indent, 0);
    return out;
  }

  static std::string quote(const std::string & s) {
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
        if (c < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\u%04x", c);
          out += buf;
        } else {
          out += static_cast<char>(c);
        }
      }
    }
    out += "\"";
    return out;
  }

private:
  void write(std::string & out, int indent, int level) const {
    auto nl = [&](int l) {
      if (indent > 0) {
        out += '\n';
        out.append(static_cast<std::size_t>(indent * l), ' ');
      }
    };
    switch (type_) {
    case Type::Null:
      out += "null";
      break;
    case Type::Bool:
      out += b_ ? "true" : "false";
      break;
    case Type::Int:
      out += std::to_string(i_);
      break;
    case Type::Real: {
      char buf[32];
      std::snprintf(buf, sizeof(buf), "%.17g", r_);
      std::string s(buf);
      if (s.find_first_of(".eE") == std::string::npos) {
        s += ".0";
      }
      out += s;
      break;
    }
    case Type::String:
      out += quote(s_);
      break;
    case Type::Raw:
      out += s_;
      break;
    case Type::Array:
      if (arr_.empty()) {
        out += "[]";
        break;
      }
      out += '[';
      for (std::size_t i = 0; i < arr_.size(); ++i) {
        if (i > 0) {
          out += ',';
        }
        nl(level + 1);
        arr_[i].write(out, indent, level + 1);
      }
      nl(level);
      out += ']';
      break;
    case Type::Object:
      if (obj_.empty()) {
        out += "{}";
        break;
      }
      out += '{';
      for (std::size_t i = 0; i < obj_.size(); ++i) {
        if (i > 0) {
          out += ',';
        }
        nl(level + 1);
        out += quote(obj_[i].first);
        out += indent > 0 ? ": " : ":";
        obj_[i].second.write(out, indent, level + 1);
      }
      nl(level);
      out += '}';
      break;
    }
  }

  Type type_{Type::Null};
  bool b_{false};
  long long i_{0};
  double r_{0.};
  std::string s_;
  std::vector<Json> arr_;
  std::vector<std::pair<std::string, Json>> obj_;
};

/* -------------------------------------------------------------------------- */
/* .npy encoding (format 1.0, little-endian, C order)                        */
/* -------------------------------------------------------------------------- */
template <class T> struct NpyType;
template <> struct NpyType<double> {
  static constexpr const char * descr = "<f8";
  static constexpr const char * name = "f64";
};
template <> struct NpyType<std::int64_t> {
  static constexpr const char * descr = "<i8";
  static constexpr const char * name = "i64";
};
template <> struct NpyType<std::int32_t> {
  static constexpr const char * descr = "<i4";
  static constexpr const char * name = "i32";
};
template <> struct NpyType<std::uint64_t> {
  static constexpr const char * descr = "<u8";
  static constexpr const char * name = "u64";
};
template <> struct NpyType<std::uint32_t> {
  static constexpr const char * descr = "<u4";
  static constexpr const char * name = "u32";
};
template <> struct NpyType<std::uint8_t> {
  static constexpr const char * descr = "|u1";
  static constexpr const char * name = "u8";
};

inline bool host_is_little_endian() {
  const std::uint16_t one = 1;
  std::uint8_t first = 0;
  std::memcpy(&first, &one, 1);
  return first == 1;
}

/// Encode an array as .npy bytes. `bool_data` stores |b1 (values 0/1).
inline std::string encode_npy(const char * descr,
                              const std::vector<std::size_t> & shape,
                              const void * data, std::size_t elem_size,
                              std::size_t count) {
  if (not host_is_little_endian()) {
    throw std::runtime_error("the fixture writer requires a little-endian host");
  }
  std::size_t expected = 1;
  for (auto d : shape) {
    expected *= d;
  }
  if (expected != count) {
    throw std::runtime_error("npy: shape does not match the element count");
  }
  std::string shape_txt = "(";
  for (std::size_t i = 0; i < shape.size(); ++i) {
    shape_txt += std::to_string(shape[i]);
    if (shape.size() == 1 or i + 1 < shape.size()) {
      shape_txt += shape.size() == 1 ? "," : ", ";
    }
  }
  shape_txt += ")";
  std::string header = std::string("{'descr': '") + descr +
                       "', 'fortran_order': False, 'shape': " + shape_txt +
                       ", }";
  std::size_t unpadded = 10 + header.size() + 1;
  header.append((64 - unpadded % 64) % 64, ' ');
  header += '\n';
  std::string out;
  out.reserve(10 + header.size() + count * elem_size);
  out.append("\x93NUMPY", 6);
  out += static_cast<char>(1);
  out += static_cast<char>(0);
  auto hlen = static_cast<std::uint16_t>(header.size());
  out += static_cast<char>(hlen & 0xFF);
  out += static_cast<char>(hlen >> 8);
  out += header;
  out.append(static_cast<const char *>(data), count * elem_size);
  return out;
}

inline void write_file(const std::filesystem::path & path,
                       const std::string & bytes) {
  std::filesystem::create_directories(path.parent_path());
  std::ofstream f(path, std::ios::binary | std::ios::trunc);
  if (not f) {
    throw std::runtime_error("cannot open " + path.string());
  }
  f.write(bytes.data(), static_cast<std::streamsize>(bytes.size()));
  if (not f) {
    throw std::runtime_error("cannot write " + path.string());
  }
}

inline std::string read_file(const std::filesystem::path & path) {
  std::ifstream f(path, std::ios::binary);
  if (not f) {
    throw std::runtime_error("cannot read " + path.string());
  }
  std::ostringstream ss;
  ss << f.rdbuf();
  return ss.str();
}

/// Read a little-endian C-order `<f8` .npy file (format 1.x/2.x/3.x), e.g.
/// a prescribed history written by a Python case driver.
inline std::vector<double> read_npy_f64(const std::filesystem::path & path,
                                        std::vector<std::size_t> & shape) {
  const auto bytes = read_file(path);
  auto fail = [&](const std::string & why) {
    return std::runtime_error("read_npy_f64(" + path.string() + "): " + why);
  };
  if (bytes.size() < 10 or bytes.compare(0, 6, "\x93NUMPY") != 0) {
    throw fail("not an .npy file");
  }
  const auto major = static_cast<unsigned char>(bytes[6]);
  std::size_t hlen = 0, start = 0;
  if (major == 1) {
    hlen = static_cast<unsigned char>(bytes[8]) |
           (static_cast<std::size_t>(static_cast<unsigned char>(bytes[9])) << 8);
    start = 10;
  } else if (major == 2 or major == 3) {
    for (int i = 0; i < 4; ++i) {
      hlen |= static_cast<std::size_t>(static_cast<unsigned char>(bytes[8 + i]))
              << (8 * i);
    }
    start = 12;
  } else {
    throw fail("unsupported format version");
  }
  const auto header = bytes.substr(start, hlen);
  if (header.find("'descr': '<f8'") == std::string::npos) {
    throw fail("dtype must be '<f8'");
  }
  if (header.find("'fortran_order': False") == std::string::npos) {
    throw fail("array must be C-ordered");
  }
  auto lp = header.find('(', header.find("'shape'"));
  auto rp = header.find(')', lp);
  if (lp == std::string::npos or rp == std::string::npos) {
    throw fail("cannot parse shape");
  }
  shape.clear();
  std::stringstream ss(header.substr(lp + 1, rp - lp - 1));
  std::string tok;
  std::size_t n = 1;
  while (std::getline(ss, tok, ',')) {
    auto b = tok.find_first_not_of(' ');
    if (b == std::string::npos) {
      continue;
    }
    shape.push_back(static_cast<std::size_t>(std::stoull(tok.substr(b))));
    n *= shape.back();
  }
  const auto payload = start + hlen;
  if (bytes.size() != payload + n * sizeof(double)) {
    throw fail("payload size does not match the shape");
  }
  std::vector<double> out(n);
  std::memcpy(out.data(), bytes.data() + payload, n * sizeof(double));
  return out;
}

/// Portable file stem for a quantity name ('/' -> "__"), same rule as the
/// Rust writer (crates/akantu-validation/src/writer.rs `sanitize`).
inline std::string sanitize(const std::string & name) {
  std::string out;
  for (char c : name) {
    if (c == '/') {
      out += "__";
    } else if (std::isalnum(static_cast<unsigned char>(c)) or c == '_' or
               c == '-' or c == '.' or c == '+') {
      out += c;
    } else {
      out += '_';
    }
  }
  if (out.empty() or out == "." or out == "..") {
    out = "_";
  }
  return out;
}

inline std::string env_or(const char * name, const std::string & fallback) {
  const char * v = std::getenv(name);
  return (v != nullptr and *v != '\0') ? std::string(v) : fallback;
}

#if !defined(AKANTU_PROBE_CXX_FLAGS)
#define AKANTU_PROBE_CXX_FLAGS "unknown"
#endif
#if !defined(AKANTU_PROBE_CONFIG)
#define AKANTU_PROBE_CONFIG "unknown"
#endif
#if !defined(AKANTU_PROBE_BUILD_TYPE)
#define AKANTU_PROBE_BUILD_TYPE "unknown"
#endif

/* -------------------------------------------------------------------------- */
/* Eigen cache-size pin (D-050) */
/* -------------------------------------------------------------------------- */
/// Query of the effective Eigen cache sizes, registered by eigen_pin.hh (this
/// header does not include Eigen: it is included before the Akantu headers).
using EigenCacheQuery = void (*)(long &, long &, long &);

inline EigenCacheQuery & eigen_cache_query() {
  static EigenCacheQuery query = nullptr;
  return query;
}

namespace detail {
inline constexpr long kPinnedEigenL1 = 16384;
inline constexpr long kPinnedEigenL2 = 524288;
inline constexpr long kPinnedEigenL3 = 524288;

/// Measured "l1,l2,l3", or empty without a registered query.
inline std::string eigen_cache_sizes_text() {
  auto query = eigen_cache_query();
  if (query == nullptr) {
    return {};
  }
  long l1 = 0;
  long l2 = 0;
  long l3 = 0;
  query(l1, l2, l3);
  return std::to_string(l1) + "," + std::to_string(l2) + "," +
         std::to_string(l3);
}

/// Empty when the D-050 pin is in force (always empty off x86_64), else the
/// reason to refuse.
inline std::string eigen_pin_error() {
#if defined(__x86_64__) && !defined(AKANTU_PROBE_NO_EIGEN)
  // Root-cause diagnostics that force other sizes on purpose
  // (oracle/x86/fp-probe/eigen-cache-run.sh) set this; the measured values
  // are still recorded, so such a manifest never claims the pin.
  const char * allow = std::getenv("AKANTU_PROBE_ALLOW_UNPINNED_EIGEN");
  if (allow != nullptr and std::string(allow) == "1") {
    return {};
  }
  auto sizes = eigen_cache_sizes_text();
  if (sizes.empty()) {
    return "akantu_probe: D-050 Eigen cache-size pin not checked: include "
           "\"akantu_probe/eigen_pin.hh\" after the Akantu headers; "
           "refusing to write a fixture";
  }
  const std::string pinned = std::to_string(kPinnedEigenL1) + "," +
                             std::to_string(kPinnedEigenL2) + "," +
                             std::to_string(kPinnedEigenL3);
  if (sizes != pinned) {
    return "akantu_probe: effective Eigen cache sizes " + sizes +
           " differ from the D-050 pin " + pinned +
           " (a module compiled without the pin, or a preloaded shim, "
           "initialised Eigen's process-wide static first); refusing to write a fixture";
  }
#endif
  return {};
}

/// `raw` (the JSON object of AKANTU_PROBE_BUILD_INFO, written by
/// build_env.py) with `"key": "value"` added to its "libraries" object
/// (created when absent). `value` must not need JSON escaping.
inline std::string with_library_entry(const std::string & raw,
                                      const std::string & key,
                                      const std::string & value) {
  const std::string entry = "\"" + key + "\": \"" + value + "\"";
  const std::string name = "\"libraries\"";
  for (auto pos = raw.find(name); pos != std::string::npos;
       pos = raw.find(name, pos + 1)) {
    auto colon = raw.find_first_not_of(" \t\r\n", pos + name.size());
    if (colon == std::string::npos or raw[colon] != ':') {
      continue;
    }
    auto brace = raw.find_first_not_of(" \t\r\n", colon + 1);
    if (brace == std::string::npos or raw[brace] != '{') {
      continue;
    }
    auto next = raw.find_first_not_of(" \t\r\n", brace + 1);
    bool empty = next != std::string::npos and raw[next] == '}';
    return raw.substr(0, brace + 1) + entry + (empty ? "" : ", ") +
           raw.substr(brace + 1);
  }
  auto end = raw.find_last_of('}');
  if (end == std::string::npos) {
    throw std::runtime_error("AKANTU_PROBE_BUILD_INFO is not a JSON object");
  }
  auto last = raw.find_last_not_of(" \t\r\n", end == 0 ? 0 : end - 1);
  bool empty = last == std::string::npos or raw[last] == '{';
  return raw.substr(0, end) + (empty ? "" : ", ") + name + ": {" + entry +
         "}" + raw.substr(end);
}

/// Build object of AKANTU_PROBE_BUILD_INFO plus, on x86_64 with a registered
/// query, the measured Eigen cache sizes.
inline std::string build_info_with_measurements(const std::string & raw) {
#if defined(__x86_64__) && !defined(AKANTU_PROBE_NO_EIGEN)
  auto sizes = eigen_cache_sizes_text();
  if (not sizes.empty()) {
    return with_library_entry(
        raw, "eigen_cache_sizes_measured",
        sizes + " (Eigen::l1/l2/l3CacheSize() read at run time in the probe "
                "process, D-050)");
  }
#endif
  return raw;
}
} // namespace detail

/// Build description synthesised from compile-time information.
inline Json compile_time_build_info() {
  Json b = Json::object();
  b["config"] = env_or("AKANTU_PROBE_CONFIG", AKANTU_PROBE_CONFIG);
#if defined(__clang__)
  b["compiler"] = std::string("clang++ ") + __clang_version__;
#elif defined(__GNUC__)
  b["compiler"] = std::string("g++ ") + __VERSION__;
#else
  b["compiler"] = "unknown";
#endif
  b["cxx_flags"] = "unknown (no AKANTU_PROBE_BUILD_INFO: library flags not recorded)";
  // Flags this translation unit was compiled with (set by the probe build).
  b["probe_cxx_flags"] = std::string(AKANTU_PROBE_CXX_FLAGS);
  b["build_type"] = std::string(AKANTU_PROBE_BUILD_TYPE);
#if defined(__aarch64__)
  b["arch"] = "aarch64";
#elif defined(__x86_64__)
  b["arch"] = "x86_64";
#else
  b["arch"] = "unknown";
#endif
  b["os"] = "unknown";
  return b;
}

/* -------------------------------------------------------------------------- */
/* Fixture writer                                                             */
/* -------------------------------------------------------------------------- */
class FixtureWriter {
public:
  /// Builder handle on the last added quantity.
  class Quantity {
  public:
    explicit Quantity(Json & j) : j_(&j) {}
    Quantity & description(const std::string & s) {
      (*j_)["description"] = s;
      return *this;
    }
    Quantity & element_type(const std::string & s) {
      (*j_)["element_type"] = s;
      return *this;
    }
    Quantity & ghost_type(const std::string & s) {
      (*j_)["ghost_type"] = s;
      return *this;
    }
    Quantity & components(const std::vector<std::string> & c) {
      (*j_)["components"] = Json::array_of(c);
      return *this;
    }
    Quantity & ordering(const std::string & axis, const std::string & kind,
                        const std::string & key_quantity = "") {
      Json o = Json::object();
      o["axis"] = axis;
      o["kind"] = kind;
      if (not key_quantity.empty()) {
        o["key_quantity"] = key_quantity;
      }
      (*j_)["ordering"].push(o);
      return *this;
    }
    Quantity & ragged(const std::string & offsets, const std::string & row_axis) {
      Json r = Json::object();
      r["offsets"] = offsets;
      r["row_axis"] = row_axis;
      (*j_)["ragged"] = r;
      return *this;
    }
    Quantity & units(const std::string & s) {
      (*j_)["units"] = s;
      return *this;
    }
    Quantity & tolerance_class(const std::string & s) {
      (*j_)["tolerance_class"] = s;
      return *this;
    }

  private:
    Json * j_;
  };

  FixtureWriter(const std::filesystem::path & root, std::string case_id,
                std::string description, std::string probe,
                std::string probe_version)
      : case_id_(std::move(case_id)) {
    // D-050 pin first, before anything in the case directory is touched.
    if (auto err = detail::eigen_pin_error(); not err.empty()) {
      throw std::runtime_error(err);
    }
    dir_ = root;
    std::stringstream ss(case_id_);
    std::string seg;
    int n = 0;
    while (std::getline(ss, seg, '/')) {
      if (seg.empty() or seg == "." or seg == "..") {
        throw std::runtime_error("invalid case id " + case_id_);
      }
      dir_ /= seg;
      ++n;
    }
    if (n < 2) {
      throw std::runtime_error("case id needs <area>/<case>: " + case_id_);
    }
    // Regeneration replaces the generated files of the case, never the
    // reviewed tolerances.json.
    if (std::filesystem::exists(dir_ / "data")) {
      std::filesystem::remove_all(dir_ / "data");
    }
    if (std::filesystem::exists(dir_ / "inputs")) {
      std::filesystem::remove_all(dir_ / "inputs");
    }
    std::filesystem::create_directories(dir_);

    m_["format_version"] = 1;
    m_["case_id"] = case_id_;
    m_["description"] = std::move(description);
    m_["status"] = "golden";
    Json gen = Json::object();
    gen["probe"] = std::move(probe);
    gen["probe_version"] = std::move(probe_version);
    gen["command"] = env_or("AKANTU_PROBE_COMMAND", "");
    m_["generator"] = gen;
    Json ref = Json::object();
    ref["sha"] = env_or("AKANTU_PROBE_REFERENCE_SHA",
                        "35a851780d0b9471d258ed53a003ae98aaa95ea0");
    auto describe = env_or("AKANTU_PROBE_REFERENCE_DESCRIBE", "");
    if (not describe.empty()) {
      ref["describe"] = describe;
    }
    auto patches = env_or("AKANTU_PROBE_PATCHES", "");
    if (not patches.empty()) {
      Json p = Json::array();
      std::stringstream ps(patches);
      std::string one;
      while (std::getline(ps, one, ',')) {
        if (not one.empty()) {
          p.push(one);
        }
      }
      ref["patches"] = p;
    }
    m_["reference"] = ref;
    auto info = env_or("AKANTU_PROBE_BUILD_INFO", "");
    m_["build"] = info.empty() ? compile_time_build_info()
                               : Json::raw(detail::build_info_with_measurements(
                                     trim(read_file(info))));
    auto created = env_or("AKANTU_PROBE_CREATED", "");
    if (not created.empty()) {
      m_["created"] = created;
    }
    parameters_ = Json::object();
    quantities_ = Json::array();
  }

  const std::filesystem::path & dir() const { return dir_; }

  /// Free-form parameters object.
  Json & parameters() { return parameters_; }

  void set_command(const std::string & cmd) { m_["generator"]["command"] = cmd; }
  void set_status(const std::string & status) { m_["status"] = status; }
  void add_tag(const std::string & tag) { tags_.push_back(tag); }
  void set_notes(const std::string & notes) { notes_ = notes; }

  void set_steps(const std::vector<long long> & recorded, double dt = NAN,
                 const std::string & time_quantity = "") {
    Json s = Json::object();
    s["recorded"] = Json::array_of(recorded);
    if (std::isfinite(dt)) {
      s["dt"] = dt;
    }
    if (not time_quantity.empty()) {
      s["time_quantity"] = time_quantity;
    }
    steps_ = s;
  }

  void set_parallel(int ranks, int rank = -1, const std::string & partitioner = "",
                    const std::string & partition_quantity = "") {
    Json p = Json::object();
    p["ranks"] = ranks;
    if (rank >= 0) {
      p["rank"] = rank;
    }
    if (not partitioner.empty()) {
      p["partitioner"] = partitioner;
    }
    if (not partition_quantity.empty()) {
      p["partition_quantity"] = partition_quantity;
    }
    parallel_ = p;
  }

  void expect_error(const std::string & kind, const std::string & message,
                    int exit_code = 0) {
    Json e = Json::object();
    e["status"] = "error";
    e["exit_code"] = exit_code;
    if (not kind.empty()) {
      e["error_kind"] = kind;
    }
    if (not message.empty()) {
      e["message_contains"] = message;
    }
    expect_ = e;
  }

  /// Copy an input file into inputs/ and record its checksum.
  void add_input_file(const std::string & role,
                      const std::filesystem::path & src,
                      const std::string & source = "") {
    auto bytes = read_file(src);
    auto rel = std::string("inputs/") + src.filename().string();
    write_file(dir_ / rel, bytes);
    Json i = Json::object();
    i["role"] = role;
    i["path"] = rel;
    if (not source.empty()) {
      i["source"] = source;
    }
    i["sha256"] = Sha256::of(bytes);
    inputs_.push(i);
  }

  /// Record an input by reference only (e.g. a large upstream mesh).
  void add_input_reference(const std::string & role, const std::string & source,
                           const std::filesystem::path & file_for_checksum) {
    Json i = Json::object();
    i["role"] = role;
    i["source"] = source;
    i["sha256"] = Sha256::of(read_file(file_for_checksum));
    inputs_.push(i);
  }

  /// Add an array. The returned handle is only valid until the next add.
  template <class T>
  Quantity add(const std::string & name, const std::vector<std::size_t> & shape,
               const std::vector<std::string> & axes, const std::string & layout,
               const T * data) {
    std::size_t n = 1;
    for (auto d : shape) {
      n *= d;
    }
    return add_raw(name, shape, axes, layout, NpyType<T>::descr,
                   NpyType<T>::name, data, sizeof(T), n);
  }

  template <class T>
  Quantity add(const std::string & name, const std::vector<std::size_t> & shape,
               const std::vector<std::string> & axes, const std::string & layout,
               const std::vector<T> & data) {
    std::size_t n = 1;
    for (auto d : shape) {
      n *= d;
    }
    if (n != data.size()) {
      throw std::runtime_error("quantity " + name + ": shape/data size mismatch");
    }
    return add<T>(name, shape, axes, layout, data.data());
  }

  /// Boolean array stored as |b1.
  Quantity add_bool(const std::string & name,
                    const std::vector<std::size_t> & shape,
                    const std::vector<std::string> & axes,
                    const std::string & layout, const std::vector<bool> & v) {
    std::vector<std::uint8_t> bytes(v.begin(), v.end());
    return add_raw(name, shape, axes, layout, "|b1", "bool", bytes.data(), 1,
                   bytes.size());
  }

  /// Write manifest.json. Returns the case directory.
  std::filesystem::path finish() {
    if (not inputs_.is_null()) {
      m_["inputs"] = inputs_;
    }
    if (not parameters_.is_null()) {
      m_["parameters"] = parameters_;
    }
    if (not parallel_.is_null()) {
      m_["parallel"] = parallel_;
    }
    Json storage = Json::object();
    storage["mode"] = "committed";
    storage["bytes"] = static_cast<long long>(bytes_);
    m_["storage"] = storage;
    if (not steps_.is_null()) {
      m_["steps"] = steps_;
    }
    m_["quantities"] = quantities_;
    if (not expect_.is_null()) {
      m_["expect"] = expect_;
    }
    if (not tags_.empty()) {
      m_["tags"] = Json::array_of(tags_);
    }
    if (not notes_.empty()) {
      m_["notes"] = notes_;
    }
    write_file(dir_ / "manifest.json", m_.dump(2) + "\n");
    return dir_;
  }

private:
  static std::string trim(std::string s) {
    while (not s.empty() and std::isspace(static_cast<unsigned char>(s.back()))) {
      s.pop_back();
    }
    std::size_t b = 0;
    while (b < s.size() and std::isspace(static_cast<unsigned char>(s[b]))) {
      ++b;
    }
    return s.substr(b);
  }

  Quantity add_raw(const std::string & name,
                   const std::vector<std::size_t> & shape,
                   const std::vector<std::string> & axes,
                   const std::string & layout, const char * descr,
                   const char * dtype_name, const void * data,
                   std::size_t elem_size, std::size_t count) {
    if (axes.size() != shape.size()) {
      throw std::runtime_error("quantity " + name + ": axes/shape rank mismatch");
    }
    for (const auto & existing : names_) {
      if (existing == name) {
        throw std::runtime_error("duplicate quantity " + name);
      }
    }
    names_.push_back(name);
    auto bytes = encode_npy(descr, shape, data, elem_size, count);
    auto rel = std::string("data/") + sanitize(name) + ".npy";
    write_file(dir_ / rel, bytes);
    bytes_ += bytes.size();
    Json q = Json::object();
    q["name"] = name;
    q["file"] = rel;
    q["dtype"] = dtype_name;
    q["shape"] = Json::array_of(shape);
    q["axes"] = Json::array_of(axes);
    q["layout"] = layout;
    q["sha256"] = Sha256::of(bytes);
    return Quantity(quantities_.push(q));
  }

  std::string case_id_;
  std::filesystem::path dir_;
  Json m_ = Json::object();
  Json parameters_;
  Json quantities_;
  Json inputs_;
  Json parallel_;
  Json steps_;
  Json expect_;
  std::vector<std::string> tags_;
  std::vector<std::string> names_;
  std::string notes_;
  std::size_t bytes_{0};
};

} // namespace akantu_probe

#endif // AKANTU_PROBE_FIXTURE_WRITER_HH_
