# `unsafe` sites of `akantu-solid`

The library is `#![forbid(unsafe_code)]`. The only `unsafe` code is the
counting global allocator of one **test** binary. Every site is listed
below (`scripts/unsafe_inventory.py --check`, run by `cargo test -p
akantu-validation --test unsafe_inventory`; `--update-lines` after an edit
that only moves code).
Each row is also pinned to a fingerprint of its site's code (comments and
whitespace ignored) in `UNSAFE.fingerprints`: a site
whose code changed fails the check, and `--update-lines` does not move its
row, until the row is reviewed and re-pinned with `--refresh <Id>`.
A fingerprint covers the Rust site's own tokens only: not the code around
it that sets up its invariants, and not C sources, which the guard
does not read; a change there shows only in the commit's diff and its
review.

**Counts**: test sites only — `unsafe impl` 1, `unsafe fn` 2, block 2.

## Site table

### tests/mazars_allocations.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| SO-T01 | 37 | `impl GlobalAlloc for Counting` impl 1 | counting global allocator (the Mazars / anisotropic-damage kernels must not allocate per quadrature point) | forwards every call unchanged to `System`; adds a const-initialised thread-local counter (`try_with`: never allocates, never panics during thread teardown) | the forwarding (same layout, same pointer; `realloc` falls back to the trait default `alloc` + copy + `dealloc`) | `mazars_allocations` |
| SO-T02 | 38 | `impl GlobalAlloc for Counting › fn alloc` fn 1 | `GlobalAlloc::alloc` | trait contract | std | as SO-T01 |
| SO-T03 | 41 | `impl GlobalAlloc for Counting › fn alloc` block 1 | `System.alloc(layout)` | the caller's contract | forwarding | as SO-T01 |
| SO-T04 | 43 | `impl GlobalAlloc for Counting › fn dealloc` fn 1 | `GlobalAlloc::dealloc` | trait contract | std | as SO-T01 |
| SO-T05 | 45 | `impl GlobalAlloc for Counting › fn dealloc` block 1 | `System.dealloc(ptr, layout)` | `ptr` from `System.alloc` with this layout | forwarding | as SO-T01 |
