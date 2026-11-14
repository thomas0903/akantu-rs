# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: promotes a successful generation run into the fixtures tree
# and applies the size policy (docs/NUMERICAL_VALIDATION.md).
"""Promote staged fixtures, apply the size policy, rewrite SHA256SUMS.

  python3 -I finalize_fixtures.py <fixtures_root> [--stage DIR] [--created TS]
          [--complete-area AREA]... [--limit-kib N]

Finalisation is transactional: the complete
candidate tree is planned and validated first, without writing anything;
the destination is modified only when every check passed, and an I/O
failure while modifying it rolls every change back.

1. Promotion (`--stage`): every case generated in the staging directory
   replaces `manifest.json`, `data/` and `inputs/` of the same case under the
   fixtures root (`tolerances.json` is never touched). An *external* case
   whose regenerated data has exactly the checksums of its committed
   manifest, produced with the same provenance (reference, build, probe
   version), keeps that manifest (materialisation of a fresh clone leaves
   git clean). Run only after every probe of the run succeeded.
2. Size policy: cases whose data exceed the limit (default 1024 KiB) become
   `storage.mode = "external"` with a concrete per-case regeneration command
   (`storage.generate`), and their `data/`/`inputs/` are git-ignored. External
   cases are reclassified only when completely materialised and verified:
   * an external case without any data (fresh clone) keeps its manifest and
     ignore rules (reported, not an error);
   * partially present data, checksum mismatches (staged or committed), a
     committed case without its data, or a case written by this run
     (`created` = `--created`) that lost data are errors (exit 1).
3. `--complete-area A`: the run regenerated area A completely; a case of A
   that this run did not write is stale (exit 1).
4. `SHA256SUMS` indexes every committed generated file.

Steps 1-4 are evaluated on the candidate tree (destination files overlaid
with the staged cases and the rewritten manifests); any problem aborts
before the destination changes. The new content is then copied into
`<root>/.finalize-transaction/new/`, and every replaced path is swapped in
by renames that keep the previous version under `old/` (same file system:
`<root>` may be a bind mount). An exception during the swap restores the
previous versions; on success the transaction directory is removed. A
transaction directory left by a killed process blocks the next run: its
`journal.json` lists the swapped paths, whose previous versions are under
`old/` (or restore `fixtures/` from git), then remove it.

Concurrency: the whole finalisation — planning (which
computes the candidate tree and `SHA256SUMS`) and the swap — runs under an
exclusive `flock` on the fixtures root directory itself (no lock file is
created), so generators running in parallel processes or containers on the
same fixtures root (the regeneration lanes of oracle/x86/regenerate-corpus.sh)
are serialised: none fails on the other's transaction directory, and none
writes an index computed before another lane's cases were promoted.
"""
from __future__ import annotations

import contextlib
import fcntl
import hashlib
import json
import os
import shutil
import sys
from pathlib import Path

GENERATED = ("manifest.json", "data", "inputs")
NOT_INDEXED = ("SHA256SUMS", ".gitignore", ".gitkeep", "tolerances.json")
IGNORE_HEADER = ("# Data of external fixtures (size policy, docs/NUMERICAL_VALIDATION.md).\n"
                 "# Maintained by oracle/probes/python/finalize_fixtures.py.\n")
TRANSACTION = ".finalize-transaction"


class FinalizeError(Exception):
    """The fixtures tree cannot be finalised."""


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def dump(m: dict) -> bytes:
    return (json.dumps(m, indent=2) + "\n").encode("utf-8")


def stored_files(m: dict) -> list[tuple[str, str]]:
    """(relative path, sha256) of every data file and stored input."""
    files = [(q["file"], q["sha256"]) for q in m.get("quantities", [])]
    files += [(i["path"], i["sha256"]) for i in m.get("inputs", []) if i.get("path")]
    return files


def regeneration_target(m: dict) -> str:
    """Driver target that regenerates exactly this case (or its probe)."""
    case, probe = m["case_id"], m.get("generator", {}).get("probe", "")
    if case.startswith("regression/"):
        return f"regression:{case}"
    if probe == "mesh_probe":
        return f"mesh:{case}"
    if probe == "material_probe":
        return f"material:{case}"
    if probe == "model_probe":
        return f"model:{case}"
    if probe == "element_probe":
        return "element"
    if probe.startswith("writer_selftest"):
        return "selftest"
    raise FinalizeError(f"{case}: no regeneration target for probe {probe!r}")


def regeneration_command(m: dict) -> str:
    config = (m.get("build") or {}).get("config") or "strict"
    return f"oracle/scripts/generate-fixtures.sh --config {config} {regeneration_target(m)}"


def provenance(m: dict) -> dict:
    """Provenance that identifies how data was produced (the akantu-rs
    commit is informational: the probe tree hash identifies the sources)."""
    build = json.loads(json.dumps(m.get("build") or {}))
    build.get("libraries", {}).pop("probe_sources_commit", None)
    gen = m.get("generator", {})
    return {"reference": m.get("reference"), "build": build,
            "probe": (gen.get("probe"), gen.get("probe_version"))}


class Candidate:
    """The fixtures tree as finalisation would leave it, built without
    writing anything: relative POSIX path -> source (a file of the root or
    of the staging directory, or new content as bytes)."""

    def __init__(self, root: Path):
        self.root = root
        self.files: dict[str, Path | bytes] = {}
        for f in sorted(root.rglob("*")):
            rel = f.relative_to(root).as_posix()
            if rel == TRANSACTION or rel.startswith(TRANSACTION + "/"):
                continue
            if f.is_file():
                self.files[rel] = f

    def read(self, rel: str) -> bytes:
        src = self.files[rel]
        return src if isinstance(src, bytes) else src.read_bytes()

    def size(self, rel: str) -> int:
        src = self.files[rel]
        return len(src) if isinstance(src, bytes) else src.stat().st_size

    def under(self, prefix: str) -> list[str]:
        """Files below directory `prefix` (relative, without trailing `/`)."""
        p = prefix + "/"
        return sorted(r for r in self.files if r.startswith(p))

    def replace_dir(self, rel_dir: str, src_dir: Path) -> None:
        for r in self.under(rel_dir):
            del self.files[r]
        if src_dir.is_dir():
            for f in sorted(src_dir.rglob("*")):
                if f.is_file():
                    self.files[f"{rel_dir}/{f.relative_to(src_dir).as_posix()}"] = f

    def manifests(self) -> list[str]:
        """Case directories (relative) holding a manifest.json."""
        return sorted(r[: -len("/manifest.json")] for r in self.files
                      if r.endswith("/manifest.json"))


def materialisation(tree: Candidate, rel: str, m: dict) -> str:
    """`complete`, `absent` or `partial`; raises on checksum mismatch."""
    files = stored_files(m)
    present = [(f, s) for f, s in files if f"{rel}/{f}" in tree.files]
    if not present:
        return "absent" if files else "complete"
    if len(present) != len(files):
        return "partial"
    for f, s in present:
        if sha256_bytes(tree.read(f"{rel}/{f}")) != s:
            raise FinalizeError(f"{m['case_id']}: {f} does not match its manifest checksum")
    return "complete"


def data_size(tree: Candidate, rel: str) -> int:
    return sum(tree.size(r) for r in tree.under(rel)
               if r.rsplit("/", 1)[-1] not in ("manifest.json", "tolerances.json"))


def plan(root: Path, stage: Path | None, created: str | None, complete_areas: list[str],
         limit: int) -> tuple[Candidate, list[str], list[str], dict]:
    """Validate the complete candidate tree without writing anything.

    Returns the candidate, the directories replaced as a whole, the files
    whose content changes, and the summary. Raises FinalizeError on any
    problem (nothing has been modified then)."""
    txn = root / TRANSACTION
    if txn.exists() or txn.is_symlink():
        raise FinalizeError(
            f"{txn} exists: an earlier finalisation was interrupted. Restore the paths listed in "
            "its journal.json from its old/ directory (or restore the fixtures from git) and "
            "remove it before finalising again")
    tree = Candidate(root)
    promoted: list[str] = []
    replaced_dirs: list[str] = []
    kept: list[str] = []
    for sm in sorted(stage.rglob("manifest.json")) if stage else []:
        src = sm.parent
        m = load(sm)
        case = m["case_id"]
        if src.relative_to(stage).as_posix() != case:
            raise FinalizeError(f"{sm}: case_id {case} does not match its directory")
        old = f"{case}/manifest.json"
        keep_manifest = False
        if old in tree.files:
            om = json.loads(tree.read(old))
            keep_manifest = (om.get("storage", {}).get("mode") == "external"
                             and sorted(stored_files(om)) == sorted(stored_files(m))
                             and provenance(om) == provenance(m))
        for sub in ("data", "inputs"):
            tree.replace_dir(f"{case}/{sub}", src / sub)
            replaced_dirs.append(f"{case}/{sub}")
        if keep_manifest:
            kept.append(case)
        else:
            tree.files[old] = sm
        promoted.append(case)
    ignored = set()
    if ".gitignore" in tree.files:
        ignored = {line.strip() for line in tree.read(".gitignore").decode("utf-8").splitlines()
                   if line.strip() and not line.startswith("#")}
    problems: list[str] = []
    summary = {"committed_bytes": 0, "external": 0, "unmaterialised": [], "promoted": promoted,
               "kept_manifests": kept}
    for rel in tree.manifests():
        mrel = f"{rel}/manifest.json"
        m = json.loads(tree.read(mrel))
        mode = m.get("storage", {}).get("mode", "committed")
        written_now = created is not None and m.get("created") == created
        area = rel.split("/")[0]
        if area in complete_areas and not written_now and rel not in promoted:
            problems.append(f"{rel}: stale case (area {area} was regenerated completely but this "
                            "run did not write it)")
        try:
            state = materialisation(tree, rel, m)
        except FinalizeError as e:
            problems.append(str(e))
            continue
        rules = {f"/{rel}/data/", f"/{rel}/inputs/"}
        if state == "partial":
            problems.append(f"{rel}: data partially present (some files missing)")
            continue
        if state == "absent":
            if mode == "external" and not written_now and rel not in promoted:
                ignored |= rules
                summary["unmaterialised"].append(rel)
                summary["external"] += 1
                continue
            problems.append(f"{rel}: {mode} case has no data"
                            + (" although this run wrote it" if written_now else ""))
            continue
        size = data_size(tree, rel)
        storage = m.setdefault("storage", {})
        if size > limit:
            new = dict(storage)
            new.update({"mode": "external", "bytes": size, "generate": regeneration_command(m)})
            new.setdefault("location", "regenerate locally (no remote store yet)")
            if new != storage:
                m["storage"] = new
                tree.files[mrel] = dump(m)
            ignored |= rules
            summary["external"] += 1
        else:
            if mode == "external":
                m["storage"] = {"mode": "committed", "bytes": size}
                tree.files[mrel] = dump(m)
            ignored -= rules
            summary["committed_bytes"] += size
    if problems:
        raise FinalizeError("finalisation failed (nothing was modified):\n  "
                            + "\n  ".join(problems))
    tree.files[".gitignore"] = (IGNORE_HEADER + "".join(f"{x}\n" for x in sorted(ignored))
                                ).encode("utf-8")
    lines = []
    prefixes = tuple(x.strip("/") for x in ignored if x.endswith("/"))
    for rel in sorted(tree.files):
        if rel.rsplit("/", 1)[-1] in NOT_INDEXED:
            continue
        if prefixes and rel.startswith(prefixes):
            continue
        lines.append(f"{sha256_bytes(tree.read(rel))}  {rel}\n")
    tree.files["SHA256SUMS"] = "".join(lines).encode("utf-8")
    summary["indexed"] = len(lines)
    in_replaced = tuple(d + "/" for d in replaced_dirs)
    changed = sorted(rel for rel, src in tree.files.items()
                     if not (in_replaced and rel.startswith(in_replaced))
                     and (isinstance(src, bytes) or src != root / rel))
    return tree, replaced_dirs, changed, summary


# I/O primitives of the transaction (indirections so that tests can inject
# failures).
def _move(src: Path, dst: Path) -> None:
    os.replace(src, dst)


def _copy(src: Path, dst: Path) -> None:
    shutil.copyfile(src, dst)


def _write(path: Path, data: bytes) -> None:
    path.write_bytes(data)


def _make_parents(d: Path, root: Path) -> list[Path]:
    """Create the missing ancestors of a path below `root`; returns them,
    outermost first."""
    missing = []
    while d != root and not d.exists():
        missing.append(d)
        d = d.parent
    created = list(reversed(missing))
    for p in created:
        p.mkdir()
    return created


def _rollback(root: Path, new: Path, old: Path, done: list[list], created: list[Path]) -> None:
    for t, had_old, placed in reversed(done):
        dst = root / t
        if placed:
            (new / t).parent.mkdir(parents=True, exist_ok=True)
            _move(dst, new / t)
        if had_old:
            _move(old / t, dst)
    for d in reversed(created):
        d.rmdir()


def commit(root: Path, tree: Candidate, replaced_dirs: list[str], changed: list[str]) -> None:
    """Swap the planned content in, transactionally (see the module doc)."""
    txn = root / TRANSACTION
    new, old = txn / "new", txn / "old"
    targets = [(d, True) for d in replaced_dirs] + [(f, False) for f in changed]
    txn.mkdir()
    try:
        _write(txn / "journal.json", (json.dumps(
            {"root": str(root), "paths": [t for t, _ in targets],
             "recovery": "for each path: if old/<path> exists, move it back to <root>/<path> "
                         "(replacing what is there); otherwise remove <root>/<path>"},
            indent=2) + "\n").encode("utf-8"))
        for t, is_dir in targets:
            for rel in tree.under(t) if is_dir else [t]:
                dst = new / rel
                dst.parent.mkdir(parents=True, exist_ok=True)
                src = tree.files[rel]
                if isinstance(src, bytes):
                    _write(dst, src)
                else:
                    _copy(src, dst)
    except BaseException:
        shutil.rmtree(txn, ignore_errors=True)
        raise
    done: list[list] = []
    created: list[Path] = []
    try:
        for t, _ in targets:
            dst = root / t
            had_old = dst.exists() or dst.is_symlink()
            if had_old:
                (old / t).parent.mkdir(parents=True, exist_ok=True)
                _move(dst, old / t)
            done.append([t, had_old, False])
            if (new / t).exists():
                created += _make_parents(dst.parent, root)
                _move(new / t, dst)
                done[-1][2] = True
    except BaseException as e:
        try:
            _rollback(root, new, old, done, created)
        except BaseException as r:
            raise FinalizeError(
                f"finalisation failed while replacing files ({e!r}) and the rollback failed "
                f"({r!r}): previous versions are kept under {old} (see {txn / 'journal.json'})"
            ) from e
        shutil.rmtree(txn, ignore_errors=True)
        raise
    shutil.rmtree(txn)


@contextlib.contextmanager
def finalize_lock(root: Path):
    """Exclusive lock over a whole finalisation of `root` (module docs);
    blocks until a concurrent finalisation of the same root ends."""
    if not root.is_dir():
        raise FinalizeError(f"{root} is not a directory")
    fd = os.open(root, os.O_RDONLY)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX)
        yield
    finally:
        try:
            fcntl.flock(fd, fcntl.LOCK_UN)
        finally:
            os.close(fd)


def finalize(root: Path, stage: Path | None, created: str | None,
             complete_areas: list[str], limit: int) -> dict:
    with finalize_lock(root):
        tree, replaced_dirs, changed, summary = plan(root, stage, created, complete_areas, limit)
        commit(root, tree, replaced_dirs, changed)
    return summary


def main(argv: list[str]) -> int:
    args = list(argv[1:])
    opts: dict[str, list[str]] = {"--stage": [], "--created": [], "--complete-area": [],
                                  "--limit-kib": []}
    rest = []
    i = 0
    while i < len(args):
        if args[i] in opts:
            if i + 1 >= len(args):
                print(f"finalize: {args[i]} needs a value", file=sys.stderr)
                return 2
            opts[args[i]].append(args[i + 1])
            i += 2
        else:
            rest.append(args[i])
            i += 1
    if len(rest) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    root = Path(rest[0])
    stage = Path(opts["--stage"][-1]) if opts["--stage"] else None
    created = opts["--created"][-1] if opts["--created"] else None
    limit = int(opts["--limit-kib"][-1]) * 1024 if opts["--limit-kib"] else 1024 * 1024
    try:
        s = finalize(root, stage, created, opts["--complete-area"], limit)
    except (FinalizeError, OSError, json.JSONDecodeError, KeyError) as e:
        print(f"finalize: {e}", file=sys.stderr)
        return 1
    for rel in s["kept_manifests"]:
        print(f"finalize: {rel}: external data materialised, identical to the committed "
              "manifest (manifest kept)")
    for rel in s["unmaterialised"]:
        print(f"finalize: {rel}: external case not materialised here (manifest and ignore "
              "rules preserved; regenerate with its storage.generate command)")
    print(f"finalize: {len(s['promoted'])} case(s) promoted, committed data "
          f"{s['committed_bytes'] / 1024:.0f} KiB, {s['external']} external case(s) "
          f"({len(s['unmaterialised'])} not materialised), {s['indexed']} files indexed "
          "in SHA256SUMS")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
