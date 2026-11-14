// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs cohesive scalability harness (benchmarks/cohesive-perf). Not a
// port of Akantu code: it interposes Akantu's exported
// akantu::Mesh::fillNodesToElements(Int) from the benchmark executable to
// emulate the JOSS patch (performance-testing-cohesive, akantu-v5.0.4.patch)
// without rebuilding or modifying the oracle library.
//
// The executable exports a function with the mangled name of the member
// function, so libakantu's PLT calls resolve to it first (ELF symbol
// interposition; the library is built with -fPIC and default semantic
// interposition, every call goes through the PLT: see fnte_sites.py). The
// real function is found with dlsym(RTLD_NEXT). The return address tells the
// call site: sites listed in the generated fnte_sites.h are the instances of
// Mesh::sendEvent<NewElementsEvent|RemovedElementsEvent>, whose
// fillNodesToElements() call the JOSS patch comments out.
//
// AKANTU_RS_FILL_NODES_TO_ELEMENTS=upstream (default): every call runs
//   (the oracle behaviour); calls from event sites are counted and timed.
// AKANTU_RS_FILL_NODES_TO_ELEMENTS=joss: calls from event sites return
//   immediately (the JOSS behaviour); other calls run.
//
// Binding: fnte_sites.h records the GNU build-id of the
// library it was generated from; at start-up (and before the first call)
// the interposer reads the build-id of the libakantu that actually provides
// the real function and aborts if it differs or is missing, so the site
// table can never be applied to another library.
//
// Itanium C++ ABI on aarch64/x86_64: a non-virtual member function takes
// `this` as its first argument, so `void(void *, int)` is call-compatible
// with akantu::Mesh::fillNodesToElements(Int) (Int = int, AKANTU_TYPE_INTEGER
// "int (32bit)" of the oracle builds).

#include "fnte_sites.h"

#include <dlfcn.h>
#include <elf.h>
#include <link.h>

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <iterator>
#include <string>

namespace {
using fill_fn = void (*)(void *, int);
fill_fn real_fill = nullptr;
int fnte_mode = 0; // 0 = upstream, 1 = joss
unsigned long nb_event_calls = 0;
unsigned long nb_event_skipped = 0;
unsigned long nb_other_calls = 0;
double time_event_calls = 0.;
double time_other_calls = 0.;

struct BuildIdSearch {
  const char * object_name;
  bool object_found;
  std::string build_id; // lowercase hex, empty if the object has none
};

// dl_iterate_phdr callback: GNU build-id note of the object named
// search->object_name (the name the dynamic linker loaded it under).
int find_build_id(dl_phdr_info * info, std::size_t /*size*/, void * data) {
  auto * search = static_cast<BuildIdSearch *>(data);
  if (info->dlpi_name == nullptr or
      std::strcmp(info->dlpi_name, search->object_name) != 0) {
    return 0;
  }
  search->object_found = true;
  for (int i = 0; i < info->dlpi_phnum; ++i) {
    const auto & ph = info->dlpi_phdr[i];
    if (ph.p_type != PT_NOTE) {
      continue;
    }
    const std::size_t align = ph.p_align >= 8 ? 8 : 4;
    auto pad = [align](std::size_t n) { return (n + align - 1) & ~(align - 1); };
    const auto * p =
        reinterpret_cast<const unsigned char *>(info->dlpi_addr + ph.p_vaddr);
    const auto * end = p + ph.p_memsz;
    while (p + sizeof(ElfW(Nhdr)) <= end) {
      const auto * nh = reinterpret_cast<const ElfW(Nhdr) *>(p);
      const auto * name = p + sizeof(ElfW(Nhdr));
      const auto * desc = name + pad(nh->n_namesz);
      const auto * next = desc + pad(nh->n_descsz);
      if (next > end) {
        break;
      }
      if (nh->n_type == NT_GNU_BUILD_ID and nh->n_namesz == 4 and
          std::memcmp(name, "GNU", 4) == 0) {
        static const char digits[] = "0123456789abcdef";
        for (std::size_t j = 0; j < nh->n_descsz; ++j) {
          search->build_id += digits[desc[j] >> 4];
          search->build_id += digits[desc[j] & 15];
        }
        return 1;
      }
      p = next;
    }
  }
  return 1;
}

void check_library_binding() {
  Dl_info info;
  if (dladdr(reinterpret_cast<void *>(real_fill), &info) == 0 or
      info.dli_fname == nullptr) {
    std::fprintf(stderr, "akantu-rs interposer: dladdr failed\n");
    std::abort();
  }
  BuildIdSearch search{info.dli_fname, false, {}};
  dl_iterate_phdr(find_build_id, &search);
  if (not search.object_found or
      search.build_id != akantu_rs_fnte_library_build_id) {
    std::fprintf(stderr,
                 "akantu-rs interposer: %s has build-id '%s' but the call-site "
                 "table (fnte_sites.h) was generated for build-id '%s' "
                 "(sha256 %s): rebuild the driver (scripts/build.sh)\n",
                 info.dli_fname, search.build_id.c_str(),
                 akantu_rs_fnte_library_build_id,
                 akantu_rs_fnte_library_sha256);
    std::abort();
  }
}

void init_once() {
  if (real_fill != nullptr) {
    return;
  }
  real_fill = reinterpret_cast<fill_fn>(
      dlsym(RTLD_NEXT, "_ZN6akantu4Mesh19fillNodesToElementsEi"));
  if (real_fill == nullptr) {
    std::fprintf(stderr, "akantu-rs interposer: dlsym failed: %s\n", dlerror());
    std::abort();
  }
  check_library_binding();
  const char * mode = std::getenv("AKANTU_RS_FILL_NODES_TO_ELEMENTS");
  if (mode != nullptr and std::strcmp(mode, "joss") == 0) {
    fnte_mode = 1;
  } else if (mode != nullptr and std::strcmp(mode, "upstream") != 0 and
             mode[0] != '\0') {
    std::fprintf(stderr, "akantu-rs interposer: unknown mode '%s'\n", mode);
    std::abort();
  }
}

// Resolve and check at start-up, before main() (libakantu is loaded by
// then), so that a mismatched library stops the run before any work.
[[gnu::constructor]] void init_at_startup() { init_once(); }

bool is_event_site(void * return_address) {
  Dl_info info;
  if (dladdr(return_address, &info) == 0 or info.dli_fname == nullptr or
      std::strstr(info.dli_fname, "libakantu.so") == nullptr) {
    return false;
  }
  auto offset = reinterpret_cast<std::uintptr_t>(return_address) -
                reinterpret_cast<std::uintptr_t>(info.dli_fbase);
  return std::binary_search(std::begin(akantu_rs_fnte_event_sites),
                            std::end(akantu_rs_fnte_event_sites), offset);
}
} // namespace

extern "C" {
// NOLINTNEXTLINE: the mangled name of akantu::Mesh::fillNodesToElements(int)
void _ZN6akantu4Mesh19fillNodesToElementsEi(void * mesh, int dimension) {
  void * return_address = __builtin_return_address(0);
  init_once();
  bool event = is_event_site(return_address);
  if (event and fnte_mode == 1) {
    ++nb_event_skipped;
    return;
  }
  auto start = std::chrono::steady_clock::now();
  real_fill(mesh, dimension);
  std::chrono::duration<double> elapsed =
      std::chrono::steady_clock::now() - start;
  if (event) {
    ++nb_event_calls;
    time_event_calls += elapsed.count();
  } else {
    ++nb_other_calls;
    time_other_calls += elapsed.count();
  }
}

// Statistics for the driver's summary (mode: 0 upstream, 1 joss).
void akantu_rs_fnte_stats(int * mode, unsigned long * event_calls,
                          unsigned long * event_skipped,
                          unsigned long * other_calls, double * event_time,
                          double * other_time) {
  init_once();
  *mode = fnte_mode;
  *event_calls = nb_event_calls;
  *event_skipped = nb_event_skipped;
  *other_calls = nb_other_calls;
  *event_time = time_event_calls;
  *other_time = time_other_calls;
}
}
