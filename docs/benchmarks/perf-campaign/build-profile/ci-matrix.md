# CI tiers built with each build configuration

`scripts/perf/build/ci_matrix.sh` on a boat.dev `large` sandbox (AMD EPYC
4585PX, 8 vCPU, 16 GB; `AKANTU_RS_DOCKER_CPUS=8`,
`AKANTU_RS_DOCKER_MEMORY=12g`), project image `akantu-rs-mpi:dev`,
2026-11-16, tree of commit `59dd2229` plus the branch's tooling commits
(no crate source changed). Each configuration: a scratch checkout with an
untracked `.cargo/config.toml` holding its rustflags (`[build] rustflags`)
or release-profile settings (`[profile.release]`), then
`ci/run-all.sh --backend docker --tiers <tiers>`. Verdicts are those of
`ci/tools/analyze_tests.py` against the empty waiver list
`ci/tools/known-failures.txt`. Seconds are wall time of the tier on the
sandbox (shared with other jobs; not a measurement).

| configuration | tier | verdict | tests (test tier) | seconds |
|---|---|---|---|---|
| `-C target-cpu=x86-64-v2` | test | pass | 494 result lines, 3,769 passed, 0 failed, 25 ignored | 941 |
| `-C target-cpu=x86-64-v2` | release | pass | | 678 |
| `-C target-cpu=x86-64-v3` | test (rerun, standalone clone) | pass | 3,769 passed, 0 failed, 25 ignored | 903 |
| `-C target-cpu=x86-64-v3` | release | pass | | 569 |
| `-C target-cpu=x86-64-v4` | test (rerun, standalone clone) | pass | 3,769 passed, 0 failed, 25 ignored | 910 |
| `-C target-cpu=x86-64-v4` | release | pass | | 578 |
| `-C target-cpu=znver4` | test | pass | 3,769 passed, 0 failed, 25 ignored | 1,243 |
| `-C target-cpu=znver4` | release | pass | | 784 |
| `lto = "fat"`, `codegen-units = 1` | release | pass | | 1,991 |
| `lto = "thin"`, `codegen-units = 1` | release | pass | | 2,240 |

First runs of `x86-64-v3` and `x86-64-v4` (scratch checkout = git
worktree): test tier 3,766 passed, 3 failed, all three tree guards that
read git (`akantu-validation`: `public_tree_guard::no_home_paths_and_no_broken_doc_links`
reported the worktree's `.git` file, which names a host path;
`upstream_data_manifest::{copies_on_disk_match_the_manifest,
upstream_data_directories_are_documented}`: `git ls-files` cannot follow
the worktree's gitdir inside the container). Release tiers of those runs:
pass. The reruns above use a standalone clone.

The `test` tier builds with the dev profile, so the release-profile
variants (LTO, codegen units) are checked by the `release` tier only. The
rustflags variants apply to both tiers.
