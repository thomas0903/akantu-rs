// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle tooling (not ported from Akantu): SHA-256
// written from the specification (FIPS 180-4, §4.1.2, §4.2.2, §5.1.1, §5.3.3,
// §6.2), for per-step digests of the state of the oracle example variants.
// self_test() checks the FIPS 180-2 example digests; the probes abort if it
// fails.
#pragma once
#include <algorithm>
#include <array>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <string>

namespace akantu_rs {

class Sha256 {
public:
  Sha256() = default;

  void update(const void * data, std::size_t size) {
    const auto * p = static_cast<const unsigned char *>(data);
    length += static_cast<std::uint64_t>(size);
    while (size > 0) {
      const std::size_t take = std::min<std::size_t>(64 - fill, size);
      for (std::size_t i = 0; i < take; ++i) {
        buffer[fill + i] = p[i];
      }
      fill += take;
      p += take;
      size -= take;
      if (fill == 64) {
        block(buffer.data());
        fill = 0;
      }
    }
  }

  /// Lowercase hexadecimal digest; the object must not be reused after.
  std::string hex() {
    const std::uint64_t bits = length * 8U;
    const unsigned char one = 0x80;
    update(&one, 1);
    const unsigned char zero = 0;
    while (fill != 56) {
      update(&zero, 1);
    }
    unsigned char len_be[8];
    for (int i = 0; i < 8; ++i) {
      len_be[i] = static_cast<unsigned char>(bits >> (56 - 8 * i));
    }
    update(len_be, 8);
    std::string out;
    char b[9];
    for (auto v : h) {
      std::snprintf(b, sizeof(b), "%08x", static_cast<unsigned>(v));
      out += b;
    }
    return out;
  }

  static bool self_test() {
    Sha256 a;
    a.update("abc", 3);
    Sha256 b;
    const char * m = "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    b.update(m, 56);
    Sha256 c; // 1,000,000 × 'a', fed in odd-sized chunks
    std::string chunk(997, 'a');
    std::size_t left = 1000000;
    while (left > 0) {
      const std::size_t n = std::min<std::size_t>(left, chunk.size());
      c.update(chunk.data(), n);
      left -= n;
    }
    Sha256 e;
    return a.hex() == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" &&
           b.hex() == "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1" &&
           c.hex() == "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0" &&
           e.hex() == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
  }

private:
  static std::uint32_t rotr(std::uint32_t x, int n) {
    return (x >> n) | (x << (32 - n));
  }

  void block(const unsigned char * p) {
    static constexpr std::array<std::uint32_t, 64> k = {
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
        0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
        0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2};
    std::array<std::uint32_t, 64> w{};
    for (int t = 0; t < 16; ++t) {
      w[t] = (static_cast<std::uint32_t>(p[4 * t]) << 24) |
             (static_cast<std::uint32_t>(p[4 * t + 1]) << 16) |
             (static_cast<std::uint32_t>(p[4 * t + 2]) << 8) |
             static_cast<std::uint32_t>(p[4 * t + 3]);
    }
    for (int t = 16; t < 64; ++t) {
      const std::uint32_t s0 =
          rotr(w[t - 15], 7) ^ rotr(w[t - 15], 18) ^ (w[t - 15] >> 3);
      const std::uint32_t s1 =
          rotr(w[t - 2], 17) ^ rotr(w[t - 2], 19) ^ (w[t - 2] >> 10);
      w[t] = w[t - 16] + s0 + w[t - 7] + s1;
    }
    std::uint32_t a = h[0], b = h[1], c = h[2], d = h[3], e = h[4], f = h[5],
                  g = h[6], hh = h[7];
    for (int t = 0; t < 64; ++t) {
      const std::uint32_t S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      const std::uint32_t ch = (e & f) ^ (~e & g);
      const std::uint32_t t1 = hh + S1 + ch + k[t] + w[t];
      const std::uint32_t S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      const std::uint32_t maj = (a & b) ^ (a & c) ^ (b & c);
      const std::uint32_t t2 = S0 + maj;
      hh = g;
      g = f;
      f = e;
      e = d + t1;
      d = c;
      c = b;
      b = a;
      a = t1 + t2;
    }
    h[0] += a;
    h[1] += b;
    h[2] += c;
    h[3] += d;
    h[4] += e;
    h[5] += f;
    h[6] += g;
    h[7] += hh;
  }

  std::array<std::uint32_t, 8> h = {0x6a09e667, 0xbb67ae85, 0x3c6ef372,
                                    0xa54ff53a, 0x510e527f, 0x9b05688c,
                                    0x1f83d9ab, 0x5be0cd19};
  std::array<unsigned char, 64> buffer{};
  std::size_t fill = 0;
  std::uint64_t length = 0;
};

} // namespace akantu_rs
