# ParaView visual check of the example outputs

Loads every ParaView file written by the port's examples and by upstream's
(the C++ reference run in the oracle image) in a headless `pvpython`, checks
that ParaView reads them without a message, compares the two sides and draws
side-by-side pictures (port left, upstream right, same camera, same colour
scale). Report: `docs/reports/2026-11-12-paraview-visual-check.md`; pictures
and gallery: `docs/reports/paraview-visual-check/`.

**Run it on a Linux x86_64 machine with Docker and the oracle image** (docs/TESTING.md).

```sh
AKANTU_SRC_HOST=<reference checkout at the pinned commit> scripts/paraview/run-all.sh <work-dir>
# the differing files read directly, per example (on the machine of the runs):
python3 -I scripts/paraview/vtu_diff.py <work-dir>/port/<example> <work-dir>/oracle/<case> <vd>/<example>.tsv > <vd>/<example>.txt
python3 -I scripts/paraview/vtu_diff.py <work-dir>/py-runs/<case>/port/run <work-dir>/py-runs/<case>/upstream/run <vd>/py-<case>.tsv > <vd>/py-<case>.txt
# then, with python3 only, on the copied <work-dir>/res, <work-dir>/runinfo.json and <vd>:
python3 -I scripts/paraview/make_report.py <res> <runinfo.json> <vd>
```

`run-all.sh` runs these steps (set `STEPS="..."` to resume part of them):

| Script | What it does |
|---|---|
| `docker/paraview/Dockerfile` | the image `akantu-rs-paraview:5.13.3`: ParaView 5.13.3 official headless (OSMesa) binary tarball, SHA-256 checked |
| `run-oracle.sh` | the upstream examples (cases of `oracle/probes/examples/cases.tsv`, one run each) in the oracle image; every file they write is kept |
| `run-port.sh` | the Rust examples, release build, with the arguments of `cases.tsv` (not the `python:` row) |
| `run-ex012.sh` | `python_user_defined_bc` (the program embedding CPython) on both sides: upstream's binary of the build tree, the port built with `--features python` and run with Debian's NumPy |
| `build-wheel.sh`, `run-python-examples.sh` | the port's Python wheel; upstream's Python examples (`python/examples`) run with the port's module and with the oracle's module (`python-cases.tsv`) |
| `run-paraview.sh`, `run-paraview-python.sh` | `pv_example.py` in the ParaView image, once per example: loads, compares, renders |
| `runinfo.py` | what each run wrote and printed (exit code, file counts), as one JSON file |
| `make_report.py` | PNGs, `gallery.md`, `gallery.html`, `details.md`, `arrays.md`, per-file array tables (gzip with a zero time stamp), the vtu-diff tables and the report from `report_template.md`, `differences.md` (the hand-written section 3) and `verdicts.tsv` (the verdict of every run) |

Not part of `run-all.sh`: `vtu_diff.py` (above) reads the two files of every
differing pair directly, without ParaView, and classifies each array
(identical values, upstream's zero-filled matrix squares of UD-110, the
uninitialised entries of UD-111, the invalid empty base64 of UD-114, arrays
shorter or longer than their piece, other meshes, other values).

Tables: `cases.tsv` (example, binary, oracle case, port arguments,
description), `python-cases.tsv`. The `scalability_test` rows use the oracle
cases `scalability_test_paraview` / `scalability_test_cohesive_paraview`, whose
input files (`oracle/probes/examples/scalability_*_paraview.dat`) switch the
ParaView dumps on; the port reads the same files from the oracle's run
directory.

What the loader does and does not do: it opens every `.pvd` (all time steps),
`.pvtu` and `.vtu` with ParaView's own readers and counts the messages ParaView
prints (the VTK output window and standard error); a message is a failure and
is reported, never filtered. It compares file names and SHA-256 of both sides
and, for files that differ, reads them back through ParaView and reports the
largest difference of every array. It draws the first, a middle and the last
dump. Nothing is fixed or smoothed: a difference is recorded as found.

Notes on the pictures: the stress is the von Mises stress computed in the
ParaView filter from the dumped `stress` (plane-stress formula for 2-D
bodies); points that no cell uses (the pure-ghost points of a distributed
dump) are left out; the PNGs are 8-bit indexed (at most 256 colours: the frequent colours exact,
the rest median-cut) so that the whole gallery stays repository-sized.
