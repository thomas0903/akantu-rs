#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu; row CI-017).
"""Version check of the `version` tier.

    version_check.py <cargo-metadata.json> [--tag TAG] [--pyproject FILE]
                     [--describe FILE]

* every workspace member of `cargo metadata --no-deps` carries one version
  (`workspace.package.version`, inherited by every member);
* `--pyproject`: the Python package takes its version from Cargo
  (`dynamic = ["version"]` listing "version", no static `version =` key in
  the [project] table), so the wheel's version is the tag's;
* `--tag`: the tag must be `v<version>` or `<version>`;
* `--describe`: the output of `git describe --tags --always --dirty` (printed).

Exit 0 when every check passes, 1 otherwise. Without --tag the tag check is
reported as not applicable (never as passed).
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


def check(meta: dict, tag: str | None, pyproject: str | None) -> tuple[str, list[str], list[str]]:
    """Return (version, messages, failures)."""
    msgs: list[str] = []
    fails: list[str] = []
    members = meta.get("packages", [])
    if not members:
        return "", msgs, ["cargo metadata lists no workspace member"]
    versions: dict[str, list[str]] = {}
    for p in members:
        versions.setdefault(p["version"], []).append(p["name"])
    if len(versions) != 1:
        detail = "; ".join(f"{v}: {', '.join(sorted(n))}" for v, n in sorted(versions.items()))
        fails.append(f"workspace members carry {len(versions)} different versions ({detail})")
    version = sorted(versions, key=lambda v: -len(versions[v]))[0]
    msgs.append(f"workspace.package.version = {version} ({len(members)} members)")
    if pyproject is not None:
        path = Path(pyproject)
        if not path.is_file():
            msgs.append("python/pyproject.toml: absent (not checked)")
        else:
            text = path.read_text(encoding="utf-8")
            m = re.search(r"^\[project\]\n(.*?)(?=^\[)", text + "\n[end]\n", re.S | re.M)
            body = m.group(1) if m else ""
            if re.search(r"^version\s*=", body, re.M):
                fails.append("python/pyproject.toml has a static `version =`: the wheel would not follow the tag")
            elif not re.search(r'^dynamic\s*=\s*\[[^\]]*"version"', body, re.M):
                fails.append('python/pyproject.toml: `dynamic = ["version"]` is missing')
            else:
                msgs.append("python/pyproject.toml: version is dynamic (maturin reads it from Cargo)")
    if tag:
        if tag in (version, "v" + version):
            msgs.append(f"tag {tag} names version {version}")
        else:
            fails.append(f"tag {tag!r} does not name workspace.package.version {version} (expected v{version})")
    else:
        msgs.append("no tag given: the tag check is not applicable")
    return version, msgs, fails


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("metadata")
    ap.add_argument("--tag")
    ap.add_argument("--pyproject")
    ap.add_argument("--describe")
    a = ap.parse_args(argv)
    meta = json.loads(Path(a.metadata).read_text(encoding="utf-8"))
    _version, msgs, fails = check(meta, a.tag, a.pyproject)
    if a.describe and Path(a.describe).is_file():
        print("git describe:", Path(a.describe).read_text(encoding="utf-8").strip())
    for m in msgs:
        print("ok  ", m)
    for f in fails:
        print("FAIL", f)
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
