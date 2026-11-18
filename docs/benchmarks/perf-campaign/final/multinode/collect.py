#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs (no upstream counterpart): copies the records of the
# multi-host sessions and of the final one-machine run into this directory,
# with the hosts' addresses and names replaced.
"""Copy and redact the records of one session directory.

    python3 -I collect.py OUT SESSION_DIR [SESSION_LOG] [--c1] [--root DIR] [--extra FILE ...]

SESSION_DIR is an mn_session.py results directory (check.json, runs/<run>/)
or a c1prep_campaign.sh run directory (<campaign>/rust/np<N>/rep<k>/, with
--c1). Per run it copies status.txt, summary.json, chrono.csv (rank 0),
wall.txt, binary.sha256, input.dat and hostfile, and writes outputs.sha256
(the SHA-256 of energies.csv and insertions.csv, which are not copied); per
session check.json, SESSION_LOG as session.log, and every --extra file
under its own name (FILE=NAME: under NAME).

Redaction: the hosts of a session are written host1 ... hostN in the order
of the first run's `hosts` (host1 holds rank 0), their names node-host1 ...;
home directories are written `~`, and the directory --root (where the
run's tool kept its data) `<root>`. An address or host name that is not one
of the session's hosts stops the copy.
"""

import hashlib
import json
import re
import sys
from pathlib import Path

RUN_FILES = ["status.txt", "summary.json", "chrono.csv", "wall.txt", "binary.sha256",
             "input.dat", "hostfile"]
OUTPUTS = ["energies.csv", "insertions.csv"]
IPV4 = re.compile(r"\b(?:\d{1,3}\.){3}\d{1,3}\b")
AWS_NAME = re.compile(r"\bip-\d{1,3}-\d{1,3}-\d{1,3}-\d{1,3}\b")
HOME = re.compile(r"/home/[A-Za-z0-9_.-]+")
# Dotted numbers that are versions, not addresses.
NOT_ADDRESSES = {"7.0.0-1014", "2.1.so.7", "5.0.7"}


class Redactor:
    def __init__(self, hosts, root=None):
        self.root = root
        self.map = {}
        for i, h in enumerate(hosts, 1):
            self.map[h] = f"host{i}"
            self.map["ip-" + h.replace(".", "-")] = f"node-host{i}"

    def __call__(self, text):
        if self.root:
            text = text.replace(self.root, "<root>")
        text = AWS_NAME.sub(lambda m: self._sub(m.group(0)), text)
        text = IPV4.sub(lambda m: self._sub(m.group(0)), text)
        return HOME.sub("~", text)

    def _sub(self, s):
        if s in self.map:
            return self.map[s]
        raise SystemExit(f"collect.py: {s!r} is not a host of this session")


def first_hosts(session):
    runs = sorted((session / "runs").iterdir())
    for d in runs:
        st = d / "status.txt"
        if st.exists():
            m = re.search(r"^hosts=(.*)$", st.read_text(), re.MULTILINE)
            if m:
                return json.loads(m.group(1))
    return []


def sha256(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def copy_run(src, dst, red):
    dst.mkdir(parents=True, exist_ok=True)
    for name in RUN_FILES:
        p = src / name
        if p.exists():
            (dst / name).write_text(red(p.read_text()))
    lines = [f"{sha256(src / n)}  {n}\n" for n in OUTPUTS if (src / n).exists()]
    (dst / "outputs.sha256").write_text("".join(lines))


def main(argv):
    c1 = "--c1" in argv
    argv = [a for a in argv if a != "--c1"]
    root = None
    if "--root" in argv:
        i = argv.index("--root")
        root = argv[i + 1]
        argv = argv[:i] + argv[i + 2:]
    extra = []
    if "--extra" in argv:
        i = argv.index("--extra")
        extra, argv = argv[i + 1:], argv[:i]
    out, session = Path(argv[1]), Path(argv[2])
    log = Path(argv[3]) if len(argv) > 3 else None
    out.mkdir(parents=True, exist_ok=True)
    if c1:
        red = Redactor([], root)
        copy_run(session, out / "run", red)
    else:
        red = Redactor(first_hosts(session), root)
        for d in sorted((session / "runs").iterdir()):
            if d.is_dir():
                copy_run(d, out / d.name, red)
        if (session / "check.json").exists():
            (out / "check.json").write_text(red((session / "check.json").read_text()))
    if log is not None:
        (out / "session.log").write_text(red(log.read_text()))
    for e in extra:
        src, _, name = e.partition("=")
        p = Path(src)
        (out / (name or p.name)).write_text(red(p.read_text()))
    for p in out.rglob("*"):
        if p.is_file():
            t = p.read_text()
            left = [s for s in IPV4.findall(t) + AWS_NAME.findall(t) if s not in NOT_ADDRESSES]
            if left:
                raise SystemExit(f"collect.py: {p}: unredacted {left[:3]}")
    print(f"collect.py: {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
