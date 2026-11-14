#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Build the PDF of each report from its Markdown source.
#
#   docs/reports/build/build.sh            # figures, then every NN-*.md
#   docs/reports/build/build.sh 02-*.md    # one report (path relative to docs/reports)
#   SKIP_FIGURES=1 docs/reports/build/build.sh
#
# Needs: pandoc, Google Chrome (or Chromium), and uv for the figures
# (matplotlib is fetched by uv; nothing is installed system-wide).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPORTS="$(dirname "$HERE")"
cd "$REPORTS"

CHROME="${CHROME:-}"
if [[ -z "$CHROME" ]]; then
  for c in "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
           "$(command -v chromium 2>/dev/null || true)" \
           "$(command -v chromium-browser 2>/dev/null || true)" \
           "$(command -v google-chrome 2>/dev/null || true)"; do
    if [[ -n "$c" && -x "$c" ]]; then CHROME="$c"; break; fi
  done
fi
[[ -n "$CHROME" ]] || { echo "Chrome or Chromium not found (set CHROME=...)" >&2; exit 1; }
command -v pandoc >/dev/null || { echo "pandoc not found" >&2; exit 1; }

if [[ -z "${SKIP_FIGURES:-}" ]]; then
  MPLBACKEND=Agg uv run --quiet --with matplotlib==3.10.7 python3 "$HERE/figures.py"
fi

if [[ $# -gt 0 ]]; then FILES=("$@"); else FILES=([0-9][0-9]-*.md); fi
TMP="$(mktemp -d "${TMPDIR:-/tmp}/akantu-reports.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
REVISION="${REPORT_DATE:-$(date +'%-d %B %Y')}"

for md in "${FILES[@]}"; do
  base="${md%.md}"
  title="$(grep -m1 '^# ' "$md" | sed 's/^# //')"
  # The first H1 becomes the title block; the line of metadata follows it.
  python3 - "$md" "$TMP/$base.md" "$REVISION" <<'PY'
import re, sys
src, dst, rev = sys.argv[1:4]
text = open(src, encoding="utf-8").read()
meta = f'\n<div class="report-meta">akantu-rs, a Rust port of Akantu &nbsp;·&nbsp; {rev}</div>\n'
text, n = re.subn(r"(?m)^(# .+)$", lambda m: m.group(1) + "\n" + meta, text, count=1)
open(dst, "w", encoding="utf-8").write(text)
PY
  pandoc "$TMP/$base.md" \
    --from=markdown+pipe_tables+implicit_figures+raw_html-smart \
    --to=html5 --standalone --embed-resources \
    --resource-path="$REPORTS" \
    --css="$HERE/report.css" \
    --metadata pagetitle="$title" --metadata lang=en \
    --output="$TMP/$base.html"
  python3 "$HERE/postprocess.py" "$TMP/$base.html"
  "$CHROME" --headless=new --disable-gpu --no-pdf-header-footer \
    --run-all-compositor-stages-before-draw --virtual-time-budget=5000 \
    --print-to-pdf="$REPORTS/$base.pdf" "file://$TMP/$base.html" 2>/dev/null
  echo "built $base.pdf"
done
