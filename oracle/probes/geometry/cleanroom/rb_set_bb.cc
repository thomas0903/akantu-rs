// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room black-box experiment B2 (D-030): observable behaviour
// of std::set<K, Comp>::insert (return flag, iteration order afterwards) for
// a strict weak ordering and for comparators that are not one, in particular
// the symmetric "differ" comparator of upstream's segmentPairsLess
// (src/geometry/geom_helper_functions.hh:66-73). Only the public <set> API is
// used.
//
// Output: {"cases":[{comp,keys,inserted,order},...]}: keys[i] is inserted
// with tag i; inserted[i] the bool returned; order = tags in iteration order.
#include "bb_common.hh"

#include <set>
#include <string>

namespace {
bool first_case = true;

struct E {
  int key;
  int tag;
};

template <class Cmp> void run(const char * name, const std::vector<int> & keys, Cmp cmp) {
  std::set<E, Cmp> s(cmp);
  std::vector<int> inserted;
  for (std::size_t i = 0; i < keys.size(); ++i)
    inserted.push_back(s.insert(E{keys[i], static_cast<int>(i)}).second ? 1 : 0);
  std::vector<int> order;
  for (const auto & e : s)
    order.push_back(e.tag);
  std::printf(first_case ? "\n" : ",\n");
  first_case = false;
  std::printf("{\"comp\":\"%s\",", name);
  bb::print_ints("keys", keys);
  std::printf(",");
  bb::print_ints("inserted", inserted);
  std::printf(",");
  bb::print_ints("order", order);
  std::printf("}");
}

struct Lt {
  bool operator()(const E & a, const E & b) const { return a.key < b.key; }
};
struct Ne { // segmentPairsLess shape: true whenever the keys differ
  bool operator()(const E & a, const E & b) const { return a.key != b.key; }
};
struct Le { // another non-strict comparator
  bool operator()(const E & a, const E & b) const { return a.key <= b.key; }
};
} // namespace

int main() {
  bb::Rng rng(0xD1B54A32D192ED03ULL);
  std::printf("{\"cases\":[");
  for (int len = 1; len <= 48; ++len) {
    for (int m : {2, 3, 5, 12, 1000000}) {
      for (int rep = 0; rep < 2; ++rep) {
        std::vector<int> keys(len);
        for (auto & k : keys)
          k = rng.below(m);
        run("lt", keys, Lt{});
        run("ne", keys, Ne{});
        run("le", keys, Le{});
      }
    }
  }
  for (int len : {100, 300}) {
    for (int m : {4, 40, 1000000}) {
      std::vector<int> keys(len);
      for (auto & k : keys)
        k = rng.below(m);
      run("lt", keys, Lt{});
      run("ne", keys, Ne{});
      run("le", keys, Le{});
    }
  }
  std::printf("\n]}\n");
  return 0;
}
