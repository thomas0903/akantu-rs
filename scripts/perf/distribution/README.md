<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Scripts of the distribution and communication-layer measurements

akantu-rs project tooling (not ported from Akantu). They ran on a
development machine of the performance campaign (an 8-vCPU AMD EPYC
Milan virtual machine, Docker with the project's `akantu-rs-scotch:dev`
image) from a directory `~/w/dist/` holding the binaries in
`bin/<variant>/`; the results are in
`docs/benchmarks/perf-campaign/distribution/`.

| Script | What it does |
|---|---|
| `build.sh <tree> <label>` | release build of `suite_setup` (features `mpi,scotch`), `suite_models` and `cohesive_extrinsic` in the Scotch image, copied to `bin/<label>/` |
| `ab.sh <series> <bin> <cases> <ranks> <reps> <size> <variant>...` | interleaved A/B of suite binaries under `mpirun` (rank r on core r), the variants' order rotated per repetition; `runs/<series>/<variant>/np<P>/<case>.r<rep>.jsonl` |
| `summ.py <series-dir> [base]` | median [min, max] per call of every phase, paired ratios against `base`, digest check |
| `coh.sh <series> <reps> <ranks> <variant>...` | the cohesive driver with `coh-input.dat` (insertion off, the port's Eigen-order static solver), interleaved and rotated |
| `cohsumm.py <series-dir> <base>` | equality of the driver's physical summary across variants and repetitions, phase times (maximum over the ranks) |
| `prof.sh`, `flat.sh`, `incl.sh`, `callers.py` | `perf record` of a running suite case from the host and the flat / inclusive / caller reports (a symbol tree of the image's libraries resolves their symbols) |
| `final.sh` | the before / after series of the report |
