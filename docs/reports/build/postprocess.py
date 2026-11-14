# SPDX-License-Identifier: LGPL-3.0-or-later
"""Adjust pandoc's HTML of a report for print (used by build.sh).

    python3 postprocess.py report.html

1. Removes the <colgroup> widths that pandoc derives from the dashes of a
   pipe table's separator line, so that the browser sizes the columns by
   their content (a column of short labels no longer takes half the width).
2. Lays out record tables as one block per row: a table of five or more
   columns whose first header cell is "ID" (the defect tables of report 03,
   with file paths and paragraphs in their cells) cannot be read at the A4
   text width. Each row becomes a heading (the ID) followed by the other
   cells, each under its column header. Every cell is kept, in order; the
   Markdown source and its table on GitHub are unchanged.
3. Marks a paragraph that ends with a colon and is followed by a list, a
   table or a code block with the class "lead-in", which report.css keeps on
   the page of what follows it.
"""
from __future__ import annotations

import re
import sys

TABLE = re.compile(r"<table[^>]*>.*?</table>", re.S)
CELL = re.compile(r"<(th|td)[^>]*>(.*?)</\1>", re.S)
ROW = re.compile(r"<tr[^>]*>(.*?)</tr>", re.S)


def record_layout(table: str) -> str | None:
    head = re.search(r"<thead>(.*?)</thead>", table, re.S)
    body = re.search(r"<tbody>(.*?)</tbody>", table, re.S)
    if not head or not body:
        return None
    headers = [c[1].strip() for c in CELL.findall(head.group(1))]
    if len(headers) < 5 or re.sub(r"<[^>]+>", "", headers[0]).strip() != "ID":
        return None
    out = ['<div class="records">']
    for row in ROW.findall(body.group(1)):
        cells = [c[1].strip() for c in CELL.findall(row)]
        out.append('<section class="record">')
        out.append(f'<p class="record-id">{cells[0]}</p><dl>')
        for h, c in zip(headers[1:], cells[1:]):
            out.append(f"<dt>{h}</dt><dd>{c or '&nbsp;'}</dd>")
        out.append("</dl></section>")
    out.append("</div>")
    return "\n".join(out)


LEAD_IN = re.compile(r"<p>((?:(?!</p>).)*?:)</p>(\s*<(?:ul|ol|table|pre|div class=\"(?:records|sourceCode)\")\b)", re.S)


def process(html: str) -> str:
    html = re.sub(r"<colgroup>.*?</colgroup>", "", html, flags=re.S)
    html = TABLE.sub(lambda m: record_layout(m.group(0)) or m.group(0), html)
    return LEAD_IN.sub(r'<p class="lead-in">\1</p>\2', html)


if __name__ == "__main__":
    path = sys.argv[1]
    with open(path, encoding="utf-8") as f:
        text = f.read()
    with open(path, "w", encoding="utf-8") as f:
        f.write(process(text))
