<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Documents

An index of the repository's documents, one line each.

## For users

- [manual/](manual/): the user manual (mdBook), with the chapter on
  differences from the C++ library; `mdbook build docs/manual` builds it
  (mdBook 0.4.40 with the preprocessor mdbook-katex 0.9.4, which typesets
  the formulas).
- [../README.md](../README.md): what akantu-rs is, its state and a quick start.
- [../CHANGELOG.md](../CHANGELOG.md): the changes of each version.
- [PLATFORMS.md](PLATFORMS.md): the supported platforms and what holds on each.
- [FEATURE_PARITY.md](FEATURE_PARITY.md): every feature of the C++ library
  with its status in the port, its tests and its evidence.
- [UPSTREAM_DEFECTS.md](UPSTREAM_DEFECTS.md): the defects found in the C++
  library and what the port does about each, with the switches that reproduce
  upstream's behaviour.
- [reports/](reports/): the reports on the port (how it was made and verified,
  performance, upstream defects, possible improvements).

## For contributors and maintainers

- [../CONTRIBUTING.md](../CONTRIBUTING.md): how to contribute, the technical
  rules and their reasons.
- [../AGENTS.md](../AGENTS.md): the instructions for coding agents.
- [MAINTAINING.md](MAINTAINING.md): procedures (a new material, element type
  or dumper field, a bug fix that changes results, fixtures, tolerances, a new
  upstream commit, dependencies, releases).
- [ARCHITECTURE.md](ARCHITECTURE.md): the crates and their layers, the main
  traits and types, ownership and index types, the parallel layout, the
  `unsafe` policy.
- [DECISIONS.md](DECISIONS.md): the technical decisions (D-ids) with their
  reasons and consequences.
- [TESTING.md](TESTING.md): the test tiers, how to run each, how to add a test
  and a fixture.
- [../ci/README.md](../ci/README.md): the CI tooling and `ci/run-all.sh`.
- [../ci/github/README.md](../ci/github/README.md): the GitHub workflows, their
  triggers, the gate map and the repository settings.
- [../ci/MAPPING.md](../ci/MAPPING.md): how upstream's GitLab CI maps onto the
  tiers.
- [DEPENDENCIES.md](DEPENDENCIES.md) and [../NOTICE.md](../NOTICE.md): the
  third-party code and native libraries, with their licences.

## Validation

- [NUMERICAL_VALIDATION.md](NUMERICAL_VALIDATION.md): the comparison method,
  the fixture format, the tolerance method and the results per area.
- [VALIDATION_SUMMARY.md](VALIDATION_SUMMARY.md): the summary of the numerical
  validation.
- [MPI_VALIDATION.md](MPI_VALIDATION.md): the validation of the parallel runs
  and of partition independence.
- [TOLERANCE_LEDGER.md](TOLERANCE_LEDGER.md): every tolerance rule with its
  justification (generated from the `tolerances.json` files).
- [REFERENCE.md](REFERENCE.md): the pinned upstream commit and the facts of
  the reference tree.
- [ORACLE_BUILD.md](ORACLE_BUILD.md): how to build and run the C++ oracle and
  regenerate fixtures.
- [inventory/](inventory/): the source inventories of the C++ library, read
  file by file, behind the parity matrix.
- [BENCHMARKS.md](BENCHMARKS.md) and [benchmarks/](benchmarks/): the measured
  performance of the port against the C++ library, with the data the
  documents cite.

## Project

- [../SECURITY.md](../SECURITY.md): how to report a security problem.
- [../CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md): the code of conduct.
- [../CITATION.cff](../CITATION.cff): how to cite the port and the Akantu
  paper.
