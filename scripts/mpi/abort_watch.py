#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs MPI tier support (no upstream counterpart):
# run one launcher command and measure, separately, when the job's rank
# processes ended and when the launcher returned.
#
#   abort_watch.py [--timeout S] [--kill-after S] [--snapshot-after S]
#                  [--snapshot FILE] --match BINARY --out FILE --err FILE
#                  -- command...
#
# The command runs in its own session with stdout/stderr redirected to the
# given files; at --timeout seconds the session gets SIGTERM, --kill-after
# seconds later SIGKILL (as `timeout -k`; exit status 124 then). Every
# 20 ms the watcher scans /proc for the rank processes: processes whose
# command line contains BINARY and that descend from the launcher (the
# launcher itself excluded; a process once adopted stays tracked by its pid,
# also if it is re-parented after the launcher died, so other jobs running
# the same binary are ignored), and records the moment the last one ended:
# a rank has ended once it is gone or a zombie (exited, not yet reaped). A run whose launcher
# is still running after --snapshot-after seconds gets a snapshot of the
# process table and of the per-thread state of the launcher and of every
# remaining rank written to --snapshot (diagnostics only).
#
# Prints one line:
#   rc=<launcher exit status> seconds=<launcher wall time>
#   ranks_seen=<rank processes observed> ranks_ended=<seconds, or -1 if a
#   rank was still running when the launcher returned or was killed>
#   snapshot=<0|1>
# The MPI tier (run-mpi-tier.sh) bounds ranks_ended (the abort terminated
# every rank) and requires the launcher's exit status; the launcher's own
# teardown time is reported (Open MPI 5.0.7's prterun can take long
# after every rank has ended, docs/DEPENDENCIES.md).
import argparse
import os
import signal
import subprocess
import sys
import time


def stat_fields(pid):
    """(state, ppid) of `pid` from /proc/<pid>/stat, or None."""
    try:
        with open(f"/proc/{pid}/stat", "rb") as f:
            stat = f.read()
    except OSError:
        return None
    # the fields after the parenthesised command name: state, ppid, ...
    rest = stat[stat.rfind(b")") + 2:].split()
    if len(rest) < 2:
        return None
    return rest[0].decode(errors="replace"), int(rest[1])


def descends(pid, root):
    """`pid` has `root` among its ancestors (at most 16 levels)."""
    for _ in range(16):
        f = stat_fields(pid)
        if f is None or f[1] <= 1:
            return False
        if f[1] == root:
            return True
        pid = f[1]
    return False


def scan(binary, launcher, adopted, exclude):
    """(pid, state) of the job's rank processes: command line naming
    `binary` and descending from `launcher` (or already `adopted`)."""
    found = []
    for name in os.listdir("/proc"):
        if not name.isdigit():
            continue
        pid = int(name)
        if pid in exclude:
            continue
        try:
            with open(f"/proc/{pid}/cmdline", "rb") as f:
                cmd = f.read()
        except OSError:
            continue
        if binary.encode() not in cmd:
            continue
        if pid not in adopted:
            if not descends(pid, launcher):
                continue
            adopted.add(pid)
        f = stat_fields(pid)
        if f is not None:
            found.append((pid, f[0]))
    # adopted ranks no longer listed are gone (reaped)
    return found


def snapshot(path, launcher, ranks, elapsed):
    lines = [f"== snapshot at {elapsed:.1f} s: launcher {launcher}, ranks {ranks}"]
    try:
        ps = subprocess.run(["ps", "-eo", "pid,ppid,pgid,stat,etimes,wchan:32,args"],
                            capture_output=True, text=True, timeout=10)
        lines.append(ps.stdout)
    except (OSError, subprocess.SubprocessError) as e:
        lines.append(f"ps failed: {e}")
    for pid in [launcher] + [p for p, _ in ranks]:
        try:
            tasks = sorted(os.listdir(f"/proc/{pid}/task"), key=int)
        except OSError:
            lines.append(f"-- pid {pid}: gone")
            continue
        lines.append(f"-- pid {pid}")
        for t in tasks:
            def rd(n):
                try:
                    with open(f"/proc/{pid}/task/{t}/{n}", errors="replace") as f:
                        return f.read().strip()
                except OSError:
                    return "?"
            st = rd("stat")
            state = st[st.rfind(")") + 2:st.rfind(")") + 3] if ")" in st else "?"
            lines.append(f"   task {t} comm={rd('comm')} state={state} wchan={rd('wchan')}")
    with open(path, "a", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--timeout", type=float, default=120.0)
    ap.add_argument("--kill-after", type=float, default=10.0)
    ap.add_argument("--snapshot-after", type=float, default=0.0)
    ap.add_argument("--snapshot", default="")
    ap.add_argument("--match", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--err", required=True)
    ap.add_argument("command", nargs=argparse.REMAINDER)
    a = ap.parse_args()
    cmd = a.command[1:] if a.command[:1] == ["--"] else a.command
    if not cmd:
        ap.error("no command")
    same = os.path.abspath(a.out) == os.path.abspath(a.err)
    with open(a.out, "wb") as out, (open(os.devnull, "wb") if same else open(a.err, "wb")) as err_f:
        err = out if same else err_f
        t0 = time.monotonic()
        p = subprocess.Popen(cmd, stdout=out, stderr=err, start_new_session=True)
        exclude = {os.getpid(), p.pid}
        seen = set()  # adopted rank processes
        ended = None  # seconds at which no rank was running any more
        snapped = 0
        killed = False
        term_at = None
        while True:
            rc = p.poll()
            now = time.monotonic() - t0
            ranks = scan(a.match, p.pid, seen, exclude)
            running = [(pid, s) for pid, s in ranks if s not in ("Z", "X")]
            if running:
                ended = None
            elif seen and ended is None:
                ended = now
            if rc is not None:
                break
            if a.snapshot and a.snapshot_after > 0 and not snapped and now >= a.snapshot_after:
                snapshot(a.snapshot, p.pid, ranks, now)
                snapped = 1
            if not killed and now >= a.timeout:
                killed = True
                term_at = now
                try:
                    os.killpg(p.pid, signal.SIGTERM)
                except OSError:
                    pass
            if killed and term_at is not None and now >= term_at + a.kill_after:
                try:
                    os.killpg(p.pid, signal.SIGKILL)
                except OSError:
                    pass
                term_at = None
            time.sleep(0.02)
        seconds = time.monotonic() - t0
        if killed:
            rc = 124
        # a rank still running when the launcher returned (or was killed)
        if any(s not in ("Z", "X") for _, s in scan(a.match, p.pid, seen, exclude)):
            ended = None
        elif ended is None:
            ended = seconds  # ended between two scans, or never observed
        ranks_ended = -1.0 if ended is None else ended
    print(f"rc={rc} seconds={seconds:.2f} ranks_seen={len(seen)} "
          f"ranks_ended={ranks_ended:.2f} snapshot={snapped}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
