#!/bin/sh
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: entrypoint of the multi-node node
# container (docker/bench/Dockerfile.mn), run as root.
#
# Installs the session's SSH material from the read-only mount /mn-ssh
# (generated on the head node by `mn_cluster.py keygen`; never part of the
# image, never committed):
#   id_ed25519, id_ed25519.pub   client key of user akantu (the public half
#                                is the only authorized key)
#   ssh_host_ed25519_key         the session's host key (one for all nodes)
#   known_hosts                  "* ssh-ed25519 <host key>": strict host key
#                                checking without trust-on-first-use
# then starts sshd in the foreground (MN_SSH_PORT, default 2222; MN_LISTEN:
# optional listen address, e.g. the host's private address under
# --network host).
set -eu
src=/mn-ssh
port="${MN_SSH_PORT:-2222}"
for f in id_ed25519 id_ed25519.pub ssh_host_ed25519_key known_hosts; do
  [ -s "${src}/${f}" ] || { echo "mn-entrypoint: ${src}/${f} missing" >&2; exit 1; }
done
install -d -m 0755 -o root -g root /etc/akantu-mn
install -m 0600 -o root -g root "${src}/ssh_host_ed25519_key" /etc/akantu-mn/ssh_host_ed25519_key
install -m 0644 -o root -g root "${src}/id_ed25519.pub" /etc/akantu-mn/authorized_keys
h=/home/akantu/.ssh # the container user (public-tree-guard: allow)
install -d -m 0700 -o akantu -g akantu "${h}"
install -m 0600 -o akantu -g akantu "${src}/id_ed25519" "${h}/id_ed25519"
install -m 0644 -o akantu -g akantu "${src}/known_hosts" "${h}/known_hosts"
cat > "${h}/config" <<EOF
Host *
  Port ${port}
  User akantu
  IdentityFile ~/.ssh/id_ed25519
  IdentitiesOnly yes
  UserKnownHostsFile ~/.ssh/known_hosts
  StrictHostKeyChecking yes
  BatchMode yes
  ConnectTimeout 10
  ServerAliveInterval 10
  ServerAliveCountMax 3
  LogLevel ERROR
EOF
chown akantu:akantu "${h}/config"
chmod 0600 "${h}/config"
mkdir -p /run/sshd
if [ -n "${MN_LISTEN:-}" ]; then
  exec /usr/sbin/sshd -D -e -f /etc/ssh/sshd_config_mn -p "${port}" \
    -h /etc/akantu-mn/ssh_host_ed25519_key -o "ListenAddress=${MN_LISTEN}"
fi
exec /usr/sbin/sshd -D -e -f /etc/ssh/sshd_config_mn -p "${port}" -h /etc/akantu-mn/ssh_host_ed25519_key
