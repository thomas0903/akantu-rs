#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness (not a port of Akantu code).
"""Immutable build record of a benchmark driver build.

    python3 -I build_record.py record <config> <install> <build dir>  > build-record.json
    python3 -I build_record.py info <build-record.json>               > build-info.txt
    python3 -I build_record.py verify <build-record.json>             > runtime.json

`record` (scripts/build.sh, in the container) captures what the numbers of a
run depend on: the executable and the libakantu it loads (SHA-256, GNU
build-id), every shared library the executable resolves (path, size,
SHA-256), the harness sources (driver, interposer, CMakeLists, site
generator, generated site table), compile commands and flags of the driver,
the library's build info and oracle flags, the sparse solver that
`SparseSolverType::_auto` selects in this library (MUMPS when the install was
configured with AKANTU_USE_MUMPS: model_solver.cc:325-330, DOFManagerDefault),
the MPI version and the image id.

`verify` (in-container/run-one.sh, before every run) re-resolves the
libraries with the run's environment and fails (exit 3) unless the
executable, the resolved library paths and the content of every library in a
mutable location (/build, /perf: the oracle install volume and the scratch
volume) are those of the record, and the image id is the recorded one. It
prints the run-time check (re-hashed digests) for run.json. A run therefore
always carries the record of the build that produced it; collect.py and
make_fixtures.py read that copy, never the current build directory.
"""

import datetime
import hashlib
import json
import os
import re
import subprocess
import sys

MUTABLE_PREFIXES = ("/build/", "/perf/")
HARNESS = "/harness"
HARNESS_FILES = ("src/cohesive_extrinsic.cc", "src/joss_fnte_interposer.cc", "src/CMakeLists.txt",
                 "scripts/in-container/fnte_sites.py", "scripts/in-container/build_record.py",
                 "scripts/in-container/run-one.sh", "scripts/in-container/validate.sh",
                 "inputs/material-cohesive.dat", "inputs/material-elastic.dat")


class RecordError(Exception):
    pass


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def sh(*cmd):
    return subprocess.run(cmd, check=True, capture_output=True, text=True).stdout


def parse_ldd(text):
    """[(soname, resolved path)] of `ldd` output; raises on 'not found'."""
    libs = []
    for line in text.splitlines():
        line = line.strip()
        if "=> not found" in line:
            raise RecordError(f"unresolved library: {line}")
        m = re.match(r"^(\S+) => (/\S+) \(0x[0-9a-f]+\)$", line)
        if m:
            libs.append((m.group(1), m.group(2)))
        elif re.match(r"^/\S+ \(0x[0-9a-f]+\)$", line):  # the dynamic loader
            path = line.split()[0]
            libs.append((os.path.basename(path), path))
    return libs


def resolved_libraries(binary):
    return parse_ldd(sh("ldd", binary))


def build_id(path):
    m = re.search(r"Build ID:\s*([0-9a-f]+)", sh("readelf", "--notes", path))
    return m.group(1) if m else None


def is_mutable(path):
    return path.startswith(MUTABLE_PREFIXES)


def sparse_solver_auto(install):
    """What SparseSolverType::_auto resolves to for the default DOF manager."""
    cfg = os.path.join(install, "include", "akantu", "aka_config.hh")
    try:
        text = open(cfg).read()
    except OSError:
        return {"auto": "unknown", "evidence": f"{cfg} not readable"}
    mumps = re.search(r"^#define AKANTU_USE_MUMPS\b", text, re.M) is not None
    return {"auto": "mumps" if mumps else "eigen",
            "evidence": f"AKANTU_USE_MUMPS {'defined' if mumps else 'not defined'} in "
                        f"{cfg}; model_solver.cc:325-330 selects _mumps when the "
                        "factory knows it (DOFManagerDefault), else _eigen"}


def compile_commands(build_dir):
    out = {}
    try:
        entries = json.load(open(os.path.join(build_dir, "compile_commands.json")))
    except OSError:
        return out
    for e in entries:
        out[os.path.basename(e["file"])] = e.get("command") or " ".join(e.get("arguments", []))
    return out


def library_build_info(install):
    keys = ("reference_sha", "image_id", "compiler", "CMAKE_BUILD_TYPE",
            "CMAKE_CXX_FLAGS_RELWITHDEBINFO", "CMAKE_CXX_FLAGS_RELEASE", "AKANTU_EXTRA_CXX_FLAGS")
    info = {}
    path = os.path.join(install, "share", "akantu-oracle", "build-info.txt")
    for line in open(path):
        if line.startswith(keys):
            k, _, v = line.rstrip("\n").partition("=")
            info[k] = v
    flags = open(os.path.join(install, "share", "akantu-oracle", "oracle-flags.cmake")).read()
    m = re.search(r'AKANTU_ORACLE_CXX_FLAGS "([^"]*)"', flags)
    info["oracle_cxx_flags"] = m.group(1) if m else None
    return info


def digest(record):
    """SHA-256 of the canonical JSON form (the record's identity)."""
    return hashlib.sha256(json.dumps(record, sort_keys=True, separators=(",", ":"))
                          .encode()).hexdigest()


def record(config, install, build_dir):
    binary = os.path.join(build_dir, "cohesive_extrinsic")
    libs = []
    libakantu = None
    for soname, path in resolved_libraries(binary):
        real = os.path.realpath(path)
        entry = {"soname": soname, "path": path, "realpath": real,
                 "size": os.path.getsize(real), "sha256": sha256_file(real)}
        if soname.startswith("libakantu.so"):
            entry["build_id"] = build_id(real)
            libakantu = entry
        libs.append(entry)
    if libakantu is None:
        raise RecordError(f"{binary} does not resolve libakantu.so")
    sites_txt = open(os.path.join(build_dir, "fnte_sites.txt")).read()
    m = re.search(r"sha256=([0-9a-f]{64}) build_id=([0-9a-f]+)", sites_txt)
    if not m or m.group(1) != libakantu["sha256"] or m.group(2) != libakantu["build_id"]:
        raise RecordError("the call-site table was generated from another library than the "
                          f"one the executable resolves ({libakantu['realpath']})")
    harness = {}
    for rel in HARNESS_FILES:
        p = os.path.join(HARNESS, rel)
        harness[rel] = sha256_file(p) if os.path.exists(p) else None
    cc = compile_commands(build_dir)
    rec = {
        "format": "cohesive-perf build record v1",
        "config": config,
        "install": install,
        "install_ready": open(os.path.join(install, "READY")).read().strip(),
        "built_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "image": os.environ.get("PERF_IMAGE"),
        "image_id": os.environ.get("PERF_IMAGE_ID"),
        "machine": os.uname().machine,
        "binary": {"path": binary, "sha256": sha256_file(binary), "size": os.path.getsize(binary)},
        "libakantu": libakantu,
        "libraries": libs,
        "library_build_info": library_build_info(install),
        "sparse_solver": sparse_solver_auto(install),
        "driver_compile_command": cc.get("cohesive_extrinsic.cc"),
        "interposer_compile_command": cc.get("joss_fnte_interposer.cc"),
        "compiler": sh("c++", "--version").splitlines()[0],
        "mpi": sh("mpirun", "--version").splitlines()[0],
        "harness_sha256": harness,
        "fnte_sites_h_sha256": sha256_file(os.path.join(build_dir, "fnte_sites.h")),
        "fnte_sites": sites_txt.splitlines(),
    }
    if not rec["image_id"]:
        raise RecordError("PERF_IMAGE_ID is not set (run through scripts/common.sh perf_docker)")
    return rec


def info(rec):
    """Human-readable build-info.txt (same facts as the record)."""
    lines = [f"config={rec['config']}", f"install={rec['install']}",
             f"install_READY={rec['install_ready']}", f"built_utc={rec['built_utc']}",
             f"image={rec['image']}", f"image_id={rec['image_id']}", f"machine={rec['machine']}"]
    lines += [f"library_{k}={v}" for k, v in sorted(rec["library_build_info"].items())]
    lines += [f"driver_compile_command={rec['driver_compile_command']}",
              f"driver_sha256={rec['harness_sha256']['src/cohesive_extrinsic.cc']}",
              f"binary_sha256={rec['binary']['sha256']}",
              f"libakantu={rec['libakantu']['realpath']} sha256={rec['libakantu']['sha256']} "
              f"build_id={rec['libakantu']['build_id']}",
              f"sparse_solver_auto={rec['sparse_solver']['auto']} ({rec['sparse_solver']['evidence']})",
              f"compiler={rec['compiler']}", f"mpi={rec['mpi']}",
              f"build_record_sha256={digest(rec)}",
              "fnte_interposer_sites (JOSS patch emulation, fnte_sites.py):"]
    lines += ["  " + s for s in rec["fnte_sites"]]
    return "\n".join(lines) + "\n"


def verify(rec, image_id):
    """Run-time check of a record; returns the run-time facts or raises."""
    binary = rec["binary"]["path"]
    problems = []
    if image_id != rec["image_id"]:
        problems.append(f"image id {image_id} differs from the build's {rec['image_id']}")
    now_bin = sha256_file(binary)
    if now_bin != rec["binary"]["sha256"]:
        problems.append(f"{binary} changed since the build record (sha256 {now_bin})")
    resolved = [(s, p) for s, p in resolved_libraries(binary)]
    recorded = [(lib["soname"], lib["path"]) for lib in rec["libraries"]]
    if resolved != recorded:
        problems.append("the executable resolves other libraries than at build time: "
                        f"{sorted(set(resolved) ^ set(recorded))}")
    rehashed = {}
    for lib in rec["libraries"]:
        if not is_mutable(lib["realpath"]):
            continue
        real = os.path.realpath(lib["path"])
        h = sha256_file(real) if os.path.exists(real) else None
        rehashed[lib["path"]] = h
        if real != lib["realpath"] or h != lib["sha256"]:
            problems.append(f"{lib['path']} -> {real} (sha256 {h}) differs from the build "
                            f"record ({lib['realpath']}, {lib['sha256']})")
    if problems:
        raise RecordError("; ".join(problems))
    return {"build_record_sha256": digest(rec), "binary_sha256": now_bin,
            "mutable_libraries_sha256": rehashed,
            "libakantu_sha256": rec["libakantu"]["sha256"],
            "libakantu_build_id": rec["libakantu"]["build_id"],
            "sparse_solver_auto": rec["sparse_solver"]["auto"]}


def main():
    try:
        if sys.argv[1] == "record":
            rec = record(*sys.argv[2:5])
            print(json.dumps(rec, indent=1, sort_keys=True))
        elif sys.argv[1] == "info":
            sys.stdout.write(info(json.load(open(sys.argv[2]))))
        elif sys.argv[1] == "verify":
            rec = json.load(open(sys.argv[2]))
            print(json.dumps(verify(rec, os.environ.get("PERF_IMAGE_ID")), indent=1, sort_keys=True))
        else:
            sys.exit(f"unknown command {sys.argv[1]}")
    except (RecordError, OSError, KeyError, subprocess.CalledProcessError) as e:
        print(f"build_record.py {sys.argv[1]}: {e}", file=sys.stderr)
        sys.exit(3)


if __name__ == "__main__":
    main()
