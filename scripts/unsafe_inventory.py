#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling; no
# Akantu counterpart.
"""Inventory guard of the workspace's `unsafe` code.

Every crate under `crates/` whose Rust sources (library, tests, examples,
benches, build script) contain an `unsafe` site must have an `UNSAFE.md`
that lists each site exactly once, with its current line. A *site* is

* `block`   an `unsafe { ... }` block (an expression or statement block);
* `fn`      an `unsafe fn` item, also `unsafe extern "C" fn`;
* `impl`    an `unsafe impl` (e.g. `Send` / `Sync` / `GlobalAlloc`);
* `trait`   an `unsafe trait`;
* `extern`  an `extern` block (`unsafe extern "C" { ... }`); the guard also
            checks the names of the items it declares (a new foreign function
            in an existing block must be added to its row);
* `attr`    an unsafe attribute (`#[unsafe(no_mangle)]`, `#[unsafe(export_name)]`);
* `ffi-fn`  a function with a foreign ABI defined in Rust without `unsafe`
            (`extern "C" fn name(..) { .. }`: C calls it with C's arguments).

`unsafe` in comments, string literals and doc tests is not a site; an `unsafe
extern "C" fn(..)` *type* (a function-pointer type) is not a site either.

A site is identified by its file, its **item path** (the chain of enclosing
named items: inline `mod`, `impl` headers, `trait`, `fn`, `extern` blocks,
`macro_rules!`), its kind and its ordinal among the sites of the same kind in
the same innermost item. In `UNSAFE.md` a site is one table row under a
heading naming the file (`### src/mpi_backend.rs`, path relative to the crate)::

    | Id | Line | Site | ... |
    | CM-01 | 262 | `fn initialize` block 1 | ... |

The first cell is an identifier (letters, `-`, digits), the second the line
of the `unsafe` keyword (for `ffi-fn`: of `fn`), the third the item path in
backticks followed by the kind and, for every kind but `extern`, the ordinal.
`extern` rows also list the declared items: `` `extern "C"` extern [a, b, c] ``.
Item path segments are separated by ` › `: `` `impl Drop for Foo › fn drop` ``.

**Fingerprints**. The key above survives an edit that
inserts a new block *before* an existing one of the same item, or swaps two
blocks: every key still exists and each row then describes the other block.
Each row is therefore also pinned to a fingerprint of its site's code: the
SHA-256 (first 16 hex digits) of the site's tokens — for a block from
`unsafe` to its closing brace, for an `unsafe fn` / `impl` / `trait`,
`extern` block or foreign-ABI function the whole item, for an attribute its
`unsafe(...)` — with comments and whitespace ignored and string literals
kept. The fingerprints live next to the inventory, in
`<crate>/UNSAFE.fingerprints` (one `Id fingerprint` line per row).

Usage::

    python3 scripts/unsafe_inventory.py --check          # the guard (exit 1 on drift)
    python3 scripts/unsafe_inventory.py --list [CRATE]   # every site, one per line
    python3 scripts/unsafe_inventory.py --skeleton CRATE # table rows for a new UNSAFE.md
    python3 scripts/unsafe_inventory.py --update-lines   # rewrite the Line cells in place
    python3 scripts/unsafe_inventory.py --refresh ID ... # re-pin reviewed rows (prints old -> new and the code)
    python3 scripts/unsafe_inventory.py --pin-missing    # pin rows without a fingerprint
    python3 scripts/unsafe_inventory.py --pin-missing --force  # also rows pinned in HEAD
    python3 scripts/unsafe_inventory.py --self-test      # the scanner's own tests

`--check` fails when a site exists that no row lists, a row names a site that
does not exist, a row's line is not the site's current line (run
`--update-lines`), a row's fingerprint is missing or is not its site's current
fingerprint (the site's code changed, or another site took its key: review
the row against the code, then `--refresh ID`), a fingerprint names no row,
an `extern` row's item list differs from the block, a crate with sites has no
`UNSAFE.md`, or (`--strict-safety`) an `unsafe` block / impl has no `SAFETY`
comment (a comment whose text starts with `SAFETY:` or `SAFETY (`, within
the eight lines above it, on its line, or on the first lines inside the
block; AGENTS.md, "Rules for `unsafe` and licensing"). `--update-lines` rewrites the line of a row only when
its fingerprint matches, and exits 1 naming the rows whose code changed;
`--refresh ID` prints each row's old and new fingerprint and the site's code,
so the re-pin shows what it accepts; `--pin-missing`
pins only rows that have no fingerprint yet (new rows), never a changed one,
and refuses a row that git's `HEAD` pins (its line was deleted from
`UNSAFE.fingerprints`, or the file was) unless `--force`.

**Scope**. A fingerprint covers the tokens of the
Rust site itself: not the code around it that establishes its invariants
(a `SAFETY:` comment may rest on a check several lines above the block),
and not C sources (the FFI shims `crates/akantu-comm/src/mpi_shim.c`,
`crates/akantu-linalg/src/{mumps,petsc}/shim.c`,
`crates/akantu-mesh/src/scotch_shim.c`), which the guard does not read: a
change there is seen only in the commit's diff and its review. Standard
library only (plus `git` for `HEAD`'s fingerprints).
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SEP = " › "
KINDS = ("block", "fn", "impl", "trait", "extern", "attr", "ffi-fn")


# --------------------------------------------------------------------- lexer


@dataclass
class Tok:
    kind: str  # 'id', 'p' (punctuation), 'str', 'lt' (lifetime), 'num'
    text: str
    line: int
    raw: str = ""  # the literal's source text (strings and chars only; fingerprints)


@dataclass
class Comment:
    line: int
    end_line: int
    text: str


def lex(src: str) -> tuple[list[Tok], list[Comment]]:
    """Tokens of a Rust source with comments, strings and chars removed."""
    toks: list[Tok] = []
    comments: list[Comment] = []
    i, n, line = 0, len(src), 1
    ident_start = re.compile(r"[A-Za-z_\u0080-￿]")
    ident_char = re.compile(r"[A-Za-z0-9_\u0080-￿]")

    def raw_string(j: int) -> int:
        # src[j] == 'r' (after an optional b/c prefix); returns index after the literal
        k = j + 1
        hashes = 0
        while k < n and src[k] == "#":
            hashes += 1
            k += 1
        if k >= n or src[k] != '"':
            return -1
        close = '"' + "#" * hashes
        end = src.find(close, k + 1)
        if end < 0:
            raise ValueError("unterminated raw string")
        return end + len(close)

    def quoted(j: int) -> int:
        # src[j] == '"'; returns index after the closing quote
        k = j + 1
        while k < n:
            c = src[k]
            if c == "\\":
                k += 2
                continue
            if c == '"':
                return k + 1
            k += 1
        raise ValueError("unterminated string")

    while i < n:
        c = src[i]
        if c == "\n":
            line += 1
            i += 1
            continue
        if c in " \t\r":
            i += 1
            continue
        if src.startswith("//", i):
            end = src.find("\n", i)
            end = n if end < 0 else end
            comments.append(Comment(line, line, src[i:end]))
            i = end
            continue
        if src.startswith("/*", i):
            depth, k, start_line = 1, i + 2, line
            while k < n and depth:
                if src.startswith("/*", k):
                    depth += 1
                    k += 2
                elif src.startswith("*/", k):
                    depth -= 1
                    k += 2
                else:
                    if src[k] == "\n":
                        line += 1
                    k += 1
            comments.append(Comment(start_line, line, src[i:k]))
            i = k
            continue
        # string-like literals with prefixes
        if c in "bcr" and i + 1 < n:
            j = i
            if src[j] in "bc" and j + 1 < n and src[j + 1] == "r":
                j += 1
            if src[j] == "r" and j + 1 < n and src[j + 1] in '#"':
                end = raw_string(j)
                if end > 0:
                    line += src.count("\n", i, end)
                    toks.append(Tok("str", "<str>", line, src[i:end]))
                    i = end
                    continue
            if src[i] in "bc" and src[i + 1] == '"':
                end = quoted(i + 1)
                line += src.count("\n", i, end)
                toks.append(Tok("str", "<str>", line, src[i:end]))
                i = end
                continue
            if src[i] == "b" and src[i + 1] == "'":
                end = src.find("'", i + 2 + (1 if src[i + 2] == "\\" else 0) + 1)
                toks.append(Tok("str", "<char>", line, src[i : end + 1]))
                i = end + 1
                continue
        if c == '"':
            end = quoted(i)
            start = line
            line += src.count("\n", i, end)
            toks.append(Tok("str", "<str>", start, src[i:end]))
            i = end
            continue
        if c == "'":
            # char literal or lifetime / label
            if i + 2 < n and src[i + 1] == "\\":
                end = src.find("'", i + 2)
                # '\'' : the escaped quote
                if src[i + 2] == "'":
                    end = src.find("'", i + 3)
                toks.append(Tok("str", "<char>", line, src[i : end + 1]))
                i = end + 1
                continue
            if i + 2 < n and src[i + 2] == "'":
                toks.append(Tok("str", "<char>", line, src[i : i + 3]))
                i += 3
                continue
            # a multi-byte char such as '›' is one code point in Python
            k = i + 1
            while k < n and ident_char.match(src[k]):
                k += 1
            toks.append(Tok("lt", src[i:k], line))
            i = k
            continue
        if ident_start.match(c):
            k = i + 1
            while k < n and ident_char.match(src[k]):
                k += 1
            word = src[i:k]
            # raw identifiers r#foo
            toks.append(Tok("id", word, line))
            i = k
            continue
        if c.isdigit():
            k = i + 1
            while k < n and (src[k].isalnum() or src[k] in "_."):
                if src[k] == "." and k + 1 < n and not src[k + 1].isdigit():
                    break
                k += 1
            toks.append(Tok("num", src[i:k], line))
            i = k
            continue
        if src.startswith("::", i):
            toks.append(Tok("p", "::", line))
            i += 2
            continue
        if src.startswith("->", i):
            toks.append(Tok("p", "->", line))
            i += 2
            continue
        if src.startswith("=>", i):
            toks.append(Tok("p", "=>", line))
            i += 2
            continue
        toks.append(Tok("p", c, line))
        i += 1
    return toks, comments


# ------------------------------------------------------------------- scanner


@dataclass
class Site:
    file: str  # relative to the crate
    line: int
    path: str  # item path
    kind: str
    ordinal: int = 0
    items: list[str] = field(default_factory=list)  # extern: declared items
    has_safety: bool = True
    fingerprint: str = ""  # Digest of the site's normalised tokens
    end_line: int = 0  # line of the last token of the site's extent

    def key(self) -> tuple[str, str, str, int]:
        return (self.file, self.path, self.kind, self.ordinal)

    def cell(self) -> str:
        p = f"`{self.path}`" if self.path else "`(crate root)`"
        if self.kind == "extern":
            return f"{p} extern {self.ordinal} [{', '.join(self.items)}]"
        return f"{p} {self.kind} {self.ordinal}"


def _header_text(toks: list[Tok], a: int, b: int) -> str:
    """Normalised text of toks[a:b] (an impl header): rustfmt-like spacing."""
    out = " ".join(t.text if t.kind != "str" else '"C"' for t in toks[a:b])
    for pat, rep in (
        (r"\s*::\s*", "::"),
        (r"\s+([,>)\]])", r"\1"),
        (r"([<(\[&])\s+", r"\1"),
        (r"(\w)\s+<", r"\1<"),
        (r"\s+:", ":"),
        (r"\?\s+", "?"),
        (r"\s+!", "!"),
        (r"\s+", " "),
    ):
        out = re.sub(pat, rep, out)
    out = out.strip()
    # "impl<T> Send for X<T>" (generic parameters glued to the keyword)
    return out if out.startswith("<") else " " + out


def scan_source(src: str, file: str) -> list[Site]:
    toks, comments = lex(src)
    lines = src.split("\n")
    safety_lines = {ln for c in comments for ln in range(c.line, c.end_line + 1) if is_safety_comment(c.text)}
    sites: list[Site] = []
    stack: list[str | None] = []  # labels of the open braces
    pending: str | None = None
    pending_depth = 0  # paren/bracket depth when pending was set
    pend_extern: list[str] | None = None  # extern block items being collected
    extern_stack: list[tuple[int, Site]] = []
    paren = 0
    n = len(toks)

    def path_of() -> str:
        return SEP.join(x for x in stack if x)

    def prev_sig(i: int) -> Tok | None:
        return toks[i - 1] if i > 0 else None

    def find_brace_or_semi(j: int) -> int:
        """Index of the `{` or `;` ending the item head starting at j."""
        depth = 0
        while j < n:
            t = toks[j]
            if t.kind == "p":
                if t.text in "([":
                    depth += 1
                elif t.text in ")]":
                    depth -= 1
                elif depth == 0 and t.text in "{;":
                    return j
            j += 1
        return n

    # comment blocks: consecutive comment lines; a SAFETY comment counts for a
    # site when its block ends at most eight lines above the site (or on the
    # site's line) and no other site lies between the comment and the site,
    # or when it is on one of the first two lines inside the site's block.
    comment_lines = {ln for c in comments for ln in range(c.line, c.end_line + 1)}
    block_end: dict[int, int] = {}
    for ln in sorted(comment_lines, reverse=True):
        block_end[ln] = block_end.get(ln + 1, ln) if (ln + 1) in comment_lines else ln

    def safety_near(line: int, block_open_line: int | None) -> bool:
        if block_open_line is not None and any(
            ln in safety_lines for ln in (block_open_line, block_open_line + 1, block_open_line + 2)
        ):
            return True
        return any(
            ln < line and line - block_end[ln] <= 8 and not any(ln < s.line < line for s in sites)
            for ln in safety_lines
        ) or line in safety_lines

    i = 0
    while i < n:
        t = toks[i]
        if t.kind == "p":
            if t.text in "([":
                paren += 1
            elif t.text in ")]":
                paren -= 1
            elif t.text == "{":
                label = None
                if pending is not None and paren == pending_depth:
                    label = pending
                    pending = None
                stack.append(label)
                if label is not None and label.startswith("extern ") and extern_stack:
                    pass
            elif t.text == "}":
                if stack:
                    closed = stack.pop()
                    if extern_stack and extern_stack[-1][0] == len(stack):
                        extern_stack.pop()
                    _ = closed
            elif t.text == ";":
                if pending is not None and paren == pending_depth and not pending.startswith("macro"):
                    pending = None
            i += 1
            continue
        if t.kind != "id":
            i += 1
            continue
        w = t.text
        nxt = toks[i + 1] if i + 1 < n else None
        # extern block item collection: `fn name` / `static name` directly inside
        if extern_stack and extern_stack[-1][0] == len(stack) - 1:
            if w in ("fn", "static") and nxt is not None and nxt.kind == "id":
                name = nxt.text
                if w == "static" and name == "mut" and i + 2 < n:
                    name = toks[i + 2].text
                extern_stack[-1][1].items.append(name)
                i += 2
                continue
        if w == "unsafe":
            pv = prev_sig(i)
            if nxt is not None and nxt.kind == "p" and nxt.text == "{":
                s = Site(file, t.line, path_of(), "block")
                s.has_safety = safety_near(t.line, nxt.line)
                sites.append(s)
            elif nxt is not None and nxt.kind == "p" and nxt.text == "(" and pv is not None and pv.text == "[":
                sites.append(Site(file, t.line, path_of(), "attr"))
            elif nxt is not None and nxt.kind == "id" and nxt.text == "impl":
                end = find_brace_or_semi(i + 2)
                hdr = _header_text(toks, i + 2, end)
                hdr = re.sub(r"\s+where\s.*$", "", hdr)
                s = Site(file, t.line, path_of() + (SEP if path_of() else "") + "impl" + hdr, "impl")
                s.path = (path_of() + SEP if path_of() else "") + "impl" + hdr
                s.has_safety = safety_near(t.line, None)
                sites.append(s)
                pending, pending_depth = "impl" + hdr, paren
                i = i + 2
                continue
            elif nxt is not None and nxt.kind == "id" and nxt.text == "trait":
                name = toks[i + 2].text
                sites.append(Site(file, t.line, (path_of() + SEP if path_of() else "") + "trait " + name, "trait"))
                pending, pending_depth = "trait " + name, paren
                i += 3
                continue
            elif nxt is not None and nxt.kind == "id" and nxt.text in ("fn", "extern"):
                # unsafe fn NAME / unsafe extern "abi" fn NAME / unsafe extern "abi" { / fn( type
                j = i + 1
                if toks[j].text == "extern":
                    j += 1
                    if j < n and toks[j].kind == "str":
                        j += 1
                    if j < n and toks[j].kind == "p" and toks[j].text == "{":
                        s = Site(file, t.line, path_of(), "extern")
                        sites.append(s)
                        pending, pending_depth = 'extern "C"', paren
                        extern_stack.append((len(stack), s))
                        i = j
                        continue
                if j < n and toks[j].text == "fn" and j + 1 < n and toks[j + 1].kind == "id":
                    name = toks[j + 1].text
                    in_extern = bool(extern_stack) and extern_stack[-1][0] == len(stack) - 1
                    if in_extern:
                        extern_stack[-1][1].items.append(name)
                        i = j + 2
                        continue
                    s = Site(file, t.line, (path_of() + SEP if path_of() else "") + "fn " + name, "fn")
                    sites.append(s)
                    pending, pending_depth = "fn " + name, paren
                    i = j + 2
                    continue
                # a function-pointer type: not a site
            i += 1
            continue
        if w == "extern" and nxt is not None:
            pv = prev_sig(i)
            j = i + 1
            if toks[j].kind == "str":
                j += 1
            # `extern crate` is not FFI
            if toks[j].kind == "p" and toks[j].text == "{" and (pv is None or pv.text != "unsafe"):
                s = Site(file, t.line, path_of(), "extern")
                sites.append(s)
                pending, pending_depth = 'extern "C"', paren
                extern_stack.append((len(stack), s))
                i = j
                continue
            if toks[j].text == "fn" and j + 1 < n and toks[j + 1].kind == "id" and (pv is None or pv.text != "unsafe"):
                name = toks[j + 1].text
                in_extern = bool(extern_stack) and extern_stack[-1][0] == len(stack) - 1
                end = find_brace_or_semi(j + 2)
                if not in_extern and end < n and toks[end].text == "{":
                    s = Site(file, toks[j].line, (path_of() + SEP if path_of() else "") + "fn " + name, "ffi-fn")
                    sites.append(s)
                pending, pending_depth = "fn " + name, paren
                i = j + 2
                continue
        if w == "fn" and nxt is not None and nxt.kind == "id":
            pending, pending_depth = "fn " + nxt.text, paren
            i += 2
            continue
        if w == "mod" and nxt is not None and nxt.kind == "id":
            pending, pending_depth = "mod " + nxt.text, paren
            i += 2
            continue
        if w == "trait" and nxt is not None and nxt.kind == "id":
            pv = prev_sig(i)
            if pv is None or pv.text != "unsafe":
                pending, pending_depth = "trait " + nxt.text, paren
            i += 2
            continue
        if w == "macro_rules" and nxt is not None and nxt.text == "!" and i + 2 < n:
            pending, pending_depth = "macro_rules! " + toks[i + 2].text, paren
            i += 3
            continue
        if w == "impl":
            pv = prev_sig(i)
            if pv is None or (pv.kind == "p" and pv.text in ";{}]") or pv.text in ("unsafe",):
                end = find_brace_or_semi(i + 1)
                hdr = _header_text(toks, i + 1, end)
                hdr = re.sub(r"\s+where\s.*$", "", hdr)
                pending, pending_depth = "impl" + hdr, paren
                i += 1
                continue
        i += 1

    # ordinals per (path, kind)
    counts: dict[tuple[str, str], int] = {}
    for s in sites:
        k = (s.path, s.kind)
        counts[k] = counts.get(k, 0) + 1
        s.ordinal = counts[k]
    _ = lines
    # fingerprints: the tokens of each site's extent
    starts: dict[int, list[int]] = {}
    for idx, t in enumerate(toks):
        starts.setdefault(t.line, []).append(idx)
    for s in sites:
        i = site_start(toks, starts, s)
        s.fingerprint = fingerprint(toks, i, s.kind)
        s.end_line = toks[site_extent(toks, i, s.kind)[1]].line
    return sites


def is_safety_comment(text: str) -> bool:
    """A `SAFETY` comment: one of its lines, without the
    comment markers, starts with `SAFETY:` or `SAFETY (` (the word itself,
    not a longer one such as `SAFETYNET`, and not the word inside prose)."""
    for ln in text.split("\n"):
        body = ln.strip().lstrip("/*!").strip()
        if re.match(r"SAFETY(:| \()", body):
            return True
    return False


def site_start(toks: list[Tok], starts: dict[int, list[int]], s: Site) -> int:
    """Index of the token that starts site `s` (its `unsafe`, or `fn` for a
    foreign-ABI function defined without `unsafe`, whose line is its `fn`)."""
    want = ("fn",) if s.kind == "ffi-fn" else ("unsafe", "extern")
    for idx in starts.get(s.line, []):
        if toks[idx].kind == "id" and toks[idx].text in want:
            if s.kind == "ffi-fn":
                # include the `extern "abi"` before `fn`
                k = idx - 1
                if k >= 0 and toks[k].kind == "str":
                    k -= 1
                if k >= 0 and toks[k].kind == "id" and toks[k].text == "extern":
                    return k
            return idx
    raise ValueError(f"{s.file}:{s.line}: no start token for the {s.kind} site")


def _match(toks: list[Tok], j: int, open_: str, close: str) -> int:
    """Index of the token closing the `open_` at toks[j]."""
    depth = 0
    for k in range(j, len(toks)):
        t = toks[k]
        if t.kind == "p" and t.text == open_:
            depth += 1
        elif t.kind == "p" and t.text == close:
            depth -= 1
            if depth == 0:
                return k
    return len(toks) - 1


def site_extent(toks: list[Tok], i: int, kind: str) -> tuple[int, int]:
    """[i, end] token range of the site starting at toks[i]."""
    n = len(toks)
    if kind == "block":
        return i, _match(toks, i + 1, "{", "}")
    if kind == "attr":
        return i, _match(toks, i + 1, "(", ")")
    # item kinds: up to the end of the item (its body's closing brace, or `;`)
    depth = 0
    j = i
    while j < n:
        t = toks[j]
        if t.kind == "p":
            if t.text in "([":
                depth += 1
            elif t.text in ")]":
                depth -= 1
            elif depth == 0 and t.text == ";":
                return i, j
            elif depth == 0 and t.text == "{":
                return i, _match(toks, j, "{", "}")
        j += 1
    return i, n - 1


def fingerprint(toks: list[Tok], i: int, kind: str) -> str:
    """SHA-256 (16 hex digits) of the site's tokens, comments and
    whitespace ignored, literals kept."""
    a, b = site_extent(toks, i, kind)
    text = "\x1f".join(t.raw if t.kind == "str" else t.text for t in toks[a : b + 1])
    return hashlib.sha256(text.encode("utf-8")).hexdigest()[:16]


# ------------------------------------------------------------- workspace walk


def crate_dirs() -> list[Path]:
    return sorted(p for p in (ROOT / "crates").iterdir() if (p / "Cargo.toml").is_file())


def rust_files(crate: Path) -> list[Path]:
    out = []
    for dirpath, dirnames, filenames in os.walk(crate):
        dirnames[:] = sorted(d for d in dirnames if d not in ("target", ".git", "node_modules"))
        for f in sorted(filenames):
            if f.endswith(".rs"):
                out.append(Path(dirpath) / f)
    return out


_SCANNED: dict[Path, list[Site]] = {}


def scan_crate(crate: Path) -> list[Site]:
    """Sites of one crate (scanned once per process; the tree is read-only here)."""
    if crate in _SCANNED:
        return _SCANNED[crate]
    sites: list[Site] = []
    for f in rust_files(crate):
        rel = f.relative_to(crate).as_posix()
        sites.extend(scan_source(f.read_text(encoding="utf-8"), rel))
    _SCANNED[crate] = sites
    return sites


# ---------------------------------------------------------- UNSAFE.md parsing

ROW = re.compile(r"^\|\s*([A-Za-z][A-Za-z0-9]*(?:-[A-Za-z0-9]+)+)\s*\|\s*(\d+)\s*\|\s*(.+?)\s*\|")
HEAD = re.compile(r"^#{2,4}\s+`?((?:src|tests|examples|benches)/[^`\s]+\.rs|build\.rs)`?\s*$")
CELL = re.compile(r"^`([^`]*)`\s+(block|fn|impl|trait|attr|ffi-fn)\s+(\d+)\s*$|^`([^`]*)`\s+extern\s+(\d+)\s+\[([^\]]*)\]\s*$")


@dataclass
class Row:
    ident: str
    line: int
    file: str
    path: str
    kind: str
    ordinal: int
    items: list[str]
    md_line: int

    def key(self) -> tuple[str, str, str, int]:
        return (self.file, self.path, self.kind, self.ordinal)


def parse_inventory(text: str) -> tuple[list[Row], list[str]]:
    rows: list[Row] = []
    errors: list[str] = []
    current: str | None = None
    for no, raw in enumerate(text.split("\n"), 1):
        h = HEAD.match(raw.strip())
        if h:
            current = h.group(1)
            continue
        if raw.startswith("#"):
            # any other heading ends the file section
            if not HEAD.match(raw.strip()):
                current = None if raw.startswith("## ") else current
            continue
        m = ROW.match(raw)
        if not m:
            continue
        ident, line, cell = m.group(1), int(m.group(2)), m.group(3)
        c = CELL.match(cell)
        if current is None:
            errors.append(f"UNSAFE.md:{no}: row {ident} is not under a `### <file>.rs` heading")
            continue
        if not c:
            errors.append(f"UNSAFE.md:{no}: row {ident}: cannot parse the site cell {cell!r}")
            continue
        if c.group(2):
            path = c.group(1)
            path = "" if path == "(crate root)" else path
            rows.append(Row(ident, line, current, path, c.group(2), int(c.group(3)), [], no))
        else:
            path = c.group(4)
            path = "" if path == "(crate root)" else path
            items = [x.strip() for x in c.group(6).split(",") if x.strip()]
            rows.append(Row(ident, line, current, path, "extern", int(c.group(5)), items, no))
    return rows, errors


FP_NAME = "UNSAFE.fingerprints"
FP_HEADER = (
    "# Fingerprints of the rows of UNSAFE.md (scripts/unsafe_inventory.py):\n"
    "# <row id> <first 16 hex digits of the SHA-256 of the site's tokens, comments and\n"
    "# whitespace ignored>. Written by --pin-missing (new rows) and --refresh ID (a row\n"
    "# reviewed after its site's code changed); checked by --check.\n"
)
FP_LINE = re.compile(r"^([A-Za-z][A-Za-z0-9]*(?:-[A-Za-z0-9]+)+)\s+([0-9a-f]{16})\s*$")


def read_fingerprints(crate: Path) -> tuple[dict[str, str], list[str]]:
    """The pinned fingerprints of a crate (id -> digest) and parse errors."""
    f = crate / FP_NAME
    pins: dict[str, str] = {}
    errors: list[str] = []
    if not f.is_file():
        return pins, errors
    for no, raw in enumerate(f.read_text(encoding="utf-8").split("\n"), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = FP_LINE.match(line)
        if not m:
            errors.append(f"{FP_NAME}:{no}: cannot parse {raw!r}")
            continue
        if m.group(1) in pins:
            errors.append(f"{FP_NAME}:{no}: {m.group(1)} pinned twice")
        pins[m.group(1)] = m.group(2)
    return pins, errors


def write_fingerprints(crate: Path, rows: list["Row"], pins: dict[str, str]) -> None:
    """Rewrite the crate's fingerprint file in the inventory's row order."""
    body = "".join(f"{r.ident} {pins[r.ident]}\n" for r in rows if r.ident in pins)
    (crate / FP_NAME).write_text(FP_HEADER + body, encoding="utf-8")


# ---------------------------------------------------------------- the checks


def safety_notes(crates: list[Path] | None = None) -> list[str]:
    """`unsafe` blocks / impls without an adjacent `SAFETY` comment."""
    out = []
    for crate in crates or crate_dirs():
        for s in scan_crate(crate):
            if s.kind in ("block", "impl") and not s.has_safety:
                out.append(f"{crate.name}/{s.file}:{s.line}: unsafe {s.kind} without an adjacent SAFETY comment")
    return out


def check(crates: list[Path] | None = None) -> list[str]:
    problems: list[str] = []
    for crate in crates or crate_dirs():
        sites = scan_crate(crate)
        name = crate.name
        inv = crate / "UNSAFE.md"
        if not sites:
            continue
        if not inv.is_file():
            problems.append(f"{name}: {len(sites)} unsafe sites but no {name}/UNSAFE.md")
            continue
        rows, errs = parse_inventory(inv.read_text(encoding="utf-8"))
        problems.extend(f"{name}/{e}" for e in errs)
        pins, fp_errs = read_fingerprints(crate)
        problems.extend(f"{name}/{e}" for e in fp_errs)
        row_ids = {r.ident for r in rows}
        for ident in pins:
            if ident not in row_ids:
                problems.append(f"{name}/{FP_NAME}: {ident} names no row of UNSAFE.md (delete the line)")
        by_key: dict[tuple[str, str, str, int], Site] = {s.key(): s for s in sites}
        seen_ids: dict[str, int] = {}
        seen_keys: set[tuple[str, str, str, int]] = set()
        for r in rows:
            if r.ident in seen_ids:
                problems.append(f"{name}/UNSAFE.md:{r.md_line}: id {r.ident} also used on line {seen_ids[r.ident]}")
            seen_ids[r.ident] = r.md_line
            if r.key() in seen_keys:
                problems.append(f"{name}/UNSAFE.md:{r.md_line}: site listed twice: {r.file} {r.path} {r.kind} {r.ordinal}")
                continue
            seen_keys.add(r.key())
            s = by_key.get(r.key())
            if s is None:
                problems.append(
                    f"{name}/UNSAFE.md:{r.md_line}: {r.ident} names a site that does not exist: "
                    f"{r.file} `{r.path}` {r.kind} {r.ordinal}"
                )
                continue
            pin = pins.get(r.ident)
            if pin is None:
                problems.append(
                    f"{name}/UNSAFE.md:{r.md_line}: {r.ident} has no fingerprint (review the row, then "
                    f"scripts/unsafe_inventory.py --pin-missing)"
                )
            elif pin != s.fingerprint:
                problems.append(
                    f"{name}/UNSAFE.md:{r.md_line}: {r.ident} ({r.file} line {s.line}): the site's code changed "
                    f"or another site took its place (fingerprint {pin} pinned, {s.fingerprint} now): review "
                    f"the row against the code, then scripts/unsafe_inventory.py --refresh {r.ident}"
                )
            if s.line != r.line:
                problems.append(
                    f"{name}/UNSAFE.md:{r.md_line}: {r.ident} line {r.line}, the site is on line {s.line} "
                    f"(run scripts/unsafe_inventory.py --update-lines)"
                )
            if s.kind == "extern" and s.items != r.items:
                problems.append(
                    f"{name}/UNSAFE.md:{r.md_line}: {r.ident} declares [{', '.join(r.items)}], "
                    f"the block declares [{', '.join(s.items)}]"
                )
        for s in sites:
            if s.key() not in seen_keys:
                problems.append(f"{name}/{s.file}:{s.line}: site not in UNSAFE.md: | ? | {s.line} | {s.cell()} |")
    return problems


def update_lines() -> int:
    """Rewrite the Line cell of every row whose site moved, only when the
    site's fingerprint is the pinned one; exit 1 naming the rows
    whose code changed or that have no fingerprint (their lines are kept)."""
    changed = 0
    refused: list[str] = []
    for crate in crate_dirs():
        inv = crate / "UNSAFE.md"
        if not inv.is_file():
            continue
        by_key = {s.key(): s for s in scan_crate(crate)}
        text = inv.read_text(encoding="utf-8")
        rows, _ = parse_inventory(text)
        pins, _ = read_fingerprints(crate)
        lines = text.split("\n")
        for r in rows:
            s = by_key.get(r.key())
            if s is None or s.line == r.line:
                continue
            if pins.get(r.ident) != s.fingerprint:
                refused.append(
                    f"{crate.name}/UNSAFE.md:{r.md_line}: {r.ident}: line not updated, "
                    + ("no fingerprint" if r.ident not in pins else "the site's code changed")
                    + f" (review the row, then --refresh {r.ident} or --pin-missing)"
                )
                continue
            raw = lines[r.md_line - 1]
            lines[r.md_line - 1] = re.sub(
                r"^(\|\s*" + re.escape(r.ident) + r"\s*\|\s*)\d+(\s*\|)", lambda m: f"{m.group(1)}{s.line}{m.group(2)}", raw, count=1
            )
            changed += 1
        new = "\n".join(lines)
        if new != text:
            inv.write_text(new, encoding="utf-8")
    print(f"updated {changed} line cells")
    for line in refused:
        print(line)
    return 1 if refused else 0


def head_fingerprints(crate: Path) -> dict[str, str]:
    """The fingerprints pinned in git's `HEAD` for a crate:
    empty when git, the repository or the file in `HEAD` is
    missing."""
    rel = (crate / FP_NAME).relative_to(ROOT).as_posix()
    try:
        out = subprocess.run(
            ["git", "-C", str(ROOT), "show", f"HEAD:{rel}"],
            capture_output=True, text=True, check=False,
        )
    except OSError:
        return {}
    if out.returncode != 0:
        return {}
    pins: dict[str, str] = {}
    for line in out.stdout.split("\n"):
        m = FP_LINE.match(line)
        if m:
            pins[m.group(1)] = m.group(2)
    return pins


def site_code(crate: Path, s: Site) -> str:
    """The source lines of a site's extent, numbered (for --refresh)."""
    lines = (crate / s.file).read_text(encoding="utf-8").split("\n")
    end = max(s.end_line, s.line)
    return "\n".join(f"    {n:5} | {lines[n - 1]}" for n in range(s.line, min(end, len(lines)) + 1))


def pin(idents: list[str] | None, force: bool = False) -> int:
    """`--refresh ID ...` (idents) re-pins the named rows to their sites'
    current fingerprints and prints, for each, the old and the new
    fingerprint and the site's code (a re-pin shows
    what it accepts); `--pin-missing` (None) pins the rows that have no
    fingerprint yet, but refuses a row that is pinned in git's `HEAD` (its
    line was removed from `UNSAFE.fingerprints`, or the file was) unless
    `force` (`--force`). A row whose site does not exist is reported
    (exit 1)."""
    wanted = set(idents or [])
    done: set[str] = set()
    status = 0
    for crate in crate_dirs():
        inv = crate / "UNSAFE.md"
        if not inv.is_file():
            continue
        by_key = {s.key(): s for s in scan_crate(crate)}
        rows, _ = parse_inventory(inv.read_text(encoding="utf-8"))
        pins, _ = read_fingerprints(crate)
        head = head_fingerprints(crate) if idents is None else {}
        changed = False
        for r in rows:
            if idents is None and r.ident in pins:
                continue
            if idents is not None and r.ident not in wanted:
                continue
            s = by_key.get(r.key())
            done.add(r.ident)
            if s is None:
                print(f"{crate.name}: {r.ident} names a site that does not exist; not pinned")
                status = 1
                continue
            if idents is None and r.ident in head and not force:
                print(
                    f"{crate.name}: {r.ident} is pinned in HEAD ({head[r.ident]}) but its line is gone "
                    f"from {FP_NAME}; not pinned: review the row against the code, then "
                    f"--refresh {r.ident} (or --pin-missing --force)"
                )
                status = 1
                continue
            if pins.get(r.ident) != s.fingerprint:
                old = pins.get(r.ident, "none")
                print(
                    f"{crate.name}: {r.ident} {old} -> {s.fingerprint} ({r.file}:{s.line}, {s.cell()})"
                )
                if idents is not None:
                    print(site_code(crate, s))
                pins[r.ident] = s.fingerprint
                changed = True
        if changed or (pins and not (crate / FP_NAME).is_file()):
            write_fingerprints(crate, rows, pins)
    for ident in sorted(wanted - done):
        print(f"no row {ident} in any UNSAFE.md")
        status = 1
    return status


# ----------------------------------------------------------------- self-test

SELF_TEST_SRC = r'''
// unsafe { in a comment }
/* unsafe { nested /* unsafe */ } */
const S: &str = "unsafe { in a string }";
const R: &str = r#"unsafe { raw " string }"#;
const C: char = '{';
type Cb = unsafe extern "C" fn(i32) -> i32;
fn f<'a>(x: &'a u8) -> u8 {
    // SAFETY: test
    let y = unsafe { *(x as *const u8) };
    let z = unsafe {
        // SAFETY: first line inside the block
        *(x as *const u8)
    };
    y + z
}
unsafe extern "C" {
    fn c_one(x: i32) -> i32;
    pub safe fn c_two();
    pub unsafe fn c_three(p: *mut u8);
    static mut C_FOUR: i32;
}
mod inner {
    pub struct T;
    // SAFETY: T has no fields
    unsafe impl Send for T {}
    impl Drop for T {
        fn drop(&mut self) {
            unsafe { core::hint::unreachable_unchecked() }
        }
    }
    /// # Safety
    /// none
    pub unsafe fn g(cb: impl Fn() -> u8) -> u8 { cb() }
}
#[unsafe(no_mangle)]
pub extern "C" fn exported(x: i32) -> i32 { x }
macro_rules! m { () => { unsafe { 1 } } }
'''


def self_test() -> int:
    sites = scan_source(SELF_TEST_SRC, "src/lib.rs")
    got = [(s.line, s.path, s.kind, s.ordinal, tuple(s.items), s.has_safety) for s in sites]
    want = [
        (10, "fn f", "block", 1, (), True),
        (11, "fn f", "block", 2, (), True),
        (17, "", "extern", 1, ("c_one", "c_two", "c_three", "C_FOUR"), True),
        (26, "mod inner › impl Send for T", "impl", 1, (), True),
        (29, "mod inner › impl Drop for T › fn drop", "block", 1, (), False),
        (34, "mod inner › fn g", "fn", 1, (), True),
        (36, "", "attr", 1, (), True),
        (37, "fn exported", "ffi-fn", 1, (), True),
        (38, "macro_rules! m", "block", 1, (), False),
    ]
    ok = got == want
    if not ok:
        print("scanner self-test FAILED")
        for g in got:
            print("  got ", g)
        for w in want:
            print("  want", w)
    # the parser round-trips the cells
    md = "### src/lib.rs\n\n| Id | Line | Site |\n|---|---|---|\n" + "\n".join(
        f"| T-{k} | {s.line} | {s.cell()} |" for k, s in enumerate(sites, 1)
    )
    rows, errs = parse_inventory(md)
    rt = [(r.line, r.path, r.kind, r.ordinal, tuple(r.items)) for r in rows]
    if errs or rt != [(s.line, s.path, s.kind, s.ordinal, tuple(s.items)) for s in sites]:
        print("inventory parser self-test FAILED", errs, rt)
        ok = False
    # The SAFETY word with its colon / parenthesis, not prose or longer words
    for text, want_safety in (
        ("// SAFETY: fine", True),
        ("// SAFETY (declaration): fine", True),
        ("/* SAFETY: block comment */", True),
        ("// TODO: SAFETYNET not written", False),
        ("// SAFETYNET: not the word", False),
        ("// each `unsafe` states its SAFETY.", False),
    ):
        if is_safety_comment(text) != want_safety:
            print(f"SAFETY comment self-test FAILED on {text!r}")
            ok = False
    # Fingerprints ignore comments and whitespace, see tokens and
    # literals, and a block inserted before another (or two swapped blocks)
    # changes the fingerprint behind each key
    base = "fn f() {\n    // SAFETY: a\n    unsafe { one(\"x\") };\n    // SAFETY: b\n    unsafe { two() };\n}\n"
    variants = {
        "comment": base.replace("// SAFETY: a", "// SAFETY: a, reworded"),
        "spacing": base.replace("unsafe { one(\"x\") };", "unsafe {\n        one( \"x\" )\n    };"),
        "literal": base.replace('one(\"x\")', 'one(\"y\")'),
        "token": base.replace("two()", "three()"),
        "swap": base.replace("one(\"x\")", "TMP").replace("two()", 'one(\"x\")').replace("TMP", "two()"),
        "insert": base.replace("    // SAFETY: a\n", "    // SAFETY: c\n    unsafe { zero() };\n    // SAFETY: a\n"),
    }
    fp0 = [x.fingerprint for x in scan_source(base, "src/lib.rs")]
    fpv = {k: [x.fingerprint for x in scan_source(v, "src/lib.rs")] for k, v in variants.items()}
    expect = {
        "comment": fpv["comment"] == fp0,
        "spacing": fpv["spacing"] == fp0,
        "literal": fpv["literal"][0] != fp0[0] and fpv["literal"][1] == fp0[1],
        "token": fpv["token"][0] == fp0[0] and fpv["token"][1] != fp0[1],
        "swap": fpv["swap"] == [fp0[1], fp0[0]],
        "insert": fpv["insert"][0] != fp0[0] and fpv["insert"][1:] == fp0,
    }
    for k, good in expect.items():
        if not good or len(set(fp0)) != 2:
            print(f"fingerprint self-test FAILED ({k}): {fp0} vs {fpv[k]}")
            ok = False
    # every kind has a fingerprint over its whole extent
    if any(len(x.fingerprint) != 16 for x in sites):
        print("fingerprint self-test FAILED: a site without a fingerprint")
        ok = False
    print("self-test:", "PASS" if ok else "FAIL")
    return 0 if ok else 1


# ---------------------------------------------------------------------- main


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--check", action="store_true")
    g.add_argument("--list", nargs="?", const="", metavar="CRATE")
    g.add_argument("--skeleton", metavar="CRATE")
    g.add_argument("--update-lines", action="store_true")
    g.add_argument("--refresh", nargs="+", metavar="ID", help="re-pin the fingerprints of reviewed rows")
    g.add_argument("--pin-missing", action="store_true", help="pin the rows that have no fingerprint")
    ap.add_argument(
        "--force", action="store_true",
        help="with --pin-missing: also pin rows that are pinned in HEAD but whose line is gone",
    )
    g.add_argument("--self-test", action="store_true")
    ap.add_argument("--strict-safety", action="store_true", help="with --check: a site without an adjacent SAFETY comment fails")
    a = ap.parse_args()
    if a.force and not a.pin_missing:
        ap.error("--force applies to --pin-missing only")
    if a.self_test:
        return self_test()
    if a.update_lines:
        return update_lines()
    if a.refresh:
        return pin(a.refresh)
    if a.pin_missing:
        return pin(None, a.force)
    if a.check:
        if self_test() != 0:
            return 1
        problems = check()
        notes = safety_notes()
        if a.strict_safety:
            problems += notes
        else:
            for n in notes:
                print("note:", n)
        total = {c.name: len(scan_crate(c)) for c in crate_dirs()}
        listed = {k: v for k, v in total.items() if v}
        print("unsafe sites per crate:", ", ".join(f"{k} {v}" for k, v in listed.items()))
        for p in problems:
            print(p)
        print(f"unsafe inventory: {'FAIL' if problems else 'PASS'} ({len(problems)} problems)")
        return 1 if problems else 0
    crates = crate_dirs()
    sel = a.list if a.list is not None else a.skeleton
    if sel:
        crates = [c for c in crates if c.name == sel]
        if not crates:
            print(f"no crate {sel}", file=sys.stderr)
            return 2
    for crate in crates:
        sites = scan_crate(crate)
        if a.skeleton:
            cur = None
            for k, s in enumerate(sites, 1):
                if s.file != cur:
                    cur = s.file
                    print(f"\n### {s.file}\n\n| Id | Line | Site | What it does | Invariant | Established by | Test |\n|---|---|---|---|---|---|---|")
                print(f"| X-{k:02d} | {s.line} | {s.cell()} | | | | | |")
        else:
            for s in sites:
                flag = "" if s.has_safety else "  [no SAFETY comment]"
                print(f"{crate.name}/{s.file}:{s.line}\t{s.kind}\t{s.fingerprint}\t{s.cell()}{flag}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
