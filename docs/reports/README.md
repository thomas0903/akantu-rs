# Reports on akantu-rs

Short reports for the Akantu developers at EPFL. Each Markdown file has a PDF
next to it, built from the same text.

| Report | PDF | Subject |
|---|---|---|
| [`00-final-report.md`](00-final-report.md) | [PDF](00-final-report.pdf) | Summary of all the reports |
| [`01-migration-and-features.md`](01-migration-and-features.md) | [PDF](01-migration-and-features.pdf) | How the port was made and verified against the C++ library, and what it offers |
| [`02-benchmarks-and-performance.md`](02-benchmarks-and-performance.md) | [PDF](02-benchmarks-and-performance.pdf) | Measured performance of the port against the C++ library on the same machines |
| [`03-upstream-defects.md`](03-upstream-defects.md) | [PDF](03-upstream-defects.pdf) | Defects found in the C++ library while porting it, and how the port handles them |
| [`04-possible-improvements.md`](04-possible-improvements.md) | [PDF](04-possible-improvements.pdf) | What could be improved in performance, features and code health, with a roadmap |
| [`05-ai-setup.md`](05-ai-setup.md) | [PDF](05-ai-setup.pdf) | How the work was organised: agents, models, tools, machines and cost |

[`2026-11-12-paraview-visual-check.md`](2026-11-12-paraview-visual-check.md)
is an earlier, separate report on the ParaView comparison of the example
outputs. The full lists of findings behind report 04 are in `data/`:
[`improvements-performance.md`](data/improvements-performance.md),
[`improvements-features.md`](data/improvements-features.md) and
[`improvements-maintainability.md`](data/improvements-maintainability.md).
The PDFs are rebuilt from the Markdown with the commands below.

## Layout

- `figures/`: the figures the reports include, as SVG files.
- `data/`: the numbers behind the charts, one CSV file per chart (lines
  that start with `#` name the source of the values below them), the three
  scans behind report 04 and `benchmarks-provenance.md`, the source of each
  performance number.
- `build/`: the tools that render the figures and the PDFs.
  - `figures.py`: one function per figure. Charts read their numbers from
    `data/`; diagrams are laid out by hand.
  - `figstyle.py`: the chart style (colours, fonts, CSV reader). In every
    figure the port is blue and the C++ library orange, and no other series
    uses either colour; published reference values are grey. Other series
    (phases of a run, defect categories, models) take the remaining slots
    of the palette.
  - `diagram.py`: box-and-arrow diagrams written directly as SVG.
  - `build.sh`, `postprocess.py`, `report.css`: Markdown to PDF.

## Rebuilding

Needs `pandoc`, Google Chrome or Chromium, and `uv` (which fetches
matplotlib for the figures; nothing is installed system-wide).

```sh
docs/reports/build/build.sh                      # figures, then every NN-*.md
docs/reports/build/build.sh 02-benchmarks-and-performance.md   # one report
SKIP_FIGURES=1 docs/reports/build/build.sh       # PDFs only
```

Figures alone:

```sh
MPLBACKEND=Agg uv run --with matplotlib==3.10.7 python3 docs/reports/build/figures.py [name ...]
```

The SVG output is reproducible: running the script twice gives identical
files. Set `CHROME=/path/to/chrome` if Chrome is not found, and
`REPORT_DATE` to fix the date printed under each title.

In the PDFs, tables whose first column is `ID` and that have five or more
columns (the defect tables of report 03) are printed as one block per row;
the Markdown tables are unchanged.
