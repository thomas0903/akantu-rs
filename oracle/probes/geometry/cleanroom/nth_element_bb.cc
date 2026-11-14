// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room black-box experiment B1 (D-030): observable behaviour
// of std::nth_element (the permutation it leaves and its comparison count) on
// generated inputs with many ties, sorted/patterned inputs, and inputs built
// by McIlroy's adversary ("A killer adversary for quicksort", Software:
// Practice and Experience 29(4), 1999), which drive quickselect-type
// algorithms to their depth limit. Only the public <algorithm> API is used.
//
// Output: one JSON document {"cases":[{kind,nth,keys,perm,ncomp},...]} on
// stdout; perm[i] = original index of the element left at position i.
#include "bb_common.hh"

#include <algorithm>
#include <string>

namespace {
long long ncomp = 0;
bool first_case = true;

struct E {
  int key;
  int tag;
};

void emit(const std::string & kind, const std::vector<int> & keys, std::size_t nth) {
  std::vector<E> v(keys.size());
  for (std::size_t i = 0; i < keys.size(); ++i)
    v[i] = E{keys[i], static_cast<int>(i)};
  ncomp = 0;
  std::nth_element(v.begin(), v.begin() + static_cast<long>(nth), v.end(),
                   [](const E & a, const E & b) {
                     ++ncomp;
                     return a.key < b.key;
                   });
  std::vector<int> perm(v.size());
  for (std::size_t i = 0; i < v.size(); ++i)
    perm[i] = v[i].tag;
  std::printf(first_case ? "\n" : ",\n");
  first_case = false;
  std::printf("{\"kind\":\"%s\",\"nth\":%zu,", kind.c_str(), nth);
  bb::print_ints("keys", keys);
  std::printf(",");
  bb::print_ints("perm", perm);
  std::printf(",\"ncomp\":%lld}", ncomp);
}

// McIlroy's adversary: values are decided lazily ("gas" until frozen) so
// that the algorithm's pivots are as bad as possible; the frozen values then
// form a concrete input that reproduces the same comparisons.
std::vector<int> adversary(std::size_t n, std::size_t nth) {
  std::vector<int> val(n, static_cast<int>(n)); // gas = n
  const int gas = static_cast<int>(n);
  int nsolid = 0;
  long candidate = -1;
  std::vector<int> idx(n);
  for (std::size_t i = 0; i < n; ++i)
    idx[i] = static_cast<int>(i);
  auto cmp = [&](int x, int y) {
    if (val[x] == gas && val[y] == gas) {
      if (x == candidate)
        val[x] = nsolid++;
      else
        val[y] = nsolid++;
    }
    if (val[x] == gas)
      candidate = x;
    else if (val[y] == gas)
      candidate = y;
    return val[x] < val[y];
  };
  std::nth_element(idx.begin(), idx.begin() + static_cast<long>(nth), idx.end(), cmp);
  return val; // remaining gas elements keep the (largest) value n
}
} // namespace

int main() {
  bb::Rng rng(0x9E3779B97F4A7C15ULL);
  std::printf("{\"cases\":[");
  // 1. random inputs with ties, every length 0..64
  const int moduli[] = {1, 2, 3, 8, 1000000};
  for (int len = 0; len <= 64; ++len) {
    for (int m : moduli) {
      for (int rep = 0; rep < 3; ++rep) {
        std::vector<int> keys(len);
        for (auto & k : keys)
          k = rng.below(m);
        std::size_t nth = len == 0 ? 0 : static_cast<std::size_t>(rng.below(len + 1));
        emit("random", keys, nth);
      }
    }
  }
  // 2. larger random inputs (CGAL splits at the middle)
  const int lens[] = {100, 255, 256, 257, 1000, 3001};
  for (int len : lens) {
    for (int m : {2, 16, 1000000}) {
      std::vector<int> keys(len);
      for (auto & k : keys)
        k = rng.below(m);
      emit("random_large", keys, static_cast<std::size_t>(len / 2));
      emit("random_large", keys, static_cast<std::size_t>(rng.below(len)));
    }
  }
  // 3. patterned inputs
  for (int len : {4, 5, 7, 16, 31, 64, 100, 300}) {
    std::vector<int> asc(len), desc(len), equal(len, 7), organ(len), saw(len);
    for (int i = 0; i < len; ++i) {
      asc[i] = i;
      desc[i] = len - i;
      organ[i] = std::min(i, len - 1 - i);
      saw[i] = i % 5;
    }
    for (auto * keys : {&asc, &desc, &equal, &organ, &saw}) {
      emit("pattern", *keys, static_cast<std::size_t>(len / 2));
      emit("pattern", *keys, static_cast<std::size_t>(len - 1));
      emit("pattern", *keys, 0);
    }
  }
  // 4. adversarial inputs (depth limit / heap selection)
  for (int len : {8, 16, 33, 64, 100, 128, 257, 500, 1000}) {
    for (std::size_t nth : {static_cast<std::size_t>(len / 2), static_cast<std::size_t>(len / 3),
                            static_cast<std::size_t>(len - 2)}) {
      emit("adversary", adversary(static_cast<std::size_t>(len), nth), nth);
    }
  }
  std::printf("\n]}\n");
  return 0;
}
