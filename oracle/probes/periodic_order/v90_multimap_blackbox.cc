// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs (UD-654): black-box test of the
// iteration order of equivalent keys in std::unordered_multimap<Idx, Idx>
// (the type of Akantu's Mesh::periodic_master_slave, src/mesh/mesh.hh:674),
// compiled with the oracle image's compiler. Not ported from Akantu; the
// `updating_master` pattern mimics src/mesh/mesh_periodic.cc:315-338.
//
// D-030: written from the C++ standard and black-box observation only; no
// standard-library source was read. The standard leaves the relative order
// of an inserted element among existing equivalent elements unspecified
// ([unord.req]); it guarantees that equivalent elements are adjacent in
// iteration order and that rehashing preserves their relative order.
//
// Hypothesis under test (rule R of the port): for every key, equal_range()
// iterates the values in REVERSE insertion order, whatever the other keys,
// bucket collisions, rehash/reserve history, erase of other keys or of the
// same key, copies and moves.
//
// Output: one line per pattern "PASS <name> <checks>" or
// "FAIL <name> key=<k> expected=[...] got=[...]", then "SUMMARY".
#include <aka_common.hh>

#include <algorithm>
#include <cstdint>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <unordered_map>
#include <vector>

using akantu::Idx;
using MM = std::unordered_multimap<Idx, Idx>;

namespace {

int g_fail = 0;
int g_pass = 0;

// model: key -> values in insertion order
using Model = std::map<Idx, std::vector<Idx>>;

std::string str(const std::vector<Idx> & v) {
  std::ostringstream o;
  o << '[';
  for (std::size_t i = 0; i < v.size(); ++i) {
    o << (i ? "," : "") << v[i];
  }
  o << ']';
  return o.str();
}

// compares every key's equal_range order with reversed insertion order, and
// checks adjacency (the standard's guarantee) on the full iteration
bool check(const std::string & name, const MM & mm, const Model & model,
           long long & checks) {
  for (auto && [k, ins] : model) {
    std::vector<Idx> got;
    auto r = mm.equal_range(k);
    for (auto it = r.first; it != r.second; ++it) {
      got.push_back(it->second);
    }
    std::vector<Idx> expected(ins.rbegin(), ins.rend());
    ++checks;
    if (got != expected) {
      std::cout << "FAIL " << name << " key=" << k << " expected=" << str(expected)
                << " got=" << str(got) << " bucket_count=" << mm.bucket_count()
                << " size=" << mm.size() << std::endl;
      return false;
    }
  }
  // full iteration: same per-key order, keys contiguous
  std::map<Idx, std::vector<Idx>> seen;
  Idx prev = -1;
  bool first = true;
  std::map<Idx, bool> closed;
  for (auto && [k, v] : mm) {
    if (not first and k != prev) {
      closed[prev] = true;
    }
    if (closed.count(k)) {
      std::cout << "FAIL " << name << " key " << k << " not contiguous" << std::endl;
      return false;
    }
    seen[k].push_back(v);
    prev = k;
    first = false;
  }
  for (auto && [k, ins] : model) {
    std::vector<Idx> expected(ins.rbegin(), ins.rend());
    ++checks;
    if (seen[k] != expected) {
      std::cout << "FAIL " << name << " (full iteration) key=" << k
                << " expected=" << str(expected) << " got=" << str(seen[k]) << std::endl;
      return false;
    }
  }
  return true;
}

void report(const std::string & name, bool ok, long long checks) {
  if (ok) {
    ++g_pass;
    std::cout << "PASS " << name << " " << checks << std::endl;
  } else {
    ++g_fail;
  }
}

std::uint64_t splitmix(std::uint64_t & s) {
  std::uint64_t z = (s += 0x9e3779b97f4a7c15ULL);
  z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
  z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
  return z ^ (z >> 31);
}

// P1: one key, n inserts
void p1() {
  for (int n = 1; n <= 80; ++n) {
    MM mm;
    Model model;
    long long checks = 0;
    bool ok = true;
    for (int i = 0; i < n; ++i) {
      mm.insert(std::make_pair(Idx(7), Idx(100 + i)));
      model[7].push_back(100 + i);
      ok = ok and check("p1_single_key_n" + std::to_string(n), mm, model, checks);
    }
    report("p1_single_key_n" + std::to_string(n), ok, checks);
  }
}

// P2: K keys, round robin or blocks, growing (many rehashes)
void p2() {
  for (int K : {2, 3, 5, 13, 50, 211, 1000}) {
    for (int per : {2, 3, 4, 8}) {
      for (int mode = 0; mode < 3; ++mode) {
        MM mm;
        Model model;
        long long checks = 0;
        std::uint64_t seed = 1234 + K * 31 + per * 7 + mode;
        std::vector<std::pair<Idx, Idx>> seq;
        for (int r = 0; r < per; ++r) {
          for (int k = 0; k < K; ++k) {
            seq.emplace_back(Idx(mode == 2 ? (k * 7919) % 100003 : k), Idx(r * 100000 + k));
          }
        }
        if (mode == 1) { // random interleaving
          for (std::size_t i = seq.size(); i > 1; --i) {
            std::swap(seq[i - 1], seq[splitmix(seed) % i]);
          }
        }
        for (auto && [k, v] : seq) {
          mm.insert(std::make_pair(k, v));
          model[k].push_back(v);
        }
        bool ok = check("p2", mm, model, checks);
        report("p2_K" + std::to_string(K) + "_per" + std::to_string(per) + "_mode" +
                   std::to_string(mode),
               ok, checks);
      }
    }
  }
}

// P3: keys colliding in one bucket (found black-box with bucket())
void p3() {
  for (std::size_t reserve_n : {0UL, 7UL, 64UL, 1000UL}) {
    MM mm;
    if (reserve_n) {
      mm.reserve(reserve_n);
    }
    Model model;
    long long checks = 0;
    // seed with one element to have a bucket count
    mm.insert(std::make_pair(Idx(0), Idx(-1)));
    model[0].push_back(-1);
    auto b0 = mm.bucket(Idx(0));
    std::vector<Idx> colliding;
    for (Idx k = 1; k < 200000 and colliding.size() < 5; ++k) {
      if (mm.bucket(k) == b0) {
        colliding.push_back(k);
      }
    }
    colliding.push_back(0);
    bool ok = true;
    std::uint64_t seed = 99 + reserve_n;
    for (int i = 0; i < 40; ++i) {
      Idx k = colliding[splitmix(seed) % colliding.size()];
      Idx v = 1000 + i;
      mm.insert(std::make_pair(k, v));
      model[k].push_back(v);
      ok = ok and check("p3", mm, model, checks);
    }
    report("p3_bucket_collisions_reserve" + std::to_string(reserve_n) + "_ncoll" +
               std::to_string(colliding.size()),
           ok, checks);
  }
}

// P4: erase whole keys (other and same) then reinsert; clear; erase one
// element of a group
void p4() {
  MM mm;
  Model model;
  long long checks = 0;
  bool ok = true;
  std::uint64_t seed = 4242;
  for (int round = 0; round < 400; ++round) {
    auto op = splitmix(seed) % 10;
    Idx k = Idx(splitmix(seed) % 37);
    if (op < 6) {
      Idx v = round;
      mm.insert(std::make_pair(k, v));
      model[k].push_back(v);
    } else if (op < 8) {
      mm.erase(k);
      model.erase(k);
    } else if (op < 9) {
      // erase the middle element of the group (by value search)
      auto r = mm.equal_range(k);
      std::vector<Idx> vals;
      for (auto it = r.first; it != r.second; ++it) {
        vals.push_back(it->second);
      }
      if (vals.size() >= 3) {
        Idx victim = vals[vals.size() / 2];
        for (auto it = r.first; it != r.second; ++it) {
          if (it->second == victim) {
            mm.erase(it);
            break;
          }
        }
        auto & ins = model[k];
        ins.erase(std::find(ins.begin(), ins.end(), victim));
      }
    } else {
      mm.rehash(splitmix(seed) % 300);
    }
    ok = ok and check("p4", mm, model, checks);
  }
  report("p4_erase_rehash_reinsert", ok, checks);
  MM c(mm);
  ok = check("p4_copy", c, model, checks);
  report("p4_copy_constructed", ok, checks);
  MM a;
  a = mm;
  ok = check("p4_assign", a, model, checks);
  report("p4_copy_assigned", ok, checks);
  MM m2(std::move(c));
  ok = check("p4_move", m2, model, checks);
  report("p4_move_constructed", ok, checks);
  mm.clear();
  model.clear();
  for (int i = 0; i < 20; ++i) {
    mm.insert(std::make_pair(Idx(i % 3), Idx(i)));
    model[i % 3].push_back(i);
  }
  ok = check("p4_clear", mm, model, checks);
  report("p4_after_clear", ok, checks);
}

// P5: max_load_factor changes and explicit rehash between inserts
void p5() {
  for (float mlf : {0.1F, 0.5F, 1.0F, 4.0F}) {
    MM mm;
    mm.max_load_factor(mlf);
    Model model;
    long long checks = 0;
    bool ok = true;
    std::uint64_t seed = 77 + Idx(mlf * 10);
    for (int i = 0; i < 300; ++i) {
      Idx k = Idx(splitmix(seed) % 23);
      mm.insert(std::make_pair(k, Idx(i)));
      model[k].push_back(i);
      if (i % 50 == 49) {
        mm.rehash(mm.bucket_count() * 3);
      }
      ok = ok and check("p5", mm, model, checks);
    }
    report("p5_max_load_factor_" + std::to_string(mlf), ok, checks);
  }
}

// P6: the updating_master pattern of mesh_periodic.cc (copy of a group
// through a temporary multimap, erase, re-add) on a background of other keys
void p6() {
  MM mm;
  Model model;
  long long checks = 0;
  bool ok = true;
  std::uint64_t seed = 6;
  auto add = [&](Idx slave, Idx master) { // addPeriodicSlave without flags
    auto r = mm.equal_range(master);
    auto it = std::find_if(r.first, r.second, [&](auto & p) { return p.second == slave; });
    if (it == r.second) {
      mm.insert(std::make_pair(master, slave));
      model[master].push_back(slave);
    }
  };
  for (int round = 0; round < 300; ++round) {
    Idx master = Idx(splitmix(seed) % 50);
    Idx s = Idx(1000 + round);
    add(s, master);
    if (round % 7 == 6) {
      Idx old_master = Idx(splitmix(seed) % 50);
      Idx new_master = Idx(splitmix(seed) % 50);
      if (old_master != new_master and mm.count(old_master) > 0) {
        auto slaves = mm.equal_range(old_master);
        MM tmp;
        for (auto it = slaves.first; it != slaves.second; ++it) {
          tmp.insert(std::make_pair(new_master, it->second));
        }
        // the port's model: the old master's slaves in insertion order
        std::vector<Idx> moved = model[old_master];
        mm.erase(old_master);
        model.erase(old_master);
        add(old_master + 5000, new_master); // stands for addPeriodicSlave(old_master, new_master)
        std::vector<Idx> tmp_order;
        for (auto && data : tmp) {
          tmp_order.push_back(data.second);
          add(data.second, data.first);
        }
        ++checks;
        if (tmp_order != moved) { // tmp iterates the old group in insertion order
          std::cout << "FAIL p6 tmp order expected=" << str(moved) << " got=" << str(tmp_order)
                    << std::endl;
          ok = false;
        }
      }
    }
    ok = ok and check("p6", mm, model, checks);
  }
  report("p6_updating_master_pattern", ok, checks);
}

} // namespace

int main() {
  std::cout << "sizeof(Idx)=" << sizeof(Idx) << " compiler=" << __VERSION__ << std::endl;
  p1();
  p2();
  p3();
  p4();
  p5();
  p6();
  std::cout << "SUMMARY pass=" << g_pass << " fail=" << g_fail << std::endl;
  return g_fail == 0 ? 0 : 1;
}
