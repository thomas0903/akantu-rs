# `unsafe` in `akantu-core`

`src/lib.rs` has `#![deny(unsafe_code)]`. Two modules allow it (D-123):
`src/exec/spin.rs`, the thread pool of the execution policy, which hands a
borrowed closure to its workers for the duration of one region, and
`src/exec/simd.rs`: the example kernel of the SIMD dispatch
convention, whose `#[target_feature]` versions are `unsafe fn`s called from
one dispatch point, and the FMA entry point of fast-mode kernels
(`with_fma`). Every site is listed below (`scripts/unsafe_inventory.py
--check`, run by `cargo test -p akantu-validation --test unsafe_inventory`;
`--update-lines` after an edit that only moves code). Each row is pinned to
a fingerprint of its site's code in `UNSAFE.fingerprints`.

**Counts**: `unsafe impl` 2, `unsafe fn` 4, block 12 (the SIMD sites on one
architecture at a time: x86-64 compiles the AVX2, AVX-512 and FMA sites,
aarch64 the NEON sites).

## Site table

The invariant common to every site: a `SimdToken` of a level exists only
when the CPU reports the level's target feature (`SimdLevel::token` and
`SimdToken::detected` are the only constructors; the field is private).

### src/exec/simd.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| CO-01 | 228 | `fn scaled_add` block 1 | calls the AVX-512F version | CPU has avx512f; equal lengths | `SimdToken` constructors; `assert_eq!` of the lengths | `exec::simd::tests::every_level_gives_the_scalar_bits` |
| CO-02 | 234 | `fn scaled_add` block 2 | calls the AVX2 version | CPU has avx2; equal lengths | as CO-01 | as CO-01 |
| CO-03 | 240 | `fn scaled_add` block 3 | calls the NEON version | CPU has neon; equal lengths | as CO-01 | as CO-01 (aarch64) |
| CO-04 | 256 | `fn scaled_add_avx2` fn 1 | `#[target_feature(enable = "avx2")]` version | caller guarantees avx2 | CO-02 | as CO-01 |
| CO-05 | 267 | `fn scaled_add_avx2` block 1 | unaligned 4-wide loads, multiply, add, store | `i + 4 ≤ len` of both slices | loop bound `body = n − n % 4`, `n = min(len)` | as CO-01 |
| CO-06 | 279 | `fn scaled_add_avx512` fn 1 | `#[target_feature(enable = "avx512f")]` version | caller guarantees avx512f | CO-01 | as CO-01 |
| CO-07 | 290 | `fn scaled_add_avx512` block 1 | unaligned 8-wide loads, multiply, add, store | `i + 8 ≤ len` of both slices | loop bound `body = n − n % 8` | as CO-01 |
| CO-08 | 302 | `fn scaled_add_neon` fn 1 | `#[target_feature(enable = "neon")]` version | caller guarantees neon | CO-03 | as CO-01 (aarch64) |
| CO-09 | 312 | `fn scaled_add_neon` block 1 | 2-wide loads, multiply, add, store | `i + 2 ≤ len` of both slices | loop bound `body = n − n % 2` | as CO-01 (aarch64) |
| CO-10 | 202 | `fn with_fma` block 1 | calls the FMA-enabled entry point | CPU has avx2 and fma | `SimdToken` constructors (`Avx2` and `Avx512` require both) | `exec::simd::tests::fma_tasks_are_fused_at_every_level` |
| CO-11 | 210 | `fn run_with_fma` fn 1 | `#[target_feature(enable = "avx2,fma")]` entry point that runs a task inlined into it | caller guarantees avx2 and fma | CO-10 | as CO-10 |

### src/exec/spin.rs

The region protocol every site relies on: the owner of a region holds
`SpinPool::busy`, writes `job` while no worker is inside a region, creates
a `RegionGuard`, opens the region (SeqCst store of `state`: its generation,
not closed, nobody inside), takes parts itself, and drops the guard, whose
`Drop` closes the region (`fetch_or` of the closed bit), waits until no
worker is inside (Acquire) and clears `job`. A worker reads `job` only
after entering the open region of the generation it observed (SeqCst
compare-exchange that increments the count of workers inside, refused once
the region is closed or replaced); its `Inside` guard decrements the count
in its `Drop`. A worker that comes too late skips the region. A worker
starts from generation 0, so a region published before it started is not
missed.

The unwind argument: no unwind can leave a region open or a worker counted
inside it. The parts run under `catch_unwind`; their payloads are stored
(the first) or queued (the others) and never dropped inside the region, so
a payload whose `Drop` panics cannot unwind out of `run_parts`; the queued
payloads are dropped by the owner after the guard closed and emptied the
region, each under `catch_unwind` (a panic of such a `Drop` is caught and
its payload forgotten). Should anything else unwind out of `run_parts`
anyway, the owner's `RegionGuard` and the worker's `Inside` run during the
unwind: the region is closed and emptied, and `job` cleared, before `run`
unwinds further. A `busy` lock poisoned by such an unwind is taken as it
is (the region it protected was closed by its guard). Test:
`exec::spin::tests::panicking_payload_drops_never_leave_a_region_open`
(payloads whose `Drop` panics on the caller's and on the workers' parts;
no part runs after `run` returns; the pool works afterwards).

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| CO-12 | 112 | `impl Sync for Shared` impl 1 | shares the pool state (with the `UnsafeCell` job slot) between threads | writes and reads of `job` never overlap | the region protocol (enter / close) | `exec::spin::tests::*`, `exec::pool::tests::*` |
| CO-13 | 114 | `impl Send for Shared` impl 1 | moves the shared state into the workers | as CO-12 | as CO-12 | as CO-12 |
| CO-14 | 206 | `impl SpinPool › fn run` block 1 | erases the lifetime of the region's closure | the pointer is not used after `run` returns or unwinds | `RegionGuard::drop` closes the region, waits until no worker is inside and clears the slot, on every path out of `run` | `every_part_runs_once`, `concurrent_callers` |
| CO-15 | 209 | `impl SpinPool › fn run` block 2 | writes the job slot | no worker reads it now | `busy` held; the previous region is closed with nobody inside | as CO-14 |
| CO-16 | 265 | `impl Drop for RegionGuard<'_> › fn drop` block 1 | clears the job slot (in `RegionGuard::drop`) | the region is closed and nobody is inside | closed bit set, count 0 observed | as CO-14 |
| CO-17 | 400 | `fn worker` block 1 | reads the job slot | this worker is inside the open region | successful enter (compare-exchange); `Inside` not yet dropped | as CO-14, `regions_after_parking_and_nested_regions` |
| CO-18 | 403 | `fn worker` block 2 | calls the region's closure | the closure outlives the region | as CO-17 | `a_panic_reaches_the_caller_and_the_pool_survives` |

## Guarantees the `unsafe` code of other crates relies on

It matters to the inventory anyway: **its safe code establishes the
invariants that the `unsafe` code of `akantu-python` relies on**
(`crates/akantu-python/UNSAFE.md`, "The protocol every site relies on"):

| Guarantee (safe code, `docs/ARCHITECTURE.md`, D-024 / D-047 / D-056) | Where | Relied upon by |
|---|---|---|
| A lease (`ExportLease`) keeps its allocation, with at least `nb_values()` initialised values, alive until the lease drops; a block is never reallocated under a lease (growth beyond capacity, any shrink, replacement or destruction retire it into the lease's anchor) | `src/array_storage.rs` | `ForeignView::{read, write, fill, copy_to}`, the NumPy handover (PY-03…PY-07) |
| If `ExportLease::check_foreign_access(interp)` returns `Ok(())`, then while `interp` stays borrowed no native guard (`ArrayRef`, `ArrayMut`, `SliceRef`) of any allocation exported to that runtime is alive or can be acquired, and no other thread performs foreign accesses on it (the runtime's phase word, acquire/release CAS; the `Interpreter` token is `!Send + !Sync` and exclusive per runtime) | `src/array_shared.rs` | as above, and the `Send`/`Sync` impls of `ForeignView` (PY-01, PY-02) |
| A Rust slice of an exported allocation exists only under a native guard; retired blocks are reachable only through leases (raw pointers) | `src/array_shared.rs`, `src/array_storage.rs` | `ForeignView::copy_to` (`copy_nonoverlapping`, PY-06) |
| Element filters of constitutive laws are not exportable storage (snapshot copies to Python) | D-056 (2) | — (keeps them out of the protocol) |

**Evidence for these guarantees** (they are safe code, so a violation would
be a logic error that makes the `unsafe` code of `akantu-python` unsound,
not undefined behaviour inside this crate):

* Miri, Stacked and Tree Borrows (`nightly-2026-10-01`, the
  `ci/tiers/miri.sh` recipe and the Miri commands of `docs/TESTING.md`):
  `cargo miri test
  -p akantu-core --lib array_storage` 8 passed; `cargo miri test -p
  akantu-core --lib -- array element_type_map_shared` 40 passed; no
  undefined behaviour reported (the tests drive the `akantu-python` views
  over these types: `crates/akantu-python/tests/foreign_views.rs` 25 and
  `tests/v119_scoped_adversarial.rs` 6, also clean).
* `tier.sh exportall --gate` (`--features akantu-core/export-all`): every
  exportable array of the whole suite in export mode; no live guard at any
  foreign-callback site.
* The `smuggle_*` programs (a borrow carried out of a guard) are compile
  errors, pinned by `compile_fail` doc tests.
* Independent reviews of the protocol, of the storage layer and of the
  execution contexts.
