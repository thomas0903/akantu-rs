# SPDX-License-Identifier: LGPL-3.0-or-later
"""Render every figure of the reports into docs/reports/figures/.

Each figure is one function registered with @figure. Charts read their numbers
from docs/reports/data/*.csv (each file names its sources); diagrams are laid
out by hand with diagram.py. Run through build.sh, or directly:

    MPLBACKEND=Agg uv run --with matplotlib==3.10.7 python3 docs/reports/build/figures.py [name ...]

Colour follows the entity in every figure: the Rust port is blue (fs.RUST),
the C++ library orange (fs.CPP), published reference values grey
(fs.REFERENCE). No other series uses blue or orange: series that are neither
code take the free categorical slots in order (fs.OTHER); the phases of a run
keep the same colours in every figure (PHASE_COLOURS).
"""
from __future__ import annotations

import math
import sys
import textwrap
import warnings
from statistics import median

import figstyle as fs
from matplotlib.lines import Line2D
from matplotlib.patches import Patch
from matplotlib.ticker import FuncFormatter, MultipleLocator

import diagram as dg
from diagram import Diagram

# Matplotlib measures text with the first font it finds, which lacks the arrow
# glyph; the SVG keeps the text and the browser falls back to a font that has it.
warnings.filterwarnings("ignore", message="Glyph 8592")

FIGS: dict[str, callable] = {}


def figure(fn):
    FIGS[fn.__name__.replace("_", "-")] = fn
    return fn


def thousands(ax_axis) -> None:
    ax_axis.set_major_formatter(FuncFormatter(lambda v, _: fs.num(v)))


# Phases of a run (reports 02 and 04): set-up phases in neutral greys, the
# phases of the explicit loop in the next categorical slots.
PHASE_COLOURS = {
    "static solve": "#a3a29b",
    "mesh distribution": "#c4c3bc",
    "other set-up": "#e0dfd9",
    "explicit step": fs.OTHER[0],
    "insertion check": fs.OTHER[1],
    "energies": fs.OTHER[2],
}
CODE_NAME = {"rust": "port", "cpp": "C++"}


# --------------------------------------------------------------------------
# Report 01: migration and features
# --------------------------------------------------------------------------
@figure
def parity_by_area():
    rows = [r for r in fs.read_csv("parity-by-area") if r["area"] != "Total" and int(r["rows"]) > 0]
    half = (len(rows) + 1) // 2
    fig, axes = fs.new(6.7, 4.6, ncols=2, sharex=True)
    fig.subplots_adjust(wspace=0.95)
    segs = [("verified", "Verified", fs.RUST),
            ("exception_approved", "Approved exception", fs.NEUTRAL),
            ("implemented_unverified", "Implemented, unverified", fs.OTHER[0])]
    vmax = max(int(r["rows"]) for r in rows)
    for ax, part in zip(axes, (rows[:half], rows[half:])):
        ys = list(range(len(part)))
        left = [0] * len(part)
        for col, _, colour in segs:
            vals = [int(r[col]) for r in part]
            ax.barh(ys, vals, left=left, height=0.66, color=colour, edgecolor=fs.SURFACE, linewidth=0.6)
            left = [a + b for a, b in zip(left, vals)]
        ax.set_yticks(ys, [f"{r['name']} ({r['area']})" for r in part], fontsize=8)
        ax.set_ylim(len(part) - 0.4, -0.6)
        ax.grid(axis="x")
        ax.grid(axis="y", visible=False)
        ax.xaxis.set_major_locator(MultipleLocator(50))
        ax.set_xlim(0, vmax * 1.04)
        ax.set_xlabel("parity rows")
        ax.tick_params(axis="y", length=0)
    handles = [Patch(color=c, label=lab) for _, lab, c in segs]
    fig.legend(handles=handles, loc="upper left", ncols=3, bbox_to_anchor=(0.0, 1.0), frameon=False)
    fig.subplots_adjust(top=0.92)
    fs.save(fig, "parity-by-area")


@figure
def fixtures_by_area():
    rows = fs.read_csv("fixtures-by-area")
    cases = [r for r in rows if r["kind"] == "cases with manifest"]
    top = [r for r in cases if "/" not in r["group"]]
    model = [r for r in cases if r["group"].startswith("model/")]
    model_total = sum(int(r["count"]) for r in model)
    groups = [(r["label"], int(r["count"]), "per rank" in r["label"]) for r in top]
    groups.append(("models (the model/ group)", model_total, False))
    groups.sort(key=lambda g: -g[1])
    sub = sorted(((r["label"], int(r["count"])) for r in model), key=lambda g: -g[1])
    extra = [r for r in rows if r["kind"] != "cases with manifest"]

    fig, (a, b) = fs.new(6.7, 4.9, ncols=2, gridspec_kw={"width_ratios": [1.25, 1]})
    fig.subplots_adjust(wspace=0.62)
    light = "#f5b99f"  # the C++ orange at about half strength: distributed cases
    for ax, items, title in ((a, groups, "By group"), (b, [(l, c, False) for l, c in sub],
                                                       f"model/ ({fs.num(model_total)} cases)")):
        ys = list(range(len(items)))
        ax.barh(ys, [c for _, c, _ in items], height=0.66,
                color=[light if pr else fs.CPP for _, _, pr in items])
        labels = [textwrap.fill(l.replace(" (per rank)", ""), 34) for l, _, _ in items]
        ax.set_yticks(ys, labels, fontsize=7.6, linespacing=0.95)
        ax.set_ylim(len(items) - 0.4, -0.6)
        ax.grid(axis="x")
        ax.grid(axis="y", visible=False)
        ax.tick_params(axis="y", length=0)
        ax.set_title(title, fontsize=9.5)
        ax.set_xlabel("fixture cases")
        thousands(ax.xaxis)
    a.xaxis.set_major_locator(MultipleLocator(200))
    b.xaxis.set_major_locator(MultipleLocator(50))
    fig.legend(handles=[Patch(color=fs.CPP, label="one directory per case"),
                        Patch(color=light, label="distributed case: one directory per rank")],
               loc="upper left", bbox_to_anchor=(0.0, 0.0), ncols=2, fontsize=8, handlelength=1.2)
    note = ", ".join(f"{fs.num(int(r['count']))} {r['group'].rstrip('*')}" for r in extra)
    fs.footnote(fig, f"Not counted (files and digests instead of manifests): {note} directories.", y=-0.075)
    fs.save(fig, "fixtures-by-area")


@figure
def verification_pipeline():
    d = Diagram(760, 470, "How a feature of the port was verified")
    bw, bh, gap = 156, 70, 30
    xs = [22 + i * (bw + gap) for i in range(4)]
    # Lanes
    d.group(8, 8, 744, 116, "C++ reference (oracle)")
    d.group(8, 140, 744, 160, "Rust port")
    d.group(8, 316, 744, 146, "Acceptance")
    y1, y2, y3 = 40, 198, 372
    d.box("cpp", xs[0], y1, bw, bh, "Akantu C++ 35a85178", "never edited; build patches\nin oracle/patches/",
          kind="orange")
    d.box("build", xs[1], y1, bw, bh, "Strict build", "in Docker: no FMA, scalar\nEigen, BLAS pinned",
          kind="orange")
    d.box("probes", xs[2], y1, bw, bh, "Probes", "C++ and Python programs\nrunning upstream's classes",
          kind="orange")
    d.box("fix", xs[3], y1, bw, bh, "Golden fixtures", "2,023 cases: manifest\nand .npy arrays", kind="orange")
    # Rust lane: the implementation and a group of three kinds of test
    d.box("impl", xs[0], y2, bw, 80, "Rust implementation", "same operation order,\nno FMA, no fast-math",
          kind="blue")
    d.group(xs[1] - 10, 158, 3 * bw + 2 * gap + 20, 132, "Tests", fill="#ffffff", stroke="#d9d8d2")
    d.box("up", xs[1], y2, bw, 80, "Ported upstream tests", "182 TEST rows:\n177 Verified,\n5 exceptions")
    d.box("ind", xs[2], y2, bw, 80, "Independent checks", "patch tests, closed\nforms, convergence\norders, invariants")
    d.box("diff", xs[3], y2, bw, 80, "Differential tests", "bit for bit where possible,\nelse a reviewed tolerance\n(240 rules, 197 exact)")
    # Acceptance lane, read right to left
    d.box("rev", xs[3], y3, bw, bh, "Independent review", "by an agent that did\nnot write the code")
    d.box("row", xs[2], y3, bw, bh, "Parity row Verified", "tests, fixture, tolerance\nreason, review")
    d.box("gate", xs[1], y3, bw, bh, "Merge gate, x86_64", "fmt, clippy, every test,\nMPI, MUMPS, PETSc tiers")
    d.box("final", xs[0], y3, bw, bh, "Final campaign", "the gate again, MPI to\n8 ranks, 23 CI jobs")
    # Arrows (the order inside a lane needs no labels)
    d.arrow("cpp", "build")
    d.arrow("build", "probes")
    d.arrow("probes", "fix")
    d.arrow("fix", "diff", "read by", sa="b", sb="t", label_dx=6, label_dy=4, label_at=0.3, label_anchor="start")
    d.polyline([(xs[0] + bw, y2 + 40), (xs[1] - 10, y2 + 40)])
    d.arrow("diff", "rev", "evidence", sa="b", sb="t", label_dx=6, label_dy=6, label_anchor="start")
    d.arrow("rev", "row", sa="l", sb="r")
    d.arrow("row", "gate", sa="l", sb="r")
    d.arrow("gate", "final", sa="l", sb="r")
    gx, ix = xs[1] + bw * 0.5, xs[0] + bw * 0.78
    d.polyline([(gx, y3), (gx, 342), (ix, 342), (ix, y2 + 80)],
               "a failure blocks the merge", dashed=True, label_seg=0, label_at=0.5, label_dx=8,
               label_dy=4, label_anchor="start")
    d.save(fs.FIGURES / "verification-pipeline.svg")
    print("figure verification-pipeline.svg")



@figure
def crate_dependencies():
    crates = {r["crate"]: r for r in fs.read_csv("crates")}
    # Hand layout: column (0..5) of each crate in its layer's row. Rows follow
    # the `layer` column, except that akantu-parser is drawn one row lower so
    # that the edge from akantu-solver reaches it without crossing a box.
    col = {"core": 2, "validation": 5, "parser": 0, "comm": 2, "linalg": 1, "mesh": 3, "solver": 1,
           "fe": 3, "geometry": 5, "io": 2, "geometry-cgal": 5, "model": 2, "diffusion": 0,
           "contact": 1, "solid": 2, "phase-field": 3, "structural": 4, "extra-materials": 0,
           "cohesive": 1, "ntn-contact": 2, "igfem": 4, "embedded": 5, "bench": 0, "couplers": 1,
           "": 2, "python": 1, "capi": 3}
    row_shift = {"parser": 1}
    short = lambda c: c.removeprefix("akantu").removeprefix("-")
    W, bw, bh, pitch, x0, y0 = 760, 108, 28, 50, 20, 10
    d = Diagram(W, y0 + 10 * pitch + bh + 56, "The crates of akantu-rs and their direct dependencies")
    X = lambda c: x0 + c * (bw + 16)
    pos = {}
    for name, r in crates.items():
        k = short(name)
        layer = int(r["layer"]) + row_shift.get(k, 0)
        x, y = X(col[k]), y0 + layer * pitch
        pos[name] = (x, y)
        kind = "blue" if name == "akantu" else ("yellow" if r["licence"].startswith("GPL") else "default")
        dotted = name in ("akantu-validation", "akantu-bench", "akantu-capi")
        d.box(name, x, y, bw, bh, name if name == "akantu" else k, kind=kind, dashed=dotted,
              title_size=11, mono=True)
    top = lambda n, t=0.5: (pos[n][0] + bw * t, pos[n][1])
    bot = lambda n, t=0.5: (pos[n][0] + bw * t, pos[n][1] + bh)
    mid_y = lambda n, t=0.5: pos[n][1] + bh * t

    def dashed_edge(src, dst):
        r = crates[src]
        return dst in r["optional_deps"].split() and crates[dst]["off_by_default"] == "yes"

    edges = [(s_, t_) for s_, r in crates.items() for t_ in r["direct_deps_reduced"].split()]
    drawn = set()

    def edge(src, dst, pts, **kw):
        d.polyline(pts, dashed=dashed_edge(src, dst), **kw)
        drawn.add((src, dst))

    A = "akantu-"
    # Fan-in buses: the five physics models onto model, the five models built on solid onto solid.
    for target, row_layer in ((A + "model", 6), (A + "solid", 7)):
        srcs = [n for n, (s_, t_) in ((s_, (s_, t_)) for s_, t_ in edges) if t_ == target
                and int(crates[n]["layer"]) == row_layer]
        srcs = sorted(set(srcs), key=lambda n: pos[n][0])
        ybus = pos[srcs[0]][1] - 11
        xs_ = [pos[n][0] + bw / 2 for n in srcs]
        d.polyline([(min(xs_), ybus), (max(xs_), ybus)], head=False)
        for n in srcs:
            d.polyline([top(n), (top(n)[0], ybus)], head=False)
            drawn.add((n, target))
        edge(srcs[0], target, [(bot(target)[0], ybus), bot(target)])
    # Long edges, routed through free columns and the side channels.
    yl = lambda n: pos[n][1] - 11  # the gap above a box's row
    edge(A + "solver", A + "parser", [top(A + "solver", 0.12), bot(A + "parser", 0.7)])
    edge(A + "io", A + "linalg", [top(A + "io", 0.2), bot(A + "linalg", 0.85)])
    edge(A + "model", A + "solver", [(pos[A + "model"][0], mid_y(A + "model")),
                                     (bot(A + "solver")[0], mid_y(A + "model")), bot(A + "solver")])
    xg = X(5) - 10
    edge(A + "igfem", A + "geometry-cgal", [top(A + "igfem", 0.9), (top(A + "igfem", 0.9)[0], yl(A + "solid") - 6),
                                           (xg, yl(A + "solid") - 6), (xg, yl(A + "model")),
                                           (bot(A + "geometry-cgal", 0.15)[0], yl(A + "model")),
                                           bot(A + "geometry-cgal", 0.15)])
    xr = W - 8
    edge(A + "embedded", A + "geometry", [(pos[A + "embedded"][0] + bw, mid_y(A + "embedded")),
                                          (xr, mid_y(A + "embedded")), (xr, mid_y(A + "geometry")),
                                          (pos[A + "geometry"][0] + bw, mid_y(A + "geometry"))])
    cpl = A + "couplers"
    xc = X(1) - 10
    yb = lambda n: pos[n][1] + bh + 6  # just below a box
    xc = X(2) - 8
    edge(cpl, A + "contact", [top(cpl, 0.95), (top(cpl, 0.95)[0], yl(cpl)), (xc, yl(cpl)),
                              (xc, yb(A + "contact")), (bot(A + "contact", 0.9)[0], yb(A + "contact")),
                              bot(A + "contact", 0.9)])
    x3 = X(3) + 0.2 * bw
    edge(cpl, A + "phase-field", [(pos[cpl][0] + bw, mid_y(cpl, 0.5)), (x3, mid_y(cpl, 0.5)),
                                  bot(A + "phase-field", 0.2)])
    # The facade: one trunk on the right (structural, igfem, embedded), one on the left
    # (diffusion, extra-materials); the others are straight.
    ak = "akantu"
    xt = X(3) + 0.8 * bw
    ybr = yb(A + "igfem")
    start = [(pos[ak][0] + bw, mid_y(ak, 0.5)), (xt, mid_y(ak, 0.5))]
    edge(ak, A + "structural", start + [(xt, yb(A + "structural")),
                                        (bot(A + "structural", 0.1)[0], yb(A + "structural")),
                                        bot(A + "structural", 0.1)])
    edge(ak, A + "embedded", start + [(xt, ybr), (bot(A + "embedded", 0.3)[0], ybr), bot(A + "embedded", 0.3)])
    # igfem is off by default: its dashed edge gets its own trunk so that no solid edge hides it.
    xi = xt + 12
    edge(ak, A + "igfem", [(pos[ak][0] + bw, mid_y(ak, 0.78)), (xi, mid_y(ak, 0.78)), (xi, ybr + 8),
                           (bot(A + "igfem", 0.5)[0], ybr + 8), bot(A + "igfem", 0.5)])
    edge(ak, A + "diffusion", [(pos[ak][0], mid_y(ak, 0.7)), (6, mid_y(ak, 0.7)),
                               (6, mid_y(A + "diffusion")), (pos[A + "diffusion"][0], mid_y(A + "diffusion"))])
    edge(ak, A + "extra-materials", [(pos[ak][0], mid_y(ak, 0.3)), (13, mid_y(ak, 0.3)),
                                     (13, mid_y(A + "extra-materials")),
                                     (pos[A + "extra-materials"][0], mid_y(A + "extra-materials"))])
    # Everything else: straight from the dependent's top to the dependency's bottom,
    # with the attachment points spread by direction.
    for src, dst in edges:
        if (src, dst) in drawn:
            continue
        sx, dx_ = pos[src][0], pos[dst][0]
        ts = 0.5 if sx == dx_ else (0.25 if dx_ < sx else 0.75)
        td = 0.5 if sx == dx_ else (0.75 if dx_ < sx else 0.25)
        edge(src, dst, [top(src, ts), bot(dst, td)])
    # Legend
    ly = y0 + 10 * pitch + bh + 34
    x = d.swatch(20, ly, "facade crate akantu", kind="blue")
    x = d.swatch(x, ly, "GPL-3.0-or-later (feature cgal)", kind="yellow")
    x = d.swatch(x, ly, "not part of the library", dashed=True)
    x = d.line_key(x, ly, "optional, off by default", dashed=True)
    d.text(20, ly + 18, "Every crate is named akantu-<name>. Arrows point from a crate to the crates it "
           "uses; transitive edges are left out.", size=dg.LABEL_SIZE)
    d.save(fs.FIGURES / "crate-dependencies.svg")
    print("figure crate-dependencies.svg")

# --------------------------------------------------------------------------
# Report 02: benchmarks
# --------------------------------------------------------------------------
MODE_NAME = {"ss": "small strain", "fd": "finite deformation"}


def _dot_range(ax, y, med, lo, hi, colour, *, marker="o", face=None, size=5.5, label=None):
    ax.plot([lo, hi], [y, y], color=colour, lw=1.6, solid_capstyle="round", alpha=0.55, zorder=2)
    ax.plot([med], [y], marker=marker, color=colour, mfc=face or colour, ms=size, ls="none",
            mec=colour if face else fs.SURFACE, mew=1.2 if face else 0.8, zorder=3, label=label)


@figure
def perf_explicit_step_ratio():
    rows = [r for r in fs.read_csv("explicit-step-rust-vs-cpp") if r["phase"] == "step"]
    fig, ax = fs.new(6.4, 3.9)
    ticks, labels, y = [], [], 0
    series = [("strict", "against C++ strict (the port's arithmetic)", fs.RUST, None),
              ("default", "against C++ as shipped", fs.BLUES[0], None)]
    heads = []
    for mode in ("ss", "fd"):
        heads.append((y, MODE_NAME[mode]))
        y += 1
        for r in (r for r in rows if r["mode"] == mode):
            for k, (key, lab, colour, face) in enumerate(series):
                c = f"rust_over_cpp_{key}"
                _dot_range(ax, y + (k - 0.5) * 0.34, float(r[c]), float(r[c + "_min"]),
                           float(r[c + "_max"]), colour, label=lab if y == 1 and mode == "ss" else None)
            ticks.append(y)
            labels.append(f"{r['mesh'].split('-')[0]}, {fs.num(int(r['elements']))} elements")
            y += 1
        y += 0.4
    ax.set_yticks(ticks, labels)
    for yh, text in heads:
        ax.text(-0.01, yh + 0.1, text, transform=ax.get_yaxis_transform(), ha="right", va="center",
                fontsize=8.5, fontweight="bold", color=fs.INK)
    ax.set_ylim(y - 0.6, -0.7)
    ax.set_xlim(0, 1.15)
    ax.grid(axis="x")
    ax.grid(axis="y", visible=False)
    ax.tick_params(axis="y", length=0)
    ax.xaxis.set_major_locator(MultipleLocator(0.2))
    ax.set_xlabel("port time / C++ time (dot: median of 7 rounds, line: range)")
    fs.ratio_line(ax, label="same time", faster_side="right", y_text=1.0)
    ax.annotate("← port faster", (1.0, 1.0), xycoords=("data", "axes fraction"), xytext=(-4, 2),
                textcoords="offset points", ha="right", va="bottom", fontsize=8, color=fs.INK2)
    ax.legend(loc="upper left", bbox_to_anchor=(0.0, 1.12), ncols=2, handlelength=1.0)
    fs.save(fig, "perf-explicit-step-ratio")


@figure
def perf_cohesive_ratios_boat():
    rows = fs.read_csv("cohesive-port-over-upstream-boat")
    phases = [("loop", "Loop"), ("solve_step", "Explicit step"), ("check", "Insertion check"),
              ("static_solve", "Static solve")]
    fig, axes = fs.new(6.7, 4.0, ncols=4, sharey=True)
    fig.subplots_adjust(wspace=0.12)
    ys, labels, heads, y, prev = [], [], [], 0, None
    for r in rows:
        if r["series"] != prev:
            if prev is not None:
                y += 0.5
            heads.append((y, "before insertion" if r["series"] == "ws" else "with insertion"))
            y += 1
            prev = r["series"]
        n = int(r["ranks"])
        ys.append(y)
        labels.append(f"h = {r['h']}, {n} rank{'s' if n > 1 else ''}")
        y += 1
    for ax, (col, title) in zip(axes, phases):
        for yy, r in zip(ys, rows):
            _dot_range(ax, yy, float(r[col]), float(r[col + "_min"]), float(r[col + "_max"]), fs.RUST)
        ax.set_xscale("log")
        ax.set_xlim(0.3, 3.2)
        fs.plain_log_axis(ax.xaxis, [0.5, 1, 2])
        ax.grid(axis="x")
        ax.grid(axis="y", visible=False)
        ax.set_title(title, fontsize=9)
        ax.axvline(1.0, color=fs.INK3, lw=0.9, zorder=1)
        ax.tick_params(axis="y", length=0)
    axes[0].set_yticks(ys, labels, fontsize=8)
    for yh, text in heads:
        axes[0].text(-0.04, yh, text, transform=axes[0].get_yaxis_transform(), ha="right",
                     va="center", fontsize=8.5, fontweight="bold")
    axes[0].set_ylim(y - 0.4, -0.6)
    axes[0].annotate("← port faster", (1.0, 1.0), xycoords=("data", "axes fraction"), xytext=(-3, 2),
                     textcoords="offset points", ha="right", va="bottom", fontsize=7.5, color=fs.INK2)
    fig.supxlabel("port time / upstream time (log scale; dot: median of 5 paired repetitions, line: range)",
                  fontsize=8.5, color=fs.INK2, y=0.0)
    fs.save(fig, "perf-cohesive-ratios-boat")


def _c1_phases(r):
    """Seconds per phase of a full-size run row (fullsize-c1.csv)."""
    f = lambda k: float(r[k])
    steps = f("steps")
    return {
        "static solve": f("static_solve_s"),
        "mesh distribution": f("dist_mesh_s"),
        "other set-up": f("read_mesh_s") + f("init_full_s") + f("before_step_s"),
        "explicit step": f("solve_step_ms") * steps / 1000,
        "insertion check": f("check_ms") * steps / 1000,
        "energies": f("energies_ms") * steps / 1000,
    }


@figure
def perf_fullsize_phases():
    rows = [r for r in fs.read_csv("fullsize-c1") if r["run"] == "c1"]
    by = {(r["code"], int(r["ranks"])): r for r in rows}
    order = [(c, n) for n in sorted({int(r["ranks"]) for r in rows}, reverse=True) for c in ("rust", "cpp")]
    fig, ax = fs.new(6.6, 3.0)
    phases = list(PHASE_COLOURS)
    for i, key in enumerate(order):
        label_y = i
        if key not in by:
            ax.text(40, i, "not run", va="center", ha="left",
                    fontsize=8, color=fs.INK3, style="italic")
            continue
        r = by[key]
        ph = _c1_phases(r)
        left = 0.0
        for p in phases:
            ax.barh(label_y, ph[p], left=left, height=0.62, color=PHASE_COLOURS[p],
                    edgecolor=fs.SURFACE, linewidth=0.6)
            left += ph[p]
        ax.annotate(f"{fs.num(float(r['wall_s']))} s", (float(r["wall_s"]), i), xytext=(5, 0),
                    textcoords="offset points", va="center", fontsize=8, color=fs.INK2)
    ax.set_yticks(range(len(order)), [f"{CODE_NAME[c]}, {n} ranks" for c, n in order])
    ax.set_ylim(len(order) - 0.5, -0.6)
    ax.grid(axis="x")
    ax.grid(axis="y", visible=False)
    ax.tick_params(axis="y", length=0)
    ax.set_xlim(0, 12000)
    thousands(ax.xaxis)
    ax.set_xlabel("wall time (s)")
    handles = [Patch(color=PHASE_COLOURS[p], label=p) for p in phases]
    ax.legend(handles=handles, loc="upper left", bbox_to_anchor=(0.0, 1.2), ncols=6,
              handlelength=1.0, columnspacing=1.0, fontsize=8)
    fs.save(fig, "perf-fullsize-phases")


@figure
def perf_cost_per_element():
    rows = fs.read_csv("fullsize-vs-jed")
    fig, ax = fs.new(6.0, 3.3)
    series = [("port", "port, AWS node", fs.RUST, "-", "o"),
              ("upstream_35a85178", "C++ 35a85178, AWS node", fs.CPP, "-", "o"),
              ("jed_v5.0.4", "upstream v5.0.4 on jed\n(published, other machine)", fs.REFERENCE, "--", "s")]
    for key, lab, colour, ls, mk in series:
        pts = sorted((int(r["ranks"]), float(r["core_us_per_el_step"])) for r in rows if r["source"] == key)
        xs, ys = zip(*pts)
        ax.plot(xs, ys, ls=ls, marker=mk, color=colour, label=lab.replace("\n", " "),
                mec=fs.SURFACE, mew=1.0, ms=6.5)
        fs.end_label(ax, xs[-1], ys[-1], lab, dx=7, dy={"upstream_35a85178": 7}.get(key, 0))
    ax.set_xscale("log", base=2)
    fs.plain_log_axis(ax.xaxis, [8, 16, 32, 64])
    ax.set_xlim(7, 160)
    ax.legend(loc="lower right", fontsize=8)
    ax.set_ylim(0, 7)
    ax.set_xlabel("MPI ranks (cores)")
    ax.set_ylabel("core-µs per element and step")
    fs.save(fig, "perf-cost-per-element")


@figure
def perf_scaling():
    boat = fs.read_csv("scaling-boat")
    mn = fs.read_csv("multinode-aws")
    c1 = [r for r in fs.read_csv("fullsize-c1") if r["run"] == "c1"]
    fig, (a, b) = fs.new(6.7, 3.6, ncols=2, sharey=True)
    fig.subplots_adjust(wspace=0.08)
    code_colour = {"rust": fs.RUST, "cpp": fs.CPP}

    def tets_k(study):
        return f"{median(int(r['tets_per_rank']) for r in boat if r['study'] == study) / 1000:.0f} k"

    def plot(ax, pts, colour, ls, mk, whisk=None, off=1.0):
        xs = [p[0] for p in pts]
        ax.plot(xs, [p[1] for p in pts], ls=ls, marker=mk, color=colour, ms=5, mec=fs.SURFACE, mew=0.8)
        if whisk:
            for x, lo, hi in whisk:
                ax.plot([x * off, x * off], [lo, hi], color=colour, lw=1.0, alpha=0.5)

    styles_a = [("weak_A", "-", "o", f"boat, {tets_k('weak_A')} tetrahedra per rank"),
                ("weak_B", "--", "s", f"boat, {tets_k('weak_B')} tetrahedra per rank")]
    for i, (study, ls, mk, _) in enumerate(styles_a):
        for code in ("rust", "cpp"):
            rs = [r for r in boat if r["study"] == study and r["code"] == code]
            off = (0.94, 1.06)[code == "cpp"] * (1 + 0.03 * i)
            plot(a, [(int(r["ranks"]), float(r["efficiency_per_repetition"])) for r in rs],
                 code_colour[code], ls, mk,
                 [(int(r["ranks"]), float(r["efficiency_per_repetition_min"]),
                   float(r["efficiency_per_repetition_max"])) for r in rs], off)
    weak = [r for r in mn if r["series"] == "weak"]
    aws_k = f"{median(int(r['tets_per_rank']) for r in weak) / 1000:.0f} k"
    plot(a, [(int(r["ranks"]), float(r["efficiency"])) for r in weak], fs.RUST, ":", "^")
    a.set_title("Weak scaling", fontsize=9.5)
    a.set_ylabel("efficiency (1 = ideal)")

    st = "strong_h0.06_ws"
    for code in ("rust", "cpp"):
        rs = [r for r in boat if r["study"] == st and r["code"] == code]
        plot(b, [(int(r["ranks"]), float(r["efficiency_medians"])) for r in rs], code_colour[code], "-", "o")
    strong = [r for r in mn if r["series"] == "strong"]
    plot(b, [(int(r["ranks"]), float(r["efficiency"])) for r in strong], fs.RUST, ":", "^")
    for code in ("rust", "cpp"):
        rs = sorted(((int(r["ranks"]), float(r["loop_s"])) for r in c1 if r["code"] == code))
        n0, t0 = rs[0]
        plot(b, [(n, t0 * n0 / (t * n)) for n, t in rs], code_colour[code], "-.", "D")
    b.set_title("Strong scaling", fontsize=9.5)
    h06 = next(r for r in boat if r["study"] == st)
    for ax in (a, b):
        ax.set_xscale("log", base=2)
        fs.plain_log_axis(ax.xaxis, [1, 2, 4, 8, 16, 32, 64])
        ax.set_xlim(0.8, 80)
        ax.set_ylim(0, 1.05)
        ax.set_xlabel("MPI ranks")
    k = lambda ls, mk, lab: Line2D([], [], color=fs.INK2, ls=ls, marker=mk, ms=4.5, label=lab)
    a.legend(handles=[Line2D([], [], color=fs.RUST, lw=3, label="port"),
                      Line2D([], [], color=fs.CPP, lw=3, label="C++")] +
             [k(ls, mk, lab) for _, ls, mk, lab in styles_a] +
             [k(":", "^", f"AWS hosts, {aws_k} tetrahedra per rank,\nrelative to 8 ranks")],
             loc="upper left", bbox_to_anchor=(0.0, -0.2), fontsize=7.6, handlelength=2.4)
    b.legend(handles=[k("-", "o", f"boat, {fs.num(int(h06['tets_per_rank']))} tetrahedra (h = {h06['h']})"),
                      k(":", "^", "AWS hosts, h = 0.04, relative to 8 ranks"),
                      k("-.", "D", "AWS node, JOSS mesh, relative to the\nfirst point (port 8, C++ 32 ranks)")],
             loc="upper left", bbox_to_anchor=(0.0, -0.2), fontsize=7.6, handlelength=2.4)
    fs.save(fig, "perf-scaling")


@figure
def perf_memory_fullsize():
    rows = fs.read_csv("memory-fullsize")
    by = {(r["code"], int(r["ranks"])): r for r in rows}
    ranks = sorted({int(r["ranks"]) for r in rows}, reverse=True)
    fig, (a, b) = fs.new(6.6, 3.0, ncols=2, gridspec_kw={"width_ratios": [1.5, 1]})
    fig.subplots_adjust(wspace=0.32)
    w = 0.36
    for i, n in enumerate(ranks):
        r = by.get(("rust", n))
        m, rest = float(r["mumps_allocation_gib"]), float(r["rest_gib"])
        a.bar(i - w / 2 - 0.01, m, w, color=fs.RUST, edgecolor=fs.SURFACE, linewidth=0.6)
        a.bar(i - w / 2 - 0.01, rest, w, bottom=m, color=fs.BLUES[0], edgecolor=fs.SURFACE, linewidth=0.6)
        b.bar(i - w / 2 - 0.01, float(r["largest_rank_gib"]), w, color=fs.RUST)
        c = by.get(("cpp", n))
        if c:
            a.bar(i + w / 2 + 0.01, float(c["total_sampler_gib"]), w, color=fs.CPP)
            b.bar(i + w / 2 + 0.01, float(c["largest_rank_gib"]), w, color=fs.CPP)
        else:
            for ax in (a, b):
                ax.text(i + w / 2 + 0.01, 2, "not\nrun", ha="center", va="bottom", fontsize=7.5,
                        color=fs.INK3, style="italic")
    for ax, title, ylab in ((a, "Sum of the ranks' peaks", "peak resident memory (GiB)"),
                            (b, "Largest rank", "peak resident memory (GiB)")):
        ax.set_xticks(range(len(ranks)), [f"{n} ranks" for n in ranks])
        ax.set_title(title, fontsize=9.5)
        ax.set_ylabel(ylab)
        ax.tick_params(axis="x", length=0)
    a.set_ylim(0, 250)
    fig.legend(handles=[Patch(color=fs.RUST, label="port: MUMPS allocation"),
                        Patch(color=fs.BLUES[0], label="port: mesh, model and the rest"),
                        Patch(color=fs.CPP, label="C++ (no split recorded)")],
               loc="upper left", bbox_to_anchor=(0.0, 0.0), ncols=3, fontsize=8, handlelength=1.0)
    fs.save(fig, "perf-memory-fullsize")


# --------------------------------------------------------------------------
# Report 03: upstream defects
# --------------------------------------------------------------------------
@figure
def upstream_defects_by_area():
    rows = [r for r in fs.read_csv("upstream-defects-by-area") if r["area"] != "total"]
    cats = [("A", "A: undefined behaviour", fs.OTHER[0]),
            ("B", "B: wrong results", fs.OTHER[1]),
            ("C", "C: kept for compatibility", "#b5b4ad"),
            ("test infrastructure", "Test infrastructure", "#d3d2cb"),
            ("other", "Other", "#e8e7e1")]
    fig, ax = fs.new(6.4, 3.9)
    ys = range(len(rows))
    left = [0] * len(rows)
    for key, lab, colour in cats:
        vals = [int(r[key]) for r in rows]
        ax.barh(ys, vals, left=left, height=0.66, color=colour, edgecolor=fs.SURFACE, linewidth=0.6,
                label=lab)
        left = [a + b for a, b in zip(left, vals)]
    for y, r in zip(ys, rows):
        ax.annotate(r["total"], (int(r["total"]), y), xytext=(4, 0), textcoords="offset points",
                    va="center", fontsize=7.8, color=fs.INK2)
    ax.set_yticks(list(ys), [r["area"] for r in rows])
    ax.set_ylim(len(rows) - 0.4, -0.6)
    ax.grid(axis="x")
    ax.grid(axis="y", visible=False)
    ax.tick_params(axis="y", length=0)
    ax.xaxis.set_major_locator(MultipleLocator(10))
    ax.set_xlim(0, max(left) * 1.08)
    ax.set_xlabel("register entries")
    ax.legend(loc="lower left", bbox_to_anchor=(0.0, 1.01), ncols=3, fontsize=8, handlelength=1.0)
    fs.save(fig, "upstream-defects-by-area")


# --------------------------------------------------------------------------
# Report 04: possible improvements
# --------------------------------------------------------------------------
@figure
def improvements_wall_share():
    rows = fs.read_csv("improvements-wall-share")
    ranks = []
    for r in rows:
        if r["ranks"] not in ranks:
            ranks.append(r["ranks"])
    phases = []
    for r in rows:
        if r["phase"] not in phases:
            phases.append(r["phase"])
    fig, ax = fs.new(6.6, 1.9)
    for i, n in enumerate(ranks):
        left = 0.0
        for p in phases:
            r = next(x for x in rows if x["ranks"] == n and x["phase"] == p)
            # The share from the seconds, so that its rounding matches the reports' text
            # (the CSV column is rounded to one decimal).
            v = float(r["seconds"]) / float(r["wall_s"]) * 100
            ax.barh(i, v, left=left, height=0.62, color=PHASE_COLOURS[p], edgecolor=fs.SURFACE,
                    linewidth=0.8)
            label = f"{v:.0f} %"
            # Print the share inside its segment only where it fits with padding.
            seg_pt = v / 100 * ax.get_position().width * fig.get_figwidth() * 72
            if seg_pt >= len(label) * 4.6 + 6:
                ax.text(left + 0.8, i, label, va="center", ha="left", fontsize=7.8, color=fs.INK)
            left += v
        wall = next(x for x in rows if x["ranks"] == n)["wall_s"]
        ax.annotate(f"{fs.num(float(wall))} s", (100, i), xytext=(5, 0), textcoords="offset points",
                    va="center", fontsize=8, color=fs.INK2)
    ax.set_yticks(range(len(ranks)), [f"{n} ranks" for n in ranks])
    ax.set_ylim(len(ranks) - 0.5, -0.5)
    ax.set_xlim(0, 100)
    ax.xaxis.set_major_locator(MultipleLocator(20))
    ax.grid(axis="x")
    ax.grid(axis="y", visible=False)
    ax.tick_params(axis="y", length=0)
    ax.set_xlabel("share of wall time (%)")
    # Legend table: phase, the findings that address it
    findings = {p: next(x["scan_findings"] for x in rows if x["phase"] == p) for p in phases}
    import re
    from matplotlib.patches import Rectangle
    ax.text(0.0, -0.5, "Phase", transform=ax.transAxes, fontsize=8, fontweight="bold", color=fs.INK2)
    ax.text(0.24, -0.5, "Findings that address it (sections of improvements-performance.md)",
            transform=ax.transAxes, fontsize=8, fontweight="bold", color=fs.INK2)
    for k, p in enumerate(phases):
        y = -0.66 - k * 0.15
        ax.add_patch(Rectangle((0.0, y - 0.035), 0.018, 0.07, transform=ax.transAxes,
                               color=PHASE_COLOURS[p], clip_on=False))
        ax.text(0.028, y, p, transform=ax.transAxes, va="center", fontsize=8, color=fs.INK)
        ax.text(0.24, y, re.sub(r"(P\d\.\d) (?=P)", r"\1, ", findings[p]), transform=ax.transAxes,
                va="center", fontsize=8, color=fs.INK2)
    fs.save(fig, "improvements-wall-share")


@figure
def improvements_roadmap():
    rows = fs.read_csv("improvements-roadmap")
    bands = []
    for r in rows:
        if r["benefit"] not in bands:
            bands.append(r["benefit"])
    order = {"high": 0, "medium": 1, "enabling": 2}
    bands.sort(key=lambda b: order.get(b, 9))
    fig, ax = fs.new(3.7, 5.4)
    fig.subplots_adjust(left=0.02, right=0.98)
    ys, labels, heads, y = [], [], [], 0
    style = {"measured": ("o", fs.RUST), "estimated": ("o", fs.RUST),
             "unmeasured": ("o", fs.SURFACE), "not performance": ("s", fs.RUST)}
    for band in bands:
        heads.append((y, f"{band} benefit" if band != "enabling" else "enabling"))
        y += 1
        for r in (r for r in rows if r["benefit"] == band):
            lo, hi = float(r["effort_min_days"]), float(r["effort_max_days"])
            mid = math.sqrt(lo * hi)
            if hi > lo:
                ax.plot([lo, hi], [y, y], color=fs.BLUES[0], lw=2.2, solid_capstyle="round", zorder=2)
            mk, face = style[r["benefit_basis"]]
            ax.plot([mid], [y], marker=mk, ms=6.5, mfc=face, mec=fs.RUST, mew=1.4, ls="none", zorder=3)
            ys.append(y)
            labels.append(textwrap.fill(f"{r['item']}. {r['short_label']}", 34))
            y += 1
        y += 0.3
    ax.set_xscale("log")
    names = {0.25: "hours", 1: "1 day", 5: "1 week", 20: "1 month"}
    fs.plain_log_axis(ax.xaxis, list(names), lambda v, _: names.get(v, ""))
    ax.set_xlim(0.2, 45)
    ax.set_yticks(ys, labels, fontsize=8.6, linespacing=1.0)
    for yh, text in heads:
        ax.text(-0.02, yh, text, transform=ax.get_yaxis_transform(), ha="right", va="center",
                fontsize=9, fontweight="bold")
    ax.set_ylim(y - 0.5, -0.7)
    ax.grid(axis="x")
    ax.grid(axis="y", visible=False)
    ax.tick_params(axis="y", length=0)
    ax.set_xlabel("estimated effort for one developer\n(working days, log scale)")
    k = lambda mk, face, lab: Line2D([], [], marker=mk, ms=6, mfc=face, mec=fs.RUST, mew=1.3, ls="none",
                                     label=lab)
    ax.legend(handles=[k("o", fs.RUST, "performance gain measured or estimated"),
                       k("o", fs.SURFACE, "performance gain not measured"),
                       k("s", fs.RUST, "benefit is usability or maintenance"),
                       Line2D([], [], color=fs.BLUES[0], lw=2.2, label="range of the estimate")],
              loc="upper left", bbox_to_anchor=(-0.78, -0.13), ncols=2, fontsize=8.4)
    ax.text(-0.78, -0.28, textwrap.fill("Efforts are the scans' estimates, not measurements; benefit categories "
            "are the report's judgement from the evidence each finding cites.", 100),
            transform=ax.transAxes, fontsize=8.2, color=fs.INK3, va="top")
    ax.tick_params(axis="x", labelsize=8.4)
    fs.save(fig, "improvements-roadmap")


# --------------------------------------------------------------------------
# Report 05: the AI setup
# --------------------------------------------------------------------------
@figure
def ai_cost():
    rows = fs.read_csv("ai-cost")
    metered = [r for r in rows if "monthly plan fee" not in r["basis"]]
    plans = [r for r in rows if "monthly plan fee" in r["basis"]]
    fig, (a, b) = fs.new(6.4, 2.9, nrows=2, sharex=True,
                         gridspec_kw={"height_ratios": [len(metered), len(plans)]})
    fig.subplots_adjust(hspace=0.55)
    colour = fs.OTHER[0]
    for ax, items, title in ((a, metered, "Metered compute, against its cap"),
                             (b, plans, "Flat monthly plans")):
        ys = range(len(items))
        for y, r in zip(ys, items):
            v = float(r["recorded_cost_usd"])
            if r["cap_usd"]:
                ax.barh(y, float(r["cap_usd"]), height=0.62, color="none", edgecolor="#b5b4ad",
                        linewidth=0.9)
            ax.barh(y, v, height=0.62, color=colour)
            if r["cap_usd"]:
                text = f"about \\${v:g} of \\${float(r['cap_usd']):g}"
                x = float(r["cap_usd"])
            else:
                text = f"\\${v:g} per month"
                x = v
            ax.annotate(text, (x, y), xytext=(5, 0), textcoords="offset points", va="center",
                        fontsize=8, color=fs.INK2)
        ax.set_yticks(list(ys), [r["provider"] for r in items])
        ax.set_ylim(len(items) - 0.5, -0.5)
        ax.grid(axis="x")
        ax.grid(axis="y", visible=False)
        ax.tick_params(axis="y", length=0)
        ax.set_title(title, fontsize=9)
    b.set_xlim(0, 320)
    b.xaxis.set_major_locator(MultipleLocator(50))
    b.xaxis.set_major_formatter(FuncFormatter(lambda v, _: f"\\${v:g}"))
    b.set_xlabel("US dollars")
    fig.legend(handles=[Patch(color=colour, label="recorded cost"),
                        Patch(facecolor="none", edgecolor="#b5b4ad", label="cap")],
               loc="upper left", bbox_to_anchor=(0.0, -0.01), ncols=2, fontsize=7.8, handlelength=1.2)
    fs.footnote(fig, "Metered costs: the ledgers' estimates from list prices and run times; no billed "
                "figure was read.", y=-0.1)
    fs.save(fig, "ai-cost")


@figure
def ai_timeline():
    rows = fs.read_csv("ai-timeline")
    ver = {r["day"]: r for r in fs.read_csv("ai-verified-rows")}
    miles = {r["day"]: r["label"] for r in fs.read_csv("ai-milestones")}
    days = [r["day"] for r in rows]
    fig, (m, a, b) = fs.new(6.6, 5.5, nrows=3, sharex=True,
                            gridspec_kw={"height_ratios": [0.62, 1.6, 1.05]})
    fig.subplots_adjust(hspace=0.36)
    stacks = [("opus_5_5", "Claude Opus 5.5", fs.OTHER[0]),
              ("fable_5_2", "Claude Fable 5.2", fs.OTHER[1]),
              ("sonnet_5_5", "Claude Sonnet 5.5", fs.OTHER[2]),
              (("gpt_6_1_sol", "fable_5_1", "no_trailer"), "other (GPT-6.1 Sol, Fable 5.1, no trailer)",
               fs.NEUTRAL)]
    xs = list(range(len(days)))
    bottom = [0] * len(days)
    for key, lab, colour in stacks:
        keys = key if isinstance(key, tuple) else (key,)
        vals = [sum(int(r[k]) for k in keys) for r in rows]
        a.bar(xs, vals, 0.56, bottom=bottom, color=colour, edgecolor=fs.SURFACE, linewidth=0.6, label=lab)
        bottom = [p + q for p, q in zip(bottom, vals)]
    a.set_ylabel("commits per day")
    thousands(a.yaxis)
    a.legend(loc="upper right", fontsize=7.8, handlelength=1.0)
    a.set_title("Commits on main by the model named in the trailer", fontsize=9)
    vy = [int(ver[d]["verified_rows"]) for d in days]
    b.plot(xs, vy, color=fs.RUST, marker="o", ms=5.5, mec=fs.SURFACE, mew=0.8)
    total = max(int(r["total_rows"]) for r in ver.values() if r["total_rows"])
    b.axhline(total, color=fs.INK3, lw=0.9)
    b.annotate(f"{fs.num(total)} rows in the matrix", (0, total), xytext=(0, 3), textcoords="offset points",
               fontsize=7.8, color=fs.INK2, va="bottom")
    for x, d in zip(xs, days):
        t = ver[d]["time"]
        if t and x < len(days) - 1:
            b.annotate(t, (x, vy[x]), xytext=(6, -2), textcoords="offset points", fontsize=7.8,
                       color=fs.INK3, va="top")
    b.annotate(fs.num(vy[-1]), (xs[-1], vy[-1]), xytext=(0, -8), textcoords="offset points",
               ha="center", va="top", fontsize=7.8, color=fs.INK2)
    b.set_ylim(0, total * 1.18)
    b.set_ylabel("Verified rows")
    thousands(b.yaxis)
    b.set_title("Parity rows Verified, last record of each day", fontsize=9)
    b.set_xticks(xs, [f"{int(d[-2:])} Nov" for d in days])
    b.tick_params(axis="x", length=0)
    m.axis("off")
    for x, d in zip(xs, days):
        m.text(x, 1.0, textwrap.fill(miles.get(d, ""), 15), ha="center", va="top", fontsize=7.8,
               color=fs.INK2, linespacing=1.15)
    m.set_ylim(0, 1)
    m.set_xlim(-0.5, len(days) - 0.5)
    fs.save(fig, "ai-timeline")


@figure
def ai_setup_diagram():
    d = Diagram(760, 562, "Setup: the user, the agents, the repositories and the machines")
    # The Mac: the user, T3 Code with its agents, the git repository, the oracle
    d.group(8, 8, 744, 362, "The user's Mac")
    d.box("user", 22, 62, 100, 100, "The user", "brief, budgets,\nscope and\nlicence\ndecisions",
          kind="dark")
    d.group(200, 34, 356, 222, "T3 Code (desktop app)", fill="#ffffff", stroke="#d9d8d2")
    d.box("orch", 214, 66, 124, 100, "Orchestrator", "Opus 5.5 to\n11 Nov, then\nFable 5.2,\neffort high",
          kind="pink")
    sx, sw = 404, 144
    d.box("impl", sx, 54, sw, 44, "Implementation", "Opus 5.5, effort xhigh")
    d.box("mech", sx, 106, sw, 44, "Routine tasks", "Sonnet 5.5, from 11 Nov")
    d.box("rev", sx, 158, sw, 58, "Reviewers", "GPT-6.1 Sol to 11 Nov,\nthen fresh Opus 5.5")
    d.text(sx + sw, 243, "Claude through Claude Code, GPT through Codex", anchor="end", italic=True)
    # Orchestrator <-> subagents through one bus
    bus = 384
    d.polyline([(338, 116), (bus, 116)], both=True, head=True)
    d.text(361, 110, "brief", anchor="middle")
    d.text(361, 132, "report", anchor="middle")
    d.polyline([(bus, 76), (bus, 187)], head=False)
    for k, yy in (("impl", 76), ("mech", 128), ("rev", 187)):
        d.polyline([(bus, yy), (sx, yy)])
    # User <-> orchestrator
    d.polyline([(122, 92), (214, 92)], "decisions,\nlimits", label_dy=-6)
    d.polyline([(214, 140), (122, 140)], "questions,\nreports", label_dy=30)
    # Git column
    gx, gw = 610, 130
    d.box("wt", gx, 54, gw, 46, "Git worktrees", "one branch per agent", stack=True)
    d.box("main", gx, 172, gw, 46, "Local main", "the working history")
    d.box("pub", gx, 270, gw, 66, "publish script", "filtered clone, secret\nscan; branches are\nnever pushed",
          title_size=11.5, mono=True)
    d.polyline([(sx + sw, 76), (566, 76)], head=False)
    d.polyline([(sx + sw, 128), (566, 128)], head=False)
    d.polyline([(sx + sw, 187), (566, 187)], head=False)
    d.polyline([(566, 76), (566, 187)], head=False)
    d.polyline([(566, 77), (gx, 77)], "commits", label_dy=-5, label_dx=-5)
    d.arrow("wt", "main", "merged by the\norchestrator after\nreview and gate", sa="b", sb="t", ta=0.18,
            tb=0.18, label_dx=6, label_dy=12, label_anchor="start")
    d.arrow("main", "pub", "publish", sa="b", sb="t", ta=0.18, tb=0.18, label_dx=6, label_dy=4,
            label_anchor="start")
    # The C++ reference and the first oracle build
    d.box("cpp", 186, 280, 150, 72, "C++ reference", "Akantu 35a85178,\nread-only checkout", kind="orange")
    d.box("orb", 400, 280, 150, 72, "OrbStack Docker VM", "aarch64 Linux: first\noracle and fixtures,\n9 to 11 Nov")
    d.arrow("cpp", "orb", "oracle\nbuild", label_dy=-6)
    # Disposable cloud machines and GitHub
    d.group(8, 400, 524, 154, "Cloud machines")
    d.box("vultr", 22, 430, 150, 110, "Vultr hosts", "32-vCPU gate hosts,\n13 to 14 Nov")
    d.box("aws", 186, 430, 150, 110, "AWS EC2", "full-size and\neight-node runs,\n12 to 13 Nov")
    d.box("boat", 350, 430, 168, 110, "boat.dev sandboxes",
          "x86_64 VMs with Docker;\nsnapshots with the\ntoolchain, MPI images\nand the x86_64 oracle")
    d.group(544, 400, 208, 154, "GitHub")
    d.box("repo", 558, 428, 180, 46, "Repository", "published copy of main")
    d.box("ci", 558, 496, 180, 46, "GitHub Actions", "CI, run once on 14 Nov")
    d.arrow("pub", "repo", "push", sa="b", sb="t", ta=0.28, tb=(gx + gw * 0.28 - 558) / 180, label_dx=6,
            label_dy=4, label_anchor="start")
    d.arrow("repo", "ci", "CI", sa="b", sb="t", ta=0.3, tb=0.3, label_dx=6, label_dy=4, label_anchor="start")
    # Orchestrator only: Vultr and AWS
    ox = 214 + 124 * 0.12
    vx = 22 + 150 * 0.92
    ax_ = 186 + 150 * 0.4
    d.polyline([(ox, 166), (ox, 264), (vx, 264), (vx, 430)], dashed=True)
    d.text(vx - 8, 300, "merge gates,\nbenchmark runs\n(orchestrator only)", anchor="end")
    d.polyline([(vx, 384), (ax_, 384), (ax_, 430)], dashed=True)
    # Code and oracle to the sandboxes
    d.polyline([(gx, 92), (586, 92), (586, 392), (350 + 168 * 0.78, 392), (350 + 168 * 0.78, 430)],
               "code as git bundles", dashed=True, label_seg=2, label_dy=-5)
    cx = 186 + 150 * 0.8
    d.polyline([(cx, 352), (cx, 392), (350 + 168 * 0.22, 392), (350 + 168 * 0.22, 430)],
               "oracle build", label_seg=0, label_at=0.5, label_dx=6, label_dy=4, label_anchor="start")
    d.save(fs.FIGURES / "ai-setup-diagram.svg")
    print("figure ai-setup-diagram.svg")


@figure
def ai_work_package_flow():
    d = Diagram(760, 520, "Life of a work package (one agent's task: parity rows and a brief)")
    bw, bh = 150, 76
    X = [9 + i * (bw + 47) for i in range(4)]
    Y = [14, 128, 248, 368]
    d.box("inv", X[0], Y[0], bw, bh, "Inventory", "C++ tree read; one parity\nrow per feature, test,\nexample")
    d.box("brief", X[1], Y[0], bw, bh, "Brief", "rows, scope, acceptance\ncriteria, evidence,\nowned files", kind="pink")
    d.box("impl", X[2], Y[0], bw, bh, "Implementation", "own worktree: Rust\ncode, oracle probes,\nfixtures, tests", kind="aqua")
    d.box("agate", X[3], Y[0], bw, bh, "Author's gate", "fmt, clippy, tests, MPI\ntiers on boat.dev;\nreport", kind="aqua")
    d.box("rev", X[3], Y[1], bw, bh, "First review", "fresh agent reruns the\ngates, regenerates\nfixtures, tries to break it", kind="violet")
    vcx, vcy = X[2] + bw / 2, Y[1] + bh / 2
    d.diamond("verdict", vcx, vcy, 100, 70, "Verdict", kind="violet")
    d.box("fix", X[1], Y[1], bw, bh, "Fix round", "blocking and major\nfindings (from 12 Nov)", kind="aqua")
    d.box("user", X[0] + 18, Y[2], bw - 18, bh, "The user", "approves exceptions,\nscope, budgets;\nnever reviews code",
          kind="dark")
    d.box("cand", X[2], Y[2], bw, bh, "Merge candidate", "integration branch\non current main", kind="pink")
    d.box("ogate", X[3], Y[2], bw, bh, "Orchestrator's gate", "full chain on boat.dev;\nparallel on Vultr\nfrom 13 Nov",
          kind="pink")
    d.box("merge", X[3], Y[3], bw, bh, "Merge and publish", "row Verified with tests,\nfixture, tolerance,\nreviewer", kind="pink")
    d.box("recheck", X[2], Y[3], bw, bh, "Batched re-check", "after the merge, one\nreviewer per three or\nfour branches", kind="violet")
    d.box("backlog", X[1], Y[3], bw, bh, "Backlog", "minor findings,\ncleared in batches", kind="muted", dashed=True)
    # Main path
    d.arrow("inv", "brief")
    d.arrow("brief", "impl")
    d.arrow("impl", "agate")
    d.arrow("agate", "rev", sa="b", sb="t")
    d.arrow("rev", "verdict", sa="l", sb="r")
    d.arrow("verdict", "impl", "reject", sa="t", sb="b", label_dx=6, label_dy=4, label_anchor="start")
    d.arrow("verdict", "fix", "changes\nrequired", sa="l", sb="r", label_dy=-6)
    d.arrow("verdict", "cand", "accept", sa="b", sb="t", label_dx=6, label_dy=6, label_anchor="start")
    fx = X[1] + bw * 0.7
    d.polyline([(fx, Y[1] + bh), (fx, Y[2] + bh / 2), (X[2], Y[2] + bh / 2)])
    d.arrow("cand", "ogate")
    d.arrow("ogate", "merge", sa="b", sb="t")
    d.arrow("merge", "recheck", sa="l", sb="r")
    gy = Y[2] + bh + 15
    gx, fx2 = X[3] + bw * 0.25, X[1] + bw * 0.3
    d.polyline([(gx, Y[2] + bh), (gx, gy), (fx2, gy), (fx2, Y[1] + bh)], "fails", label_seg=1,
               label_at=0.62, label_dy=15)
    d.arrow("recheck", "backlog", "new\nfindings", sa="l", sb="r", dashed=True, label_dy=-8)
    d.polyline([(X[1], Y[1] + bh * 0.5), (22, Y[1] + bh * 0.5), (22, Y[3] + bh * 0.5), (X[1], Y[3] + bh * 0.5)],
               "minor findings", dashed=True, label_seg=2, label_at=0.55, label_dy=-6)
    # Legend
    ly = 488
    x = d.swatch(9, ly, "orchestrator", kind="pink")
    x = d.swatch(x, ly, "author agent", kind="aqua")
    x = d.swatch(x, ly, "reviewer agent", kind="violet")
    x = d.swatch(x, ly, "inventory agents")
    x = d.swatch(x, ly, "the user", kind="dark")
    d.text(9, ly + 22, "Reviewers: GPT-6.1 Sol until 11 Nov, then fresh Opus 5.5 agents; re-checks Opus 5.5 at "
           "effort high.", size=dg.LABEL_SIZE)
    d.save(fs.FIGURES / "ai-work-package-flow.svg")
    print("figure ai-work-package-flow.svg")


def main(argv: list[str]) -> int:
    fs.FIGURES.mkdir(exist_ok=True)
    names = argv or list(FIGS)
    failed = 0
    for name in names:
        fn = FIGS.get(name)
        if fn is None:
            print(f"unknown figure: {name}", file=sys.stderr)
            failed += 1
            continue
        fn()
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
