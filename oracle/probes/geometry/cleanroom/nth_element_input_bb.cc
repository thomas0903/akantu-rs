// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room black-box experiment B3 (D-030):
// observable behaviour of std::nth_element (permutation and
// comparison count) on given inputs: adversarial inputs of B1 with a few keys
// duplicated, chosen so that the heap-selection fallback runs with tied keys
// (the selection was made with the clean-room model; b3_inputs.txt). Only the
// public <algorithm> API is used; no library header or source is read.
//
// Input (stdin): one case per line, "nth k0 k1 ...".
// Output: {"cases":[{kind:"b3",nth,keys,perm,ncomp},...]} on stdout, the
// format of B1 (nth_element_bb.cc).
#include "bb_common.hh"

#include <algorithm>
#include <iostream>
#include <sstream>
#include <string>

namespace {
long long ncomp = 0;
struct E {
  int key;
  int tag;
};
} // namespace

int main() {
  std::string line;
  bool first = true;
  std::printf("{\"cases\":[");
  while (std::getline(std::cin, line)) {
    std::istringstream is(line);
    std::size_t nth = 0;
    if (!(is >> nth))
      continue;
    std::vector<int> keys;
    for (int k; is >> k;)
      keys.push_back(k);
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
    std::printf(first ? "\n" : ",\n");
    first = false;
    std::printf("{\"kind\":\"b3\",\"nth\":%zu,", nth);
    bb::print_ints("keys", keys);
    std::printf(",");
    bb::print_ints("perm", perm);
    std::printf(",\"ncomp\":%lld}", ncomp);
  }
  std::printf("\n]}\n");
  return 0;
}
