#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-024).
#
#   ci/github/manual-build.sh <tools-bin-dir> [<dest-dir>]
#
# Builds the user manual (docs/manual) with the `mdbook` and `mdbook-katex`
# binaries of <tools-bin-dir> (the `manual` and `site` jobs of
# ci/github/docs.yml install mdBook 0.4.40 and mdbook-katex 0.9.4 there with
# `cargo install --root`), into <dest-dir> (default: docs/manual/book).
#
# mdBook alone would not fail on a formula that KaTeX cannot parse: the
# preprocessor prints "Rendering failed, keeping the original content" and
# leaves the LaTeX source in the page. This script fails in that case, and
# also when the built book holds no rendered formula at all (the preprocessor
# did not run), so a manual with a broken formula is never a pass. It needs
# no network: the KaTeX stylesheet and fonts are files of docs/manual/src.
set -uo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
bin="${1:-}"
[ -n "$bin" ] || {
  echo "usage: manual-build.sh <tools-bin-dir> [<dest-dir>]" >&2
  exit 2
}
dest="${2:-$repo/docs/manual/book}"
for tool in mdbook mdbook-katex; do
  [ -x "$bin/$tool" ] || {
    echo "manual-build: FAIL no executable $bin/$tool" >&2
    exit 1
  }
done
bin="$(cd "$bin" && pwd)"
log="$(mktemp "${TMPDIR:-/tmp}/manual-build.XXXXXX")"
trap 'rm -f "$log"' EXIT

PATH="$bin:$PATH" "$bin/mdbook" build "$repo/docs/manual" -d "$dest" 2>&1 | tee "$log"
rc="${PIPESTATUS[0]}"
[ "$rc" = 0 ] || {
  echo "manual-build: FAIL mdbook build exited with $rc" >&2
  exit 1
}
if grep -q 'Rendering failed' "$log"; then
  echo "manual-build: FAIL $(grep -c 'Rendering failed' "$log") formula(s) did not render (the warnings above name them)" >&2
  exit 1
fi
if ! grep -rqs 'class="katex' "$dest" --include='*.html'; then
  echo "manual-build: FAIL no rendered formula in $dest (did the katex preprocessor run?)" >&2
  exit 1
fi
[ -s "$dest/katex/katex.min.css" ] || {
  echo "manual-build: FAIL $dest/katex/katex.min.css is missing (the formulas' stylesheet)" >&2
  exit 1
}
echo "manual-build: OK $dest"
