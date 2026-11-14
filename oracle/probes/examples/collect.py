#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: collects the repeated runs of one example case
# (oracle/probes/examples/run-in-container.sh) into its fixture directory:
#   status.txt        exit codes of every run, command, ranks
#   stdout.txt        standard output of run 1 (first 200 kB)
#   stderr.txt standard error of run 1 (first 200 kB)
#                     without the runner's Eigen cache-pin lines (the
#                     contact examples print their energies there)
#   digests.txt       "<sha256>  <path>" of every output file of run 1
#   NONDETERMINISTIC  output files that differ between the runs (if any)
#   files/<path>      the output files selected by the `keep` column
#   vtu_arrays.txt    (`arrays` column yes) for every .vtu output of run 1,
#                     one line per data array:
#                     path, name, type, components, values, SHA-256 of the
#                     decoded binary payload (little endian, without the
#                     IOHelper length header), and `det` / `nondet` (whether
#                     the array is identical in every run)
# Inputs copied into the run directory (input files, meshes) and the
# runner's own files are not outputs. Exit status 1 if a run failed, the
# file sets differ, or the kept files exceed the 1 MiB fixture budget.
import base64
import fnmatch
import hashlib
import os
import shutil
import sys

LIMIT = 1 << 20
RUNNER_FILES = {"stdout.txt", "stderr.txt", "exit_code", "command.txt"}  # written by the runner


def outputs(run, inputs):
    found = []
    for root, _, files in os.walk(run):
        for f in files:
            rel = os.path.relpath(os.path.join(root, f), run)
            if rel in inputs or rel in RUNNER_FILES:
                continue
            found.append(rel)
    return sorted(found)


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for block in iter(lambda: fh.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def vtu_arrays(path):
    """(name, type, components, values, sha256 of the payload) of every
    <DataArray> of a VTU file written by the IOHelper ParaView writer."""
    widths = {"Float64": 8, "Int64": 8, "UInt64": 8, "Int32": 4, "UInt32": 4,
              "Float32": 4, "UInt8": 1, "Int8": 1}
    with open(path, encoding="utf-8") as fh:
        doc = fh.read()
    out = []
    pos = 0
    while True:
        i = doc.find("<DataArray", pos)
        if i < 0:
            return out
        j = doc.index(">", i)
        tag = doc[i:j]
        k = doc.index("</DataArray>", j)
        body = doc[j + 1:k]
        pos = k

        def attr(name, default=None):
            key = f' {name}="'
            a = tag.find(key)
            if a < 0:
                key = f'{name}="'
                a = tag.find(key)
                if a < 0:
                    return default
            a += len(key)
            return tag[a:tag.index('"', a)]

        ty = attr("type")
        name = attr("Name", "")
        comps = int(attr("NumberOfComponents", "1"))
        if attr("format", "ascii") == "ascii":
            tokens = body.split()
            payload = " ".join(tokens).encode()
            nvalues = len(tokens)
        else:
            try:
                raw = base64.b64decode("".join(body.split()), validate=True)
            except ValueError:  # UD-114: invalid stream for payloads < 2 bytes
                out.append((name, ty, comps, -1, "undecodable"))
                continue
            payload = raw[4:]
            nvalues = len(payload) // widths[ty]
        out.append((name, ty, comps, nvalues, hashlib.sha256(payload).hexdigest()))


def main():
    name, out, keep, ranks, arrays, inputs, *runs = sys.argv[1:]
    inputs = set(i for i in inputs.split(",") if i)
    os.makedirs(out, exist_ok=True)
    ok = True
    codes = []
    for r in runs:
        with open(os.path.join(r, "exit_code")) as fh:
            codes.append(fh.read().strip())
    if any(c != "0" for c in codes):
        ok = False
    sets = [outputs(r, inputs) for r in runs]
    if any(s != sets[0] for s in sets[1:]):
        ok = False
    files = sets[0]
    digests = {f: sha256(os.path.join(runs[0], f)) for f in files}
    nondet = [f for f in files
              if any(not os.path.exists(os.path.join(r, f)) or
                     sha256(os.path.join(r, f)) != digests[f] for r in runs[1:])]
    with open(os.path.join(out, "digests.txt"), "w") as fh:
        for f in files:
            fh.write(f"{digests[f]}  {f}\n")
    if nondet:
        with open(os.path.join(out, "NONDETERMINISTIC"), "w") as fh:
            fh.write("".join(f + "\n" for f in nondet))
    with open(os.path.join(out, "vtu_arrays.txt") if arrays == "yes" else os.devnull, "w") as fh:
        for f in files if arrays == "yes" else []:
            if not f.endswith(".vtu"):
                continue
            first = vtu_arrays(os.path.join(runs[0], f))
            others = [vtu_arrays(os.path.join(r, f)) for r in runs[1:]]
            for n, (aname, ty, comps, nvalues, digest) in enumerate(first):
                det = all(len(o) == len(first) and o[n] == first[n] for o in others)
                fh.write(f"{f}\t{aname}\t{ty}\t{comps}\t{nvalues}\t{digest}\t"
                         f"{'det' if det else 'nondet'}\n")
    if keep == "all":
        kept = files
    elif keep == "none":
        kept = []
    else:
        globs = keep.split(",")
        kept = [f for f in files if any(fnmatch.fnmatchcase(f, g) for g in globs)]
        missing = [g for g in globs if not any(fnmatch.fnmatchcase(f, g) for f in files)]
        if missing:
            print(f"{name}: keep patterns without a match: {missing}", file=sys.stderr)
            ok = False
    size = 0
    for f in kept:
        dst = os.path.join(out, "files", f)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copyfile(os.path.join(runs[0], f), dst)
        size += os.path.getsize(dst)
    with open(os.path.join(runs[0], "stdout.txt"), "rb") as fh:
        stdout = fh.read(200_000)
    with open(os.path.join(out, "stdout.txt"), "wb") as fh:
        fh.write(stdout)
    pin = b"akantu-rs eigen_cache_pin: "
    with open(os.path.join(runs[0], "stderr.txt"), "rb") as fh:
        stderr = b"".join(l for l in fh.read().splitlines(keepends=True)
                          if not l.startswith(pin))[:200_000]
    with open(os.path.join(out, "stderr.txt"), "wb") as fh:
        fh.write(stderr)
    with open(os.path.join(runs[0], "command.txt")) as fh:
        command = fh.read().strip()
    with open(os.path.join(out, "status.txt"), "w") as fh:
        fh.write(f"case: {name}\n")
        fh.write(f"command: {command}\n")
        fh.write(f"ranks: {ranks}\n")
        fh.write(f"runs: {len(runs)}, exit codes: {' '.join(codes)}\n")
        fh.write(f"output files: {len(files)}, identical file sets: {all(s == sets[0] for s in sets)}\n")
        fh.write(f"nondeterministic files: {len(nondet)}\n")
        fh.write(f"kept files: {len(kept)} ({size} bytes)\n")
    if size > LIMIT:
        print(f"{name}: kept files ({size} bytes) exceed the 1 MiB budget", file=sys.stderr)
        ok = False
    print(f"[{name}] exit {codes}, {len(files)} outputs, {len(nondet)} nondeterministic, "
          f"{len(kept)} kept ({size} B)")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
