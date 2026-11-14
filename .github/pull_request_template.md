## What and why

<!-- What the change does and why. Link the issue (Fixes #...). -->

## How it was tested

<!-- The commands you ran and their results. Say what you could not run. -->

## Checklist

- [ ] `cargo fmt --all` and `cargo clippy --workspace --all-targets --locked -- -D warnings` are clean.
- [ ] `CI_INSTALL_TOOLCHAINS=1 ci/run-all.sh --backend docker --tiers lint,test,fixtures,parity,licence,msrv,docs` passes, or I ran the tests of the crates I changed and say above why not the rest.
- [ ] No bitwise result changed, or the change of results is described above and was agreed in the issue.
- [ ] No tolerance was loosened; a new or changed rule has its justification and evidence in `tolerances.json` and the ledger is regenerated.
- [ ] No golden file was edited by hand; regenerated fixtures are listed with the oracle command that produced them.
- [ ] No test was ignored or deleted to make CI pass.
- [ ] New `unsafe` code has `// SAFETY:` comments and its rows in the crate's `UNSAFE.md`.
- [ ] New files carry the SPDX header; no code from a source with an incompatible licence; GPL code only in `crates/akantu-geometry-cgal`.
- [ ] The documents the change affects are updated (parity matrix, `docs/UPSTREAM_DEFECTS.md`, `docs/DECISIONS.md`, `CHANGELOG.md`).
- [ ] Weekly workflows this change needs (features, medium, python, memcheck): named here for the maintainer to run.

## AI assistance

<!-- Which AI tools you used and for what, or "none". You are responsible for the result and must have run the tests. -->
