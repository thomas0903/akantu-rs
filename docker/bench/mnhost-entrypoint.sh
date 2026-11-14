#!/bin/sh
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: entrypoint of the stand-in host of the
# ssh-backend rehearsal (docker/bench/Dockerfile.mnhost; test only).
set -eu
ssh-keygen -A > /dev/null
install -d -m 0700 -o ubuntu -g ubuntu /home/ubuntu/.ssh # the image user (public-tree-guard: allow)
install -m 0600 -o ubuntu -g ubuntu /launch/authorized_keys /home/ubuntu/.ssh/authorized_keys # public-tree-guard: allow
# the user data's timer (MNHOST_NO_TIMER=1: a host whose user data failed)
[ "${MNHOST_NO_TIMER:-0}" = 1 ] || /usr/local/sbin/shutdown -h "+${MNHOST_TIMER_MIN:-330}"
/usr/sbin/sshd -e
exec dockerd-entrypoint.sh dockerd --host=unix:///var/run/docker.sock
