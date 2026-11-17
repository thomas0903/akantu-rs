#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): builds every binary of
# every variant of the final measurement campaign once, before any timing
# (scripts/perf/final/README.md). Run it from a checkout of the campaign
# scripts (this branch or later), alone on the machine; nothing is timed
# while it runs.
#
#   scripts/perf/final/prepare.sh --before REV|DIR --head REV|DIR [options]
#
#   --before, --head  a git revision of this repository (built here into
#                     FINAL_ROOT/bin/<variant>) or a directory prepared by an
#                     earlier prepare.sh (FINAL_ROOT/bin/<variant> is then a
#                     link to it; its VARIANT.json is kept)
#   --root DIR        FINAL_ROOT (default ~/final)
#   --only LIST       comma list of steps (default all): cpp, meshes, raw,
#                     host, mpi, python
#   --joss-mesh FILE  the JOSS cube.msh (part G): checked against
#                     benchmarks/cohesive-perf/meshes/cube-joss.json and copied
#                     into the harness volume as /perf/meshes/cube-joss.msh
#   --keep-targets    keep the cargo target directories (default: removed
#                     after the binaries are copied, to save disk)
#
# Steps:
#   cpp     the C++ side: the oracle installs (volume akantu-oracle-build,
#           READY), the cohesive harness builds strict and default (volume
#           akantu-perf-cohesive, benchmarks/cohesive-perf/scripts/build.sh if
#           missing), the explicit-step harness (FINAL_BASELINE/cpp-bin or
#           scripts/perf/explicit-step/cpp_build.sh) -> FINAL_ROOT/bin/cpp
#   meshes  the explicit-step meshes (FINAL_BASELINE/meshes/explicit, SHA-256
#           checked) and the cohesive meshes h = 0.0925, 0.0756, 0.06, 0.0476,
#           0.04 in the harness volume (copied from FINAL_BASELINE or made by
#           gen-meshes.sh; SHA-256 checked against the recorded statistics)
#   raw     scripts/mpi/bench_baseline.c with mpicc in akantu-rs-mpi:dev
#   host    per variant (release profile, default flags): explicit_step_timing,
#           suite_solid, suite_models, suite_setup
#   mpi     per variant, in akantu-rs-mumps:dev: cohesive_extrinsic and
#           suite_solid with mpi,scotch,mumps; suite_solid, suite_models,
#           suite_setup with mpi,scotch; in akantu-rs-mpi:dev: the akantu-comm
#           example mpi_bench (mpi); the drivers are also copied into the
#           volume akantu-rs-mpi-target at /target/final/<variant>/
#   python  per variant: a virtual environment with maturin, numpy, scipy (the
#           second variant installs the versions of the first) and the akantu
#           extension (maturin develop --release)
# Every variant gets VARIANT.json (revision, toolchain, image ids, binary
# digests, whether its SparseLU factorisation uses threads).
set -euo pipefail
# shellcheck source=lib.sh
. "$(dirname "$0")/lib.sh"

before="" head="" only="cpp,meshes,raw,host,mpi,python" joss="" keep=0
while [ $# -gt 0 ]; do
  case "$1" in
    --before) before=${2:?}; shift 2 ;;
    --head) head=${2:?}; shift 2 ;;
    --root) FINAL_ROOT=${2:?}; shift 2 ;;
    --only) only=${2:?}; shift 2 ;;
    --joss-mesh) joss=${2:?}; shift 2 ;;
    --keep-targets) keep=1; shift ;;
    -h|--help) sed -n '6,44p' "$0"; exit 0 ;;
    *) fin_die "prepare.sh: unknown argument $1" ;;
  esac
done
FIN_BIN="${FINAL_ROOT}/bin"
mkdir -p "${FIN_BIN}" "${FINAL_ROOT}/src" "${FINAL_ROOT}/meshes"
log="${FINAL_ROOT}/prepare.log"
say() { echo "$(fin_utc) $*" | tee -a "$log"; }
trap 'say "prepare: FAILED at line ${LINENO} (see ${log})"' ERR
want() { case ",${only}," in *",$1,"*) return 0 ;; *) return 1 ;; esac; }
ncpu=$(nproc)
memgb=$(( $(awk '/MemTotal/ {print $2}' /proc/meminfo) / 1048576 ))
dmem="$(( memgb > 8 ? memgb - 4 : memgb ))g"

# ---------------------------------------------------------------- C++ side
prepare_cpp() {
  local c d="${FIN_BIN}/cpp"
  mkdir -p "$d"
  for c in strict default; do
    docker run --rm -v akantu-oracle-build:/build:ro "${FIN_ORACLE_IMAGE}" test -f "/build/install-${c}/READY" \
      || fin_die "the oracle install ${c} is not READY (docs/ORACLE_BUILD.md; volume akantu-oracle-build)"
    if ! docker run --rm -v akantu-perf-cohesive:/perf:ro "${FIN_ORACLE_IMAGE}" test -x "/perf/builds/${c}/cohesive_extrinsic"; then
      say "cpp: building the cohesive harness (${c})"
      (cd "${FIN_REPO}" && bash benchmarks/cohesive-perf/scripts/build.sh "${c}") >> "$log" 2>&1
    fi
    if [ ! -x "$d/explicit_step_timing_${c}" ]; then
      if [ -x "${FIN_BASELINE}/cpp-bin/explicit_step_timing_${c}" ]; then
        cp "${FIN_BASELINE}/cpp-bin/explicit_step_timing_${c}" "$d/"
      else
        say "cpp: building the explicit-step harness (${c})"
        "${FIN_REPO}/scripts/perf/explicit-step/cpp_build.sh" "$d" "${c}" >> "$log" 2>&1
      fi
    fi
  done
  python3 -I - "$d" "$(docker image inspect --format '{{.Id}}' "${FIN_ORACLE_IMAGE}")" \
    "$(docker run --rm -v akantu-perf-cohesive:/perf:ro "${FIN_ORACLE_IMAGE}" sh -c 'sha256sum /perf/builds/strict/cohesive_extrinsic /perf/builds/default/cohesive_extrinsic' | awk '{print $1}' | paste -sd,)" \
    "$(docker run --rm -v akantu-oracle-build:/build:ro "${FIN_ORACLE_IMAGE}" sh -c 'sha256sum /build/install-strict/lib/libakantu.so.*.*.* /build/install-default/lib/libakantu.so.*.*.* 2>/dev/null' | awk '{print $1}' | paste -sd,)" \
    "${FIN_CPP_REF}" <<'PY'
import hashlib, json, sys
d, image, coh, lib, ref = sys.argv[1:6]
coh = (coh.split(",") + ["", ""])[:2]
lib = (lib.split(",") + ["", ""])[:2]
def sha(p):
    return hashlib.sha256(open(p, "rb").read()).hexdigest()
json.dump({"variant": "cpp", "reference": ref, "oracle_image": image,
           "sha256": {"explicit_step_timing_strict": sha(d + "/explicit_step_timing_strict"),
                      "explicit_step_timing_default": sha(d + "/explicit_step_timing_default"),
                      "cohesive_extrinsic_strict": coh[0], "cohesive_extrinsic_default": coh[1],
                      "libakantu_strict": lib[0], "libakantu_default": lib[1]}},
          open(d + "/CPP.json", "w"), indent=1)
PY
  say "cpp: ready ($d/CPP.json)"
}

# ---------------------------------------------------------------- meshes
prepare_meshes() {
  local m e="${FINAL_ROOT}/meshes/explicit" h sha want_sha
  mkdir -p "$e"
  for m in tet4-20k tet4-200k tet10-20k hex8-20k hex8-200k; do
    [ -f "$e/$m.msh" ] || cp "${FIN_BASELINE}/meshes/explicit/$m.msh" "$e/" 2> /dev/null \
      || "${FIN_REPO}/scripts/perf/explicit-step/make_meshes.sh" "$e" "$m" >> "$log" 2>&1
  done
  # the recorded digests of the meshes (gmsh 4.12.1): another gmsh gives another mesh
  (cd "$e" && grep -E ' (tet4-20k|tet4-200k|tet10-20k|hex8-20k|hex8-200k)\.msh$' \
    "${FIN_REPO}/docs/benchmarks/explicit-step/final/meshes-SHA256SUMS" | sha256sum -c --quiet) \
    || fin_die "explicit-step meshes differ from docs/benchmarks/explicit-step/final/meshes-SHA256SUMS"
  for h in 0.0925 0.0756 0.06 0.0476 0.04; do
    want_sha=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha256"])' \
      "${FIN_REPO}/benchmarks/cohesive-perf/meshes/cube-h${h}.x86_64.json")
    sha=$(docker run --rm -v akantu-perf-cohesive:/perf:ro "${FIN_ORACLE_IMAGE}" sh -c "sha256sum /perf/meshes/cube-h${h}.msh 2>/dev/null || true" | cut -d' ' -f1)
    if [ "$sha" != "$want_sha" ]; then
      if [ -f "${FIN_BASELINE}/meshes/cohesive/cube-h${h}.msh" ]; then
        docker run --rm -i -v akantu-perf-cohesive:/perf "${FIN_ORACLE_IMAGE}" sh -c "cat > /perf/meshes/cube-h${h}.msh" \
          < "${FIN_BASELINE}/meshes/cohesive/cube-h${h}.msh"
      else
        say "meshes: generating h = ${h} (gen-meshes.sh, Gmsh of the oracle image)"
        local before_files
        before_files=$(git -C "${FIN_REPO}" ls-files --others --exclude-standard)
        (cd "${FIN_REPO}" && bash benchmarks/cohesive-perf/scripts/gen-meshes.sh "${h}") >> "$log" 2>&1
        # gen-meshes.sh records new statistics and freezes small meshes in the
        # checkout: the campaign needs only the mesh in the volume, and the
        # checkout must stay clean (its state is recorded with every run)
        git -C "${FIN_REPO}" ls-files --others --exclude-standard | grep -vxF "${before_files:-/}" \
          | (cd "${FIN_REPO}" && xargs -r rm -f --)
      fi
      sha=$(docker run --rm -v akantu-perf-cohesive:/perf:ro "${FIN_ORACLE_IMAGE}" sha256sum "/perf/meshes/cube-h${h}.msh" | cut -d' ' -f1)
      [ "$sha" = "$want_sha" ] || fin_die "mesh h = ${h}: SHA-256 ${sha} differs from the recorded ${want_sha}"
    fi
  done
  if [ -n "$joss" ]; then
    want_sha=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha256"])' \
      "${FIN_REPO}/benchmarks/cohesive-perf/meshes/cube-joss.json")
    [ "$(sha256sum "$joss" | cut -d' ' -f1)" = "$want_sha" ] || fin_die "--joss-mesh: SHA-256 differs from cube-joss.json"
    docker run --rm -i -v akantu-perf-cohesive:/perf "${FIN_ORACLE_IMAGE}" sh -c "cat > /perf/meshes/cube-joss.msh" < "$joss"
  fi
  say "meshes: ready"
}

# ---------------------------------------------------------------- raw MPI baseline
prepare_raw() {
  mkdir -p "${FIN_BIN}/raw"
  docker run --rm -u "$(id -u):$(id -g)" -v "${FIN_REPO}/scripts/mpi:/s:ro" -v "${FIN_BIN}/raw:/out" akantu-rs-mpi:dev \
    sh -c 'mpicc -O2 /s/bench_baseline.c -o /out/bench_baseline && mpicc --version | head -1 > /out/mpicc-version.txt'
  say "raw: ${FIN_BIN}/raw/bench_baseline"
}

# ---------------------------------------------------------------- port variants
copy_from_volume() { # path-in-volume host-dir [name]
  mkdir -p "$2"
  docker run --rm -u "$(id -u):$(id -g)" -v "${FIN_TVOL}:/target:ro" -v "$2:/out" "${FIN_MPI_IMAGE}" \
    cp "/target/$1" "/out/${3:-$(basename "$1")}"
}

variant_src() { # variant rev -> FINAL_ROOT/src/<variant> at rev
  local v=$1 rev=$2 s="${FINAL_ROOT}/src/$1"
  git -C "${FIN_REPO}" rev-parse --verify -q "${rev}^{commit}" > /dev/null || fin_die "${v}: unknown revision ${rev}"
  if [ ! -e "$s/.git" ]; then git -C "${FIN_REPO}" worktree add -q --detach "$s" "$rev"; fi
  git -C "$s" checkout -q --detach "$rev"
  echo "$s"
}

build_host() { # variant src
  local v=$1 s=$2 d="${FIN_BIN}/$1" t="${FINAL_ROOT}/target/$1"
  mkdir -p "$d"
  say "host: ${v}: cargo build --release (explicit_step_timing, suites)"
  (cd "$s" && CARGO_TARGET_DIR="$t" cargo build --release --locked -p akantu-solid --example explicit_step_timing \
    && CARGO_TARGET_DIR="$t" cargo build --release --locked -p akantu-bench --bin suite_solid --bin suite_models --bin suite_setup) >> "$log" 2>&1
  cp "$t/release/examples/explicit_step_timing" "$t/release/suite_solid" "$t/release/suite_models" "$t/release/suite_setup" "$d/"
}

build_mpi() { # variant src rev
  local v=$1 s=$2 rev=$3 d="${FIN_BIN}/$1/mpi" t="/target/final-build/$1" short
  short=$(git -C "$s" rev-parse --short=9 HEAD)
  mkdir -p "$d"
  say "mpi: ${v}: cohesive_extrinsic, suite_solid (mpi,scotch,mumps)"
  (AKANTU_RS_MPI_IMAGE="${FIN_MPI_IMAGE}" AKANTU_RS_MPI_TARGET="$t" AKANTU_RS_DOCKER_CPUS="${ncpu}" \
    AKANTU_RS_DOCKER_MEMORY="${dmem}" AKANTU_RS_GIT_HEAD="${short}" \
    "$s/scripts/mpi/in-docker.sh" cargo build --locked --release -p akantu-bench --bin cohesive_extrinsic --bin suite_solid \
    --features mpi,scotch,mumps) >> "$log" 2>&1
  copy_from_volume "final-build/$v/release/cohesive_extrinsic" "$d"
  copy_from_volume "final-build/$v/release/suite_solid" "$d" suite_solid_mumps
  docker run --rm -u 0 -v "${FIN_TVOL}:/target" "${FIN_MPI_IMAGE}" sh -c \
    "mkdir -p /target/final/$v && cp $t/release/cohesive_extrinsic /target/final/$v/ && chmod -R a+rX /target/final"
  say "mpi: ${v}: suites (mpi,scotch)"
  (AKANTU_RS_MPI_IMAGE="${FIN_MPI_IMAGE}" AKANTU_RS_MPI_TARGET="$t" AKANTU_RS_DOCKER_CPUS="${ncpu}" \
    AKANTU_RS_DOCKER_MEMORY="${dmem}" AKANTU_RS_GIT_HEAD="${short}" \
    "$s/scripts/mpi/in-docker.sh" cargo build --locked --release -p akantu-bench --bin suite_solid --bin suite_models \
    --bin suite_setup --features mpi,scotch) >> "$log" 2>&1
  copy_from_volume "final-build/$v/release/suite_solid" "$d"
  copy_from_volume "final-build/$v/release/suite_models" "$d"
  copy_from_volume "final-build/$v/release/suite_setup" "$d"
  say "mpi: ${v}: mpi_bench (akantu-rs-mpi:dev)"
  (AKANTU_RS_MPI_IMAGE=akantu-rs-mpi:dev AKANTU_RS_MPI_TARGET="${t}-mpi" AKANTU_RS_DOCKER_CPUS="${ncpu}" \
    AKANTU_RS_DOCKER_MEMORY="${dmem}" AKANTU_RS_GIT_HEAD="${short}" \
    "$s/scripts/mpi/in-docker.sh" cargo build --locked --release -p akantu-comm --features mpi --example mpi_bench) >> "$log" 2>&1
  copy_from_volume "final-build/$v-mpi/release/examples/mpi_bench" "$d"
  if [ "$keep" = 0 ]; then
    docker run --rm -u 0 -v "${FIN_TVOL}:/target" "${FIN_MPI_IMAGE}" rm -rf "$t" "${t}-mpi"
  fi
}

build_python() { # variant src
  local v=$1 s=$2 venv="${FIN_BIN}/$1/venv" pins="${FIN_BIN}/python-constraints.txt"
  say "python: ${v}: venv and extension (maturin develop --release)"
  if [ ! -x "$venv/bin/maturin" ]; then
    rm -rf "$venv"; python3 -m venv "$venv"
    if [ -f "$pins" ]; then
      "$venv/bin/python" -m pip install -q -c "$pins" maturin numpy scipy >> "$log" 2>&1
    else
      "$venv/bin/python" -m pip install -q maturin numpy scipy >> "$log" 2>&1
      "$venv/bin/python" -m pip freeze | grep -iE '^(maturin|numpy|scipy)==' > "$pins"
    fi
  fi
  (cd "$s/python" && VIRTUAL_ENV="$venv" PATH="$venv/bin:$PATH" CARGO_TARGET_DIR="${FINAL_ROOT}/target/$v-py" \
    maturin develop --release -q) >> "$log" 2>&1
  "$venv/bin/python" -c 'import akantu' || fin_die "python: ${v}: import akantu failed"
  # the variant's own Python suite (its case list is the variant's)
  cp "$s/python/benchmarks/suite_python.py" "${FIN_BIN}/$v/suite_python.py"
}

write_variant_json() { # variant src
  local v=$1 s=$2 d="${FIN_BIN}/$1" threaded=false
  # a threaded SparseLU factorisation runs the executor's parallel loops in
  # the translation's own source (the fast-mode token alone does not count)
  if grep -rqE '\.(for_each|for_each_bounds)\(|\.threads\(\)' "$s/crates/akantu-linalg/src/eigen_sparse_lu/" 2> /dev/null; then
    threaded=true
  fi
  (cd "$s" && python3 -I - "$d" "$v" "$(git rev-parse HEAD)" "$(git describe --always --tags 2> /dev/null)" \
    "$(rustc --version)" "$(cargo --version)" "${threaded}" \
    "$(docker image inspect --format '{{.Id}}' "${FIN_MPI_IMAGE}")" \
    "$(docker image inspect --format '{{.Id}}' akantu-rs-mpi:dev)" <<'PY'
import hashlib, json, os, sys
d, v, rev, desc, rustc, cargo, threaded, img_mumps, img_mpi = sys.argv[1:10]
sha = {}
for root, _, files in os.walk(d):
    if "/venv" in root:
        continue
    for f in sorted(files):
        p = os.path.join(root, f)
        if f != "VARIANT.json" and os.access(p, os.X_OK) and not os.path.islink(p):
            sha[os.path.relpath(p, d)] = hashlib.sha256(open(p, "rb").read()).hexdigest()
py = os.path.join(d, "venv", "bin", "python")
json.dump({"variant": v, "revision": rev, "describe": desc, "rustc": rustc, "cargo": cargo,
           "profile": "release (workspace profile, default flags, no target-cpu)",
           "host_features": "", "mpi_features": "mpi,scotch,mumps (cohesive_extrinsic, suite_solid_mumps); mpi,scotch (suites); mpi (mpi_bench)",
           "images": {"mumps": img_mumps, "mpi": img_mpi},
           "sparselu_threaded": threaded == "true", "python": os.path.exists(py),
           "sha256": sha}, open(os.path.join(d, "VARIANT.json"), "w"), indent=1)
PY
  )
}

prepare_variant() { # variant rev|dir
  local v=$1 spec=$2 s
  if [ -d "$spec" ] && [ -f "$spec/VARIANT.json" ]; then
    [ "$(cd "$spec" && pwd)" = "$(cd "${FIN_BIN}" && pwd)/$v" ] || ln -sfn "$(cd "$spec" && pwd)" "${FIN_BIN}/$v"
    say "${v}: prebuilt directory $spec"
    return 0
  fi
  s=$(variant_src "$v" "$spec")
  want host && build_host "$v" "$s"
  want mpi && build_mpi "$v" "$s" "$spec"
  want python && build_python "$v" "$s"
  write_variant_json "$v" "$s"
  if [ "$keep" = 0 ] && want host; then rm -rf "${FINAL_ROOT}/target/$v"; fi
  if [ "$keep" = 0 ] && want python; then rm -rf "${FINAL_ROOT}/target/$v-py"; fi
  say "${v}: ready (${FIN_BIN}/$v/VARIANT.json)"
}

say "prepare: start (root ${FINAL_ROOT}, steps ${only})"
want cpp && prepare_cpp
want meshes && prepare_meshes
want raw && prepare_raw
[ -z "$before" ] || prepare_variant before "$before"
[ -z "$head" ] || prepare_variant head "$head"
df -h / | tail -1 | tee -a "$log"
say "prepare: done"
