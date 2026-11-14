#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Independent reader check: run the akantu-io dumper tests with
# their outputs exported, then parse every ParaView file with
# check_vtu.py (xml.etree + strict base64 + NumPy) inside the oracle image,
# for the Rust output and for the oracle's own output (fixtures/dump).
#
#   oracle/probes/dump/check.sh        # from the repository root
#
# Rust: every file must be valid except the deliberate upstream-layout
# (compat) outputs used for byte comparison (m2_raw_qp, smm2d, smm3d) and
# the random-mesh robustness test (meshes with invalid node ids).
# Oracle: the files written with upstream's layout defects must be invalid
# (m2_raw_qp, smm2d, smm3d) and the dumps upstream aborts are incomplete
# (coh3d16, dkt18, smm2d_strain_pad); everything else must be valid.
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
export_dir="$(mktemp -d)"
trap 'rm -rf "$export_dir"' EXIT
(cd "$repo" && AKANTU_DUMP_EXPORT="$export_dir" cargo test -q -p akantu-io --tests >/dev/null)
run() {
  docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory 4g -v "$repo":/work:ro -v "$export_dir":/rust:ro \
    akantu-oracle:dev python3 -I /work/oracle/probes/dump/check_vtu.py "$@"
}
echo "== Rust output"
run --expect-invalid /m2_raw_qp/ --expect-invalid /smm2d/ --expect-invalid /smm3d/ \
    --skip /props_random/ /rust
echo "== Oracle output (fixtures/dump)"
run --expect-invalid /m2_raw_qp/ --expect-invalid /smm2d/ --expect-invalid /smm3d/ \
    --expect-invalid /coh3d16/ --expect-invalid /dkt18/ --expect-invalid /smm2d_strain_pad/ \
    /work/fixtures/dump
