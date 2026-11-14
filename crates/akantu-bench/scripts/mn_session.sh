#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: the multi-node session of the cohesive
# benchmark on N hosts with Docker, run on the head node from the tarballs of
# c1_images.sh export-mn (docs/TESTING.md).
#
#   bash mn_session.sh start  SESSION RESULTS BUDGET_H --plan PLAN [options]   # detached; returns at once
#   bash mn_session.sh status RESULTS                                          # where it is
#   bash mn_session.sh stop   RESULTS                                          # stop it (and the job on every host)
#   bash mn_session.sh run    SESSION RESULTS BUDGET_H --plan PLAN [options]   # in the foreground
#
#   SESSION   the cluster directory (cluster.conf, hosts, ssh/ from
#             `python3 -I mn_cluster.py keygen SESSION`)
#   RESULTS   results directory (created; one sub-directory per run with a DONE marker)
#   BUDGET_H  wall-clock budget in hours from the first start (the hosts'
#             shutdown timers must not be shorter)
#
# `start` checks the arguments in the foreground, then detaches the session
# from the terminal (setsid, nohup, stdin from /dev/null; log
# RESULTS/session.log), so it survives an SSH disconnect. Order, rules and
# options: mn_session.py (`python3 -I mn_session.py --help`); the launcher:
# mn_cluster.py.
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cmd=${1:-}
[ -n "${cmd}" ] || { sed -n '7,23p' "$0"; exit 2; }
shift
case "${cmd}" in
  start)
    [ $# -ge 4 ] || { sed -n '7,23p' "$0"; exit 2; }
    results=$2
    mkdir -p "${results}" || exit 1
    results="$(cd "${results}" && pwd)"
    if [ -f "${results}/session.pid" ] && kill -0 "$(cat "${results}/session.pid")" 2>/dev/null; then
      echo "mn_session.sh: a session already runs for ${results} (pid $(cat "${results}/session.pid"))" >&2
      exit 1
    fi
    python3 -I "${here}/mn_session.py" check "$@" || exit 2
    setsid nohup python3 -I "${here}/mn_session.py" run "$@" >> "${results}/session.log" 2>&1 < /dev/null &
    echo $! > "${results}/session.pid"
    sleep 2
    if ! kill -0 "$(cat "${results}/session.pid")" 2>/dev/null; then
      echo "mn_session.sh: the session exited at once:" >&2
      tail -20 "${results}/session.log" >&2
      exit 1
    fi
    echo "session started (pid $(cat "${results}/session.pid")); log ${results}/session.log"
    echo "status: bash $0 status ${results}"
    ;;
  run) exec python3 -I "${here}/mn_session.py" run "$@" ;;
  status) exec python3 -I "${here}/mn_session.py" status "$@" ;;
  stop)
    results=${1:?RESULTS}
    if [ -f "${results}/session.pid" ]; then
      # the session's SIGTERM handler stops the job on every host (mn_cluster.py abort)
      kill -TERM "$(cat "${results}/session.pid")" 2>/dev/null
      for _ in $(seq 60); do kill -0 "$(cat "${results}/session.pid")" 2>/dev/null || break; sleep 1; done
    fi
    python3 -I "${here}/mn_session.py" status "${results}"
    ;;
  *) sed -n '7,23p' "$0"; exit 2 ;;
esac
