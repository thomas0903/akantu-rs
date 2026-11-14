#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: adversarial phase-field oracle cases: the only
# law x irreversibility x split combination of PF-013/014/016/017 without an
# oracle case, linear (AT1) + history + volumetric_deviatoric, on the 1-D, 2-D
# and 3-D patch meshes, generated with the D-050 pin (a scratch clone
# patched by pin_probes.py; the law variant is now part of
# oracle/probes/phase_field/phase_field_probe.cc), then replayed by the port's own
# phase_field_fixtures_bitwise test binary (gate build, unchanged source)
# through AKANTU_FIXTURES_DIR on a 39-case root (36 committed + 3 new).
set -u
export AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the reference checkout}"
P=~/va4-pin/oracle/probes/phase_field/phase_field_probe.cc
python3 - "$P" <<'PY'
import sys
p = sys.argv[1]; s = open(p).read()
old = '      {"quadratic_tao_random_gc", "quadratic", "", "tao", "5e-3 uniform [0, 1e-3]"},\n'
assert s.count(old) == 1
s = s.replace(old, old + '      {"linear_voldev_history", "linear", "volumetric_deviatoric", "history", "5e-3"},\n')
for a, b in [("{_segment_2, {0, 2}}", "{_segment_2, {0, 2, 5}}"),
             ("{_triangle_3, {0, 1, 2, 3, 4}}", "{_triangle_3, {0, 1, 2, 3, 4, 5}}"),
             ("{_hexahedron_8, {1, 4}}", "{_hexahedron_8, {1, 4, 5}}")]:
    assert s.count(a) == 1, a
    s = s.replace(a, b)
open(p, "w").write(s)
print("patched", p)
PY
NEW=patch_segment_2_linear_voldev_history,patch_triangle_3_linear_voldev_history,patch_hexahedron_8_linear_voldev_history
rm -rf ~/rg/pf_adv
( cd ~/va4-pin && PF_PROBE_CASES=$NEW PF_PROBE_STAGE_ONLY=1 oracle/probes/phase_field/generate.sh --out ~/rg/pf_adv ) > ~/rg/pf_adv.log 2>&1
echo "pf_adv rc=$?" > ~/rg/adv.rc
# scratch fixture root: 36 committed cases + the 3 new ones (the replay asserts 39 cases;
# the 3 dropped cases are plain patch cases that exercise no corrected switch)
R=~/va4-adv-fixtures; rm -rf $R; mkdir -p $R/model
cp -r ~/akantu-rs/fixtures/model/phase_field $R/model/
for c in patch_segment_2_quadratic_history patch_tetrahedron_10_linear_history patch_quadrangle_8_linear_history; do rm -rf $R/model/phase_field/$c; done
cp -r ~/rg/pf_adv/model/phase_field/* $R/model/phase_field/
ls $R/model/phase_field | grep -c . >> ~/rg/adv.rc
cd ~/akantu-rs && docker run --rm --memory 6g --memory-swap 6g --cpus 4 --shm-size 512m \
  -v "$HOME/akantu-rs:/src:ro" -v akantu-rs-mpi-target:/target -v akantu-rs-mpi-cargo:/usr/local/cargo/registry \
  -v "$R:/advfix:ro" -e AKANTU_FIXTURES_DIR=/advfix -e CARGO_TARGET_DIR=/target -w /src akantu-rs-mpi:dev \
  cargo test -p akantu-phase-field --test phase_field_fixtures --locked -- --nocapture > ~/rg/adv_replay.log 2>&1
echo "replay rc=$?" >> ~/rg/adv.rc
echo ALLDONE >> ~/rg/adv.rc
