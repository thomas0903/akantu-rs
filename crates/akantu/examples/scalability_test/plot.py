#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Ported from Akantu (EPFL/LSMS): examples/c++/scalability_test/plot.py
# (Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne),
# Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides);
# Akantu is free software under the GNU Lesser General Public License,
# version 3 or later).
"""Time-to-solution plot of the scalability study (EX-049).

Upstream plots ``results/timmings_<material>_<compiler><suffix>.csv``: one
header line ``"psize", "<phase>", "<phase> nb_rep", ...`` and one line per
run, as the chronometer of ``scalability_test`` prints them (the Rust example
prints the same lines on its standard error, i.e. in the ``.err`` file of
each SLURM job of ``scalability.sh``). The port adds a ``collect`` step that
merges those job outputs into such a file::

    plot.py collect elastic_rust_*.err -o results/timmings_elastic_rustc_rs.csv
    plot.py collect cohesive_rust_*.err -o results/timmings_cohesive_rustc_rs.csv
    plot.py plot [--suffix _rs] [--no-show]

``plot`` draws, as upstream, the time to solution (``solve_step`` time times
its repetitions, plus ``check_cohesive_stress`` for the cohesive runs) per
number of cores: median, min/max band and the ideal scaling, written to
``TTS.svg``. It needs pandas, numpy and matplotlib.
"""
import argparse
import os
import sys


def collect(paths, out):
    """Merge the chronometer CSV blocks of job outputs into one file."""
    header = None
    rows = []
    for path in paths:
        with open(path, encoding="utf-8") as fh:
            lines = [line.rstrip("\n") for line in fh]
        for k, line in enumerate(lines):
            if line.startswith('"psize"') and k + 1 < len(lines):
                if header is None:
                    header = line
                elif line != header:
                    sys.exit(f"{path}: other phases than the first file's")
                rows.append(lines[k + 1])
    if header is None:
        sys.exit("no chronometer CSV found")
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    with open(out, "w", encoding="utf-8") as fh:
        fh.write(header + "\n")
        fh.writelines(r + "\n" for r in rows)
    print(f"{out}: {len(rows)} runs")


def plot(suffix, show):
    """Upstream's time-to-solution plot."""
    import numpy as np
    import pandas as pd

    # needed to generate the plots on jed
    import matplotlib

    # matplotlib.use("TKAgg")
    import matplotlib.pyplot as plt

    # Same font as JOSS
    plt.rcParams["font.sans-serif"] = "cmss10"

    # Loading data
    plots = {
        "elastic rust": {
            "prefix": "timmings_",
            "material": "elastic",
            "compiler": "rustc",
            "suffix": suffix,
        },
        "cohesive rust": {
            "prefix": "timmings_",
            "material": "cohesive",
            "compiler": "rustc",
            "suffix": suffix,
        },
    }

    fig, ax = plt.subplots(figsize=(4.5, 4))

    plotting = "TTS"

    for _plot_name, data in plots.items():
        data["df"] = pd.read_csv(
            os.path.join(
                "results",
                f"""{data["prefix"]}{data["material"]}_{data["compiler"]}{data["suffix"]}.csv""",
            ),
            sep=",",
            skipinitialspace=True,
        )

        df = data["df"]
        step = df["solve_step"] * df["solve_step nb_rep"]
        if data["material"] == "cohesive":
            step = (
                step
                + df["check_cohesive_stress"] * df["check_cohesive_stress nb_rep"]
            )

        df["TTS"] = step
        df["speedup"] = step[0] / step
        df["mumps"] = df["static_solve"] * df["static_solve nb_rep"]

    def plot_measure(ax, df, plotting, label, **kwargs):
        """Plot a given measure."""
        grouped = df.groupby("psize")  # compute stats grouped by number of procs
        med = grouped.median()
        min_ = grouped.min()
        max_ = grouped.max()
        min_psize = df["psize"][0]

        print(list(med[plotting]))

        (legend,) = ax.plot(
            med.index, med[plotting], label=f"{label} (median)", **kwargs
        )

        ax.fill_between(
            med.index,
            min_[plotting],
            max_[plotting],
            color=legend.get_color(),
            alpha=0.2,
        )

        ax.plot(
            med.index,
            min_psize * med[plotting][min_psize] / med.index,
            ls="--",
            color=legend.get_color(),
        )

    plot_measure(
        ax,
        plots["cohesive rust"]["df"],
        plotting,
        "insertion",
        marker="o",
    )

    plot_measure(
        ax,
        plots["elastic rust"]["df"],
        plotting,
        "no insertion",
        marker="o",
    )

    # Selecting appropriate tick values
    psize = np.array(np.unique(plots[list(plots.keys())[0]]["df"]["psize"]))
    labels = np.concatenate(
        [[psize[0]], psize[1:][psize[1:] >= 2 * psize[:-1]], [psize[-1]]]
    )
    ax.set_xscale("log", base=2)
    ax.set_yscale("log")

    ylabel = plotting if plotting != "TTS" else "Time to solution"
    yunit = "s" if plotting != "speedup" else "-"

    ax.set_xlabel("Nb Cores [-]")
    ax.set_ylabel(f"""{ylabel} [{yunit}]""")

    ax.set_xticks(ticks=labels, labels=map(str, labels))

    # Constructing legend with min/max and ideal labels
    handles, labels = ax.get_legend_handles_labels()
    handles += [
        matplotlib.lines.Line2D([], [], linestyle="--", color="k"),
        matplotlib.patches.Patch(color="k", alpha=0.2),
    ]
    labels += ["ideal", "min/max"]
    ax.legend(handles=handles, labels=labels)

    fig.tight_layout()
    fig.savefig(
        f"{plotting}.svg", transparent=True, bbox_inches="tight", pad_inches=0.1
    )
    if show:
        plt.show()


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("collect", help="merge the chronometer CSV of job outputs")
    c.add_argument("files", nargs="+")
    c.add_argument("-o", "--output", required=True)
    pl = sub.add_parser("plot", help="time-to-solution plot (TTS.svg)")
    pl.add_argument("--suffix", default="", help="file-name suffix of the results")
    pl.add_argument("--no-show", action="store_true", help="do not open a window")
    a = p.parse_args()
    if a.cmd == "collect":
        collect(a.files, a.output)
    else:
        plot(a.suffix, not a.no_show)


if __name__ == "__main__":
    main()
