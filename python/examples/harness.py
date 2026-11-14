# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: runs upstream's Python examples (examples/python/**
# of Akantu, EPFL/LSMS; unchanged copies below python/examples/) with
# whichever `akantu` module is importable (the oracle's upstream module or
# the port's), collects and compares their outputs.
"""Python example harness (docs/TESTING.md).

  harness.py run CASE WORK [--side oracle|port] [--repo DIR]
      Run the case (cases.py) in WORK/run (fresh directory with the script
      and its inputs). The caller redirects stdout to WORK/stdout.txt (run
      with ``python3 -u`` so that the script's prints and the module's own
      output keep their order). Writes WORK/status.txt, WORK/extra.txt
      (``key = literal`` lines: captured sparse matrices, the script's
      numeric variables at the end of the run or at the stop line, probes)
      and WORK/values.npz (the arrays behind the digests, for diffs).
      Exit status 0 if the run ended as the case expects.
  harness.py collect OUT WORK [WORK ...]
      Summarise one or more runs of a case (the oracle's three runs, D-035)
      into the fixture directory OUT: status.txt, stdout.txt, digests.txt
      (SHA-256 of every output file), vtu_files.txt (per VTU file: number
      of data arrays, SHA-256 of their decoded payloads in order with the
      UD-110 / UD-114 rules applied, the rules applied), extra.txt,
      NONDETERMINISTIC (if the runs differ) and the small text outputs
      below files/.
  harness.py compare FIXTURE WORK [--oracle-work DIR]
      Compare a run with a fixture directory; with the oracle's raw run
      directory, report the largest difference of every differing value.
      Exit status 0 if everything is equal.
"""

import argparse
import ast
import base64
import hashlib
import importlib.util
import os
import pathlib
import shutil
import stat
import sys

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
SMALL = 64            # arrays up to this size are written out in extra.txt
KEEP_BYTES = 64 << 10  # small text outputs kept verbatim in fixtures
TEXT_SUFFIXES = {".txt", ".mtx", ".pvd", ".csv"}  # kept verbatim (digests cover all)


def load_cases():
    spec = importlib.util.spec_from_file_location("ex_cases", HERE / "cases.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.CASES


# --------------------------------------------------------------------------
# value summaries
# --------------------------------------------------------------------------
def summarize(value, arrays, key):
    """A literal summary of a numeric value (None for anything else); the
    full array goes to ``arrays[key]``."""
    import numpy as np

    if isinstance(value, bool) or isinstance(value, (np.bool_,)):
        return bool(value)
    if isinstance(value, (int, np.integer)):
        return int(value)
    if isinstance(value, (float, np.floating)):
        return {"f64": float(value).hex()}
    if isinstance(value, str):
        return value
    if isinstance(value, (list, tuple)) and value and all(
            isinstance(v, (int, float, np.integer, np.floating)) and
            not isinstance(v, bool) for v in value):
        value = np.asarray(value)
    if isinstance(value, np.ndarray) and value.dtype.kind in "biuf":
        a = np.ascontiguousarray(value)
        arrays[key] = a
        d = {"dtype": str(a.dtype), "shape": list(a.shape),
             "sha256": hashlib.sha256(a.tobytes()).hexdigest()}
        if a.size <= SMALL:
            if a.dtype.kind == "f":
                d["f64"] = [float(x).hex() for x in a.ravel()]
            else:
                d["values"] = a.ravel().tolist()
        return d
    return None


def namespace_summary(ns, prefix, arrays, out):
    import types

    for name in sorted(ns):
        if name.startswith("__"):
            continue
        v = ns[name]
        if isinstance(v, (types.ModuleType, types.FunctionType, type)):
            continue
        s = summarize(v, arrays, f"{prefix}{name}")
        if s is not None:
            out.append((f"{prefix}{name}", s))


# --------------------------------------------------------------------------
# run
# --------------------------------------------------------------------------
class StopRun(BaseException):
    """Raised by the line tracer in the oracle run at the stop line."""


def prepare(case, work, repo):
    run = work / "run"
    for old in (run, work / "tools"):
        if old.exists():
            shutil.rmtree(old)
    for old in ("extra.txt", "status.txt", "values.npz", "traceback.txt", "inputs.txt"):
        if (work / old).exists():
            (work / old).unlink()
    run.mkdir(parents=True)
    script = repo / "python/examples" / case["script"]
    shutil.copy2(script, run / script.name)
    inputs = dict(case["inputs"])
    inputs.update(case["replace"])
    names = [script.name]
    for name, src in inputs.items():
        shutil.copyfile(repo / src, run / name)
        names.append(name)
    if case["gmsh"]:
        tools = work / "tools"
        tools.mkdir()
        fake = tools / "gmsh"
        mesh = (repo / case["gmsh"]).resolve()
        fake.write_text(
            "#!/bin/sh\n"
            "# akantu-rs: stand-in for `gmsh -2 <name>.geo`: copies the\n"
            "# mesh the oracle image's Gmsh 4.13.1 produced from the same .geo\n"
            f"for a; do case \"$a\" in *.geo) cp '{mesh}' \"${{a%.geo}}.msh\";; esac; done\n")
        fake.chmod(fake.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    (work / "inputs.txt").write_text("".join(n + "\n" for n in names))
    return run, run / script.name


def run_case(name, work, side, repo):
    import numpy as np  # noqa: F401  (same import order on both sides)

    # Before the example imports matplotlib: Agg, the oracle's backend on its
    # display-less Linux host, so that `plt.show()` neither opens a window nor
    # blocks when the harness runs on macOS (default backend `macosx`).
    os.environ["MPLBACKEND"] = "Agg"
    case = load_cases()[name]
    run, script = prepare(case, work, repo)
    if case["gmsh"]:
        os.environ["PATH"] = f"{work / 'tools'}{os.pathsep}{os.environ['PATH']}"
    os.chdir(run)
    sys.path.insert(0, str(run))
    import akantu as aka

    extra = []
    arrays = {}
    captured = []
    if "sparse" in case["capture"]:
        base = aka.AkantuSparseMatrix

        class Capturing(base):
            def __init__(self, *a, **k):
                super().__init__(*a, **k)
                captured.append(self)

        aka.AkantuSparseMatrix = Capturing
    if case["compat"]:
        # the port's upstream-compatibility switch (absent upstream) on every
        # coupler / contact model the script creates
        def compat_of(orig):
            def make(*a, **k):
                c = orig(*a, **k)
                if hasattr(c, "setUpstreamCompat"):  # the port only
                    c.setUpstreamCompat(True)
                return c
            return make

        for cls in ("CouplerSolidPhaseField", "CouplerSolidContact",
                    "CouplerSolidCohesiveContact", "ContactMechanicsModel"):
            if hasattr(aka, cls):
                setattr(aka, cls, compat_of(getattr(aka, cls)))
    if case["preparse"]:
        aka.parseInput(str((repo / case["preparse"]).resolve()))

    ns = {"__name__": "__main__", "__file__": str(script)}
    stop = case["stop_line"]
    state = {"reached": False, "after": None}
    script_path = str(script)

    def tracer(frame, event, arg):
        if frame.f_code.co_filename != script_path:
            return None

        def local(frame, event, arg):
            # only the script's module-level statements count
            if event == "line" and frame.f_code.co_name == "<module>":
                if state["reached"] and frame.f_lineno != stop:
                    state["after"] = frame.f_lineno
                    raise StopRun()
                if frame.f_lineno == stop and not state["reached"]:
                    state["reached"] = True
                    namespace_summary(frame.f_locals, "state.", arrays, extra)
                    for p in case["probes"]:
                        try:
                            v = eval(p, frame.f_globals, frame.f_locals)
                            s = summarize(v, arrays, f"probe.{p}")
                        except Exception as e:  # recorded, compared
                            s = f"{type(e).__name__}"
                        extra.append((f"probe.{p}", s))
                    if side == "oracle":
                        raise StopRun()
            return local

        return local

    sys.argv = [script.name] + list(case["argv"])
    status = "completed"
    ok = True
    if case["check"] == "blocked":
        sys.settrace(tracer)
    try:
        with open(script, encoding="utf-8") as fh:
            code = compile(fh.read(), script_path, "exec")
        exec(code, ns)
    except StopRun:
        if side == "oracle" and state["reached"] and state["after"] is None:
            status = f"stopped before line {stop}"
        else:
            status = f"NOT REFUSED: line {stop} completed (next line {state['after']})"
            ok = False
    except SystemExit as e:
        status = f"SystemExit {e.code}"
        ok = e.code in (None, 0)
    except BaseException as e:
        import traceback

        # the module-level statement of the script that raised
        tb = e.__traceback__
        line = None
        while tb is not None:
            if tb.tb_frame.f_code.co_filename == script_path and line is None:
                line = tb.tb_lineno
            tb = tb.tb_next
        msg = str(e).splitlines()[0] if str(e) else ""
        status = f"raised {type(e).__module__}.{type(e).__name__} at line {line}: {msg}"
        if case["check"] == "blocked" and stop is None:
            ok = True  # exploration: the refused call is being located
        elif case["check"] == "blocked" and side == "port":
            ok = line == stop and state["reached"]
        elif case["check"] == "raises":
            # upstream's own run raises (a defect of the example or of its
            # binding): both runs must raise the same exception at the same
            # line, compared through status.txt, and what the script
            # computed until then is compared through the case's probes,
            # evaluated here on the script's namespace
            ok = True
            for p in case["probes"]:
                try:
                    v = eval(p, ns)
                    s = summarize(v, arrays, f"probe.{p}")
                except Exception as pe:  # recorded, compared
                    s = f"{type(pe).__name__}"
                extra.append((f"probe.{p}", s))
        else:
            ok = False
        with open(work / "traceback.txt", "w") as fh:
            traceback.print_exception(e, file=fh)
    finally:
        sys.settrace(None)
        sys.stdout.flush()
    for i, m in enumerate(captured):
        a = m.toarray()
        arrays[f"sparse.{i}"] = a
        extra.append((f"sparse.{i}.shape", list(a.shape)))
        extra.append((f"sparse.{i}.nnz", int(m.nnz)))
        extra.append((f"sparse.{i}.matrix_type",
                      str(m.aka_sparse.getMatrixType()).split(".")[-1]))
        extra.append((f"sparse.{i}.dense_sha256",
                      hashlib.sha256(np.ascontiguousarray(a).tobytes()).hexdigest()))
    if case["check"] != "blocked":
        namespace_summary(ns, "final.", arrays, extra)
    if status == "completed" and case["check"] not in ("blocked", "raises"):
        # the probes of a completed run, evaluated on the script's final
        # namespace (EX-044's `contact_surface` group)
        for p in case["probes"]:
            try:
                v = eval(p, ns)
                s = summarize(v, arrays, f"probe.{p}")
            except Exception as pe:  # recorded, compared
                s = f"{type(pe).__name__}"
            extra.append((f"probe.{p}", s))
    with open(work / "extra.txt", "w") as fh:
        for k, v in extra:
            fh.write(f"{k} = {v!r}\n")
    if arrays:
        np.savez_compressed(work / "values.npz", **{k.replace("/", "_"): v for k, v in arrays.items()})
    (work / "status.txt").write_text(f"side {side}\ncase {name}\nstatus {status}\n")
    return 0 if ok else 1


# --------------------------------------------------------------------------
# collect / compare
# --------------------------------------------------------------------------
def outputs(work):
    run = work / "run"
    inputs = set((work / "inputs.txt").read_text().split())
    found = []
    for root, dirs, files in os.walk(run):
        dirs[:] = [d for d in dirs if d != "__pycache__"]
        for f in files:
            rel = os.path.relpath(os.path.join(root, f), run)
            if rel not in inputs:
                found.append(rel)
    return sorted(found)


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for block in iter(lambda: fh.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


_WIDTH = {"Float64": "<f8", "Int64": "<i8", "UInt64": "<u8", "Int32": "<i4",
          "UInt32": "<u4", "Float32": "<f4", "UInt8": "u1", "Int8": "i1"}


def vtu_arrays_canonical(path, upstream):
    """``[(name, section, digest, rule)]`` of every data array of a VTU
    piece, ``digest`` = SHA-256 of the decoded payload (little endian,
    without the length header). With ``upstream`` (oracle files), the
    documented layout defects are mapped to what the port writes:

    * UD-110: a matrix array (``c = m*m >= 4`` components) holding
      ``c*c`` values per point / cell (each tuple written as a zero-filled
      ``c x c`` square whose top-left ``m x m`` block is the matrix) is
      reduced to its tuples (decided by the piece's point / cell count, so
      a correct array is never mapped);
    * UD-114: an empty payload whose base64 stream is invalid is empty.
    """
    import re

    import numpy as np

    doc = pathlib.Path(path).read_text()
    m = re.search(r'NumberOfPoints=\s*"(\d+)"\s*NumberOfCells=\s*"(\d+)"', doc)
    nb = {"Points": int(m.group(1)) if m else -1, "PointData": int(m.group(1)) if m else -1,
          "Cells": int(m.group(2)) if m else -1, "CellData": int(m.group(2)) if m else -1}
    out = []
    pos = 0
    while True:
        i = doc.find("<DataArray", pos)
        if i < 0:
            return out
        j = doc.index(">", i)
        k = doc.index("</DataArray>", j)
        tag, body, pos = doc[i:j], doc[j + 1:k], k
        before = doc[:i]
        section = max(("Points", "Cells", "PointData", "CellData"),
                      key=lambda t: before.rfind(f"<{t}>"))

        def attr(n, default=None):
            mm = re.search(r'\b%s="([^"]*)"' % n, tag)
            return mm.group(1) if mm else default

        name, ty = attr("Name", ""), attr("type")
        comps = int(attr("NumberOfComponents", "1"))
        rule = "-"
        if attr("format", "ascii") == "ascii":
            payload = " ".join(body.split()).encode()
        else:
            try:
                payload = base64.b64decode("".join(body.split()), validate=True)[4:]
            except ValueError:
                if not upstream:
                    out.append((name, section, "undecodable", "-"))
                    continue
                payload, rule = b"", "UD-114"
            sq = int(round(comps ** 0.5))
            n_ent = nb.get(section, -1)
            if (upstream and ty == "Float64" and comps >= 4 and sq * sq == comps
                    and n_ent > 0 and len(payload) == 8 * n_ent * comps * comps):
                a = np.frombuffer(payload, dtype="<f8").reshape(n_ent, comps, comps)
                rest = a.copy()
                rest[:, :sq, :sq] = 0
                if not np.any(rest != 0):
                    payload = np.ascontiguousarray(a[:, :sq, :sq]).astype("<f8").tobytes()
                    rule = "UD-110"
        out.append((name, section, hashlib.sha256(payload).hexdigest(), rule))


def vtu_canonical(path, upstream):
    """(digest of the piece's arrays in order, applied rules) of a VTU file."""
    arrays = vtu_arrays_canonical(path, upstream)
    h = hashlib.sha256("".join(f"{n}\t{s}\t{d}\n" for n, s, d, _ in arrays).encode())
    rules = sorted({f"{r}:{n}" for n, _, _, r in arrays if r != "-"})
    return h.hexdigest(), len(arrays), ",".join(rules) or "-"


def summary(work, upstream=False):
    """(stdout bytes, {file: digest}, {vtu file: (canonical digest, number
    of arrays, rules)}, extra text, status text)."""
    files = outputs(work)
    digests = {f: sha256(work / "run" / f) for f in files}
    vtu = {f: vtu_canonical(work / "run" / f, upstream) for f in files if f.endswith(".vtu")}
    extra = (work / "extra.txt").read_text() if (work / "extra.txt").exists() else ""
    status = (work / "status.txt").read_text() if (work / "status.txt").exists() else ""
    return ((work / "stdout.txt").read_bytes(), digests, vtu, extra, status)


def collect(out, works):
    out = pathlib.Path(out)
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    sums = [summary(pathlib.Path(w), upstream=True) for w in works]
    first = sums[0]
    nondet = []
    names = ["stdout.txt", "digests", "vtu_files", "extra.txt", "status.txt"]
    for s in sums[1:]:
        for n, a, b in zip(names, first, s):
            if a != b and n not in nondet:
                nondet.append(n)
    (out / "stdout.txt").write_bytes(first[0])
    (out / "digests.txt").write_text(
        "".join(f"{d}  {f}\n" for f, d in first[1].items()))
    (out / "vtu_files.txt").write_text(
        "".join(f"{f}\t{n}\t{d}\t{r}\n" for f, (d, n, r) in first[2].items()))
    (out / "extra.txt").write_text(first[3])
    (out / "status.txt").write_text(first[4].replace(
        "side ", f"runs {len(works)}\nside ", 1))
    if nondet:
        (out / "NONDETERMINISTIC").write_text("".join(n + "\n" for n in nondet))
    w0 = pathlib.Path(works[0])
    for f in first[1]:
        p = w0 / "run" / f
        if pathlib.Path(f).suffix in TEXT_SUFFIXES and p.stat().st_size <= KEEP_BYTES:
            dst = out / "files" / f
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(p, dst)
    return 1 if nondet else 0


def parse_extra(text):
    d = {}
    for line in text.splitlines():
        if " = " in line:
            k, v = line.split(" = ", 1)
            d[k] = ast.literal_eval(v)
    return d


def _numbers(line):
    toks = line.replace(",", " ").replace("[", " ").replace("]", " ").split()
    out = []
    for t in toks:
        try:
            out.append(float(t))
        except ValueError:
            out.append(t)
    return out


def _rel(a, b):
    import numpy as np

    a = np.asarray(a, dtype=float).ravel()
    b = np.asarray(b, dtype=float).ravel()
    if a.shape != b.shape:
        return None
    norm = max(np.max(np.abs(a)) if a.size else 0.0, 0.0)
    diff = np.max(np.abs(a - b)) if a.size else 0.0
    return float(diff), float(diff / norm) if norm > 0 else float(diff)


def mtx_upstream_digest(path):
    """SHA-256 of a Matrix Market file written by the port (17 significant
    digits, UD-061 corrected default) re-rendered with upstream's 15
    (``std::setprecision(digits10)``, i.e. ``%.15g``)."""
    h = hashlib.sha256()
    with open(path, encoding="ascii") as fh:
        header = True
        for line in fh:
            if header:
                h.update(line.encode())
                if not line.startswith("%"):
                    header = False
                continue
            t = line.split()
            if len(t) != 3:
                h.update(line.encode())
                continue
            h.update(f"{t[0]} {t[1]} {format(float(t[2]), '.15g')}\n".encode())
    return h.hexdigest()


EMPTY = hashlib.sha256(b"").hexdigest()


def _masked(stdout, masks):
    """``stdout`` with every line matching one of the regular expressions
    ``masks`` replaced by a marker (a case's ``stdout_mask``: wall-clock
    timings the script prints)."""
    if not masks:
        return stdout
    import re

    lines = stdout.decode(errors="surrogateescape").split("\n")
    out = []
    for line in lines:
        out.append("<masked: wall-clock timing>" if any(re.search(m, line) for m in masks)
                   else line)
    return "\n".join(out).encode(errors="surrogateescape")


def compare(fixture, work, oracle_work=None, ignore=(), stdout_mask=()):
    """(differences, notes): lines describing every difference (empty if the
    run equals the fixture) and the documented comparison rules applied
    (UD-110, UD-114, UD-061; ignored keys; masked timing lines)."""
    fixture = pathlib.Path(fixture)
    work = pathlib.Path(work)
    report = []
    notes = []
    stdout, digests, arrays, extra, status = summary(work)
    f_stdout = (fixture / "stdout.txt").read_bytes()
    if stdout_mask:
        stdout, f_stdout = _masked(stdout, stdout_mask), _masked(f_stdout, stdout_mask)
        notes.append(f"stdout lines matching {list(stdout_mask)} not compared (timings)")
    if stdout != f_stdout:
        a = f_stdout.decode(errors="replace").replace("\r", "\n").splitlines()
        b = stdout.decode(errors="replace").replace("\r", "\n").splitlines()
        worst = 0.0
        text_diff = 0
        for la, lb in zip(a, b):
            if la == lb:
                continue
            na, nb = _numbers(la), _numbers(lb)
            if len(na) != len(nb):
                text_diff += 1
                continue
            for x, y in zip(na, nb):
                if isinstance(x, float) and isinstance(y, float):
                    if x != y:
                        worst = max(worst, abs(x - y) / max(abs(x), 1e-300))
                elif x != y:
                    text_diff += 1
        report.append(f"stdout differs: {len(a)} vs {len(b)} lines, "
                      f"{text_diff} non-numeric differences, largest relative "
                      f"difference of a printed number {worst:.3g}")
    f_digests = {}
    for line in (fixture / "digests.txt").read_text().splitlines():
        d, f = line.split("  ", 1)
        f_digests[f] = d
    if set(f_digests) != set(digests):
        report.append(f"output files differ: missing {sorted(set(f_digests) - set(digests))[:5]}, "
                      f"extra {sorted(set(digests) - set(f_digests))[:5]}")
    f_vtu = {}
    for line in (fixture / "vtu_files.txt").read_text().splitlines():
        f, n, d, r = line.split("\t")
        f_vtu[f] = (d, int(n), r)
    bad_vtu = []
    rules = {}
    for f, (d, n, r) in sorted(f_vtu.items()):
        got = arrays.get(f)
        if got is None:
            continue
        if got[0] != d:
            bad_vtu.append(f)
        elif r != "-":
            for x in r.split(","):
                rules[x] = rules.get(x, 0) + 1
    for rule in ("UD-110", "UD-114"):
        hits = {k: v for k, v in rules.items() if k.startswith(rule + ":")}
        if hits:
            what = ", ".join(f"{k.split(':', 1)[1]} in {v} files" for k, v in sorted(hits.items()))
            notes.append(f"{rule}: {what} equal to upstream's arrays under the {rule} rule")
    for f in sorted(set(f_digests) & set(digests)):
        if f_digests[f] == digests[f] or f.endswith(".vtu"):
            continue
        if f.endswith(".mtx") and mtx_upstream_digest(work / "run" / f) == f_digests[f]:
            notes.append(f"UD-061: {f} equal with upstream's 15 significant digits")
        else:
            report.append(f"file differs: {f}")
    if bad_vtu:
        line = f"VTU files differ: {len(bad_vtu)} of {len(f_vtu)} (first {bad_vtu[0]})"
        if oracle_work is not None:
            line += "; " + _vtu_diffs(pathlib.Path(oracle_work), work, bad_vtu)
        report.append(line)
    fe, pe = parse_extra((fixture / "extra.txt").read_text()), parse_extra(extra)
    for k in fe:
        if k in ignore:
            notes.append(f"not compared: {k} ({ignore[k]})")
            continue
        if k not in pe:
            report.append(f"extra missing: {k}")
        elif fe[k] != pe[k]:
            line = f"extra differs: {k}"
            if oracle_work is not None:
                line += _value_diff(pathlib.Path(oracle_work), work, k)
            report.append(line)
    for k in pe:
        if k not in fe:
            report.append(f"extra only in this run: {k}")
    f_status = (fixture / "status.txt").read_text().splitlines()
    p_status = status.splitlines()
    fs = [x for x in f_status if x.startswith("status ")]
    ps = [x for x in p_status if x.startswith("status ")]
    stop = fs[0][len("status stopped before line "):] if fs and fs[0].startswith(
        "status stopped before line ") else None
    if stop is not None and ps and ps[0].startswith("status raised ") and \
            f" at line {stop}:" in ps[0]:
        notes.append(f"refused at line {stop} ({ps[0][len('status raised '):]}); "
                     f"the oracle run stopped before that line")
    elif fs != ps:
        report.append(f"status: fixture {fs} run {ps}")
    return report, notes


def _value_diff(ow, pw, key):
    import numpy as np

    try:
        a = np.load(ow / "values.npz")[key.replace("/", "_")]
        b = np.load(pw / "values.npz")[key.replace("/", "_")]
    except (OSError, KeyError):
        return ""
    r = _rel(a, b)
    return "" if r is None else f" (largest difference {r[0]:.3g}, relative {r[1]:.3g})"


def _vtu_values(path, name, raw=False):
    import numpy as np

    widths = _WIDTH
    doc = pathlib.Path(path).read_text()
    pos = 0
    while True:
        i = doc.find("<DataArray", pos)
        if i < 0:
            return None
        j = doc.index(">", i)
        k = doc.index("</DataArray>", j)
        tag, body, pos = doc[i:j], doc[j + 1:k], k
        if f'Name="{name}"' not in tag:
            continue
        ty = tag.split('type="')[1].split('"')[0]
        if 'format="ascii"' in tag or "format=" not in tag:
            return np.array([float(x) for x in body.split()])
        try:
            data = base64.b64decode("".join(body.split()), validate=True)
        except ValueError:  # UD-114
            return None
        a = np.frombuffer(data[4:], dtype=widths[ty])
        return a if raw else a.astype(float)


def _vtu_diffs(ow, pw, bad_files):
    """Arrays that differ in the given VTU files and their largest
    difference (raw oracle run needed)."""
    worst = (0.0, 0.0, None)
    names = set()
    for f in bad_files:
        o = {(n, sct): d for n, sct, d, _ in vtu_arrays_canonical(ow / "run" / f, True)}
        p = {(n, sct): d for n, sct, d, _ in vtu_arrays_canonical(pw / "run" / f, False)}
        for k in sorted(set(o) | set(p)):
            if o.get(k) == p.get(k):
                continue
            names.add(k[0])
            a = _vtu_values(ow / "run" / f, k[0])
            b = _vtu_values(pw / "run" / f, k[0])
            if a is None or b is None:
                continue
            r = _rel(a, b)
            if r and r[0] > worst[0]:
                worst = (r[0], r[1], f"{k[0]} in {f}")
    return (f"arrays {sorted(names)}; largest difference {worst[0]:.3g} "
            f"(relative {worst[1]:.3g}) at {worst[2]}")


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("case")
    r.add_argument("work")
    r.add_argument("--side", default="port", choices=["oracle", "port"])
    r.add_argument("--repo", default=str(REPO))
    c = sub.add_parser("collect")
    c.add_argument("out")
    c.add_argument("works", nargs="+")
    m = sub.add_parser("compare")
    m.add_argument("fixture")
    m.add_argument("work")
    m.add_argument("--oracle-work")
    m.add_argument("--case", help="case whose `ignore` keys apply")
    args = ap.parse_args()
    if args.cmd == "run":
        return run_case(args.case, pathlib.Path(args.work).resolve(), args.side,
                        pathlib.Path(args.repo).resolve())
    if args.cmd == "collect":
        return collect(args.out, args.works)
    case = load_cases().get(args.case) if args.case else None
    rep, notes = compare(args.fixture, args.work, args.oracle_work,
                         ignore=case["ignore"] if case else {},
                         stdout_mask=case["stdout_mask"] if case else ())
    for line in rep:
        print(line)
    if not rep:
        print("equal")
    for line in notes:
        print("note: " + line)
    return 1 if rep else 0


if __name__ == "__main__":
    sys.exit(main())
