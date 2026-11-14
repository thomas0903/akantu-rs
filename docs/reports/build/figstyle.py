# SPDX-License-Identifier: LGPL-3.0-or-later
"""Shared chart style of the reports (matplotlib -> SVG).

Colour follows the entity in every figure: Rust is blue, C++ is orange, and
no other series uses either colour. Other series take the free slots of the
categorical order (OTHER), in order.
"""
from __future__ import annotations

import csv
from pathlib import Path

import matplotlib

matplotlib.use("svg")
import matplotlib.pyplot as plt  # noqa: E402

REPORTS = Path(__file__).resolve().parent.parent
DATA = REPORTS / "data"
FIGURES = REPORTS / "figures"

INK = "#1a1a19"
INK2 = "#52514e"
INK3 = "#6f6e69"
RULE = "#d9d8d2"
SURFACE = "#ffffff"

# Categorical order (validated for adjacent use; do not reorder or cycle).
SERIES = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"]
RUST = SERIES[0]
CPP = SERIES[1]
# Slots free for series that are neither code, in order. Amber (slot 3) and red
# (slot 7) read as orange next to the C++ colour, so no figure uses them.
OTHER = [SERIES[2], SERIES[4], SERIES[5], SERIES[6]]
REFERENCE = "#8f8e88"  # published or external reference values
# Sequential blue, light to dark (ordinal use starts at step 250).
BLUES = ["#86b6ef", "#5598e7", "#2a78d6", "#1c5cab", "#104281"]
# Status colours are reserved for states, never for series.
GOOD, WARNING, CRITICAL, NEUTRAL = "#0ca30c", "#fab219", "#d03b3b", "#b5b4ad"

plt.rcParams.update({
    "svg.fonttype": "none",
    "svg.hashsalt": "akantu-rs-reports",  # stable element ids: reproducible SVG files
    "font.family": "sans-serif",
    "font.sans-serif": ["Helvetica Neue", "Helvetica", "Arial", "DejaVu Sans"],
    "font.size": 9,
    "text.color": INK,
    "axes.edgecolor": INK3,
    "axes.labelcolor": INK2,
    "axes.labelsize": 9,
    "axes.titlesize": 10,
    "axes.titleweight": "bold",
    "axes.titlelocation": "left",
    "axes.titlepad": 10,
    "axes.linewidth": 0.8,
    "axes.spines.top": False,
    "axes.spines.right": False,
    "axes.grid": True,
    "axes.grid.axis": "y",
    "axes.axisbelow": True,
    "grid.color": RULE,
    "grid.linewidth": 0.6,
    "xtick.color": INK2,
    "ytick.color": INK2,
    "xtick.labelsize": 8.5,
    "ytick.labelsize": 8.5,
    "xtick.major.size": 3,
    "ytick.major.size": 0,
    "legend.frameon": False,
    "legend.fontsize": 8.5,
    "lines.linewidth": 2,
    "lines.markersize": 5.5,
    "figure.facecolor": SURFACE,
    "axes.facecolor": SURFACE,
    "savefig.facecolor": SURFACE,
    "savefig.bbox": "tight",
    "savefig.pad_inches": 0.08,
})


def read_csv(name: str) -> list[dict[str, str]]:
    """Rows of data/<name>.csv as dicts; lines starting with '#' are comments."""
    path = DATA / f"{name}.csv"
    with open(path, encoding="utf-8", newline="") as f:
        lines = [ln for ln in f if ln.strip() and not ln.lstrip().startswith("#")]
    return list(csv.DictReader(lines))


def has_data(*names: str) -> bool:
    return all((DATA / f"{n}.csv").exists() for n in names)


def new(width: float = 6.6, height: float = 3.3, **kw):
    """A figure sized for the A4 text width (inches)."""
    return plt.subplots(figsize=(width, height), **kw)


def save(fig, name: str) -> None:
    FIGURES.mkdir(exist_ok=True)
    fig.savefig(FIGURES / f"{name}.svg", metadata={"Date": None})
    plt.close(fig)
    print(f"figure {name}.svg")


def label_bars(ax, bars, fmt="{:.2f}", *, horizontal: bool = False, pad: float = 2.5,
               size: float = 8, color: str = INK2) -> None:
    """Value labels at the bar ends (use on few bars only)."""
    for b in bars:
        v = b.get_width() if horizontal else b.get_height()
        if v != v:  # NaN
            continue
        text = fmt.format(v) if isinstance(fmt, str) else fmt(v)
        if horizontal:
            ax.annotate(text, (v, b.get_y() + b.get_height() / 2), xytext=(pad, 0),
                        textcoords="offset points", va="center", ha="left", fontsize=size, color=color)
        else:
            ax.annotate(text, (b.get_x() + b.get_width() / 2, v), xytext=(0, pad),
                        textcoords="offset points", va="bottom", ha="center", fontsize=size, color=color)


def end_label(ax, x, y, text: str, color: str = INK2, dx: float = 5, dy: float = 0,
              size: float = 8.5) -> None:
    """Direct label at the end of a line (text in ink, never in the series colour)."""
    ax.annotate(text, (x, y), xytext=(dx, dy), textcoords="offset points", va="center",
                ha="left", fontsize=size, color=color)


def num(v: float, digits: int = 0) -> str:
    """A number with thousands separators, as the reports print them (2,387)."""
    return f"{v:,.{digits}f}"


def plain_log_axis(axis, ticks, fmt=None) -> None:
    """Log-scale ticks at the given values, labelled as plain numbers (no 10^x)."""
    from matplotlib.ticker import FixedLocator, FuncFormatter, NullLocator

    axis.set_major_locator(FixedLocator(ticks))
    axis.set_minor_locator(NullLocator())
    axis.set_major_formatter(FuncFormatter(fmt or (lambda v, _: f"{v:g}")))


def ratio_line(ax, *, vertical: bool = True, label: str | None = None, at: float = 1.0,
               faster_side: str = "left", y_text: float = 1.0) -> None:
    """Reference line at ratio 1 (same time), with a note on the side where the port is faster."""
    if vertical:
        ax.axvline(at, color=INK3, lw=0.9, zorder=1)
        if label:
            ha = "right" if faster_side == "left" else "left"
            dx = -4 if faster_side == "left" else 4
            ax.annotate(label, (at, y_text), xycoords=("data", "axes fraction"),
                        xytext=(dx, 2), textcoords="offset points", ha=ha, va="bottom",
                        fontsize=8, color=INK2)
    else:
        ax.axhline(at, color=INK3, lw=0.9, zorder=1)


def swatch_label(ax, x, y, color: str, text: str, *, size: float = 8.5, marker: str = "s",
                 transform=None) -> None:
    """A coloured marker followed by ink text (identity without coloured text)."""
    tr = transform or ax.transAxes
    ax.plot([x], [y], marker=marker, color=color, ms=6, transform=tr, clip_on=False, ls="none")
    ax.text(x + 0.015, y, text, transform=tr, va="center", ha="left", fontsize=size, color=INK2)


def footnote(fig, text: str, y: float = -0.02, width_chars: int | None = None) -> None:
    """A note under the figure, wrapped to the figure width so it never widens the saved SVG."""
    import textwrap

    if width_chars is None:
        width_chars = int(fig.get_figwidth() * 18)
    fig.text(0.0, y, textwrap.fill(text, width_chars), fontsize=7.5, color=INK3, ha="left", va="top",
             linespacing=1.3)
