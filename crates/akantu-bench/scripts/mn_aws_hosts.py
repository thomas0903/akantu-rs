#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: readiness of the
# multi-node session's hosts before anything is uploaded, and the stop of
# every host when the cloud CLI cannot be used. Runs on the operator's
# machine; reaches the head directly and the other hosts through it (`-J`), with
# the strict-host-key client configuration of docs/TESTING.md (`-F
# mn-ssh.conf`). It never calls a cloud API. Standard library only.
"""Readiness of the session's hosts, and their stop.

    python3 -I mn_aws_hosts.py check -F mn-ssh.conf --head HEAD --workers W2 ... W8 \
                                     [--minutes 330] [--cap-h 5.0] [--min-free-gb 15] [--dry-run]
    python3 -I mn_aws_hosts.py halt  -F mn-ssh.conf --head HEAD --workers W2 ... W8 [--dry-run]

HEAD is the head's address reachable from here (its public address); the
workers are their private addresses, reached with `ssh -J HEAD`.

check, on every host (in parallel):
  1. `cloud-init status --wait` (up to 15 min): the user data has run;
  2. the shutdown timer read back from /run/systemd/shutdown/scheduled
     (`USEC=`, armed by the user data's `shutdown -h +MIN`);
  3. `docker info` answers, and the free space of / (at least --min-free-gb).
Hosts without a timer are then armed over SSH (`sudo shutdown -h +M`) for the
earliest timer of the others (or --minutes if none has one) and read back
again. The common deadline is the earliest timer. Output: one line per host,
`DEADLINE <epoch> <UTC>`, `BUDGET_H <h>` = min(--cap-h, (deadline − 20 min −
now) / 1 h) — the session is started with `--deadline <epoch>` (mn_session.py
derives the same bound itself) — and the verdict `HOSTS: READY` or `HOSTS: NOT
READY: <reasons>` (exit status 0 iff READY).

halt (the fallback when the cloud CLI's login has expired): `sudo shutdown -h
now` on every worker through the head, then on the head (instance-initiated
shutdown = terminate). Prints what it ran.

--dry-run prints the ssh commands instead of running them.
"""

import argparse
import shlex
import subprocess
import sys
import threading
import time

SCHEDULED = "/run/systemd/shutdown/scheduled"
MARGIN_S = 20 * 60


def parse_scheduled(text):
    """Epoch seconds of `USEC=<µs>` in a systemd shutdown schedule, or None."""
    for line in text.splitlines():
        k, _, v = line.strip().partition("=")
        if k == "USEC" and v.isdigit():
            return int(v) // 1_000_000
    return None


def arming_plan(deadlines, now, minutes):
    """{host: deadline or None} -> (target epoch, {host: minutes to arm} for hosts without one).
    The target is the earliest existing timer; with none, now + minutes."""
    have = [d for d in deadlines.values() if d]
    target = min(have) if have else now + 60 * minutes
    arm = {h: max(1, int((target - now) // 60)) for h, d in deadlines.items() if not d}
    return target, arm


def budget_h(deadline, now, cap):
    return min(cap, (deadline - MARGIN_S - now) / 3600)


class Hosts:
    def __init__(self, conf, head, workers, dry):
        self.conf, self.head, self.workers, self.dry = conf, head, workers, dry

    def argv(self, host, cmd):
        base = ["ssh", "-F", self.conf, "-o", "BatchMode=yes", "-o", "ConnectTimeout=15"]
        if host != self.head:
            base += ["-J", self.head]
        return base + [host, cmd]

    def run(self, host, cmd, timeout=120):
        a = self.argv(host, cmd)
        if self.dry:
            print("DRY " + shlex.join(a))
            return 0, ""
        try:
            p = subprocess.run(a, capture_output=True, text=True, timeout=timeout)
            return p.returncode, (p.stdout + p.stderr).strip()
        except subprocess.TimeoutExpired:
            return 124, "timeout"

    def each(self, fn):
        res, ts = {}, []
        for h in [self.head] + self.workers:
            t = threading.Thread(target=lambda h=h: res.__setitem__(h, fn(h)))
            t.start()
            ts.append(t)
        for t in ts:
            t.join()
        return res


def probe(hs, h, min_free_gb):
    out = {"problems": []}
    rc, txt = hs.run(h, "cloud-init status --wait", timeout=900)
    out["cloud_init"] = txt.splitlines()[-1] if txt else ""
    if rc != 0 or "done" not in out["cloud_init"]:
        out["problems"].append(f"cloud-init: {out['cloud_init'] or rc}")
    rc, txt = hs.run(h, f"cat {SCHEDULED} 2>/dev/null || true")
    out["deadline"] = parse_scheduled(txt)
    rc, txt = hs.run(h, "docker info --format '{{.ServerVersion}}' && df -B1 --output=avail / | tail -1")
    lines = txt.splitlines()
    if rc != 0 or len(lines) < 2 or not lines[-1].strip().isdigit():
        out["problems"].append(f"docker / df: {txt[-200:]}")
    else:
        out["docker"] = lines[0].strip()
        out["free_gb"] = int(lines[-1]) / 1e9
        if out["free_gb"] < min_free_gb:
            out["problems"].append(f"free space {out['free_gb']:.1f} GB < {min_free_gb} GB")
    return out


def utc(t):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(t))


def main(argv):
    p = argparse.ArgumentParser(prog="mn_aws_hosts.py", description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("cmd", choices=("check", "halt"))
    p.add_argument("-F", dest="conf", required=True)
    p.add_argument("--head", required=True)
    p.add_argument("--workers", nargs="*", default=[])
    p.add_argument("--minutes", type=int, default=330)
    p.add_argument("--cap-h", type=float, default=5.0)
    p.add_argument("--min-free-gb", type=float, default=15.0)
    p.add_argument("--dry-run", action="store_true")
    a = p.parse_args(argv)
    hs = Hosts(a.conf, a.head, a.workers, a.dry_run)
    if a.cmd == "halt":
        for h in a.workers + [a.head]:
            rc, txt = hs.run(h, "sudo shutdown -h now", timeout=60)
            print(f"{h}: sudo shutdown -h now: exit {rc} {txt[-120:]}")
        return 0
    res = hs.each(lambda h: probe(hs, h, a.min_free_gb))
    now = time.time()
    target, arm = arming_plan({h: r.get("deadline") for h, r in res.items()}, now, a.minutes)
    for h, m in arm.items():
        rc, txt = hs.run(h, f"sudo shutdown -h +{m} && cat {SCHEDULED}")
        res[h]["armed"] = f"+{m} min (exit {rc})"
        res[h]["deadline"] = parse_scheduled(txt) if not a.dry_run else target
    problems = []
    for h in [a.head] + a.workers:
        r = res[h]
        d = r.get("deadline")
        if not d:
            r["problems"].append("no shutdown timer after arming")
        print(f"{h:<18} cloud-init {r.get('cloud_init', '?'):<16} timer {utc(d) if d else 'NONE':<20}"
              f"{' armed ' + r['armed'] if r.get('armed') else ''} docker {r.get('docker', '?')} "
              f"free {r.get('free_gb', 0):.0f} GB" + (f"  PROBLEMS: {'; '.join(r['problems'])}" if r["problems"] else ""))
        problems += [f"{h}: {x}" for x in r["problems"]]
    ds = [r["deadline"] for r in res.values() if r.get("deadline")]
    if ds:
        dl = min(ds)
        print(f"DEADLINE {dl} {utc(dl)}")
        b = budget_h(dl, now, a.cap_h)
        print(f"BUDGET_H {b:.2f}")
        if b < 0.5:
            problems.append(f"only {b:.2f} h before the timer minus 20 min")
    print("HOSTS: " + ("READY" if not problems else "NOT READY: " + "; ".join(problems)))
    return 0 if not problems else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
