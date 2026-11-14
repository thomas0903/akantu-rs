#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-024, D-114: the
# documentation is hosted on GitHub Pages).
#
#   ci/github/pages-site.sh <out-dir> [--rustdoc DIR] [--manual DIR] [--python DIR]
#
# Assembles the ONE site that the `deploy` job of ci/github/docs.yml publishes:
#   <out-dir>/index.html   the page that links the three parts
#   <out-dir>/manual/      the user manual (`mdbook build docs/manual` -> docs/manual/book)
#   <out-dir>/rust/        the Rust API documentation (`cargo doc --workspace --no-deps` -> target/doc)
#   <out-dir>/python/      the Python API documentation (tier `pydocs`: <ci-out>/pydocs/html)
# Defaults: --rustdoc ${CARGO_TARGET_DIR:-<repo>/target}/doc, --manual
# <repo>/docs/manual/book; --python has no default (the tier writes it under
# the job's --out directory). The script only copies and checks (nothing is
# built, nothing is uploaded, no network): it fails, naming the cause, when a
# part is missing or empty, when its entry page is absent, or when a link of
# index.html does not resolve to a file of the assembled site. A replay on a
# machine without GitHub (ci/github/replay.sh docs:site, ci/selftest.sh) runs
# exactly this step; the deployment itself runs only on GitHub
# (ci/github/README.md).
set -uo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
usage() {
  echo "usage: pages-site.sh <out-dir> [--rustdoc DIR] [--manual DIR] [--python DIR]" >&2
  exit 2
}
out="${1:-}"
[ -n "$out" ] || usage
shift
rustdoc="${CARGO_TARGET_DIR:-$repo/target}/doc"
manual="$repo/docs/manual/book"
python=""
while [ $# -gt 0 ]; do
  [ $# -ge 2 ] || usage
  case "$1" in
    --rustdoc) rustdoc="$2" ;;
    --manual) manual="$2" ;;
    --python) python="$2" ;;
    *) usage ;;
  esac
  shift 2
done
[ -n "$python" ] || {
  echo "pages-site: --python DIR is required (the pydocs tier's html directory)" >&2
  exit 2
}

fail=0
bad() {
  echo "pages-site: FAIL $*" >&2
  fail=1
}
# part <name> <source-dir> <entry page relative to the part>
part() {
  local name="$1" src="$2" entry="$3"
  if [ ! -d "$src" ]; then
    bad "$name: no directory $src"
  elif [ -z "$(ls -A "$src" 2> /dev/null)" ]; then
    bad "$name: $src is empty"
  elif [ ! -s "$src/$entry" ]; then
    bad "$name: $src has no entry page $entry"
  else
    mkdir -p "$out/$name"
    cp -R "$src/." "$out/$name/"
    echo "pages-site: $name <- $src ($(find "$out/$name" -type f | wc -l | tr -d ' ') files, entry $entry)"
  fi
}
rm -rf "$out"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
part manual "$manual" index.html
part rust "$rustdoc" akantu/index.html
part python "$python" akantu.html
[ "$fail" = 0 ] || exit 1

cat > "$out/index.html" << 'HTML'
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>akantu-rs documentation</title>
<style>
  body { font-family: system-ui, sans-serif; max-width: 40rem; margin: 3rem auto; padding: 0 1rem; line-height: 1.5; }
  li { margin: 0.6rem 0; }
</style>
</head>
<body>
<h1>akantu-rs documentation</h1>
<p>A Rust port of the Akantu finite-element library (LGPL-3.0-or-later).</p>
<ul>
  <li><a href="manual/index.html">User manual</a> (mdBook)</li>
  <li><a href="rust/akantu/index.html">Rust API documentation</a> (rustdoc, the <code>akantu</code> crate; the other crates are listed in its sidebar)</li>
  <li><a href="python/akantu.html">Python API documentation</a> (pdoc, the <code>akantu</code> package)</li>
</ul>
</body>
</html>
HTML

# every link of the index page resolves to a file of the assembled site
n=0
while IFS= read -r href; do
  n=$((n + 1))
  case "$href" in
    http:* | https:* | mailto:*) continue ;;
  esac
  [ -s "$out/${href%%[#?]*}" ] || bad "index.html links $href: no such file in the site"
done < <(grep -o 'href="[^"]*"' "$out/index.html" | sed -e 's/^href="//' -e 's/"$//')
[ "$n" -ge 3 ] || bad "index.html has $n links (want the three parts)"
[ "$fail" = 0 ] || exit 1
echo "pages-site: OK $out ($n links resolve)"
