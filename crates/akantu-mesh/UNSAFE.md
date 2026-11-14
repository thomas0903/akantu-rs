# `unsafe` sites of `akantu-mesh`

Every `unsafe` site of this crate is listed below, one row per site
(`scripts/unsafe_inventory.py --check`, run by `cargo test -p
akantu-validation --test unsafe_inventory`, fails when a site is missing,
moved or gone; `--update-lines` rewrites the Line column after an edit
that only moves code).
Each row is also pinned to a fingerprint of its site's code (comments and
whitespace ignored) in `UNSAFE.fingerprints`: a site
whose code changed fails the check, and `--update-lines` does not move its
row, until the row is reviewed and re-pinned with `--refresh <Id>`.
A fingerprint covers the Rust site's own tokens only: not the code around
it that sets up its invariants, and not C sources (here `src/scotch_shim.c`), which the guard
does not read; a change there shows only in the commit's diff and its
review.

**Where `unsafe` is allowed.** `src/lib.rs`:
`#![cfg_attr(not(feature = "scotch"), forbid(unsafe_code))]`,
`#![cfg_attr(feature = "scotch", deny(unsafe_code))]`. Without the `scotch`
feature (the default build) the library has no `unsafe`. With it, the
modules `src/scotch.rs` and `src/scotch_reorder.rs` opt out
(`#![allow(unsafe_code)] // the Scotch FFI edge`). The C side is
`src/scotch_shim.c` (compiled by `build.rs`; links `libscotcherr`, which
prints and returns, never exits). `tests/review_v103_allocations.rs` has its
own test-only global allocator.

**Counts** (library / tests): block 4 / 3, `unsafe fn` 0 / 3, `unsafe impl`
0 / 1, `extern` block 2 / 0 — **6 library sites, 7 test sites**.

## Invariant classes

* **M1 — Scotch call with private buffers.** Every pointer comes from a
  `Vec` built by the calling function from the mesh (private copies, unique
  borrows: a write by the library could not alias a shared borrow),
  alive for the call; empty tables get a one-entry dummy; path
  pointers are null or `CString`s alive for the call; `SCOTCH_Num` is
  32-bit (checked at run time with `scotch_num_bits()` before any call that
  passes arrays: the Rust declarations use `i32`); counts, part count and
  seed are converted with checked `try_from`; the process-global Scotch
  context (seed, reset, partitioning) is serialised by `SCOTCH_CONTEXT`.
  The array **sizes** that `SCOTCH_graphBuild` /
  `SCOTCH_meshBuild` and the debug `SCOTCH_graphSave` / `SCOTCH_meshSave`
  read **before** `SCOTCH_graphCheck` / `SCOTCH_meshCheck` run are
  established **by construction** (`partition::build_dual_graph`,
  `scotch_reorder::scotch_mesh`, safe code of this crate) and **checked
  once per call before the FFI call**:
  `partition_with_loads` checks `xadj.len() = n + 1`, `xadj[0] = 0`,
  `xadj[n] = edgenbr`, non-decreasing starts, `n` vertex loads and
  `edgenbr` edge loads (an `Error::Mesh` otherwise); `reorder` checks
  `vnodnbr + velmnbr + 1` start indices ending at `edgenbr`. The adjacency
  targets stay a debug assertion (an O(edges) pass): `SCOTCH_graphCheck`
  validates them before `SCOTCH_graphPart`, and the debug `graphSave`
  before it prints them without indexing.
* **M2 — argument-free shim query** (`sizeof(SCOTCH_Num)`,
  `SCOTCH_version` into three local integers).

## Site table

### src/scotch.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| ME-01 | 76 | `(crate root)` extern 1 [akantu_scotch_num_size, akantu_scotch_version, akantu_scotch_graph_part] | declares the Scotch shim | `SCOTCH_Num` = `i32`, `int` = `c_int`, `const char *` paths; matches `src/scotch_shim.c` (checked by this audit) | declaration; `scotch_num_bits() == 32` checked before `graph_part` is called | `upstream_mesh_partitionate_scotch`, `mesh2_scotch_oracle`, par tier `--scotch` |
| ME-02 | 104 | `fn scotch_version` block 1 | `SCOTCH_version(&v, &r, &p)` | three valid, distinct local integers; Scotch only writes them | M2 | `mesh2_scotch_oracle` (version 7.0.7 asserted) |
| ME-03 | 112 | `fn scotch_num_bits` block 1 | `sizeof(SCOTCH_Num)` | pure shim function | M2 | as ME-02 |
| ME-04 | 224 | `fn graph_part_csr` block 1 | `SCOTCH_randomSeed/Reset`, `graphBuild`, `graphSave` (debug files), `graphCheck`, `graphPart` | compact CSR graph: `xadj` has `vertnbr + 1` non-decreasing entries from 0 to `edgenbr`, `vl`/`parts` `vertnbr` entries, `adjncy`/`el` `edgenbr` (or one-entry dummies); `graphBuild` and the debug `graphSave` do not index by the adjacency targets, `graphCheck` refuses a target outside `0..vertnbr` before `graphPart` (shown: `rc ≠ 0`, memcheck clean); private uniquely borrowed buffers; null or NUL-terminated paths; Scotch writes `vertnbr` parts; `SCOTCH_CONTEXT` held | M1: sizes and start indices **checked in release builds before the call** (UA-07); adjacency targets validated by `SCOTCH_graphCheck` before `graphPart` (the dual graph's targets are also debug-asserted by `partition_with_loads`, the only caller outside the test). The site moved from `partition_with_loads` into this CSR-level function (UR-11) so that the FFI contract is testable without `unsafe` in the test | `scotch::tests::graph_part_rejects_out_of_range_targets` (UR-11, under memcheck in the review's evidence), `upstream_mesh_partitionate_scotch`, `mesh2_scotch_oracle` (bitwise against the oracle's partitions), par tier `mpi_distribution --scotch` |

### src/scotch_reorder.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| ME-05 | 56 | `(crate root)` extern 1 [akantu_scotch_mesh_order] | declares the node-ordering entry of the shim | matches `akantu_scotch_mesh_order` of `src/scotch_shim.c` (checked by this audit) | declaration; 32-bit check at the top of `reorder` | `mesh2_scotch_oracle` (reorder cases) |
| ME-06 | 318 | `fn reorder` block 1 | `SCOTCH_meshBuild/Check/Save`, `meshGraph`, `graphSave`, `graphOrder` | `verttab` has `vnodnbr + velmnbr + 1` entries (Scotch reads `vendtab = verttab + 1`), `edgetab` `edgenbr` (or a dummy), `perm` `vnodnbr` (or a dummy), private uniquely borrowed buffers; paths null or NUL-terminated; Scotch writes `vnodnbr` entries; `SCOTCH_CONTEXT` held | M1: start-index count and last start (= edge count) **checked before the call** (UA-07); `SCOTCH_meshCheck` validates the structure before the debug `meshSave`; the returned permutation is checked to be a permutation before it indexes anything | `mesh2_scotch_oracle` (reorder, both modes) |

### tests/review_v103_allocations.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| ME-T01 | 33 | `impl GlobalAlloc for Counting` impl 1 | counting global allocator of the test binary | forwards every call unchanged to `System`; adds a thread-local flag check and counter (`try_with`, const-initialised); `realloc` counted as an allocation | the forwarding (same layout, same pointer) | `review_v103_allocations` |
| ME-T02 | 34 | `impl GlobalAlloc for Counting › fn alloc` fn 1 | `GlobalAlloc::alloc` | trait contract | std | as ME-T01 |
| ME-T03 | 37 | `impl GlobalAlloc for Counting › fn alloc` block 1 | `System.alloc(layout)` | the caller's contract | forwarding | as ME-T01 |
| ME-T04 | 39 | `impl GlobalAlloc for Counting › fn dealloc` fn 1 | `GlobalAlloc::dealloc` | trait contract | std | as ME-T01 |
| ME-T05 | 41 | `impl GlobalAlloc for Counting › fn dealloc` block 1 | `System.dealloc(ptr, layout)` | `ptr` from `System` with this layout | forwarding | as ME-T01 |
| ME-T06 | 43 | `impl GlobalAlloc for Counting › fn realloc` fn 1 | `GlobalAlloc::realloc` | trait contract | std | as ME-T01 |
| ME-T07 | 46 | `impl GlobalAlloc for Counting › fn realloc` block 1 | `System.realloc(ptr, layout, new_size)` | the caller's contract | forwarding | as ME-T01 |

## Dynamic checks

* **Miri** cannot run the Scotch sites (FFI); the test allocator wraps mesh
  algorithms that are not run under Miri.
* **valgrind memcheck**: `upstream_mesh_partitionate_scotch`
  (one process, `SCOTCH_PTHREAD_NUMBER=1`) in the MUMPS image: 0 errors.
