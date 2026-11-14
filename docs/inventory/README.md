<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Source inventory of upstream Akantu

These files record upstream Akantu read file by file before the port was
written. Each inventoried feature has an id (for example `FE-012`,
`MAT-025`, `TEST-146`) with its upstream location (`path:line`, relative to
the root of the upstream repository), its behaviour as read from the code,
its tests and the risks seen at the time. The parity matrix
`docs/FEATURE_PARITY.md` was first generated from these rows and is the
place where each feature's status is kept.

The inventory was taken on 2026-11-09 (the golden-file list on
2026-11-11) at upstream commit `35a851780d0b9471d258ed53a003ae98aaa95ea0`
(`v5.0.7-post1-554-g35a851780`). It is kept as reference and is not
maintained line by line: open questions, "risk" notes and coverage states
describe what was known on those dates. Where an answer came later, the
decision records (`docs/DECISIONS.md`), the defect register
(`docs/UPSTREAM_DEFECTS.md`) and the parity matrix hold it. Each part ends
with errata from an independent completeness review; where they conflict
with the text above them, the errata win.

| File | Part | Upstream scope |
|---|---|---|
| `core.md` | R1 | `src/common`, `src/fe_engine`, `src/geometry`, `src/mesh`, `src/mesh_utils`, `src/io`, `third-party` |
| `solid.md` | R2 | `src/model/solid_mechanics` (materials, cohesive, embedded), constitutive-law and non-local toolboxes, `extra_packages` |
| `models.md` | R3 | model infrastructure, solvers, synchronizers and parallel layout, contact, diffusion, phase field, structural mechanics, couplers |
| `tests_examples_python.md` | R4 | Python bindings, examples, the test suite, CI and documentation; its appendix maps every upstream file of `python/`, `examples/`, `test/` and `extra_packages/*/test` to inventory ids |
| `golden_files.md` | | upstream golden files (`.verified` and other reference data) and how each is compared |

The part labels R1 to R4 appear in the text where one part refers to
another's scope. `tests_examples_python/coverage.py` regenerates the file
coverage appendix from an upstream checkout
(`coverage.py <akantu_root> <out.md>`). The appendix is also read by
`crates/akantu-validation/tests/upstream_data_manifest.rs`, so its format
must not change without that test.
