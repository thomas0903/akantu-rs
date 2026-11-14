#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (port of Akantu, EPFL/LSMS). Mutation check of
# crates/akantu-solid/tests/spatial_convergence.rs: apply one source mutation
# to the library (akantu-fe), run the convergence binary with --nocapture,
# record which tests fail and the rate lines they report, restore the file.
#   M0  baseline (no mutation)
#   M1  wrong quadrature weight: third weight of the 3-point triangle rule
#       (order 2: stiffness of triangle_6, the triangle factor of both
#       pentahedron rules, consistent mass of triangle_3) 1/6 -> 1/5
#   M2  wrong shape-function derivatives: quadrangle_8 dN_4/dxi and
#       dN_6/dxi swapped (an index slip; sum over the nodes unchanged, so
#       the linear patch test still passes)
#   M3  wrong shape-function derivative: quadrangle_8 dN_4/deta
#       -(1 - xi^2)/2 -> -(1 - xi)/2 (not translation invariant: the
#       library refuses the mesh with a negative Jacobian at init)
# Run from the repository root of a clean checkout (it edits and restores
# files of akantu-fe); never commit the logs. Usage:
#   crates/akantu-solid/scripts/conv_mutants.sh <log-dir> [cargo test args]
# e.g. `... <dir> --release` also runs the finest 3-D levels. Results:
# docs/NUMERICAL_VALIDATION.md.
set -u
out=${1:?usage: conv_mutants.sh <log-dir> [cargo test args]}
shift
mkdir -p "$out"
if ! git diff --quiet -- crates/akantu-fe; then
  echo "crates/akantu-fe has local changes: refusing to mutate it" >&2
  exit 2
fi
run() { # id file 'from=>to[|||from=>to...]' [cargo test args]
  local id=$1 f=$2 subs=$3 log="$out/$1.log"
  shift 3
  if [ -n "$f" ]; then
    python3 -I - "$f" "$subs" << 'EOF' || { echo -e "$id\tNOT-APPLIED\t$f"; return; }
import sys
path, subs = sys.argv[1:3]
text = open(path).read()
# every site is checked on the original text, then all are replaced together
pairs = [s.split("=>") for s in subs.split("|||")]
for old, _ in pairs:
    if text.count(old) != 1:
        sys.exit(f"{path}: {text.count(old)} occurrences of the mutation site {old!r}")
for i, (old, _) in enumerate(pairs):
    text = text.replace(old, f"@@MUTANT{i}@@")
for i, (_, new) in enumerate(pairs):
    text = text.replace(f"@@MUTANT{i}@@", new)
open(path, "w").write(text)
EOF
  fi
  cargo test -p akantu-solid --test spatial_convergence "$@" -- --nocapture --test-threads=4 > "$log" 2>&1
  local rc=$?
  if [ -n "$f" ]; then
    git checkout -q -- "$f"
    touch "$f" # the restored file keeps an old mtime: without this Cargo keeps the mutant compiled
  fi
  local failed
  failed=$(grep -E '^test .* FAILED$' "$log" | sed -e 's/^test //' -e 's/ \.\.\. FAILED$//' | tr '\n' ' ')
  if grep -q "could not compile" "$log"; then
    echo -e "$id\tCOMPILE-FAIL"
  elif [ "$rc" -eq 0 ]; then
    echo -e "$id\tSURVIVED (all tests pass)"
  else
    echo -e "$id\tKILLED: $failed"
  fi
  grep -E 'order .* for n = |Negative jacobian|patch test error' "$log" | sed -e 's/^/    /' | head -40
}
run M0 "" "" "$@"
run M1 crates/akantu-fe/src/quadrature.rs \
  "vec![1. / 6., 1. / 6., 1. / 6.],=>vec![1. / 6., 1. / 6., 1. / 5.]," "$@"
run M2 crates/akantu-fe/src/lagrange.rs \
  "(0, 4) = -xi * (1. - eta);=>(0, 4) = -xi * (1. + eta);|||(0, 6) = -xi * (1. + eta);=>(0, 6) = -xi * (1. - eta);" "$@"
run M3 crates/akantu-fe/src/lagrange.rs \
  "(1, 4) = -0.5 * (1. - xi * xi);=>(1, 4) = -0.5 * (1. - xi);" "$@"
