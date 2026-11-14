#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: rehearsal of the AWS procedure with the
# launcher's ssh backend, without AWS: N stand-in hosts are privileged
# Docker-in-Docker containers (docker/bench/Dockerfile.mnhost: their own
# dockerd, image store and network namespace, an sshd on port 22 with the
# login `ubuntu`) on a user-defined network; this machine plays the
# operator's machine. Every step is the one of the AWS procedure: upload the set to the head only,
# checksums, repository subset, session key generated on the head, its public
# half added to the other hosts' authorized_keys, the session started
# detached on the head (image copied to and loaded on every host over the
# private network, node containers with --network host, preflight, runs),
# results copied back.
#
#   bash mn_e2e_ssh.sh DIR E2E_WORK   # DIR: the export-mn set; E2E_WORK: a finished mn_e2e.sh WORK
#
# Host keys: the stand-in hosts' sshd keys are read out of
# band (`docker exec`, the role of EC2's console output) before any SSH
# connection, every ssh / scp of this "Mac" uses `-F mn-ssh.conf` (strict, also
# for the `-J` hop), and the head gets the workers' keys in
# mn-session/ssh/hosts_known_hosts. Before the upload, `mn_aws_hosts.py check`
# waits for cloud-init, reads back every host's shutdown timer (the
# last host starts without one and is armed by the script), checks Docker and
# the disk and prints the deadline the session is started with (--deadline);
# the session's public key reaches the other hosts with `ssh -J` through the
# head; at the end `mn_aws_hosts.py halt` stops every host (stand-ins record
# it). Plan (8 ranks, 8 / N per host, h = 0.12, the partition of E2E_WORK's
# reference session, the settings E2E_EXTRA): h012 (MUMPS), h012-eig (Eigen
# static solve), h012-stop (host N's node container stopped 8 s into the run:
# host-lost, restored over SSH, repeated). Verdict `SSH-E2E: PASS` when the
# hosts were READY (the timer armed where missing), h012 and h012-eig pass
# mn_compare.py --pair against E2E_WORK's single-container runs of the same
# binary (A exact, C; B bitwise for h012-eig), h012-stop is host-lost then ok
# and the halt reached every host.
# env: WORK (DIR/e2e-ssh-<UTC stamp>), N_SSH (2), SSH_NET (10.79.0), E2E_EXTRA
#   ("rust_cohesive_compat=upstream rust_doubled_node_load=follows_elements")
set -uo pipefail
die() { echo "mn_e2e_ssh.sh: $*" >&2; exit 1; }
[ $# -eq 2 ] || { sed -n '7,37p' "$0"; exit 2; }
dir="$(cd "$1" && pwd)" || exit 2
ref="$(cd "$2" && pwd)" || exit 2
n="${N_SSH:-2}" net="${SSH_NET:-10.79.0}"
extra="${E2E_EXTRA-rust_cohesive_compat=upstream rust_doubled_node_load=follows_elements}"
per=$((8 / n))
[ $((per * n)) = 8 ] || die "N_SSH must divide 8"
work="${WORK:-${dir}/e2e-ssh-$(date -u +%Y%m%dT%H%M%SZ)}"
mkdir -p "${work}/repo" "${work}/results" || exit 1
t0=$SECONDS
tar -C "${work}/repo" -xzf "${dir}/akantu-rs-mn-repo.tar.gz" || die "repository subset"
sc="${work}/repo/crates/akantu-bench/scripts"
[ -f "${ref}/parts/part-h012.csv" ] || die "no ${ref}/parts/part-h012.csv (run mn_e2e.sh first)"

echo "== stand-in hosts: ${n} Docker-in-Docker containers on ${net}.0/24"
docker build -q -t akantu-mn-fakehost:test -f "${work}/repo/docker/bench/Dockerfile.mnhost" \
  "${work}/repo/docker/bench" || die "fake host image"
ssh-keygen -q -t ed25519 -N '' -C mn-rehearsal-launch-key -f "${work}/launch_key" || die "launch key"
docker network create --subnet "${net}.0/24" mn-ssh-test > /dev/null || die "network"
addrs=()
for k in $(seq "${n}"); do
  a="${net}.$((10 + k))"
  addrs+=("${a}")
  notimer=0; [ "${k}" = "${n}" ] && notimer=1   # the last host: as if its user data had failed
  docker run -d --privileged --name "mnfh${k}" --hostname "mnfh${k}" --network mn-ssh-test --ip "${a}" \
    --cpuset-cpus "$(((k - 1) * per))-$((k * per - 1))" -e DOCKER_TLS_CERTDIR= -e MNHOST_NO_TIMER="${notimer}" \
    -v "${work}/launch_key.pub:/launch/authorized_keys:ro" -v "mnfh${k}-docker:/var/lib/docker" \
    akantu-mn-fakehost:test > /dev/null || die "fake host ${k}"
done
cleanup() {
  for k in $(seq "${n}"); do docker rm -f "mnfh${k}" > /dev/null 2>&1; docker volume rm "mnfh${k}-docker" > /dev/null 2>&1; done
  docker network rm mn-ssh-test > /dev/null 2>&1
}
trap cleanup EXIT
# the client configuration of the AWS procedure: one file for every ssh / scp, the -J hop included
printf 'Host *\n  User ubuntu\n  IdentityFile %s\n  IdentitiesOnly yes\n  UserKnownHostsFile %s\n  StrictHostKeyChecking yes\n  BatchMode yes\n  ConnectTimeout 10\n  LogLevel ERROR\n' \
  "${work}/launch_key" "${work}/known_hosts" > "${work}/mn-ssh.conf"
ssh_=(ssh -F "${work}/mn-ssh.conf")
scp_=(scp -q -F "${work}/mn-ssh.conf")
# the stand-in hosts' sshd keys, read out of band (on EC2: the console output's SSH HOST KEY
# block), before any SSH connection: no trust on first use, on this "Mac" nor on the head
: > "${work}/known_hosts"
for k in $(seq "${n}"); do
  for _ in $(seq 60); do
    key=$(docker exec "mnfh${k}" cat /etc/ssh/ssh_host_ed25519_key.pub 2>/dev/null) && [ -n "${key}" ] && break
    sleep 2
  done
  echo "${addrs[k - 1]} $(echo "${key}" | cut -d' ' -f1,2)" >> "${work}/known_hosts" || die "host key ${k}"
done
for k in $(seq "${n}"); do
  for _ in $(seq 60); do
    docker exec "mnfh${k}" docker info > /dev/null 2>&1 && "${ssh_[@]}" "${addrs[k - 1]}" true 2>/dev/null && break
    sleep 2
  done
done
head="${addrs[0]}"

echo "== 0. the hosts before any upload: cloud-init, shutdown timers, Docker, disk ($((SECONDS - t0)) s)"
python3 -I "${sc}/mn_aws_hosts.py" check -F "${work}/mn-ssh.conf" --head "${head}" --workers "${addrs[@]:1}" \
  --min-free-gb 5 | tee "${work}/hosts-check.txt"
grep -q '^HOSTS: READY' "${work}/hosts-check.txt" || die "hosts not ready"
grep -q "^${addrs[n - 1]} .* armed +" "${work}/hosts-check.txt" || die "the host without a timer was not armed"
deadline=$(awk '$1 == "DEADLINE" {print $2}' "${work}/hosts-check.txt")

echo "== 1. upload the set to the head only ($((SECONDS - t0)) s)"
"${ssh_[@]}" "${head}" mkdir -p mn/parts || die "mkdir"
# every file of the set (the names in SHA256SUMS), as the AWS procedure does
set_files=("${dir}/SHA256SUMS")
while read -r _ f; do set_files+=("${dir}/${f}"); done < "${dir}/SHA256SUMS"
"${scp_[@]}" "${set_files[@]}" "${head}:mn/" || die "upload"
"${scp_[@]}" "${ref}/parts/part-h012.csv" "${head}:mn/parts/" || die "upload partition"

echo "== 2. on the head: checksums, repository subset, session, key ($((SECONDS - t0)) s)"
{ printf 'backend = ssh\nimage = akantu-rs-bench:mn\nmemory = 6g\nnet_cidr = %s.0/24\nssh_user = ubuntu\n' "${net}"
  printf 'bind = core\n'; } > "${work}/cluster.conf"
: > "${work}/hosts"
for a in "${addrs[@]}"; do echo "${a} slots=${per}" >> "${work}/hosts"; done
{ echo "h012      h0.12 8 ${n} 1500 auto  file:part-h012.csv 0.03 ${extra} @required"
  echo "h012-eig  h0.12 8 ${n} 1500 eigen file:part-h012.csv 0.03 ${extra}"
  echo "h012-stop h0.12 8 ${n} 1500 auto  file:part-h012.csv 0.03 ${extra}"; } > "${work}/plan.txt"
"${ssh_[@]}" "${head}" mkdir -p mn-session || die "session dir"
"${scp_[@]}" "${work}/cluster.conf" "${work}/hosts" "${head}:mn-session/" || die "session files"
"${scp_[@]}" "${work}/plan.txt" "${head}:mn/plan.txt" || die "plan"
"${ssh_[@]}" "${head}" 'cd mn && sha256sum -c SHA256SUMS && tar -xzf akantu-rs-mn-repo.tar.gz' || die "checksums"
"${ssh_[@]}" "${head}" python3 -I mn/crates/akantu-bench/scripts/mn_cluster.py keygen mn-session \
  > "${work}/session_key.pub" || die "keygen"
"${scp_[@]}" "${work}/known_hosts" "${head}:mn-session/ssh/hosts_known_hosts" || die "hosts' keys"

echo "== 3. the session's public key into the other hosts' authorized_keys ($((SECONDS - t0)) s)"
for a in "${addrs[@]:1}"; do   # through the head, as on AWS (the workers have no public address there)
  "${ssh_[@]}" -J "${head}" "${a}" 'cat >> .ssh/authorized_keys' < "${work}/session_key.pub" || die "authorize ${a}"
done

echo "== 4. start the session on the head (detached) ($((SECONDS - t0)) s)"
"${ssh_[@]}" "${head}" bash mn/crates/akantu-bench/scripts/mn_session.sh start '$HOME/mn-session' '$HOME/mn-results' 1 \
  --deadline "${deadline}" \
  --plan mn/plan.txt --image-tarball mn/akantu-rs-bench-mn.tar.gz --images-txt mn/images.txt \
  --meshes mn/akantu-mn-meshes-test.tar.gz --partitions mn/parts --fail-host "h012-stop:${n}:stop:8" \
  || die "session start"
while "${ssh_[@]}" "${head}" 'kill -0 $(cat mn-results/session.pid)' 2>/dev/null; do sleep 10; done
"${ssh_[@]}" "${head}" bash mn/crates/akantu-bench/scripts/mn_session.sh status mn-results | tee "${work}/status.txt"
grep -q '^session: finished' "${work}/status.txt" || die "the session did not finish"

echo "== 5. results back ($((SECONDS - t0)) s)"
"${ssh_[@]}" "${head}" 'tar -C mn-results --exclude=./meshes -cf - .' | tar -C "${work}/results" -xf - || die "results"

echo "== 6. compare with the single-container runs of ${ref} ($((SECONDS - t0)) s)"
fails=""
for c in h012 h012-eig; do
  python3 -I "${sc}/mn_compare.py" --pair "${ref}/r1/runs/${c}-a1" "${work}/results/runs/${c}-a1" || fails="${fails} ${c}"
done
seq_=$(python3 -I - "${work}/results/state.json" <<'EOF'
import json, sys
st = json.load(open(sys.argv[1]))
print(" ".join("%s:%s:%s" % (p["attempt"], p["outcome"], ",".join(p.get("lost_hosts") or []))
               for p in st["points"] if p["name"] == "h012-stop"))
EOF
)
echo "h012-stop attempts: ${seq_}"
[ "${seq_}" = "1:host-lost:${addrs[n - 1]} 2:ok:" ] || fails="${fails} h012-stop"
grep -q '"deadline": '"${deadline}" "${work}/results/state.json" || fails="${fails} deadline-not-recorded"

echo "== 7. the fallback stop of every host: workers through the head, then the head ($((SECONDS - t0)) s)"
python3 -I "${sc}/mn_aws_hosts.py" halt -F "${work}/mn-ssh.conf" --head "${head}" --workers "${addrs[@]:1}"
for k in $(seq "${n}"); do
  docker exec "mnfh${k}" test -s /run/mnhost-halted || fails="${fails} halt-${k}"
done
echo "== done in $((SECONDS - t0)) s (work ${work})"
echo "SSH-E2E: $([ -z "${fails}" ] && echo PASS || echo "FAIL:${fails}")"
[ -z "${fails}" ]
