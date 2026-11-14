<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Clean-room experiments for the random module (D-030)

`crates/akantu-core/src/random/engine.rs` and `distributions_impl.rs` are a
clean-room re-implementation of the behaviour of the oracle's
`std::minstd_rand0`, `std::generate_canonical<double, 53>` and eleven
`std::*_distribution<double>` (GCC 14 libstdc++, Debian trixie aarch64). No
standard-library source or header was read; the behaviour was taken from the
C++ standard, published algorithms, the committed fixtures and the black-box
experiments in this directory. The provenance of every design choice is
recorded in `crates/akantu-core/src/random/CLEANROOM_LOG.md`.

| File | Experiment |
|---|---|
| `exp_canonical.cc` | E1: `generate_canonical` call count, digit order, clamp, arithmetic (candidate formulas vs. the library on 2·10⁷ digit pairs) |
| `exp_canonical_k.cc` | E2: number of generator calls for ranges 2^k, 2^k ± 1 |
| `exp_assertions.cc` | E3: which parameters the `_GLIBCXX_ASSERTIONS` build rejects |
| `edges.cc` | E4: scripted-generator boundary cases, `reset()`, copies, invalid parameters, printing → `crates/akantu-core/tests/fixtures/random/cleanroom_edges.json` |
| `exp_gamma_boundary.cc` | E5: gamma acceptance test at `log(u) ==` threshold |
| `exp_invalid.cc` | E6: termination / calls / NaN for parameters outside the preconditions |
| `variants/` | V-series: replays the committed fixtures through switchable alternative operation orders (`run_all.sh`) |
| `run.sh` | Builds and runs E1–E6 in `akantu-oracle:dev`, refreshes the E4 fixture (`--no-fixture` to only compare) |

Every program uses only the public `<random>` API (custom generators that
script or count engine outputs). Outputs land in `target/cleanroom-random/`.

```sh
oracle/probes/random/cleanroom/run.sh            # E1–E6 (+ refresh cleanroom_edges.json)
oracle/probes/random/cleanroom/variants/run_all.sh   # V-series (needs serde_json in the cargo cache)
```
