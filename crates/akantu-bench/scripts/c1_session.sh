#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: the full-size (C1) session of the cohesive
# scalability benchmark on one x86_64 Linux host with Docker, run from the
# tarballs of c1_images.sh export (docs/TESTING.md).
#
#   bash c1_session.sh start  DIR MEMORY CPUS RESULTS BUDGET_H [options]   # detached; returns at once
#   bash c1_session.sh status RESULTS                                       # where it is
#   bash c1_session.sh stop   RESULTS                                       # stop it (and its containers)
#   bash c1_session.sh run    DIR MEMORY CPUS RESULTS BUDGET_H [options]   # the session in the foreground
#
#   DIR       the directory with the tarballs, SHA256SUMS and images.txt
#   MEMORY    container memory limit of every run, no swap (480g on the 512 GiB host)
#   CPUS      CPU quota of the Rust container = the node's cores (64)
#   RESULTS   results directory (created; one sub-directory per run with a DONE marker)
#   BUDGET_H  wall-clock budget in hours, counted from the first start (the host's
#             shutdown timer must not be shorter)
#
# Start it from the repository subset of the tarballs:
#   tar -xzf DIR/akantu-rs-c1-repo.tar.gz crates/akantu-bench/scripts/c1_session.sh \
#       crates/akantu-bench/scripts/c1_session.py crates/akantu-bench/scripts/c1_images.sh
#   bash crates/akantu-bench/scripts/c1_session.sh start DIR 480g 64 ~/c1-results 14
# `start` detaches the session from the terminal (setsid, nohup, stdin from
# /dev/null; log RESULTS/session.log), so it survives an SSH disconnect.
# The order of the session, its rules and options: c1_session.py (or
# `python3 -I c1_session.py --help`).
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cmd=${1:-}
[ -n "${cmd}" ] || { sed -n '7,27p' "$0"; exit 2; }
shift
case "${cmd}" in
  start)
    [ $# -ge 5 ] || { sed -n '7,27p' "$0"; exit 2; }
    results=$4
    mkdir -p "${results}" || exit 1
    results="$(cd "${results}" && pwd)"
    if [ -f "${results}/session.pid" ] && kill -0 "$(cat "${results}/session.pid")" 2>/dev/null; then
      echo "c1_session.sh: a session already runs for ${results} (pid $(cat "${results}/session.pid"))" >&2
      exit 1
    fi
    # check the arguments in the foreground before detaching
    python3 -I "${here}/c1_session.py" check "$@" || exit 2
    setsid nohup python3 -I "${here}/c1_session.py" run "$@" >> "${results}/session.log" 2>&1 < /dev/null &
    echo $! > "${results}/session.pid"
    sleep 2
    if ! kill -0 "$(cat "${results}/session.pid")" 2>/dev/null; then
      echo "c1_session.sh: the session exited at once:" >&2
      tail -20 "${results}/session.log" >&2
      exit 1
    fi
    echo "session started (pid $(cat "${results}/session.pid")); log ${results}/session.log"
    echo "status: bash $0 status ${results}"
    ;;
  run) exec python3 -I "${here}/c1_session.py" run "$@" ;;
  status) exec python3 -I "${here}/c1_session.py" status "$@" ;;
  stop)
    results=${1:?RESULTS}
    if [ -f "${results}/session.pid" ]; then
      pid=$(cat "${results}/session.pid")
      # the session leads its own process group (setsid): TERM reaches the campaign
      # tool and the docker clients, which forward it to their containers
      kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null
    fi
    prefix=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1])).get("prefix", "c1s"))' \
      "${results}/state.json" 2>/dev/null || echo c1s)
    sleep 5
    ids=$(docker ps -q --filter "name=^${prefix}-")
    # shellcheck disable=SC2086
    [ -z "${ids}" ] || docker stop -t 20 ${ids}
    python3 -I "${here}/c1_session.py" status "${results}"
    ;;
  *) sed -n '7,27p' "$0"; exit 2 ;;
esac
