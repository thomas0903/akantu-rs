#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: move the two benchmark images (and the full-size
# mesh) to another x86_64 Linux host with Docker (the full-size session:
# c1_session.sh).
#
#   JOSS_MESH=/path/to/cube.msh bash crates/akantu-bench/scripts/c1_images.sh export DIR  # build host
#   bash c1_images.sh smoke DIR                                                         # new host (tarballs only)
#   bash crates/akantu-bench/scripts/c1_images.sh export-mn DIR                          # multi-node set 
#   JOSS_MESH=/path/to/cube.msh bash crates/akantu-bench/scripts/c1_images.sh export-both DIR  # both, one build
#
# Every export first builds the driver from the checkout's HEAD (clean tree
# required): `scripts/mpi/in-docker.sh cargo build --release --locked -p
# akantu-bench --features mpi,scotch,mumps --bin cohesive_extrinsic` in the
# akantu-rs-mumps:dev image, target /target/c1 of the akantu-rs-mpi-target
# volume (the exported binary is the one of the recorded
# `git_head`, not whatever the volume held). `export-both DIR` builds once and
# writes the full-size (C1) set into DIR/c1 and the multi-node set into
# DIR/mn from that one binary, then checks that the two images.txt record the
# same rust_bin_sha256 (DIR/rust_bin_sha256.txt). env: C1_BUILD_CPUS (6),
# C1_BUILD_MEMORY (12g) of the build container.
#
# export writes into DIR (nothing is uploaded anywhere):
#   akantu-oracle-dev.tar.gz      docker save of akantu-oracle:dev (C++ oracle image, amd64, D-050 pins)
#   akantu-oracle-installs.tar.gz install-strict/ and install-default/ of the oracle build volume
#                                 (the C++ harness compiles and links against them; build trees omitted)
#   akantu-rs-bench-c1.tar.gz     docker save of akantu-rs-bench:c1 = akantu-rs-mumps:dev + the release
#                                 driver (docker/bench/Dockerfile)
#   akantu-rs-c1-repo.tar.gz      git archive of HEAD: benchmarks/cohesive-perf (harness, inputs,
#                                 scripts; without results/), fixtures/perf_cohesive/meshes (frozen
#                                 h = 0.12 mesh), crates/akantu-bench/scripts (campaign tool, session
#                                 script, this script), docker/bench, and C1_REVISION (the commit id)
#   akantu-c1-mesh-joss.tar.gz    cube-joss.msh (the JOSS mesh: JOSS_MESH, checked against the SHA-256
#                                 of benchmarks/cohesive-perf/meshes/cube-joss.json) + cube-joss.json
#   images.txt, SHA256SUMS, sizes.txt
#   (NO_MESH=1 skips the mesh tarball; the session then needs --mesh-tarball.)
# smoke (one command on the new host; DIR holds the files above; needs only
# docker, tar, gzip, sha256sum, python3, flock): checks SHA256SUMS, loads
# both images and checks their ids against images.txt, creates the oracle
# build volume from the installs tarball, extracts the repository subset,
# builds the C++ harness (strict install) and runs, with the campaign tool
# of the subset (c1prep_campaign.sh, the tool of the full-size runs), the
# h = 0.12 problem (material-cohesive.dat, blocked_node = global, 1500 steps)
# on 2 ranks with both codes on the same partition (the C++ harness exports
# it, the Rust run replays it). Verdict "SMOKE: PASS" when the campaign tool
# exits 0 (both runs `ok`, the C++ run re-created the exported partition),
# the Rust static solve ran with MUMPS on 2 processes, and both report 404
# cohesive elements in 294 insertion steps from step 710 (the counts of
# fixtures/perf_cohesive*/coh_np2_global). Energies of steps 0-800 (before
# the first opening at step 839) and the first differing insertion step are
# printed, not judged: at 2 ranks the energies vary from run to run with
# MUMPS's parallel factorisation (UD-348), and the
# insertion steps of upstream's distributed run leave the serial list once
# its insertion has corrupted ghost boundary facets (UD-890).
# env (smoke): ORACLE_VOLUME (c1-oracle-build), PERF_VOLUME (c1-perf),
#   RUST_VOLUME (c1-rust), WORK (DIR/smoke-<UTC stamp>), SMOKE_MEMORY (8g),
#   SMOKE_CPUS (2); the volumes must not exist yet or be empty.
# env (export): ORACLE_VOLUME (akantu-oracle-build), JOSS_MESH (required
#   unless NO_MESH=1).
# export-mn (docs/TESTING.md) writes
# into DIR the set of the multi-node session (no oracle image: the session
# runs the Rust driver only):
#   akantu-rs-bench-mn.tar.gz     docker save of akantu-rs-bench:mn = akantu-rs-bench:c1 (built as by
#                                 export, same binary) + SSH for Open MPI's remote launch
#                                 (docker/bench/Dockerfile.mn)
#   akantu-rs-mn-repo.tar.gz      git archive of HEAD: crates/akantu-bench/scripts (launcher
#                                 mn_cluster.py, session mn_session.*, test mn_e2e.sh, compare
#                                 mn_compare.py, plans), docker/bench, benchmarks/cohesive-perf/inputs
#                                 and meshes/*.json, fixtures/perf_cohesive/meshes, and MN_REVISION
#   akantu-mn-meshes-test.tar.gz  cube-h<H>.msh + .json for H in MN_TEST_MESHES (0.06): the end-to-end test
#   akantu-mn-meshes-scaling.tar.gz  the same for MN_SCALING_MESHES (0.04 0.032 0.025): the AWS series
#   images.txt, SHA256SUMS, sizes.txt
# The meshes come from the perf volume MN_PERF_VOLUME (mn-perf); a missing one
# is generated there first by benchmarks/cohesive-perf/scripts/gen-meshes.sh
# (in a scratch copy of the harness; needs akantu-oracle:dev); each is checked
# against its JSON.
set -uo pipefail
cmd=${1:-}; dir=${2:-}
[ -n "${cmd}" ] && [ -n "${dir}" ] || { sed -n '7,77p' "$0"; exit 2; }
mkdir -p "${dir}" && dir="$(cd "${dir}" && pwd)"
die() { echo "c1_images.sh: $*" >&2; exit 1; }

BIN=/target/c1/release/cohesive_extrinsic
build_driver() { # the release driver from the checkout's HEAD (run in the repository root)
  [ "${MN_BUILT:-0}" = 1 ] && return 0   # export-both: built once for both sets
  echo "== driver: cargo build --release from HEAD $(git rev-parse --short HEAD) $(date -u +%T)"
  AKANTU_RS_MPI_IMAGE=akantu-rs-mumps:dev AKANTU_RS_MPI_TARGET=/target/c1 \
  AKANTU_RS_DOCKER_CPUS="${C1_BUILD_CPUS:-6}" AKANTU_RS_DOCKER_MEMORY="${C1_BUILD_MEMORY:-12g}" \
    scripts/mpi/in-docker.sh cargo build --release --locked -p akantu-bench --features mpi,scotch,mumps \
      --bin cohesive_extrinsic || die "driver build failed"
}

build_bench_c1() { # ctx rev: the driver binary + inputs -> akantu-rs-bench:c1 (docker/bench/Dockerfile)
  local ctx=$1 rev=$2 bin="${BIN}"
  docker run --rm -u "$(id -u):$(id -g)" -v akantu-rs-mpi-target:/target:ro -v "${ctx}:/out" \
    akantu-rs-mumps:dev sh -c "cp ${bin} /out/cohesive_extrinsic && cd /out && sha256sum cohesive_extrinsic > binary.sha256" \
    || die "no driver binary ${bin}"
  cp benchmarks/cohesive-perf/inputs/material-cohesive.dat benchmarks/cohesive-perf/inputs/material-elastic.dat \
    docker/bench/Dockerfile "${ctx}/"
  { echo "git_head=${rev}"; echo "built=$(date -u +%FT%TZ)"
    echo "features=mpi,scotch,mumps profile=release built_from=HEAD (c1_images.sh build_driver)"; } > "${ctx}/provenance.txt"
  docker build -q -t akantu-rs-bench:c1 --build-arg BASE_IMAGE=akantu-rs-mumps:dev "${ctx}" || die "docker build"
}

if [ "${cmd}" = export-both ]; then
  repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
  cd "${repo}" || exit 1
  [ -z "$(git status --porcelain)" ] || die "export-both needs a clean committed tree"
  build_driver
  MN_BUILT=1 bash "$0" export "${dir}/c1" || die "export (c1) failed"
  MN_BUILT=1 bash "$0" export-mn "${dir}/mn" || die "export-mn failed"
  a=$(awk '$1 == "rust_bin_sha256" {print $2}' "${dir}/c1/images.txt")
  b=$(awk '$1 == "rust_bin_sha256" {print $2}' "${dir}/mn/images.txt")
  [ -n "${a}" ] && [ "${a}" = "${b}" ] || die "rust_bin_sha256 differs: c1 ${a}, mn ${b}"
  echo "${a}  git_head $(git rev-parse HEAD)" | tee "${dir}/rust_bin_sha256.txt"
  echo "export-both: one driver ${a} in ${dir}/c1 and ${dir}/mn"
  exit 0
fi

if [ "${cmd}" = export-mn ]; then
  repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
  cd "${repo}" || exit 1
  [ -z "$(git status --porcelain)" ] || die "export-mn needs a clean committed tree"
  build_driver
  rev="$(git rev-parse HEAD)"
  pvol="${MN_PERF_VOLUME:-mn-perf}"
  ctx="$(mktemp -d "${dir}/ctx.XXXXXX")"
  echo "== akantu-rs-bench:c1 $(date -u +%T)"
  build_bench_c1 "${ctx}" "${rev}"
  mkdir -p "${ctx}/mn"
  cp docker/bench/Dockerfile.mn docker/bench/mn-entrypoint.sh docker/bench/sshd_config_mn \
    crates/akantu-bench/scripts/memsample.py "${ctx}/mn/"
  { echo "git_head=${rev}"; echo "built=$(date -u +%FT%TZ)"
    echo "base=akantu-rs-bench:c1 $(docker image inspect --format '{{.Id}}' akantu-rs-bench:c1)"
    echo "rust_bin_sha256=$(cut -d' ' -f1 "${ctx}/binary.sha256")"; } > "${ctx}/mn/mn-provenance.txt"
  echo "== akantu-rs-bench:mn $(date -u +%T)"
  docker build -q -t akantu-rs-bench:mn -f "${ctx}/mn/Dockerfile.mn" --build-arg BASE_IMAGE=akantu-rs-bench:c1 \
    "${ctx}/mn" || die "docker build (mn)"
  echo "== save $(date -u +%T)"
  docker save akantu-rs-bench:mn | gzip -1 > "${dir}/akantu-rs-bench-mn.tar.gz" || die "save mn"
  git archive --format=tar --add-virtual-file="MN_REVISION:${rev}
" HEAD -- crates/akantu-bench/scripts docker/bench benchmarks/cohesive-perf/inputs \
    benchmarks/cohesive-perf/meshes fixtures/perf_cohesive/meshes \
    | gzip -9 > "${dir}/akantu-rs-mn-repo.tar.gz" || die "git archive"
  files=(akantu-rs-bench-mn.tar.gz akantu-rs-mn-repo.tar.gz)
  # meshes missing from the volume: generated now, in a scratch copy of the harness
  # (gen-meshes.sh rewrites tracked JSONs; Gmsh of the oracle image, byte-identical twice)
  missing=()
  for h in ${MN_TEST_MESHES-0.06} ${MN_SCALING_MESHES-0.04 0.032 0.025}; do
    docker run --rm -v "${pvol}:/perf:ro" akantu-oracle:dev test -s "/perf/meshes/cube-h${h}.msh" || missing+=("${h}")
  done
  if [ ${#missing[@]} -gt 0 ]; then
    echo "== meshes ${missing[*]}: gen-meshes.sh into ${pvol} $(date -u +%T)"
    gdir="$(mktemp -d "${dir}/gen.XXXXXX")"
    git archive HEAD benchmarks/cohesive-perf fixtures/perf_cohesive | tar -x -C "${gdir}" || die "scratch harness"
    (cd "${gdir}" && PERF_VOLUME="${pvol}" bash benchmarks/cohesive-perf/scripts/gen-meshes.sh "${missing[@]}") \
      > "${dir}/gen-meshes.log" 2>&1 || { tail -20 "${dir}/gen-meshes.log"; die "gen-meshes.sh ${missing[*]}"; }
    find "${gdir}" -type f -delete && find "${gdir}" -depth -type d -empty -delete
  fi
  meshlines=()
  for set in test scaling; do
    if [ "${set}" = test ]; then hs="${MN_TEST_MESHES-0.06}"; else hs="${MN_SCALING_MESHES-0.04 0.032 0.025}"; fi
    [ -n "${hs}" ] || continue
    mkdir -p "${ctx}/mesh-${set}"
    names=()
    for h in ${hs}; do
      docker run --rm -u "$(id -u):$(id -g)" -v "${pvol}:/perf:ro" -v "${ctx}/mesh-${set}:/out" akantu-oracle:dev \
        cp "/perf/meshes/cube-h${h}.msh" "/perf/meshes/cube-h${h}.json" /out/ \
        || die "mesh h=${h} not in volume ${pvol} (gen-meshes.sh ${h} with PERF_VOLUME=${pvol})"
      want=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha256"])' "${ctx}/mesh-${set}/cube-h${h}.json")
      got=$(sha256sum "${ctx}/mesh-${set}/cube-h${h}.msh" | cut -d' ' -f1)
      [ "${want}" = "${got}" ] || die "mesh h=${h}: sha256 ${got} != ${want} (its JSON)"
      names+=("cube-h${h}.msh" "cube-h${h}.json")
      meshlines+=("mesh_h${h}_sha256 ${got}")
    done
    tar -C "${ctx}/mesh-${set}" -cf - "${names[@]}" | gzip -6 > "${dir}/akantu-mn-meshes-${set}.tar.gz" \
      || die "mesh tarball ${set}"
    files+=("akantu-mn-meshes-${set}.tar.gz")
  done
  { for i in akantu-rs-bench:mn akantu-rs-bench:c1 akantu-rs-mumps:dev; do
      echo "${i} $(docker image inspect --format '{{.Id}}' "${i}")"; done
    # the identity checked on the hosts (mn_cluster.py rootfs_digest: the layer diff ids,
    # independent of the Docker version's image store, unlike the id above)
    echo "akantu-rs-bench:mn-rootfs $(docker image inspect --format '{{json .RootFS.Layers}}' akantu-rs-bench:mn \
      | python3 -I -c 'import hashlib, json, sys; print("rootfs:" + hashlib.sha256("\n".join(json.load(sys.stdin)).encode()).hexdigest())')"
    echo "rust_bin_sha256 $(cut -d' ' -f1 "${ctx}/binary.sha256")"
    for l in "${meshlines[@]}"; do echo "${l}"; done
    echo "git_head ${rev}"; } > "${dir}/images.txt"
  find "${ctx}" -type f -delete && find "${ctx}" -depth -type d -empty -delete
  cd "${dir}" || exit 1
  sha256sum "${files[@]}" images.txt > SHA256SUMS
  du -b "${files[@]}" | tee sizes.txt
  cat images.txt SHA256SUMS
  exit 0
fi

if [ "${cmd}" = export ]; then
  repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
  cd "${repo}" || exit 1
  [ -z "$(git status --porcelain)" ] || die "export needs a clean committed tree (gen-meshes.sh rewrites benchmarks/cohesive-perf/meshes/*.json: restore them first)"
  build_driver
  ovol="${ORACLE_VOLUME:-akantu-oracle-build}"
  rev="$(git rev-parse HEAD)"
  if [ "${NO_MESH:-0}" != 1 ]; then
    [ -n "${JOSS_MESH:-}" ] && [ -f "${JOSS_MESH}" ] || die "JOSS_MESH=/path/to/cube.msh (the JOSS mesh) or NO_MESH=1"
    want=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["sha256"])' \
      benchmarks/cohesive-perf/meshes/cube-joss.json)
    got=$(sha256sum "${JOSS_MESH}" | cut -d' ' -f1)
    [ "${want}" = "${got}" ] || die "JOSS_MESH ${JOSS_MESH}: sha256 ${got}, expected ${want} (cube-joss.json)"
  fi
  ctx="$(mktemp -d "${dir}/ctx.XXXXXX")"
  build_bench_c1 "${ctx}" "${rev}"
  echo "== save images $(date -u +%T)"
  docker save akantu-oracle:dev | gzip -1 > "${dir}/akantu-oracle-dev.tar.gz" || die "save oracle"
  docker save akantu-rs-bench:c1 | gzip -1 > "${dir}/akantu-rs-bench-c1.tar.gz" || die "save bench"
  echo "== installs $(date -u +%T)"
  docker run --rm -u "$(id -u):$(id -g)" -v "${ovol}:/build:ro" -v "${dir}:/out" akantu-oracle:dev \
    tar -C /build -czf /out/akantu-oracle-installs.tar.gz install-strict install-default || die "installs"
  git archive --format=tar --add-virtual-file="C1_REVISION:${rev}
" HEAD -- benchmarks/cohesive-perf ':(exclude)benchmarks/cohesive-perf/results' \
    fixtures/perf_cohesive/meshes crates/akantu-bench/scripts docker/bench \
    | gzip -9 > "${dir}/akantu-rs-c1-repo.tar.gz" || die "git archive"
  files=(akantu-oracle-dev.tar.gz akantu-oracle-installs.tar.gz akantu-rs-bench-c1.tar.gz akantu-rs-c1-repo.tar.gz)
  if [ "${NO_MESH:-0}" != 1 ]; then
    echo "== mesh $(date -u +%T)"
    mkdir -p "${ctx}/mesh"
    cp "${JOSS_MESH}" "${ctx}/mesh/cube-joss.msh"
    cp benchmarks/cohesive-perf/meshes/cube-joss.json "${ctx}/mesh/cube-joss.json"
    tar -C "${ctx}/mesh" -cf - cube-joss.msh cube-joss.json | gzip -6 > "${dir}/akantu-c1-mesh-joss.tar.gz" \
      || die "mesh tarball"
    files+=(akantu-c1-mesh-joss.tar.gz)
  fi
  { for i in akantu-oracle:dev akantu-rs-bench:c1 akantu-rs-mumps:dev; do
      echo "${i} $(docker image inspect --format '{{.Id}}' "${i}")"; done
    echo "rust_bin_sha256 $(cut -d' ' -f1 "${ctx}/binary.sha256")"
    [ "${NO_MESH:-0}" = 1 ] || echo "joss_mesh_sha256 ${got}"
    echo "git_head ${rev}"; } > "${dir}/images.txt"
  find "${ctx}" -type f -delete && find "${ctx}" -depth -type d -empty -delete
  cd "${dir}" || exit 1
  sha256sum "${files[@]}" images.txt > SHA256SUMS
  du -b "${files[@]}" | tee sizes.txt
  cat images.txt SHA256SUMS
  exit 0
fi

[ "${cmd}" = smoke ] || die "unknown command ${cmd} (export | export-mn | export-both | smoke)"
cd "${dir}" || exit 1
work="${WORK:-${dir}/smoke-$(date -u +%Y%m%dT%H%M%SZ)}"
ovol="${ORACLE_VOLUME:-c1-oracle-build}"
pvol="${PERF_VOLUME:-c1-perf}"
rvol="${RUST_VOLUME:-c1-rust}"
t0=$SECONDS
echo "== checksums"; sha256sum -c SHA256SUMS || die "checksum mismatch"
echo "== load images"
docker load -i akantu-oracle-dev.tar.gz | tail -1 || die "load oracle"
docker load -i akantu-rs-bench-c1.tar.gz | tail -1 || die "load bench"
for i in akantu-oracle:dev akantu-rs-bench:c1; do
  want=$(awk -v n="${i}" '$1 == n {print $2}' images.txt)
  got=$(docker image inspect --format '{{.Id}}' "${i}")
  [ "${want}" = "${got}" ] || die "${i}: id ${got}, expected ${want}"
  echo "${i} ${got} ok"
done
for v in "${ovol}" "${pvol}" "${rvol}"; do
  if docker volume inspect "${v}" > /dev/null 2>&1; then
    n=$(docker run --rm -v "${v}:/v:ro" akantu-oracle:dev sh -c 'ls -A /v | wc -l')
    [ "${n}" = 0 ] || die "volume ${v} exists and is not empty (set ORACLE_VOLUME / PERF_VOLUME / RUST_VOLUME)"
  fi
done
echo "== oracle installs -> volume ${ovol}"
docker volume create "${ovol}" > /dev/null
docker run --rm -u 0 -v "${ovol}:/build" -v "${dir}:/in:ro" akantu-oracle:dev \
  tar -C /build -xzf /in/akantu-oracle-installs.tar.gz || die "extract installs"
mkdir -p "${work}/repo" || die "work directory ${work}"
tar -C "${work}/repo" -xzf akantu-rs-c1-repo.tar.gz || die "extract repo"
cd "${work}/repo" || exit 1
export ORACLE_VOLUME="${ovol}" PERF_VOLUME="${pvol}" RUST_VOLUME="${rvol}"
export AKA_SEED=1 SCOTCH_PTHREAD_NUMBER=1 FNTE=joss OUT="${work}/campaign"
echo "== build the C++ harness (strict) $(date -u +%T)"
benchmarks/cohesive-perf/scripts/build.sh strict > "${work}/cpp-build.log" 2>&1 \
  || { tail -20 "${work}/cpp-build.log"; die "harness build"; }
echo "== campaign tool: h = 0.12, 2 ranks, 1500 steps, C++ and Rust $(date -u +%T)"
bash crates/akantu-bench/scripts/c1prep_campaign.sh --memory "${SMOKE_MEMORY:-8g}" --cpus "${SMOKE_CPUS:-2}" \
  --rust-image akantu-rs-bench:c1 --rust-bin /usr/local/bin/cohesive_extrinsic --rust-volume "${rvol}" \
  smoke 0.12 1500 2 1 "cpp rust" > "${work}/campaign.log" 2>&1
crc=$?
tail -5 "${work}/campaign.log"
echo "== verdict ($((SECONDS - t0)) s)"
python3 -I - "${work}/campaign/smoke" "${crc}" <<'EOF'
import json, sys
from pathlib import Path
w, crc = Path(sys.argv[1]), int(sys.argv[2])
fail = []
if crc != 0:
    fail.append(f"campaign tool exit {crc}")
def load(side):
    d = w / side / "np2" / "rep1"
    s = json.loads((d / "summary.json").read_text()) if (d / "summary.json").exists() else {}
    e = (d / "energies.csv").read_text().splitlines()[1:] if (d / "energies.csv").exists() else []
    ins = [ln.split(",")[0] for ln in (d / "insertions.csv").read_text().splitlines()[1:]] \
        if (d / "insertions.csv").exists() else None
    st = dict(l.split("=", 1) for l in (d / "status.txt").read_text().splitlines()) \
        if (d / "status.txt").exists() else {}
    return s, e, ins, st
(cs, ce, ci, cst), (rs, re, ri, rst) = load("cpp"), load("rust")
for side, st in (("C++", cst), ("Rust", rst)):
    if st.get("outcome") != '"ok"':
        fail.append(f"{side} run outcome {st.get('outcome')}")
if rs.get("rust_static_sparse_solver") != "mumps":
    fail.append(f"Rust static solver {rs.get('rust_static_sparse_solver')}")
nb = (rs.get("rust_static_solver_stats") or {}).get("nb_proc")
if nb != 2:
    fail.append(f"Rust MUMPS ran on {nb} processes, expected 2")
for side, s in (("C++", cs), ("Rust", rs)):
    got = (s.get("nb_cohesive_elements"), s.get("nb_insertion_steps"), s.get("first_insertion_step"))
    if got != (404, 294, 710):
        fail.append(f"{side}: (cohesive, insertion steps, first) = {got}, expected (404, 294, 710)")
if ci != ri:
    k = next((i for i, (x, y) in enumerate(zip(ci or [], ri or [])) if x != y), None)
    print(f"insertion steps differ from entry {k} (UD-890, printed, not judged)")
cols = ("ed", "ep", "ek", "ew", "et")
if len(ce) > 800 and len(re) > 800:
    worst = dict.fromkeys(cols, 0.0)
    for lc, lr in zip(ce[:801], re[:801]):
        a = [float(x) for x in lc.split(",")]
        b = [float(x) for x in lr.split(",")]
        for k, x, y in zip(cols, a[1:], b[1:]):
            m = max(abs(x), abs(y))
            worst[k] = max(worst[k], abs(x - y) / m if m else 0.0)
    print("steps 0-800, max relative difference C++ vs Rust (printed, not judged: UD-348): "
          + ", ".join(f"{k} {v:.2e}" for k, v in worst.items()))
else:
    fail.append("missing or short energies.csv")
print(f"C++: {cs.get('nb_cohesive_elements')} cohesive elements, E_d {cs.get('energy_dissipated')}")
print(f"Rust: {rs.get('nb_cohesive_elements')} cohesive elements, E_d {rs.get('energy_dissipated')}, "
      f"static solver {rs.get('rust_static_sparse_solver')}")
print("SMOKE: " + ("PASS" if not fail else "FAIL: " + "; ".join(fail)))
sys.exit(1 if fail else 0)
EOF
