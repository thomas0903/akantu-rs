#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
"""Check that the tests the parity matrix cites exist (and passed).

Every code span of the *Tests* column of `docs/FEATURE_PARITY.md` that
names a test is resolved:

    python3 scripts/parity/check_cited_tests.py --source [--rows ID,AREA,...] [-v]
    python3 scripts/parity/check_cited_tests.py <job-dir> [<job-dir> ...] [--rows ...] [-v]
    python3 scripts/parity/check_cited_tests.py --self-test

`--source` resolves the citations against the source tree: Rust test
functions (`#[test]`, `#[tokio::test]`, `#[rstest]` and test macros' named
functions) of `crates/*/src` and `crates/*/tests`, integration-test
binaries (`crates/<c>/tests/<bin>.rs` or `<bin>/main.rs`), and Python test
files under `python/` (`file.py` or `file.py::name`). This is the mode the
fast tier runs (`crates/akantu-validation/tests/parity_scripts.rs`).

With job directories, the citations are resolved against the test logs of
a test run (`cargo test` output, `test.log`, `feature-*.log`, `par.log`,
with the `*-artifacts.jsonl` files of `cargo test --no-run
--message-format=json` that map each test executable to its package, as
the merge gate writes them): every cited test must have passed. A
citation that no log contains but the source defines is reported as "not
run in these logs" (a feature-gated or MPI binary whose tests the logs do
not list); it fails only with `--strict`.

Which code spans name a test: a span with `::` whose segments are
lower-case Rust paths (`types::tests::limits`, `test_array.rs::*::reserve`,
`upstream_cohesive::{a, b}`), a span ending in `.rs` or `.py` (a test
binary or a Python test file, optionally followed by `::name`). A span
`akantu-<crate>` qualifies the next span (`akantu-geometry`
`test_grid::…`); `crates/<c>/tests/…` and `<c>/tests/…` paths name the
crate explicitly. Spans with a space, upper-case segments (Rust types and
upstream symbols), repository paths (`fixtures/…`, `oracle/…`, `docs/…`,
`scripts/…`, `ci/…`) and parenthesised text name no test and are skipped.
`*` globs and `{a, b}` alternatives are allowed; every alternative is
checked on its own, and an explicit crate is enforced. Exit 1 if any cited
test is missing (or failed).
"""

from __future__ import annotations

import fnmatch
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MATRIX = ROOT / "docs" / "FEATURE_PARITY.md"

AREA_CRATE = {"MAT": "akantu-solid", "PF": "akantu-phase-field", "CORE": "akantu-core", "DOC": "akantu"}
TEST_ATTR = re.compile(r"^\s*#\[(?:\w+::)*(?:test|rstest|tokio::test|test_case)\b")
FN_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_]\w*)")
MOD_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*\{")


# --------------------------------------------------------------- matrix ---
def split_row(line: str) -> list[str]:
    parts = re.split(r"(?<!\\)\|", line.strip())
    return [c.strip() for c in parts[1:-1]]


def matrix_cells(text: str, column: str = "Tests") -> list[tuple[str, str]]:
    out, header = [], None
    for line in text.splitlines():
        if line.startswith("| ID |"):
            header = split_row(line)
            continue
        if header and line.startswith("| ") and not re.fullmatch(r"\|[-: |]+\|", line.strip()):
            cells = dict(zip(header, split_row(line)))
            if column in cells and re.fullmatch(r"[A-Z]+-\d+", cells.get("ID", "")):
                out.append((cells["ID"], cells[column]))
    return out


# ---------------------------------------------------------- source index --
class SourceIndex:
    """Rust test functions, test binaries and Python test files of a tree."""

    def __init__(self, root: Path):
        self.root = root
        self.tests: list[tuple[str, str, str]] = []  # (crate, binary, full path)
        self.binaries: dict[str, set[str]] = defaultdict(set)  # binary -> crates
        self.py_files: dict[str, list[Path]] = defaultdict(list)
        self.words: dict[tuple[str, str], set[str]] = defaultdict(set)  # (crate, binary) -> identifiers
        for crate_dir in sorted((root / "crates").glob("*")):
            if not (crate_dir / "Cargo.toml").is_file():
                continue
            crate = crate_dir.name
            for f in sorted(crate_dir.rglob("*.rs")):
                rel = f.relative_to(crate_dir)
                parts = rel.parts
                if parts[0] == "target":
                    continue
                if parts[0] == "tests":
                    if len(parts) == 2:
                        binary, mod = rel.stem, []
                        self.binaries[binary].add(crate)
                    else:
                        binary = parts[1] if parts[-1] == "main.rs" else "?"
                        if parts[-1] == "main.rs":
                            self.binaries[binary].add(crate)
                        mod = [p for p in parts[1:-1]] + ([] if rel.stem in ("main", "mod") else [rel.stem])
                        if parts[-1] == "main.rs":
                            mod = []
                elif parts[0] in ("src", "benches", "examples"):
                    binary = "lib" if parts[0] == "src" else parts[0]
                    mod = list(parts[1:-1]) + ([] if rel.stem in ("lib", "main", "mod") else [rel.stem])
                else:
                    continue
                self._scan(crate, binary, mod, f)
        for f in sorted((root / "python").rglob("*.py")) if (root / "python").is_dir() else []:
            self.py_files[f.name].append(f)

    def _scan(self, crate: str, binary: str, mod: list[str], path: Path) -> None:
        text = path.read_text(errors="replace")
        self.words[(crate, binary)].update(re.findall(r"\b[a-z_][a-z0-9_]*\b", text))
        stack: list[tuple[str, int]] = []
        depth = 0
        pending_test = False
        for line in text.splitlines():
            code = line.split("//", 1)[0]
            m = MOD_RE.match(code)
            if m:
                stack.append((m.group(1), depth))
            if TEST_ATTR.match(code):
                pending_test = True
            fm = FN_RE.match(code)
            if fm:
                if pending_test:
                    full = "::".join(mod + [s for s, _ in stack] + [fm.group(1)])
                    self.tests.append((crate, binary, full))
                pending_test = False
            depth += code.count("{") - code.count("}")
            while stack and depth <= stack[-1][1]:
                stack.pop()

    def runs(self):
        """The index as pseudo test results (package, binary, name, 'ok');
        every test binary also gets an entry (binaries with a custom
        harness or macro-generated tests have no indexed function)."""
        out = [(c, b, n, "ok") for c, b, n in self.tests]
        out += [(c, b, "", "ok") for b, cs in self.binaries.items() for c in cs]
        return out

    def defines(self, alt: str, crate: str | None) -> bool:
        """Lenient source check for a citation the function index does not
        resolve (tests generated by macros, custom harnesses): its binary
        exists (when it names one) and its last path segment is an
        identifier of that binary (or of the crate)."""
        segs = alt.split("::")
        first = segs[0][:-3] if segs[0].endswith(".rs") else segs[0]
        first = first.split("/")[-1]
        last = segs[-1]
        prefixes = None
        if re.fullmatch(r"[a-z]\d+(?:/[a-z]?\d+)+", last):
            # `values::f03/f04/f05`: the tests whose names start with f03_, f04_, …
            prefixes = [p + "_*" for p in last.split("/")]
            last = None
        elif not re.fullmatch(r"[a-z0-9_*?\[\]]+", last) or last == "*":
            last = None
        keys = [k for k in self.words if crate is None or k[0] == crate]
        if first in self.binaries:
            keys = [k for k in keys if k[1] == first]
            if not keys:
                return False
            if prefixes is not None:
                return all(any(fnmatch.filter(self.words[k], p) for k in keys) for p in prefixes)
            if len(segs) == 1 or last is None:
                return True
        if last is None:
            return False
        return any(fnmatch.filter(self.words[k], last) for k in keys)


# ------------------------------------------------------------ test logs ---
def load_runs(job: Path):
    """Tests that ran in every log of a job directory:
    list of (package, binary, name, result)."""
    exe_pkg = {}
    for art in sorted(job.glob("*artifacts.jsonl")):
        for line in art.read_text(errors="replace").splitlines():
            try:
                d = json.loads(line)
            except json.JSONDecodeError:
                continue
            if d.get("reason") == "compiler-artifact" and d.get("executable"):
                pid = d["package_id"]
                m = re.search(r"#([\w-]+)@", pid) or re.search(r"/([\w-]+)(?:#[^/]*)?$", pid)
                pkg = m.group(1) if m else pid
                exe_pkg[Path(d["executable"]).name] = (pkg, d["target"]["name"], d["target"]["kind"])
    runs = []
    for log in sorted(job.glob("*.log")):
        pkg = binary = None
        for line in log.read_text(errors="replace").splitlines():
            m = re.match(r"^\s+Running (.+?) \((\S+)\)\s*$", line)
            if m:
                exe = Path(m.group(2)).name
                pkg, target, kind = exe_pkg.get(exe, (None, re.sub(r"-[0-9a-f]+$", "", exe), ["?"]))
                if pkg is None:
                    mm = re.match(r"(?:unittests )?(?:src|tests)/", m.group(1))
                    pkg = None if not mm else None
                binary = "lib" if "lib" in kind or m.group(1).startswith("unittests src/lib.rs") else target
                continue
            m = re.match(r"^\s+Doc-tests (\S+)", line)
            if m:
                pkg, binary = m.group(1).replace("_", "-"), "doctest"
                continue
            m = re.match(r"^test (.+?) \.\.\. (ok|FAILED|ignored)\b", line)
            if m and binary is not None:
                runs.append((pkg, binary, m.group(1), m.group(2)))
    return runs


# ----------------------------------------------------------- resolution ---
def expand_braces(s: str) -> list[str]:
    m = re.search(r"\{([^{}]*)\}", s)
    if not m:
        return [s]
    out = []
    for alt in m.group(1).split(","):
        out.extend(expand_braces(s[: m.start()] + alt.strip() + s[m.end():]))
    return out


def resolve(span: str, explicit_crate: str | None, default_crate: str | None, runs, doc: bool):
    """Per brace alternative of one code span: `(alternative, hits, how)`."""
    binaries = defaultdict(list)
    for r in runs:
        binaries[r[1]].append(r)
    out = []
    for pat in expand_braces(span):
        pat = re.sub(r"::<[^>]*>$", "", pat.strip())
        m = re.match(r"^(?:crates/)?(akantu[\w-]*)/tests/(.+)$", pat)
        pcrate = explicit_crate
        if m:
            pcrate, pat = m.group(1), m.group(2)

        def in_crate(hits):
            return [r for r in hits if pcrate is None or r[0] is None or r[0] == pcrate]

        if doc and pat.startswith("manual::"):
            h = in_crate([r for r in runs if r[1] == "doctest" and f" - {pat} (" in r[2]])
            out.append((pat, h, f"doc-tests {pat}"))
            continue
        if pat.endswith(".rs") and "::" not in pat:
            b = pat[:-3].split("/")[-1]
            hits = [r for name in fnmatch.filter(list(binaries), b) for r in binaries[name]]
            out.append((pat, in_crate(hits), f"binary {b}" + (f" in {pcrate}" if pcrate else "")))
            continue
        first, _, rest = pat.partition("::")
        first = (first[:-3] if first.endswith(".rs") else first).split("/")[-1]
        if rest and first in binaries and first != "lib":
            h = in_crate([r for r in binaries[first] if fnmatch.fnmatchcase(r[2], rest) or fnmatch.fnmatchcase(r[2], "*::" + rest)])
            if h or pcrate is not None or pat.split("::")[0].endswith(".rs"):
                out.append((pat, h, f"binary {first} :: {rest}" + (f" in {pcrate}" if pcrate else "")))
                continue
        h = in_crate([
            r
            for r in runs
            if r[1] != "doctest" and (fnmatch.fnmatchcase(r[2], pat) or fnmatch.fnmatchcase(r[2], "*::" + pat))
        ])
        if pcrate is None and default_crate and any(r[0] == default_crate for r in h):
            h = [r for r in h if r[0] == default_crate]
        out.append((pat, h, f"name {pat}" + (f" in {pcrate}" if pcrate else "")))
    return out


SKIP_PREFIX = ("fixtures/", "oracle/", "docs/", "scripts/", "ci/", "benchmarks/", "joss/", "test/", "examples/")


def is_test_span(sp: str) -> bool:
    """Does a code span name a test (see the module docstring)?"""
    flat = sp
    while True:
        f2 = re.sub(r"\{[^{}]*\}", "x", flat)
        if f2 == flat:
            break
        flat = f2
    flat = re.sub(r"::<[^>]*>$", "", flat)
    flat = re.sub(r"\[[^\]]*\]", "", flat)
    if " " in flat or sp.startswith(SKIP_PREFIX) or sp.startswith("--"):
        return False
    if re.search(r"\.(rs|py)(::|$)", flat):
        return True
    if "::" not in flat:
        return False
    for seg in flat.split("::"):
        if not re.fullmatch(r"[a-z0-9_*.\-/]+", seg):
            return False
    return True


def check_python(sp: str, index: SourceIndex | None, root: Path):
    """A Python test citation: the file exists (and the named function)."""
    path, _, name = sp.partition("::")
    name = re.sub(r"\[.*$", "", name)
    cands = [root / path] if "/" in path else []
    cands = [c for c in cands if c.is_file()]
    if not cands and index is not None:
        cands = index.py_files.get(Path(path).name, [])
    if not cands:
        return f"`{sp}`: Python file not found"
    if name:
        for c in cands:
            defs = re.findall(r"^\s*(?:async\s+)?def (\w+)", c.read_text(errors="replace"), re.M)
            if fnmatch.filter(defs, name):
                return None
        return f"`{sp}`: no `def {name}` in {', '.join(str(c.relative_to(root)) for c in cands)}"
    return None


def crate_of(sp: str) -> str | None:
    m = re.match(r"^(?:crates/)?(akantu[\w-]*)/tests/", sp)
    return m.group(1) if m else None


def check_cell(row: str, cell: str, runs, fallback=None, root: Path = ROOT, index=None, lenient=False):
    """Problems, notes and resolutions of one Tests cell.

    `runs` are the results to resolve against; `fallback` (the source
    index's pseudo results) resolves what `runs` lack, reported in
    `notes` as not run."""
    area = row.split("-")[0]
    default_crate = AREA_CRATE.get(area)
    problems, notes, resolved = [], [], []
    # parenthesised text qualifies the preceding test: it names no test
    stripped = cell
    while True:
        s2 = re.sub(r"\([^()`]*\)", "", stripped)
        if s2 == stripped:
            break
        stripped = s2
    stripped = re.sub(r"`(\w+)` unit tests", r"`\1::*`", stripped)
    spans = re.findall(r"`([^`]+)`", stripped)
    explicit = None
    for sp in spans:
        if re.fullmatch(r"akantu[\w-]*", sp):
            explicit = sp  # qualifies the next span
            continue
        qualifier, explicit = explicit, None
        if not is_test_span(sp):
            continue
        if re.search(r"\.py(::|$)", sp):
            if "/" not in sp.split("::")[0] and not Path(sp.split("::")[0]).name.startswith("test_"):
                continue  # an upstream script named for reference (`py_mazars.py`)
            for alt in expand_braces(sp):
                p = check_python(alt, index, root)
                if p:
                    problems.append(p)
            continue
        if re.match(r"^(?:crates/[\w-]+/)?(?:src|tests)/\S+\.rs$", sp) and "::" not in sp and (
            sp.startswith("src/") or sp.endswith("/mod.rs")
        ):
            rel = re.sub(r"^crates/[\w-]+/", "", sp)
            crates = [qualifier] if qualifier else [p.name for p in (root / "crates").glob("*")]
            if not any((root / "crates" / c / rel).is_file() for c in crates if c):
                problems.append(f"`{sp}`: source file not found")
            continue
        doc = "doc-tests" in stripped.split(f"`{sp}`")[0][-12:] or sp.startswith("manual::")
        for alt, hits, how in resolve(sp, qualifier, default_crate, runs, doc):
            if not hits and lenient and index is not None and index.defines(alt, qualifier or crate_of(sp)):
                resolved.append((alt, 0, [("source", "identifier")]))
                continue
            if not hits and fallback is not None:
                fb = resolve(alt, qualifier, default_crate, fallback, doc)[0][1]
                if fb or (index is not None and index.defines(alt, qualifier or crate_of(sp))):
                    notes.append(f"`{alt}`: not run in these logs, defined in the source ({how})")
                    continue
            if not hits:
                problems.append(f"`{sp}` -> `{alt}`: no test found ({how})")
                continue
            bad = [h for h in hits if h[3] != "ok"]
            if bad and not any(h[3] == "ok" for h in hits):
                problems.append(
                    f"`{sp}` -> `{alt}`: {len(bad)} of {len(hits)} not ok: "
                    + ", ".join(f"{b[0]}/{b[1]}/{b[2]} {b[3]}" for b in bad[:5])
                )
            resolved.append((alt, len(hits), sorted({(str(h[0]), h[1]) for h in hits})))
    return problems, notes, resolved


# ------------------------------------------------------------- self-test --
def self_test() -> None:
    """A synthetic log: a renamed test inside a brace group, a test cited in
    the wrong crate, an ignored test and a missing binary are reported; the
    correct citations pass. A synthetic source tree: test functions in
    nested modules, binaries and Python files are found."""
    runs = [
        ("akantu-core", "lib", "types::tests::limits", "ok"),
        ("akantu-core", "lib", "types::tests::sentinels", "ok"),
        ("akantu-core", "test_array", "real::reserve", "ok"),
        ("akantu-solid", "review_v34_regressions", "mv03_phasefield_damaged_thermal_cycle", "ok"),
        ("akantu-solid", "review_v34_regressions", "mv03_phasefield_undamaged_thermal_cycles", "ignored"),
        ("akantu-mesh", "parallel_oracle", "distribution", "ok"),
        ("akantu", "doctest", "crates/akantu/src/../x.md - manual::Output (line 1)", "ok"),
    ]
    good = [
        ("CORE-001", "`types::tests::{limits, sentinels}`"),
        ("CORE-012", "`akantu-core/tests/test_array.rs`"),
        ("CORE-012", "`akantu-core` `test_array.rs::*::reserve`"),
        ("TEST-001", "`akantu-core` `test_array.rs::*::reserve`, `parallel_oracle.rs`"),
        ("CORE-009", "`parallel_oracle.rs` (distributed flags), `ElementType::ALL`, `fixtures/x/y`"),
        ("DOC-008", "doc-tests `manual::Output`"),
    ]
    for row, cell in good:
        problems, _, _ = check_cell(row, cell, runs)
        assert not problems, (row, cell, problems)
    bad = [
        ("CORE-001", "`types::tests::{limits, renamed_limits}`", "renamed_limits"),
        ("MAT-030", "`review_v34_regressions::{mv03_phasefield_damaged_thermal_cycle, mv03_renamed}`", "mv03_renamed"),
        ("CORE-012", "`akantu-solid/tests/test_array.rs`", "binary test_array in akantu-solid"),
        ("CORE-012", "`akantu-solid` `test_array.rs`", "binary test_array in akantu-solid"),
        ("MAT-030", "`review_v34_regressions::mv03_phasefield_undamaged_thermal_cycles`", "ignored"),
        ("CORE-009", "`missing_binary.rs`", "binary missing_binary"),
    ]
    for row, cell, why in bad:
        problems, _, _ = check_cell(row, cell, runs)
        assert any(why in p for p in problems), (row, cell, why, problems)

    import tempfile

    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        (root / "crates/akantu-x/src/inner").mkdir(parents=True)
        (root / "crates/akantu-x/tests/multi").mkdir(parents=True)
        (root / "python/tests").mkdir(parents=True)
        (root / "crates/akantu-x/Cargo.toml").write_text("[package]\n")
        (root / "crates/akantu-x/src/inner/mod.rs").write_text(
            "pub fn f() {}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn works() {\n        f();\n    }\n}\n"
        )
        (root / "crates/akantu-x/tests/one.rs").write_text(
            "mod nested {\n    #[test]\n    fn deep() {}\n}\n#[test]\nfn top() {}\n"
        )
        (root / "crates/akantu-x/tests/multi/main.rs").write_text("#[test]\nfn m() {}\n")
        (root / "python/tests/test_a.py").write_text("def test_b():\n    pass\n")
        index = SourceIndex(root)
        runs = index.runs()
        for cell in ["`inner::tests::works`", "`one.rs::nested::deep`", "`one.rs`", "`akantu-x` `one.rs::top`",
                     "`multi::m`", "`python/tests/test_a.py::test_b`", "`test_a.py`"]:
            problems, _, _ = check_cell("CORE-001", cell, runs, root=root, index=index, lenient=True)
            assert not problems, (cell, problems)
        for cell, why in [("`inner::tests::missing`", "no test found"), ("`akantu-y` `one.rs`", "binary one in akantu-y"),
                          ("`python/tests/test_a.py::test_c`", "no `def test_c`"), ("`test_z.py`", "not found")]:
            problems, _, _ = check_cell("CORE-001", cell, runs, root=root, index=index, lenient=True)
            assert any(why in p for p in problems), (cell, why, problems)


# ------------------------------------------------------------------ main --
def main() -> int:
    args = sys.argv[1:]
    if "--self-test" in args:
        self_test()
        print("check_cited_tests.py self-test: ok")
        return 0
    rows_filter = None
    if "--rows" in args:
        i = args.index("--rows")
        rows_filter = set(args[i + 1].split(","))
        del args[i : i + 2]
    verbose = "-v" in args
    strict = "--strict" in args
    source = "--source" in args
    jobs = [Path(a) for a in args if not a.startswith("-")]
    if not source and not jobs:
        print(__doc__)
        return 2
    index = SourceIndex(ROOT)
    if source:
        runs, fallback = index.runs(), None
        print(f"{len(runs)} test functions in the source tree")
    else:
        runs = [r for j in jobs for r in load_runs(j)]
        fallback = index.runs()
        print(f"{len(runs)} test results in {', '.join(map(str, jobs))}")
    cells = matrix_cells(MATRIX.read_text())
    status = 0
    n_alt = n_rows = n_notes = 0
    for row, cell in cells:
        if rows_filter and row not in rows_filter and row.split("-")[0] not in rows_filter:
            continue
        problems, notes, resolved = check_cell(row, cell, runs, fallback, ROOT, index, lenient=source)
        n_rows += 1
        n_alt += len(resolved)
        n_notes += len(notes)
        if problems or verbose or (notes and strict):
            print(f"{row}: {'FAIL' if problems else 'ok'}, {len(resolved)} citations resolved")
        for p in problems:
            print(f"    MISSING/FAILED {p}")
            status = 1
        for n in notes:
            if strict:
                print(f"    NOT RUN {n}")
                status = 1
            elif verbose:
                print(f"    note: {n}")
        if verbose:
            for sp, k, where in resolved:
                print(f"    {k:4d}  `{sp}`  {where[:3]}")
    print(f"{n_rows} rows, {n_alt} cited tests resolved, {n_notes} not run in the logs; "
          + ("all cited tests found" if status == 0 else "missing tests above"))
    return status


if __name__ == "__main__":
    sys.exit(main())
