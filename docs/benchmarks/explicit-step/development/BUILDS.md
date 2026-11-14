# Builds of the measurements

Labels of the JSON lines → commit measured. The branch's commit messages were
reworded afterwards (measured gains appended), which changed the SHAs from
`17c00809` on; the trees are identical (`git rev-parse <sha>^{tree}`).

| Label | Measured commit | Commit on the branch | Tree |
|---|---|---|---|
| base | `0dbc8f7c` | `0dbc8f7c` | `9beabfaec852` |
| fe1 | `59831902` | `70770ee4` | `35d2d12864cc` |
| fd1 | `790755c0` | `26a24fdf` | `d4d2683cdf93` |
| o5a | `c5051857` | `7a6e7e1f` | `49f473f1d3c0` |
| o5b | `6c110929` | `e7a3a8dd` | `2b1e6aea3505` |
| o5c | `c64a3f11` | `484c1bf2` | `bb64823d2686` |
| g1 | `42b3f510` | `61b0a323` | `8a2157a2ca24` |
| f2 | `37c18811` | `6a922850` | `e388b5c7f776` |
| cur | `ebb080a9` | `3c166d03` | `329fbef9a6b4` |
| coh-cur | `ebb080a9` | `3c166d03` | `329fbef9a6b4` |
| int2 | `19aabbea` | `12d4a70f` | `6185e7e6937a` |
| coh-cauchy | `f19004dc` | `205143e4` | — |
| head, coh-head | `f7d8f8dd` | `19099be2` | — |
| head-lto, coh-head-lto | `f7d8f8dd` with `CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1` | `19099be2` | — |
| coh-base | `0dbc8f7c` + `crates/akantu-cohesive/examples/cohesive_step_timing.rs` of `3c166d03` | — | — |
| cpp-strict | `scripts/perf/explicit-step/explicit_step_timing.cc` of `3c166d03`, oracle image `sha256:45122c21…`, `/build/install-strict` | — | — |
