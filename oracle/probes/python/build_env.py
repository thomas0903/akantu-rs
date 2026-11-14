# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: writes the `build` object of fixture manifests (consumed via
# AKANTU_PROBE_BUILD_INFO by the C++ and Python fixture writers) and binds
# a generation run to the oracle it uses.
"""Provenance of a fixture generation run.

Usage (inside akantu-oracle:dev, called by run-in-container.sh):

  build_env.py base <config> <mode> <flags_file> <out.json>
        [--expect-sha SHA] [--expect-image ID] [--allow-image-mismatch]
        [--install-prefix DIR] [--probe-tree DIR] [--library-compile-commands FILE]
        [--reference-tree DIR --patched-tree DIR --patches-out FILE]
        [--library KEY=VALUE]...
  build_env.py probe <base.json> <out.json> <probe_name>
        (--compile-commands FILE --source-suffix SUFFIX | --command CMD)
  build_env.py python <base.json> <out.json> <script> [--module-dir DIR]

`base`: in `installed` mode the flags file is O1's
share/akantu-oracle/build-info.txt. Its `reference_sha` and `config` must
equal the requested reference and configuration, and its `image_id` the
running image (unless --allow-image-mismatch, which records the mismatch);
otherwise the run is refused (exit 3): golden data never comes from a stale
or mismatched install. `cxx_flags` records the *library* flags: the actual
compile flags of a representative library translation unit
(src/model/solid_mechanics/solid_mechanics_model.cc) from the configured
tree's compile_commands.json when given, else CMAKE_CXX_FLAGS +
CMAKE_CXX_FLAGS_<BUILD_TYPE> from the cache. Content hashes identify the
probe sources (`probe_tree_sha256`), the installed library and Python module,
and each file of a patched oracle source tree (written to --patches-out as
`modified:<path> sha256=<hex>` entries for AKANTU_PROBE_PATCHES).

`probe`: per-probe build object with the *actual* compile command of the
probe translation unit (from the probe build's compile_commands.json, or the
explicit standalone command) in `probe_compile_command` and its compiler
flags (command minus compiler, include paths, -o/-c and the source file) in
`probe_cxx_flags`.

`python`: per-probe build object of a Python probe (no probe translation
unit: the kernels run inside libakantu, compiled with `cxx_flags`).
"""
from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import shlex
import subprocess
import sys
from pathlib import Path

PACKAGES = {
    "eigen3": "libeigen3-dev",
    "boost": "libboost-dev",
    "openmpi": "libopenmpi-dev",
    "mumps": "libmumps-dev",
    "petsc": "petsc-dev",
    "scotch": "libscotch-dev",
    "openblas": "libopenblas-dev",
    "scalapack": "libscalapack-openmpi-dev",
    "hdf5": "libhdf5-dev",
    "cgal": "libcgal-dev",
    "gmsh": "gmsh",
    "pybind11": "pybind11-dev",
    "numpy": "python3-numpy",
    "scipy": "python3-scipy",
    "gcc": "g++",
    "gfortran": "gfortran",
}

EXCLUDED_NAMES = {".git", "__pycache__"}


class ProvenanceError(Exception):
    """The oracle installation does not match what was requested."""


def dpkg_version(pkg: str) -> str | None:
    try:
        r = subprocess.run(["dpkg-query", "-W", "-f=${Version}", pkg],
                           capture_output=True, text=True, check=False)
    except OSError:
        return None
    return r.stdout.strip() or None


def cache_value(text: str, key: str) -> str:
    """Value of a CMake cache line `KEY[:TYPE]=value` or a `key=value` line."""
    m = re.search(rf"^{re.escape(key)}(?::[A-Z]+)?=(.*)$", text, re.M)
    return m.group(1).strip() if m else ""


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def tree_files(root: Path) -> list[Path]:
    out = []
    for p in sorted(root.rglob("*")):
        if any(part in EXCLUDED_NAMES for part in p.relative_to(root).parts):
            continue
        if p.is_file() and not p.name.endswith(".pyc"):
            out.append(p)
    return out


def tree_sha256(root: Path) -> str:
    """Content identity of a directory: SHA-256 over sorted
    `<sha256>  <relative path>` lines (the format of `sha256sum`)."""
    lines = "".join(f"{sha256_file(p)}  {p.relative_to(root).as_posix()}\n"
                    for p in tree_files(root))
    return hashlib.sha256(lines.encode()).hexdigest()


def patch_entries(reference: Path, patched: Path) -> list[str]:
    """Files of `patched` that differ from `reference`, with the SHA-256 of
    the patched content (`removed:` entries carry the reference hash)."""
    ref = {p.relative_to(reference).as_posix(): p for p in tree_files(reference)}
    pat = {p.relative_to(patched).as_posix(): p for p in tree_files(patched)}
    entries = []
    for rel in sorted(set(ref) | set(pat)):
        if rel in ref and rel in pat:
            a, b = sha256_file(ref[rel]), sha256_file(pat[rel])
            if a != b:
                entries.append(f"modified:{rel} sha256={b} (reference sha256={a})")
        elif rel in pat:
            entries.append(f"added:{rel} sha256={sha256_file(pat[rel])}")
        else:
            entries.append(f"removed:{rel} (reference sha256={sha256_file(ref[rel])})")
    return entries


def check_install(text: str, config: str, expect_sha: str | None,
                  expect_image: str | None, allow_image_mismatch: bool) -> dict:
    """Validate build-info.txt against the request; return its identifiers."""
    ids = {k: cache_value(text, k) for k in
           ("reference_sha", "config", "image_id", "scripts_sha", "compiler")}
    if not expect_sha:
        raise ProvenanceError("installed mode needs --expect-sha (the requested reference)")
    if not ids["reference_sha"]:
        raise ProvenanceError("build-info.txt does not record reference_sha; "
                              "cannot bind the fixtures to the requested reference")
    if ids["reference_sha"] != expect_sha:
        raise ProvenanceError(
            f"installed oracle was built from reference {ids['reference_sha']}, "
            f"the requested reference is {expect_sha}")
    if ids["config"] != config:
        raise ProvenanceError(
            f"installed oracle is configuration {ids['config']!r}, requested {config!r}")
    if expect_image and ids["image_id"] != expect_image:
        msg = (f"installed oracle was built in image {ids['image_id'] or '(unrecorded)'}, "
               f"the probes run in {expect_image}")
        if not allow_image_mismatch:
            raise ProvenanceError(msg + " (pass --allow-image-mismatch to record it anyway)")
        ids["image_mismatch"] = msg
    return ids


def compiler_version() -> str:
    try:
        return subprocess.run(["g++", "--version"], capture_output=True, text=True,
                              check=False).stdout.splitlines()[0]
    except (OSError, IndexError):
        return "unknown"


def os_name() -> str:
    osr = Path("/etc/os-release")
    if osr.is_file():
        m = re.search(r'^PRETTY_NAME="?([^"\n]*)"?', osr.read_text(), re.M)
        if m:
            return m.group(1)
    return platform.platform()


def library_flags(text: str) -> tuple[str, str]:
    build_type = cache_value(text, "CMAKE_BUILD_TYPE") or "RelWithDebInfo"
    flags = " ".join(x for x in (cache_value(text, "CMAKE_CXX_FLAGS"),
                                 cache_value(text, f"CMAKE_CXX_FLAGS_{build_type.upper()}")) if x)
    return flags, build_type


LIBRARY_UNIT = "src/model/solid_mechanics/solid_mechanics_model.cc"


def library_unit_flags(cc_file: Path) -> str:
    """Actual compile flags of the representative library unit."""
    entries = [e for e in json.loads(cc_file.read_text(encoding="utf-8"))
               if e["file"].endswith(LIBRARY_UNIT)]
    if len(entries) != 1:
        raise ProvenanceError(f"{cc_file}: {len(entries)} compile commands for {LIBRARY_UNIT}")
    e = entries[0]
    args = e["arguments"] if "arguments" in e else shlex.split(e["command"])
    # Keep the FP-relevant defines (EIGEN_*, NDEBUG); drop warnings and the
    # long list of feature defines.
    flags = [a for a in strip_command(args, LIBRARY_UNIT).split(" ")
             if not a.startswith(("-W", "-pedantic"))
             and (not a.startswith("-D") or a.startswith(("-DEIGEN_", "-DNDEBUG")))]
    return " ".join(flags) + (f" (actual flags of {LIBRARY_UNIT}; warnings and non-FP "
                              "-D defines omitted)")


def host_cpu(cpuinfo: str | None = None) -> str:
    try:
        text = Path("/proc/cpuinfo").read_text() if cpuinfo is None else cpuinfo
        m = re.search(r"^model name\s*:\s*(.+)$", text, re.M)
    except OSError:
        m = None
    return m.group(1).strip() if m else "unknown"


EIGEN_PIN_FLAGS = ("-DEIGEN_NO_CPUID", "-DEIGEN_DEFAULT_L1_CACHE_SIZE=16384",
                   "-DEIGEN_DEFAULT_L2_CACHE_SIZE=524288", "-DEIGEN_DEFAULT_L3_CACHE_SIZE=524288")
X86_64_V3_FEATURES = ("avx", "avx2", "bmi1", "bmi2", "f16c", "fma", "movbe", "xsave", "abm")


def eigen_pinned(flags: str) -> bool:
    """All four D-050 flags (no CPUID query, the three cache sizes)."""
    words = flags.split()
    return all(f in words for f in EIGEN_PIN_FLAGS)


def x86_64_level(cpuinfo: str | None = None) -> str:
    """`v3` when the CPU that runs the container has every x86-64-v3 feature
    (D-073 (2): the authoritative class; LZCNT appears as `abm`), else
    `below-v3 (missing: ...)`. Same rule as oracle/scripts/oracle-env.sh."""
    if cpuinfo is None:
        try:
            cpuinfo = Path("/proc/cpuinfo").read_text()
        except OSError:
            return "unknown (/proc/cpuinfo not readable)"
    m = re.search(r"^flags\s*:\s*(.*)$", cpuinfo, re.M)
    have = set(m.group(1).split()) if m else set()
    missing = [f for f in X86_64_V3_FEATURES if f not in have]
    return "v3" if not missing else "below-v3 (missing: " + " ".join(missing) + ")"


def eigen_statement(library_flags: str, probe_flags: str | None, python: bool = False) -> str:
    """What the COMPILE flags say about Eigen's cache sizes.
    The sizes live in one process-wide static shared by every module, and the
    first module that calls Eigen initialises it: the library's pin alone is
    not enough, the probe translation unit must carry it too. C++ probes add
    the value measured at run time (`eigen_cache_sizes_measured`,
    akantu_probe/eigen_pin.hh); never claim the pin otherwise."""
    if not eigen_pinned(library_flags):
        return "cpuid (host-dependent)"
    if python:
        return ("16384,524288,524288 (compile-time pin of libakantu and py11_akantu: "
                "EIGEN_NO_CPUID + defaults, D-050; no probe translation unit uses Eigen)")
    if probe_flags is None:
        return ("16384,524288,524288 in the library (EIGEN_NO_CPUID + defaults, D-050); "
                "probe translation unit not recorded")
    if eigen_pinned(probe_flags):
        return ("16384,524288,524288 (compile-time pin of the library and of the probe "
                "translation unit: EIGEN_NO_CPUID + defaults, D-050)")
    return ("NOT pinned: the probe translation unit was compiled without the D-050 flags "
            "(the shared Eigen static is initialised by whichever module calls Eigen "
            "first)")


def numerical_env(flags: str, machine: str | None = None,
                  environ: dict[str, str] | None = None,
                  cpuinfo: str | None = None) -> dict[str, str]:
    """x86_64 run-time environment pins of D-050, recorded as
    provenance: OpenBLAS kernel set and threads (MUMPS/PETSc results depend on
    them), what the library flags say about Eigen's cache sizes (refined per
    probe by `cmd_probe`/`cmd_python`), the host CPU and its x86-64 level
    (D-073 (2)). Empty on other architectures (aarch64 manifests unchanged)."""
    machine = platform.machine() if machine is None else machine
    env = os.environ if environ is None else environ
    if machine != "x86_64":
        return {}
    return {
        "openblas_coretype": env.get("OPENBLAS_CORETYPE") or "auto (unset: chosen from the host CPU)",
        "openblas_num_threads": env.get("OPENBLAS_NUM_THREADS") or "unset",
        "eigen_cache_sizes": eigen_statement(flags, None),
        "host_cpu": host_cpu(cpuinfo),
        "x86_64_level": x86_64_level(cpuinfo),
    }


def base_info(config: str, mode: str, text: str, ids: dict, extra: dict[str, str],
              library_cc: Path | None = None) -> dict:
    flags, build_type = library_flags(text)
    if library_cc is not None:
        extra = dict(extra, cmake_cxx_flags=flags)
        flags = library_unit_flags(library_cc)
    libs = {k: v for k, p in PACKAGES.items() if (v := dpkg_version(p))}
    image = Path("/usr/share/akantu-oracle-image-id")
    if os.environ.get("ORACLE_IMAGE_ID"):
        libs["oracle_image"] = os.environ["ORACLE_IMAGE_ID"]
    elif image.is_file():
        libs["oracle_image"] = image.read_text().strip()
    for key, lib_key in (("scripts_sha", "oracle_build_scripts"),
                         ("reference_sha", "oracle_install_reference_sha"),
                         ("image_id", "oracle_install_image")):
        if ids.get(key):
            libs[lib_key] = ids[key]
    libs.update(numerical_env(flags))
    libs.update(extra)
    info = {
        "config": config,
        "compiler": compiler_version(),
        "cxx_flags": flags,
        "build_type": build_type,
        "arch": platform.machine(),
        "os": os_name(),
        "libraries": libs,
    }
    if mode == "standalone":
        info["cxx_flags"] = (f"{flags} (library flags of the configured tree; standalone probe "
                             "build: header-only kernels + src/fe_engine/gauss_integration.cc + "
                             "src/common/aka_error.cc compiled with the probe flags, libakantu "
                             "not linked)")
    return info


def strip_command(args: list[str], source_suffix: str | None) -> str:
    """Compiler flags of a compile command: drop the compiler, include
    paths, output/input file and -c."""
    out, skip = [], False
    for i, a in enumerate(args):
        if skip:
            skip = False
            continue
        if i == 0:
            continue
        if a in ("-o", "-I", "-isystem", "-MF", "-MT", "-MQ"):
            skip = True
            continue
        if a.startswith(("-I", "-isystem", "-o")) or a in ("-c", "-MD", "-MMD"):
            continue
        if source_suffix and a.endswith(source_suffix):
            continue
        if a.endswith((".cc", ".cpp", ".cxx")):
            continue
        out.append(a)
    return " ".join(out)


def take(opts: list[str], name: str, default=None, multi: bool = False):
    vals, rest, i = [], [], 0
    while i < len(opts):
        if opts[i] == name:
            if i + 1 >= len(opts):
                raise SystemExit(f"build_env: {name} needs a value")
            vals.append(opts[i + 1])
            i += 2
        else:
            rest.append(opts[i])
            i += 1
    opts[:] = rest
    if multi:
        return vals
    return vals[-1] if vals else default


def flag(opts: list[str], name: str) -> bool:
    present = name in opts
    opts[:] = [o for o in opts if o != name]
    return present


def cmd_base(argv: list[str]) -> int:
    if len(argv) < 4:
        print(__doc__, file=sys.stderr)
        return 2
    config, mode, flags_file, out = argv[:4]
    opts = argv[4:]
    expect_sha = take(opts, "--expect-sha")
    expect_image = take(opts, "--expect-image")
    allow_image_mismatch = flag(opts, "--allow-image-mismatch")
    prefix = take(opts, "--install-prefix")
    probe_tree = take(opts, "--probe-tree")
    library_cc = take(opts, "--library-compile-commands")
    reference_tree = take(opts, "--reference-tree")
    patched_tree = take(opts, "--patched-tree")
    patches_out = take(opts, "--patches-out")
    extra = dict(kv.partition("=")[::2] for kv in take(opts, "--library", multi=True))
    if opts:
        print(f"build_env: unknown arguments {opts}", file=sys.stderr)
        return 2
    if mode not in ("installed", "standalone"):
        print(f"build_env: mode must be installed|standalone, got {mode!r}", file=sys.stderr)
        return 2
    path = Path(flags_file)
    if not path.is_file():
        print(f"build_env: flags file {flags_file} does not exist", file=sys.stderr)
        return 3
    text = path.read_text(encoding="utf-8")
    ids: dict = {}
    if mode == "installed":
        try:
            ids = check_install(text, config, expect_sha, expect_image, allow_image_mismatch)
        except ProvenanceError as e:
            print(f"build_env: provenance mismatch: {e}", file=sys.stderr)
            return 3
        if ids.get("image_mismatch"):
            extra["oracle_image_mismatch"] = ids["image_mismatch"]
        if prefix:
            libs = sorted(p for p in Path(prefix, "lib").glob("libakantu*.so*")
                          if p.is_file() and not p.is_symlink())
            for lib in libs:
                extra[f"sha256:{lib.name}"] = sha256_file(lib)
    if probe_tree:
        extra["probe_tree_sha256"] = tree_sha256(Path(probe_tree))
    if os.environ.get("AKANTU_PROBE_SOURCES_COMMIT"):
        extra["probe_sources_commit"] = os.environ["AKANTU_PROBE_SOURCES_COMMIT"]
    if patched_tree:
        if not (reference_tree and patches_out):
            print("build_env: --patched-tree needs --reference-tree and --patches-out",
                  file=sys.stderr)
            return 2
        entries = patch_entries(Path(reference_tree), Path(patched_tree))
        Path(patches_out).write_text(",".join(entries), encoding="utf-8")
    try:
        info = base_info(config, mode, text, ids, extra,
                         Path(library_cc) if library_cc else None)
    except (ProvenanceError, OSError, json.JSONDecodeError) as e:
        print(f"build_env: {e}", file=sys.stderr)
        return 3
    Path(out).write_text(json.dumps(info, indent=2) + "\n", encoding="utf-8")
    return 0


def cmd_probe(argv: list[str]) -> int:
    if len(argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    base, out, probe = argv[:3]
    opts = argv[3:]
    cc_file = take(opts, "--compile-commands")
    suffix = take(opts, "--source-suffix")
    command = take(opts, "--command")
    if opts or bool(cc_file) == bool(command):
        print("build_env probe: give --compile-commands/--source-suffix or --command",
              file=sys.stderr)
        return 2
    info = json.loads(Path(base).read_text(encoding="utf-8"))
    if cc_file:
        entries = [e for e in json.loads(Path(cc_file).read_text(encoding="utf-8"))
                   if e["file"].endswith(suffix)]
        if len(entries) != 1:
            print(f"build_env probe: {len(entries)} compile commands for {suffix}",
                  file=sys.stderr)
            return 3
        e = entries[0]
        args = e["arguments"] if "arguments" in e else shlex.split(e["command"])
        command = shlex.join(args)
    else:
        args = shlex.split(command)
    info["probe_compile_command"] = command
    info["probe_cxx_flags"] = strip_command(args, suffix)
    libs = info.setdefault("libraries", {})
    libs["probe"] = probe
    if "eigen_cache_sizes" in libs:  # x86_64 only (numerical_env)
        libs["eigen_cache_sizes"] = eigen_statement(
            libs.get("cmake_cxx_flags") or info.get("cxx_flags", ""), info["probe_cxx_flags"])
    Path(out).write_text(json.dumps(info, indent=2) + "\n", encoding="utf-8")
    return 0


def cmd_python(argv: list[str]) -> int:
    if len(argv) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    base, out, script = argv[:3]
    opts = argv[3:]
    module_dir = take(opts, "--module-dir")
    if opts:
        print(f"build_env python: unknown arguments {opts}", file=sys.stderr)
        return 2
    info = json.loads(Path(base).read_text(encoding="utf-8"))
    libs = info.setdefault("libraries", {})
    libs["python"] = platform.python_version()
    ident = ""
    if module_dir:
        mods = sorted(Path(module_dir).glob("akantu/py11_akantu*.so"))
        for m in mods:
            libs[f"sha256:{m.name}"] = sha256_file(m)
        ident = f" (akantu module {module_dir}/akantu)"
    if "eigen_cache_sizes" in libs:  # x86_64 only (numerical_env)
        libs["eigen_cache_sizes"] = eigen_statement(
            libs.get("cmake_cxx_flags") or info.get("cxx_flags", ""), None, python=True)
    info["probe_compile_command"] = f"python3 {script}{ident}"
    info["probe_cxx_flags"] = ("n/a: Python probe, no probe translation unit; the numerical "
                               "kernels run inside libakantu compiled with cxx_flags")
    Path(out).write_text(json.dumps(info, indent=2) + "\n", encoding="utf-8")
    return 0


def main(argv: list[str]) -> int:
    commands = {"base": cmd_base, "probe": cmd_probe, "python": cmd_python}
    if len(argv) < 2 or argv[1] not in commands:
        print(__doc__, file=sys.stderr)
        return 2
    return commands[argv[1]](argv[2:])


if __name__ == "__main__":
    sys.exit(main(sys.argv))
