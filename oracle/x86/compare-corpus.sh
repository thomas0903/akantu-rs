#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: (D-043) corpus comparison.
#
# Compares a staged (regenerated) corpus with the committed one:
#   * manifest-based cases (fixtures/**/manifest.json) with
#     `fixture-diff --json` (akantu-validation), value by value;
#   * every other oracle output (fixtures/dump, fixtures/meshes,
#     crates/*/tests/fixtures, SHA256SUMS files) with compare_files.py;
#   * the re-frozen Gmsh meshes (if present) against fixtures/meshes.
#
# Usage: oracle/x86/compare-corpus.sh <committed-clone> <staging-clone> <out-dir>
#        [<refrozen-meshes-dir>]
# Env:   FIXTURE_DIFF (path of the fixture-diff binary; default: build it with
#        cargo in <committed-clone>, release profile).
# Writes <out-dir>/fixture-diff.json, <out-dir>/files-*.json and prints a
# summary. Exit 0 when every comparison ran (differences are measurements).
set -uo pipefail
a="$(cd "${1:?usage: compare-corpus.sh <committed> <staging> <out> [meshes]}" && pwd)"
b="$(cd "${2:?staging clone}" && pwd)"
out="${3:?out dir}"
mkdir -p "${out}"
out="$(cd "${out}" && pwd)"
meshes="${4:-}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ -z "${FIXTURE_DIFF:-}" ]; then
  (cd "${a}" && cargo build --release -q -p akantu-validation --bin fixture-diff) || exit 1
  FIXTURE_DIFF="${CARGO_TARGET_DIR:-${a}/target}/release/fixture-diff"
fi

echo "== fixture-diff (manifest cases)"
"${FIXTURE_DIFF}" "${a}/fixtures" "${b}/fixtures" --json > "${out}/fixture-diff.json"
echo "fixture-diff exit $? -> ${out}/fixture-diff.json"

echo "== non-manifest files"
python3 -I "${here}/compare_files.py" "${a}/fixtures" "${b}/fixtures" \
  --exclude '*.npy' --exclude '*/manifest.json' --json "${out}/files-fixtures.json"
for d in "${a}"/crates/*/tests/fixtures; do
  rel="${d#"${a}/"}"
  name="$(echo "${rel}" | tr '/' '_')"
  python3 -I "${here}/compare_files.py" "${d}" "${b}/${rel}" --json "${out}/files-${name}.json"
done
if [ -n "${meshes}" ]; then
  echo "== re-frozen meshes"
  python3 -I "${here}/compare_files.py" "${a}/fixtures/meshes" "${meshes}" \
    --json "${out}/files-refrozen-meshes.json"
fi
