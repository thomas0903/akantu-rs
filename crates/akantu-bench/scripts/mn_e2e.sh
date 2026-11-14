#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: end-to-end test of the multi-node launcher
# and session without AWS: on one Linux machine with Docker and >= 8 CPUs, N
# node containers on a user-defined network stand in for N hosts (N = 1, 2,
# 4, 8; 8 ranks in total, 8 / N per container, each container its own CPU
# set, memory limit and work volume), from the tarballs of
# `c1_images.sh export-mn` only (docs/TESTING.md).
#
#   bash mn_e2e.sh DIR     # DIR = the export-mn set (SHA256SUMS, images.txt, tarballs)
#
# Per N, one session (mn_session.sh start: detached; polled until it ends):
#   N = 1  the reference: h012 (h = 0.12, 1,500 steps, MUMPS, the driver's
#          Scotch partition, exported), h012-eig (Eigen static solve, same
#          partition), h012-rep2 (MUMPS again: the run-to-run band), h006,
#          h006-eig, h006-rep2 (h = 0.06, 2,500 steps: 1,788 after the
#          first insertion at step 712)
#   N > 1  h012, h012-eig, h006, h006-eig on the partitions exported at N = 1
#   N = 4  + h012-stop (host 3 stopped 6 s into the run, in the explicit
#          loop) and h012-pause (host 2 paused 6 s into the run: no socket
#          is closed, only the watchdog sees it): `host-lost`, host
#          restored, run repeated once
#   N = 8  + h006-oom (host 2's container memory at 150m: a real
#          out-of-memory kill of rank 1): `oom`
# Estimates (and so the wall-clock limits, 2 x the estimate) leave room for
# the sandbox's speed, which varied by up to 5x during a day: the h = 0.06
# runs took 4-8 min (one 12+ min), the Eigen ones 12-30 min.
# then mn_compare.py WORK prints what is compared and the verdict
# `E2E: PASS | FAIL` (exit status 0 iff PASS).
# env: WORK (DIR/e2e-<UTC stamp>), E2E_HOSTS ("1 2 4 8"), E2E_QUICK (1: the
#   h = 0.12 runs only, no failure tests), E2E_MEMORY (12g per container),
#   E2E_BUDGET_H (3 per session), E2E_NET (10.78: networks 10.78.<N>.0/24),
#   E2E_EXTRA (settings added to every run: "rust_cohesive_compat=upstream
#   rust_doubled_node_load=follows_elements", the full-size rerun's, D-108;
#   empty for a driver without these keys)
set -uo pipefail
die() { echo "mn_e2e.sh: $*" >&2; exit 1; }
[ $# -eq 1 ] || { sed -n '7,36p' "$0"; exit 2; }
dir="$(cd "$1" && pwd)" || exit 2
work="${WORK:-${dir}/e2e-$(date -u +%Y%m%dT%H%M%SZ)}"
hosts_list="${E2E_HOSTS:-1 2 4 8}"
quick="${E2E_QUICK:-0}"
extra="${E2E_EXTRA-rust_cohesive_compat=upstream rust_doubled_node_load=follows_elements}"
budget="${E2E_BUDGET_H:-3}"
t0=$SECONDS
cd "${dir}" || exit 1
echo "== checksums"; sha256sum -c SHA256SUMS || die "checksum mismatch"
echo "== image"
docker load -i akantu-rs-bench-mn.tar.gz | tail -1 || die "docker load"
want=$(awk '$1 == "akantu-rs-bench:mn-rootfs" {print $2}' images.txt)
got=$(docker image inspect --format '{{json .RootFS.Layers}}' akantu-rs-bench:mn \
  | python3 -I -c 'import hashlib, json, sys; print("rootfs:" + hashlib.sha256("\n".join(json.load(sys.stdin)).encode()).hexdigest())')
[ -n "${want}" ] && [ "${want}" = "${got}" ] || die "akantu-rs-bench:mn layers ${got}, expected ${want} (images.txt)"
echo "akantu-rs-bench:mn ${got} ok (id $(docker image inspect --format '{{.Id}}' akantu-rs-bench:mn))"
mkdir -p "${work}/repo" "${work}/parts" || die "work directory ${work}"
tar -C "${work}/repo" -xzf akantu-rs-mn-repo.tar.gz || die "repository subset"
sc="${work}/repo/crates/akantu-bench/scripts"
echo "revision $(cat "${work}/repo/MN_REVISION")"

plan() { # n -> plan lines, each with the run settings E2E_EXTRA
  plan_lines "$1" | sed -e "s|\$| ${extra}|"
}
plan_lines() { # n
  local n=$1 p
  if [ "${n}" = 1 ]; then
    echo "h012      h0.12 8 1 1500 auto  scotch    0.03 @required"
    echo "h012-eig  h0.12 8 1 1500 eigen run:h012  0.03"
    echo "h012-rep2 h0.12 8 1 1500 auto  run:h012  0.03"
    [ "${quick}" = 1 ] && return
    echo "h006      h0.06 8 1 2500 auto  scotch    0.25"
    echo "h006-eig  h0.06 8 1 2500 eigen run:h006  0.5"
    echo "h006-rep2 h0.06 8 1 2500 auto  run:h006  0.25"
    return
  fi
  echo "h012      h0.12 8 ${n} 1500 auto  file:part-h012.csv 0.03 @required"
  echo "h012-eig  h0.12 8 ${n} 1500 eigen file:part-h012.csv 0.03"
  [ "${quick}" = 1 ] && return
  echo "h006      h0.06 8 ${n} 2500 auto  file:part-h006.csv 0.25"
  echo "h006-eig  h0.06 8 ${n} 2500 eigen file:part-h006.csv 0.5"
  if [ "${n}" = 4 ]; then
    echo "h012-stop  h0.12 8 4 1500 auto file:part-h012.csv 0.03"
    echo "h012-pause h0.12 8 4 1500 auto file:part-h012.csv 0.03"
  fi
  [ "${n}" = 8 ] && echo "h006-oom h0.06 8 8 2500 auto file:part-h006.csv 0.25"
  return 0
}

for n in ${hosts_list}; do
  per=$((8 / n))
  [ $((per * n)) = 8 ] || die "N = ${n} does not divide 8 ranks"
  s="${work}/s${n}" r="${work}/r${n}"
  mkdir -p "${s}"
  cat > "${s}/cluster.conf" <<EOF
backend = local
image = akantu-rs-bench:mn
memory = ${E2E_MEMORY:-12g}
net_cidr = ${E2E_NET:-10.78}.${n}.0/24
network = mn-e2e-${n}
prefix = e2e${n}
bind = core
EOF
  : > "${s}/hosts"
  for k in $(seq "${n}"); do echo "e2e${n}-h${k} slots=${per}" >> "${s}/hosts"; done
  plan "${n}" > "${s}/plan.txt"
  python3 -I "${sc}/mn_cluster.py" keygen "${s}" > /dev/null || die "keygen"
  args=("${s}" "${r}" "${budget}" --plan "${s}/plan.txt" --meshes "${dir}/akantu-mn-meshes-test.tar.gz")
  [ "${n}" = 1 ] || args+=(--partitions "${work}/parts")
  if [ "${quick}" != 1 ]; then
    [ "${n}" = 4 ] && args+=(--fail-host h012-stop:3:stop:6 --fail-host h012-pause:2:pause:6)
    [ "${n}" = 8 ] && args+=(--force-oom h006-oom:2 --oom-memory 150m)
  fi
  echo "== session N = ${n} ($((SECONDS - t0)) s)"
  bash "${sc}/mn_session.sh" start "${args[@]}" || die "session N = ${n} did not start"
  pid=$(cat "${r}/session.pid")
  while kill -0 "${pid}" 2>/dev/null; do sleep 10; done
  bash "${sc}/mn_session.sh" status "${r}"
  if [ "${n}" = 1 ]; then
    cp "${r}/runs/h012-a1/partition.csv" "${work}/parts/part-h012.csv" || die "no h012 partition"
    [ "${quick}" = 1 ] || cp "${r}/runs/h006-a1/partition.csv" "${work}/parts/part-h006.csv" || die "no h006 partition"
  fi
  python3 -I "${sc}/mn_cluster.py" down "${s}" --volumes
done
echo "== compare ($((SECONDS - t0)) s)"
python3 -I "${sc}/mn_compare.py" "${work}" | tee "${work}/compare.txt"
rc=${PIPESTATUS[0]}
echo "== done in $((SECONDS - t0)) s (work ${work})"
exit "${rc}"
