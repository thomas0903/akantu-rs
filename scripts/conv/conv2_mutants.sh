#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (port of Akantu, EPFL/LSMS); project tooling, no C++
# counterpart. Mutation check of the convergence studies of the
# diffusion model, the phase-field model and the solid / phase-field coupler
# (crates/akantu-diffusion/tests/spatial_convergence.rs,
# crates/akantu-phase-field/tests/spatial_convergence.rs,
# crates/akantu-couplers/tests/spatial_convergence_phase_field.rs): apply one
# source mutation to the library, run the studies together with the patch
# tests (the upstream heat-transfer patch tests, the studies' own linear
# patch tests) and the phase field's independent checks, record which tests
# fail and the order lines they report, restore the file (git checkout +
# touch: a restored file keeps an old mtime and cargo would keep the
# mutant).
#   M0  baseline (no mutation)
#   D1  conductivity tensor without its off-diagonal terms (heat_diffusion
#       updateInternalParameters keeps only C_ii; residual and K alike)
#   P1  phase field: the l0 factor of the gradient term dropped
#       (damage_energy = g_c I, AT2, and 3/4 g_c I, AT1)
#   C1  coupler strain transfer: eps = grad_u + grad_u^T (the 1/2 dropped,
#       phi four times too large)
#   M1  wrong quadrature weight: third weight of the 3-point triangle rule
#       1/6 -> 1/5 (as M1 of the first convergence study)
#   M2  wrong shape-function derivatives: quadrangle_8 dN_4/dxi and
#       dN_6/dxi swapped (as M2 of the first convergence study; node sums unchanged)
#   R1 capacity density rho*c -> rho (heat_diffusion rho(); it
#       survived the transient study while that used
#       rho = c = 1)
# Run from the repository root of a clean checkout; never commit the logs.
# Usage: scripts/conv/conv2_mutants.sh <log-dir> [ids, default all]
#        [cargo test args...]; e.g. `... <dir> D1,P1 --release`. The id list
#        is the second argument whenever cargo arguments follow: write
#        `... <dir> M0,D1,P1,C1,M1,M2,R1 --release` for all of them (an id
#        list starting with `-` is refused).
# Results: docs/NUMERICAL_VALIDATION.md.
set -u
out=${1:?usage: conv2_mutants.sh <log-dir> [ids] [cargo test args]}
ids=${2:-M0,D1,P1,C1,M1,M2,R1}
case $ids in
  -*)
    echo "conv2_mutants.sh: the second argument is the id list (e.g. M0,D1), not '$ids'" >&2
    exit 2
    ;;
esac
shift
[ $# -gt 0 ] && shift
mkdir -p "$out"
for d in crates/akantu-fe crates/akantu-diffusion crates/akantu-phase-field crates/akantu-couplers; do
  if ! git diff --quiet -- "$d"; then
    echo "$d has local changes: refusing to mutate it" >&2
    exit 2
  fi
done
run() { # id file 'from~~>to[|||from~~>to...]' [cargo test args] (`~~>`: the
  # sites contain Rust's `=>`)
  local id=$1 f=$2 subs=$3 log="$out/$1.log"
  shift 3
  case ",$ids," in *",$id,"*) ;; *) return ;; esac
  if [ -n "$f" ]; then
    python3 -I - "$f" "$subs" << 'PY' || { echo -e "$id\tNOT-APPLIED\t$f"; return; }
import sys
path, subs = sys.argv[1:3]
text = open(path).read()
# every site is checked on the original text, then all are replaced together
pairs = [s.split("~~>") for s in subs.split("|||")]
for old, _ in pairs:
    if text.count(old) != 1:
        sys.exit(f"{path}: {text.count(old)} occurrences of the mutation site {old!r}")
for i, (old, _) in enumerate(pairs):
    text = text.replace(old, f"@@MUTANT{i}@@")
for i, (_, new) in enumerate(pairs):
    text = text.replace(f"@@MUTANT{i}@@", new)
open(path, "w").write(text)
PY
  fi
  : > "$log"
  local rc=0
  cargo test -p akantu-diffusion --test spatial_convergence --test patch_test_linear_heat_transfer --no-fail-fast "$@" \
    -- --nocapture --test-threads=4 >> "$log" 2>&1 || rc=1
  cargo test -p akantu-phase-field --test spatial_convergence --test phase_field_checks --no-fail-fast "$@" \
    -- --nocapture --test-threads=4 >> "$log" 2>&1 || rc=1
  cargo test -p akantu-couplers --test spatial_convergence_phase_field --no-fail-fast "$@" \
    -- --nocapture --test-threads=4 >> "$log" 2>&1 || rc=1
  if [ -n "$f" ]; then
    git checkout -q -- "$f"
    touch "$f"
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
  grep -E 'order .* for n = |patch test error|panicked' "$log" | sed -e 's/^/    /' | head -60
}
run M0 "" "" "$@"
run D1 crates/akantu-diffusion/src/heat_diffusion.rs \
  "*c.at_mut(i, j) = self.conductivity.at(i, j);~~>*c.at_mut(i, j) = if i == j { self.conductivity.at(i, j) } else { 0.0 };" "$@"
run P1 crates/akantu-phase-field/src/phasefield.rs \
  "Some(f) => f * g * l0,~~>Some(f) => f * g,|||None => g * l0,~~>None => g," "$@"
run C1 crates/akantu-couplers/src/phase_field.rs \
  "sq[i + j * dim] = (gq[i + j * dim] + gq[j + i * dim]) / 2. - th * delta;~~>sq[i + j * dim] = (gq[i + j * dim] + gq[j + i * dim]) - th * delta;" "$@"
run M1 crates/akantu-fe/src/quadrature.rs \
  "vec![1. / 6., 1. / 6., 1. / 6.],~~>vec![1. / 6., 1. / 6., 1. / 5.]," "$@"
run M2 crates/akantu-fe/src/lagrange.rs \
  "(0, 4) = -xi * (1. - eta);~~>(0, 4) = -xi * (1. + eta);|||(0, 6) = -xi * (1. + eta);~~>(0, 6) = -xi * (1. - eta);" "$@"
run R1 crates/akantu-diffusion/src/heat_diffusion.rs \
  "    fn rho(&self) -> Real {
        self.density * self.capacity
    }~~>    fn rho(&self) -> Real {
        self.density
    }" "$@"
