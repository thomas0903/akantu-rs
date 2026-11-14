#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: oracle replays of the Python tests: the upstream
# module (oracle image `akantu-oracle:dev`, strict install) against the port
# (a Linux abi3 wheel, run in the same image without the upstream
# PYTHONPATH, so both sides share Debian trixie, Python and NumPy):
#   1. the five snippets (python/oracle/snippets, compared with
#      compare.py at relative tolerance 0), and the snippets (inputs
#      from python/oracle/inputs.txt, one fresh directory per snippet and
#      side; expected differences: bi_material, selector_temporary —
#      docs/TESTING.md),
#   2. the adversarial programs python/oracle/programs/cc1_*.py and
#      py_p10_*.py (upstream API only), unchanged,
#   3. upstream examples/python/solid_mechanics_model/dynamics/dynamics.py.
# Usage (on an x86_64 Linux host with the oracle image, see
# docs/ORACLE_BUILD.md):
#   WHEEL=~/akantu-....whl AKANTU_SRC_HOST=<akantu-src> python/oracle/replay-in-oracle-image.sh
# Env: WHEEL (port wheel, required), AKANTU_SRC_HOST (reference checkout,
#      required), OUT (default ~/oracle-replay).
set -u
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WHEEL="${WHEEL:?set WHEEL to the port wheel}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
SRC="$AKANTU_SRC_HOST"
W="${OUT:-$HOME/oracle-replay}"; rm -rf "$W"; mkdir -p "$W/port"; cd "$W/port"
python3 -c "import zipfile,sys; zipfile.ZipFile(sys.argv[1]).extractall('.')" "$WHEEL"
cd "$repo"
IMG=akantu-oracle:dev
SUM=$W/summary.txt; : > $SUM
oracle() { docker run --rm -i --memory 4g --init -v akantu-oracle-build:/build:ro -v "$1:/work" -w /work $IMG bash -c ". /build/install-strict/share/akantu-oracle/env.sh && $2"; }
port()   { docker run --rm -i --memory 4g --init -v "$W/port:/port:ro" -v "$1:/work" -w /work -e PYTHONPATH=/port $IMG bash -c "$2"; }
echo "image $(docker images $IMG --format '{{.ID}}')" >> $SUM
port $W "python3 -c 'import akantu, numpy, scipy; print(akantu.__file__, numpy.__version__, scipy.__version__)'" >> $SUM 2>&1
oracle $W "python3 -c 'import akantu; print(akantu.__file__)'" >> $SUM 2>&1
# 1. the five snippets
D=$W/snippets; mkdir -p $D; cp python/oracle/snippets/*.py $D/
cp fixtures/meshes/gmsh-4.13.1/test/test_python_interface/mesh_dcb_2d.msh crates/akantu-parser/tests/fixtures/test/test_python_interface/elastic.dat crates/akantu-parser/tests/fixtures/test/test_io/test_parser/input_file.dat $D/
for s in mesh_read parser_values fe_engine smm_static smm_explicit; do
  oracle $D "python3 $s.py" > $D/$s.oracle 2> $D/$s.oracle.err
  port $D "python3 $s.py" > $D/$s.port 2> $D/$s.port.err
  python3 python/oracle/compare.py $D/$s.oracle $D/$s.port 0 > $D/$s.cmp 2>&1
  echo "snippet $s compare_rc=$? oracle_lines=$(wc -l < $D/$s.oracle) port_lines=$(wc -l < $D/$s.port)" >> $SUM
done
# 1b. the snippets
for s in solver_layer custom_material bi_material bi_material_eigen laws_api selector_temporary dumpable cohesive other_models phase_field contact_detection contact_coupler fe_engine_ops; do
  for side in o p; do
    E=$W/py1/$s-$side; mkdir -p $E; cp python/oracle/snippets/*.py $E/
    while read -r name path; do
      case "$name" in ''|'#'*) continue ;; esac
      cp "$path" "$E/$name"
    done < python/oracle/inputs.txt
  done
  oracle $W/py1/$s-o "python3 $s.py" > $W/py1/$s.oracle 2> $W/py1/$s.oracle.err
  port $W/py1/$s-p "python3 $s.py" > $W/py1/$s.port 2> $W/py1/$s.port.err
  python3 python/oracle/compare.py $W/py1/$s.oracle $W/py1/$s.port 0 > $W/py1/$s.cmp 2>&1
  echo "snippet $s compare_rc=$? oracle_lines=$(wc -l < $W/py1/$s.oracle) port_lines=$(wc -l < $W/py1/$s.port)" >> $SUM
done
# 2. the adversarial programs, unchanged, on both modules
C=$W/cc1; mkdir -p $C; cp python/oracle/programs/*.py $C/
cp fixtures/meshes/gmsh-4.13.1/test/test_python_interface/mesh_dcb_2d.msh crates/akantu-parser/tests/fixtures/test/test_python_interface/elastic.dat $C/
for p in cc1_p1_inplace_writes cc1_p2_callback_writes cc1_p3_callback_reads cc1_p4_current_position cc1_p5_mesh_write_after_model cc1_p6_thread_write_during_callback cc1_p8_param_set_reverse_stale py_p10_view_outlives_owner; do
  oracle $C "python3 $p.py" > $C/$p.oracle 2>&1
  port $C "python3 $p.py" > $C/$p.port 2>&1
  if cmp -s $C/$p.oracle $C/$p.port; then r=identical; else r=DIFFER; fi
  echo "program $p $r | oracle: $(grep -c ' PASS' $C/$p.oracle) PASS $(grep -c ' FAIL' $C/$p.oracle) FAIL | port: $(grep -c ' PASS' $C/$p.port) PASS $(grep -c ' FAIL' $C/$p.port) FAIL" >> $SUM
done
# 3. upstream example dynamics.py
for side in oracle port; do
  E=$W/dyn-$side; mkdir -p $E
  cp "$SRC"/examples/python/solid_mechanics_model/dynamics/dynamics.py "$SRC"/examples/python/solid_mechanics_model/dynamics/material.dat $E/
  cp fixtures/meshes/gmsh-4.13.1/examples/python/solid_mechanics_model/dynamics/bar.msh $E/
  $side $E "python3 dynamics.py" > $E/out.txt 2> $E/err.txt
  echo "dynamics $side rc=$? lines=$(wc -l < $E/out.txt) last=$(tail -1 $E/out.txt)" >> $SUM
done
if cmp -s $W/dyn-oracle/out.txt $W/dyn-port/out.txt; then echo "dynamics output identical (bitwise text)" >> $SUM; else echo "dynamics output DIFFER: $(diff $W/dyn-oracle/out.txt $W/dyn-port/out.txt | grep -c '^<') lines" >> $SUM; fi
tar -C $W -czf "$W.tgz" .
echo done >> $SUM
