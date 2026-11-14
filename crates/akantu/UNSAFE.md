# `unsafe` sites of `akantu` (the facade)

The library is `#![forbid(unsafe_code)]`. The only `unsafe` code is in the
MPI exit-hook test binary `tests/exit_hook_mpi.rs` (feature `mpi`, `harness
= false`, run by the MPI tier: D-067 scenarios), which plays the part of a
host application that initialises and finalises MPI itself. Every site is
listed below (`scripts/unsafe_inventory.py --check`, run by `cargo test -p
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

**Counts**: test sites only — `extern` block 1, block 2.

## Site table

### tests/exit_hook_mpi.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| AK-T01 | 43 | `(crate root)` extern 1 [MPI_Init_thread, MPI_Finalize] | declares the two MPI calls of the host role | the MPI C prototypes (`int MPI_Init_thread(int *, char ***, int, int *)`, `int MPI_Finalize(void)`), linked through rsmpi's MPI library | declaration | `exit_hook_mpi` `hostfirst`, `hostfirst-universe` |
| AK-T02 | 112 | `fn run` block 1 | `MPI_Init_thread(NULL, NULL, FUNNELED, &provided)` as the host | the first MPI call of the process, on the main thread; NULL `argc`/`argv` allowed (MPI-4.1 §11.2.1); `provided` a valid out-pointer; `1` is Open MPI's `MPI_THREAD_FUNNELED` | the scenario's structure (first statement of the `hostfirst` branch) | `exit_hook_mpi` `hostfirst` |
| AK-T03 | 128 | `fn run` block 2 | `MPI_Finalize()` by the host before exit | MPI initialised by this host on this (main) thread; every akantu communication completed (barrier above) | the scenario's structure; afterwards the exit hook must make no MPI call (D-067 (1)) | `exit_hook_mpi` `hostfirst` |
