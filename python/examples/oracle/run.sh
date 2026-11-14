#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs upstream's Python examples (python/examples,
# cases.py) with the oracle's upstream module and with the port, both in the
# pinned oracle image (akantu-oracle:dev: Debian trixie, CPython 3.13.5,
# NumPy 2.2.4, SciPy 1.15.3; D-050 pins: Eigen cache sizes compiled into the
# strict install, OPENBLAS_CORETYPE=Haswell, OPENBLAS_NUM_THREADS=1), and
# compares them (docs/TESTING.md).
#
# Usage (x86_64 Linux host with the oracle image, from a clone of the repository):
#   PORT=<extracted port wheel dir> python/examples/oracle/run.sh OUT [case ...]
# Env: ORACLE_RUNS (oracle runs per case, default 3, D-035), SIDES
#      ("oracle port", default both), PORT (directory with akantu/ and
#      py11_akantu*.so from the port's wheel).
# Writes OUT/oracle/<case>/run<N> (raw runs), OUT/fixtures/<case> (collected
# oracle runs: the content of fixtures/python/examples/<case>),
# OUT/port/<case> (raw port run), OUT/compare/<case>.txt and OUT/summary.txt.
set -uo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
out="${1:?usage: run.sh OUT [case ...]}"; shift
mkdir -p "$out"; out="$(cd "$out" && pwd)"
cases=("$@")
if [ ${#cases[@]} -eq 0 ]; then
  mapfile -t cases < <(python3 -c "import importlib.util as u; s=u.spec_from_file_location('c','$repo/python/examples/cases.py'); m=u.module_from_spec(s); s.loader.exec_module(m); print('\n'.join(m.CASES))")
fi
runs="${ORACLE_RUNS:-3}"
hpy="${HOST_PYTHON:-$HOME/pyenv/bin/python}"  # collect/compare (NumPy)
sides="${SIDES:-oracle port}"
img=akantu-oracle:dev
envs=(-e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 -e OMP_NUM_THREADS=1)
sum="$out/summary.txt"
{
  echo "image $(docker images $img --format '{{.ID}}')"
  echo "repo $(git -C "$repo" rev-parse HEAD)"
  echo "host $(uname -m) $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2)"
  echo "port ${PORT:-none} $(cat "${PORT:-/nonexistent}/../port.sha" 2>/dev/null)"
} >> "$sum"
for c in "${cases[@]}"; do
  t0=$(date +%s)
  if [[ " $sides " == *" oracle "* ]]; then
    works=()
    for r in $(seq 1 "$runs"); do
      w="$out/oracle/$c/run$r"; mkdir -p "$w"
      docker run --rm -i --memory 4g --init "${envs[@]}" -v akantu-oracle-build:/build:ro \
        -v "$repo:/src:ro" -v "$w:/w" -w /w $img bash -c \
        ". /build/install-strict/share/akantu-oracle/env.sh && python3 -u /src/python/examples/harness.py run $c /w --side oracle --repo /src > /w/stdout.txt 2> /w/stderr.txt; echo \$? > /w/exit_code"
      works+=("$w")
    done
    "$hpy" "$repo/python/examples/harness.py" collect "$out/fixtures/$c" "${works[@]}"
    echo "oracle $c runs=$runs exit=$(cat "$out/oracle/$c/run1/exit_code") nondet=$([ -f "$out/fixtures/$c/NONDETERMINISTIC" ] && tr '\n' ' ' < "$out/fixtures/$c/NONDETERMINISTIC" || echo none) $(grep '^status' "$out/oracle/$c/run1/status.txt" 2>/dev/null) t=$(( $(date +%s) - t0 ))s" >> "$sum"
  fi
  t1=$(date +%s)
  if [[ " $sides " == *" port "* ]]; then
    w="$out/port/$c"; mkdir -p "$w"
    docker run --rm -i --memory 4g --init "${envs[@]}" -v "${PORT:?set PORT}:/port:ro" \
      -v "$repo:/src:ro" -v "$w:/w" -w /w -e PYTHONPATH=/port $img bash -c \
      "python3 -u /src/python/examples/harness.py run $c /w --side port --repo /src > /w/stdout.txt 2> /w/stderr.txt; echo \$? > /w/exit_code"
    oc=$(python3 -c "import importlib.util as u; s=u.spec_from_file_location('c','$repo/python/examples/cases.py'); m=u.module_from_spec(s); s.loader.exec_module(m); print(m.CASES['$c']['oracle'] or '$c')")
    mkdir -p "$out/compare"
    if [ -d "$out/fixtures/$oc" ]; then
      "$hpy" "$repo/python/examples/harness.py" compare "$out/fixtures/$oc" "$w" --oracle-work "$out/oracle/$oc/run1" --case "$c" > "$out/compare/$c.txt" 2>&1
      rc=$?
    else
      echo "no oracle fixture $oc" > "$out/compare/$c.txt"; rc=2
    fi
    echo "port $c exit=$(cat "$w/exit_code") $(grep '^status' "$w/status.txt" 2>/dev/null) vs=$oc compare_rc=$rc t=$(( $(date +%s) - t1 ))s :: $(head -c 300 "$out/compare/$c.txt" | tr '\n' '|')" >> "$sum"
  fi
done
df -h / | tail -1 >> "$sum"
echo done >> "$sum"
