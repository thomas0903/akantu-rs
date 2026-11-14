<!-- availability: available akantu::extra_materials -->
# Extra materials

<!-- BEGIN GENERATED: availability-status extra-materials -->
**Available behind the Cargo feature `extra-materials`**, off by default (facade module `akantu::extra_materials`, crate `akantu-extra-materials`).
<!-- END GENERATED: availability-status extra-materials -->

Upstream: the package `extra-materials` of `extra_packages/` (parity rows
XMAT-*).
The port's crate `akantu-extra-materials` is reached as
`akantu::extra_materials` with the facade feature `extra-materials` (off by
default, as upstream's package), or directly as `akantu_extra_materials`:

```toml
[dependencies]
akantu = { path = "…/akantu-rs/crates/akantu", features = ["extra-materials"] }
```

<!-- BEGIN GENERATED: availability-status extra-materials-non-local -->
**Available behind the Cargo feature `extra-materials-non-local`**, off by default (crate `akantu-extra-materials`).
<!-- END GENERATED: availability-status extra-materials-non-local -->

The non-local variants (`damage_iterative_non_local`, `brittle_non_local`,
`vreepeerlings_non_local`, `damage_orthotropic_iterative_non_local`;
upstream package `extra_materials_non_local`) are registered by
`register_extra_non_local_materials`, behind the facade feature
`extra-materials-non-local` (it enables `extra-materials` and the crate
feature `akantu-extra-materials/damage-non-local`; off by default, as
upstream's package), and are averaged by the solid model's non-local
manager like the non-local laws of the solid model, one
`non_local <law name> <weight function> [ … ]` section per law; like every
non-local law they run on distributed meshes too
([Parallel computations](../parallel.md)).

Per-law descriptions are kept in the XMAT rows of
[`docs/FEATURE_PARITY.md`](../../../FEATURE_PARITY.md); the departures from
upstream's code are the entries of
[`docs/UPSTREAM_DEFECTS.md`](../../../UPSTREAM_DEFECTS.md) that name them.
