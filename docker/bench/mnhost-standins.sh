#!/bin/sh
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: test-only stand-ins of the stand-in host
# (docker/bench/Dockerfile.mnhost), installed as /usr/local/bin/cloud-init and
# /usr/local/sbin/shutdown, so that mn_aws_hosts.py runs against it as against
# an Ubuntu instance: `cloud-init status --wait` reports done, and `shutdown`
# only records what it was asked (a timer in /run/systemd/shutdown/scheduled
# as systemd writes it, a halt in /run/mnhost-halted); nothing is stopped.
case "$(basename "$0")" in
  cloud-init)
    [ "${1:-}" = status ] || { echo "stand-in cloud-init: $*" >&2; exit 2; }
    echo "status: done" ;;
  shutdown)
    mkdir -p /run/systemd/shutdown
    case "${1:-} ${2:-}" in
      "-h now") date -u +%s > /run/mnhost-halted; echo "stand-in: halt requested" ;;
      "-h +"*)
        m=${2#+}
        printf 'USEC=%s\nWARN_WALL=1\nMODE=poweroff\n' "$(( ($(date +%s) + m * 60) * 1000000 ))" \
          > /run/systemd/shutdown/scheduled
        chmod 0644 /run/systemd/shutdown/scheduled ;;
      *) echo "stand-in shutdown: unsupported $*" >&2; exit 2 ;;
    esac ;;
  *) exit 2 ;;
esac
