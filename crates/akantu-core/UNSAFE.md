# `unsafe` in `akantu-core`

**This crate contains no `unsafe` code**: `src/lib.rs` has
`#![forbid(unsafe_code)]`, and `scripts/unsafe_inventory.py --check` (run by
`cargo test -p akantu-validation --test unsafe_inventory`) finds no site in
its sources, tests or examples (it would then require a site table here).
A plain text search for `unsafe` finds one line: the `forbid(unsafe_code)`
attribute itself.

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
