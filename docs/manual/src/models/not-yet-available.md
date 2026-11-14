# Models not yet available

The features below are not part of this release, and none of them is
being ported. Upstream has no distributed support for the first two, so
the port refuses a distributed mesh for them; upstream has no C API (an
exception for now, D-100). The table's last column says why each is
missing.

<!-- BEGIN GENERATED: availability-waiting-list -->
| Feature | Upstream manual | Facade module | Why it is not available |
|---|---|---|---|
| Contact mechanics on a distributed mesh | — | — | not planned: upstream has no distributed support for it; the port refuses (approved exception, parity row CONT-019) |
| The structural model on a distributed mesh | — | — | not planned: upstream has no distributed support for it (UD-902); the port refuses |
| The C API (`akantu-capi` exports only its version string) | — | — | not planned for now: upstream has none (approved exception, D-100) |
<!-- END GENERATED: availability-waiting-list -->

Other limits of this release are smaller than a whole feature and are
described where they apply: configurations that upstream accepts with an
undefined or wrong result, which the port refuses by default and runs only
behind a compatibility switch ([Differences from the C++
library](../differences.md)); features refused on several MPI ranks
([Parallel computations](../parallel.md)); the model level of the Python
interface on several ranks ([Python interface](python.md)); and the
approved exceptions of the parity matrix, upstream code that is never
built or never called (`docs/VALIDATION_SUMMARY.md`).

The table is generated from the availability table of the manual
(`docs/manual/availability.md`), which a test
(`tests/manual.rs::manual_availability_matches_the_workspace`) checks
against the workspace: a facade module listed here is a placeholder
crate, every facade module that is not a placeholder is described as
available, and no sentence of the manual or of the README calls
something unavailable that the table lists as available. The completion
status of every row is in
[`docs/FEATURE_PARITY.md`](../../../FEATURE_PARITY.md).
