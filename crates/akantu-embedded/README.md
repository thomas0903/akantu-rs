# akantu-embedded

Embedded-interface model of the Rust port of Akantu (parity rows
EMB-001…005): 1-D reinforcement bars embedded in a 2-D/3-D solid matrix
without conforming meshes (upstream `EmbeddedInterfaceModel`,
`EmbeddedInterfaceIntersector`, `MaterialReinforcement` /
`reinforcement elastic`).

```rust,ignore
let mut model = EmbeddedInterfaceModel::with_parser(mesh, reinforcement_mesh, None, "model", parser)?;
model.init_full(&EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static))?;
model.solve_step(None)?;
```

* The reinforcement mesh holds `_segment_2` elements with the element data
  `physical_names`; each name selects a `material reinforcement elastic [ … ]`
  law (`E`, `area`, `pre_stress`, …), the background elements are assigned by
  their own `physical_names`.
* **Licence:** LGPL-3.0-or-later. Upstream needs CGAL; this crate uses the
  LGPL segment intersector of `akantu-geometry` (`LinearScan`) and never
  depends on the GPL-3.0-or-later crate `akantu-geometry-cgal` (D-042;
  `tests/license_guard.rs`), so the `embedded` feature of the umbrella crate
  stays in the default build.
* **Verification:** the upstream model does not compile at the reference
  commit; a patched oracle variant (`oracle/patches/embedded`, D-060,
  `docs/ORACLE_BUILD.md`) makes it run without changing a formula, and
  its fixtures `fixtures/model/embedded/*` are replayed by
  `tests/oracle_fixtures.rs`. The port is also validated by the upstream
  tests' criteria, the 2015 golden stiffness file (byte for byte) and
  independent mathematical checks (`docs/NUMERICAL_VALIDATION.md`).
  Upstream defects and their handling:
  `docs/UPSTREAM_DEFECTS.md` UD-587…601, UD-787.
* **Interface pieces:** the pieces of every reinforcement segment tile its
  part inside the background mesh exactly once (interval tiling of
  `akantu-geometry`, UD-599/UD-601), also for bars through mesh vertices at
  grazing angles; `EmbeddedInterfaceIntersector::remove_near_duplicates =
  false` gives upstream's collected set.
* Serial only (`Error::Unsupported` on more than one rank).
