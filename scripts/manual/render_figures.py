# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tooling (not ported from Akantu): renders the result images of
# the Examples chapter of the user manual (docs/manual/src/examples.md)
# from the ParaView and CSV output of the port's examples.
"""Render the manual's example figures from the examples' output.

This is a maintenance tool. It is not part of the build, of the tests or of
CI; run it by hand when an example's output changes.

1. Build and run the examples, each in its own directory below one output
   root (nothing is written into the repository):

   ```sh
   R=$PWD                      # the repository
   OUT=/some/scratch/runs      # any directory outside the repository
   cargo build -p akantu --release --examples --features cgal --locked
   for e in user_defined_bc static explicit_dynamic implicit_dynamic \
            example_parser new_local_material dumper_low_level parallel_2d \
            cohesive_intrinsic cohesive_extrinsic cohesive_extrinsic_ig_tg \
            heat_diffusion_static_2d heat_diffusion_dynamics_2d \
            heat_diffusion_dynamics_3d phase_field_notch contact_explicit_static \
            contact_explicit_dynamic cohesive_contact_explicit_dynamic \
            bloc_friction steady_state material_viscoelastic_maxwell_energies \
            embedded; do
     mkdir -p $OUT/$e && (cd $OUT/$e && $R/target/release/examples/$e --quiet)
   done
   # parallel_2d on four in-process ranks (the partition panel)
   mkdir -p $OUT/parallel_2d_np4 && (cd $OUT/parallel_2d_np4 && \
     $R/target/release/examples/parallel_2d --quiet --local-ranks 4)
   # scalability_test with its ParaView output switched on
   mkdir -p $OUT/scalability_test && cd $OUT/scalability_test && \
     sed 's/output_paraview = false/output_paraview = true/' \
       $R/crates/akantu/examples/scalability_test/material-cohesive.dat \
       > material-cohesive-paraview.dat && \
     $R/target/release/examples/scalability_test --quiet \
       --aka_input_file $PWD/material-cohesive-paraview.dat \
       --mesh-dir $R/crates/akantu/examples/scalability_test
   ```

   The release build adds about 1.2 GB to `target/`; the outputs take about
   3 GB (`explicit_dynamic` alone 1 GB).

2. Render (off-screen; no window is opened). The figures of the manual were
   made with pyvista 0.46.4, VTK 9.5.2 (OSMesa), matplotlib 3.11.2 and
   pngquant in a Linux container, so that nothing opens on the desktop:

   ```sh
   cat > Dockerfile <<'EOF'
   FROM python:3.12-slim
   RUN apt-get update && apt-get install -y --no-install-recommends \
         libosmesa6 libgl1 libxrender1 libgomp1 pngquant \
    && rm -rf /var/lib/apt/lists/*
   RUN python -m venv /venv && /venv/bin/pip install --no-cache-dir \
         pyvista==0.46.4 vtk==9.5.2 matplotlib==3.11.2 pillow
   ENV PATH=/venv/bin:$PATH VTK_DEFAULT_OPENGL_WINDOW=vtkOSOpenGLRenderWindow \
       MPLBACKEND=Agg
   EOF
   docker build -t akantu-render:local .
   docker run --rm -v $OUT:/runs:ro -v $R/scripts/manual:/tool:ro \
     -v $R/docs/manual/src/figures/examples:/out \
     akantu-render:local python -I /tool/render_figures.py /runs /out
   ```

   `--only name[,name…]` renders a subset. Any environment with pyvista,
   matplotlib and an off-screen OpenGL works the same way.

Every image is a PNG of at most 1400 px width on a white background with the
`coolwarm` colour map; a magnification of the deformed shape, when there is
one, is stated in the manual's caption (the factors are below).
"""

from __future__ import annotations

import argparse
import glob
import os
import shutil
import subprocess
import sys

import numpy as np

os.environ.setdefault("MPLBACKEND", "Agg")

import pyvista as pv  # noqa: E402

pv.OFF_SCREEN = True
pv.global_theme.font.family = "arial"
pv.global_theme.font.color = "#222222"

CMAP = "coolwarm"
WIDTH = 1400
INK = "#222222"
EDGE = "#3a3a3a"
FIGURES = {}


def figure(name):
    """Register a figure function under the image's base name."""

    def wrap(fn):
        FIGURES[name] = fn
        return fn

    return wrap


# --------------------------------------------------------------------------
# Reading


def frames(runs, example, base):
    """The numbered `.pvtu` files of a dump series, in step order."""
    pattern = os.path.join(runs, example, "paraview", f"{base}_[0-9]*.pvtu")
    files = sorted(glob.glob(pattern))
    if not files:
        raise FileNotFoundError(pattern)
    return files


def read(path):
    mesh = pv.read(path)
    if isinstance(mesh, pv.MultiBlock):
        mesh = mesh.combine()
    return mesh


def vec3(a):
    """A point vector as three components (2-D arrays get a zero z)."""
    a = np.asarray(a, dtype=float)
    if a.ndim == 1:
        a = a[:, None]
    if a.shape[1] < 3:
        a = np.hstack([a, np.zeros((a.shape[0], 3 - a.shape[1]))])
    return a[:, :3]


def warped(mesh, factor, field="displacement"):
    """A copy of `mesh` moved by `factor` times its displacement."""
    out = mesh.copy()
    out.points = mesh.points + factor * vec3(mesh.point_data[field])
    return out


def magnitude(mesh, field="displacement", name=None):
    name = name or f"|{field}|"
    mesh.point_data[name] = np.linalg.norm(vec3(mesh.point_data[field]), axis=1)
    return name


def component(mesh, field, comp, name, cell=False):
    data = mesh.cell_data if cell else mesh.point_data
    data[name] = np.asarray(data[field])[:, comp]
    return name


def shifted(mesh, dx=0.0, dy=0.0, dz=0.0):
    out = mesh.copy()
    out.points = mesh.points + np.array([dx, dy, dz])
    return out


# --------------------------------------------------------------------------
# Drawing


def plotter(height):
    p = pv.Plotter(off_screen=True, window_size=(WIDTH, int(height)))
    p.set_background("white")
    p.enable_anti_aliasing("ssaa")
    return p


def bar_args(title, vertical=False, fmt="%.3g", n_labels=5):
    common = dict(
        title=title,
        color=INK,
        fmt=fmt,
        n_labels=n_labels,
        title_font_size=20,
        label_font_size=17,
        font_family="arial",
    )
    if vertical:
        common.update(vertical=True, position_x=0.80, position_y=0.12, width=0.06, height=0.72)
    else:
        common.update(vertical=False, position_x=0.25, position_y=0.075, width=0.5, height=0.06)
    return common


def add_field(p, mesh, scalars, clim, title, edges=False, bar=True, vertical=False, fmt="%.3g",
              cmap=CMAP, lighting=False, n_labels=5, **kw):
    p.add_mesh(
        mesh,
        scalars=scalars,
        cmap=cmap,
        clim=clim,
        show_edges=edges,
        edge_color=EDGE,
        line_width=0.6,
        show_scalar_bar=bar,
        scalar_bar_args=bar_args(title, vertical, fmt, n_labels) if bar else None,
        lighting=lighting,
        **kw,
    )


def add_outline(p, mesh, color=INK, width=1.6):
    """The boundary of a 2-D mesh (the undeformed shape), drawn on top."""
    edges = mesh.extract_feature_edges(boundary_edges=True, feature_edges=False,
                                       manifold_edges=False, non_manifold_edges=False)
    b = mesh.bounds
    edges = shifted(edges, dz=1e-3 * max(b[1] - b[0], b[3] - b[2]))
    p.add_mesh(edges, color=color, line_width=width, lighting=False)


def add_label(p, point, text, size=20, color=INK):
    p.add_point_labels([point], [text], font_size=size, text_color=color, shape=None,
                       show_points=False, always_visible=True, font_family="arial", bold=False)


def fit_2d(p, bounds, reserve=(0.02, 0.02, 0.14, 0.02)):
    """Parallel projection of the x-y plane fitting `bounds` into the
    window minus the reserved fractions (left, right, bottom, top)."""
    left, right, bottom, top = reserve
    w, h = p.window_size
    x0, x1, y0, y1 = bounds[0], bounds[1], bounds[2], bounds[3]
    cw, ch = max(x1 - x0, 1e-30), max(y1 - y0, 1e-30)
    scale = max(ch / (2 * (1 - bottom - top)), cw * h / (2 * (1 - left - right) * w)) * 1.04
    wpp = 2 * scale / h
    cx_px = (left + (1 - left - right) / 2) * w
    cy_px = (bottom + (1 - bottom - top) / 2) * h
    fx = (x0 + x1) / 2 - (cx_px - w / 2) * wpp
    fy = (y0 + y1) / 2 - (cy_px - h / 2) * wpp
    p.camera.parallel_projection = True
    p.camera.focal_point = (fx, fy, 0.0)
    p.camera.position = (fx, fy, 10.0 * max(cw, ch))
    p.camera.up = (0.0, 1.0, 0.0)
    p.camera.parallel_scale = scale
    p.camera.clipping_range = (1e-3 * max(cw, ch), 100.0 * max(cw, ch))


def union_bounds(meshes):
    b = np.array([m.bounds for m in meshes])
    return [b[:, 0].min(), b[:, 1].max(), b[:, 2].min(), b[:, 3].max(), b[:, 4].min(), b[:, 5].max()]


def height_for(bounds, reserve=(0.02, 0.02, 0.14, 0.02), extra=1.0):
    """Window height that gives the content its aspect ratio."""
    left, right, bottom, top = reserve
    cw, ch = bounds[1] - bounds[0], (bounds[3] - bounds[2]) * extra
    h = WIDTH * (1 - left - right) * ch / cw / (1 - bottom - top)
    return int(min(max(h, 360), 1100))


def save(p, out_dir, name):
    path = os.path.join(out_dir, name + ".png")
    for actor in p.scalar_bars.values():
        # labels on the other side of the bar from the title (no overlap)
        actor.SetTextPositionToPrecedeScalarBar()
    p.screenshot(path, transparent_background=False)
    p.close()
    optimise(path)
    return path


def optimise(path):
    """Quantise the PNG (pngquant when installed, else Pillow)."""
    if shutil.which("pngquant"):
        subprocess.run(["pngquant", "--force", "--skip-if-larger", "--quality", "70-95",
                        "--output", path, path], check=False)
        return
    try:
        from PIL import Image
    except ImportError:
        return
    im = Image.open(path).convert("RGB").quantize(colors=256, method=Image.Quantize.MEDIANCUT)
    im.save(path, optimize=True)


def clim_of(meshes, scalars, cell=False, symmetric=False):
    vals = np.concatenate([np.asarray((m.cell_data if cell else m.point_data)[scalars]).ravel()
                           for m in meshes])
    lo, hi = float(vals.min()), float(vals.max())
    if symmetric:
        a = max(abs(lo), abs(hi))
        lo, hi = -a, a
    if hi == lo:
        hi = lo + 1.0
    return [lo, hi]


def frame_row_2d(meshes, labels, scalars, title, out_dir, name, cell=False, edges=False,
                 gap=0.15, symmetric=False, outlines=None, clim=None, vertical_stack=False,
                 fmt="%.3g", n_labels=5):
    """Several 2-D frames side by side (or stacked), one colour scale."""
    placed = []
    offset = 0.0
    for m in meshes:
        b = m.bounds
        if vertical_stack:
            dy = offset - b[3]
            placed.append(dy)
            offset = offset - (b[3] - b[2]) * (1 + gap)
        else:
            dx = offset - b[0]
            placed.append(dx)
            offset = offset + (b[1] - b[0]) * (1 + gap)
    moved = [shifted(m, dy=d) if vertical_stack else shifted(m, dx=d) for m, d in zip(meshes, placed)]
    clim = clim or clim_of(moved, scalars, cell, symmetric)
    bounds = union_bounds(moved)
    label_room = 0.08 if labels else 0.02
    reserve = (0.02, 0.02, 0.16, label_room) if not vertical_stack else (0.02, 0.02, 0.14, label_room)
    p = plotter(height_for(bounds, reserve))
    for i, m in enumerate(moved):
        add_field(p, m, scalars, clim, title, edges=edges, bar=(i == 0), fmt=fmt,
                  n_labels=n_labels)
        if outlines is not None:
            add_outline(p, shifted(outlines[i], dy=placed[i]) if vertical_stack
                        else shifted(outlines[i], dx=placed[i]))
    fit_2d(p, bounds, reserve)
    if labels:
        w, h = p.window_size
        for m, text in zip(moved, labels):
            b = m.bounds
            if vertical_stack:
                height = b[3] - b[2]
                add_label(p, ((b[0] + b[1]) / 2, b[3] + gap * height * 0.45, 0), text)
            else:
                add_label(p, ((b[0] + b[1]) / 2, bounds[3] + 0.04 * (bounds[3] - bounds[2]), 0), text)
    return save(p, out_dir, name)


# --------------------------------------------------------------------------
# Figures: solid mechanics


@figure("user_defined_bc")
def fig_user_defined_bc(runs, out):
    m = read(frames(runs, "user_defined_bc", "plate")[0])
    s = magnitude(m)
    return frame_row_2d([m], None, s, "displacement magnitude [m]", out, "user_defined_bc",
                        edges=True)


@figure("static")
def fig_static(runs, out):
    # deformed shape magnified 10 times; the grey line is the undeformed plate
    m = read(frames(runs, "static", "static")[-1])
    s = magnitude(m)
    w = warped(m, 10.0)
    return frame_row_2d([w], None, s, "displacement magnitude [m]", out, "static", edges=True,
                        outlines=[m])


@figure("explicit_dynamic")
def fig_explicit_dynamic(runs, out):
    files = frames(runs, "explicit_dynamic", "explicit_dynamic")
    steps = [0, 60, 120, 180]
    meshes = []
    for k in steps:
        m = read(files[k]).extract_surface()
        component(m, "displacement", 0, "u_x")
        meshes.append(m)
    clim = clim_of(meshes, "u_x", symmetric=True)
    gap = 1.0
    moved = [shifted(m, dy=-i * (1.0 + gap)) for i, m in enumerate(meshes)]
    p = plotter(900)
    for i, m in enumerate(moved):
        add_field(p, m, "u_x", clim, "displacement u_x [m]", bar=(i == 0), lighting=True,
                  ambient=0.45, diffuse=0.6)
        add_label(p, (-4.2, m.bounds[3] + 0.3, 1.0), f"step {steps[i]}")
    p.view_vector((0.25, 0.35, 1.0), viewup=(0, 1, 0))
    p.camera.parallel_projection = True
    p.reset_camera()
    p.camera.zoom(1.15)
    # move the bars up: the lower part of the image belongs to the colour bar
    up = np.array(p.camera.up)
    p.camera.position = tuple(np.array(p.camera.position) - 0.9 * up)
    p.camera.focal_point = tuple(np.array(p.camera.focal_point) - 0.9 * up)
    return save(p, out, "explicit_dynamic")


@figure("implicit_dynamic")
def fig_implicit_dynamic(runs, out):
    # deformed beam magnified 50 times at t = 0.1, 0.2 and 0.3 s
    files = frames(runs, "implicit_dynamic", "dynamic")
    idx = [10, 20, 30]
    meshes, outlines = [], []
    for k in idx:
        m = read(files[k])
        component(m, "displacement", 1, "u_y")
        meshes.append(warped(m, 50.0))
        outlines.append(m)
    return frame_row_2d(meshes, [f"t = {k * 100 * 1e-4:.1f} s" for k in idx], "u_y",
                        "vertical displacement u_y [m]", out, "implicit_dynamic",
                        outlines=outlines, vertical_stack=True, gap=0.6)


@figure("implicit_dynamic-deflection")
def fig_implicit_dynamic_deflection(runs, out):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    data = np.genfromtxt(os.path.join(runs, "implicit_dynamic", "position.csv"), delimiter=",",
                         names=True)
    fig, ax = plt.subplots(figsize=(9.0, 3.6), dpi=150)
    ax.plot(data["time"], data["position"] * 1e3, color="#3b4cc0", lw=1.6,
            label="akantu-rs (loaded node)")
    ax.plot(data["time"], data["solution"] * 1e3, color="#b40426", lw=1.2, ls="--",
            label="Euler–Bernoulli, modes 1, 3, 5")
    ax.set_xlabel("time [s]")
    ax.set_ylabel("mid-span deflection [mm]")
    ax.grid(color="#dddddd", lw=0.6)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
    ax.legend(frameon=False)
    fig.tight_layout()
    path = os.path.join(out, "implicit_dynamic-deflection.png")
    fig.savefig(path, facecolor="white")
    plt.close(fig)
    optimise(path)
    return path


@figure("example_parser")
def fig_example_parser(runs, out):
    # deformed shape magnified 2 times
    m = read(frames(runs, "example_parser", "swiss_cheese")[-1])
    s = magnitude(m)
    return frame_row_2d([warped(m, 2.0)], None, s, "displacement magnitude [m]", out,
                        "example_parser", outlines=[m])


@figure("new_local_material")
def fig_new_local_material(runs, out):
    m = read(frames(runs, "new_local_material", "local_material")[-1])
    component(m, "stress", 0, "sigma_xx", cell=True)
    return frame_row_2d([m], None, "sigma_xx", "stress sigma_xx [Pa]", out,
                        "new_local_material", cell=True)


@figure("dumper_low_level")
def fig_dumper(runs, out):
    files = sorted(glob.glob(os.path.join(runs, "dumper_low_level", "paraview", "dumper",
                                          "train_[0-9]*.pvtu")))
    # dump 0 holds the mesh only (the fields are added after it, as upstream)
    idx = [1, 50, 100]
    meshes = [warped(read(files[k]), 1.0) for k in idx]
    return frame_row_2d(meshes, [f"dump {k}" for k in idx], "colour", "colour", out,
                        "dumper_low_level", cell=True, vertical_stack=True, gap=0.3, fmt="%.0f",
                        n_labels=3)


@figure("parallel_2d")
def fig_parallel(runs, out):
    # left: the partition of the four-rank run; right: u_x of the serial run
    pieces = sorted(glob.glob(os.path.join(runs, "parallel_2d_np4", "paraview", "multi-VTU",
                                           "multi_0000.proc*.vtu")))
    parts = []
    for r, f in enumerate(pieces):
        m = pv.read(f)
        m.cell_data["rank"] = np.full(m.n_cells, float(r))
        parts.append(m)
    field = read(frames(runs, "parallel_2d", "multi")[-1])
    component(field, "displacement", 0, "u_x")
    dx = 1.18
    field = shifted(field, dx=dx)
    bounds = union_bounds(parts + [field])
    reserve = (0.02, 0.02, 0.17, 0.08)
    p = plotter(height_for(bounds, reserve))
    clim_r = [0, max(len(parts) - 1, 1)]
    for m in parts:
        p.add_mesh(m, scalars="rank", cmap=CMAP, clim=clim_r, show_scalar_bar=False,
                   show_edges=True, edge_color=EDGE, line_width=0.4)
        c = m.center
        add_label(p, (c[0], c[1], 0.01), f"rank {int(m.cell_data['rank'][0])}", size=24,
                  color="white")
    clim = clim_of([field], "u_x", symmetric=True)
    add_field(p, field, "u_x", clim, "displacement u_x [m]", fmt="%.2g")
    fit_2d(p, bounds, reserve)
    add_label(p, (0.5, 1.06, 0), "partition (4 ranks)")
    add_label(p, (0.5 + dx, 1.06, 0), "u_x, step 10 000")
    return save(p, out, "parallel_2d")


# --------------------------------------------------------------------------
# Figures: cohesive elements


@figure("cohesive_intrinsic")
def fig_cohesive_intrinsic(runs, out):
    files = frames(runs, "cohesive_intrinsic", "intrinsic")
    idx = [20, 60, 100]
    meshes = []
    for k in idx:
        m = read(files[k])
        component(m, "displacement", 0, "u_x")
        meshes.append(warped(m, 1.0))
    return frame_row_2d(meshes, [f"step {k}" for k in idx], "u_x", "displacement u_x", out,
                        "cohesive_intrinsic", edges=True, gap=0.6)


@figure("cohesive_extrinsic")
def fig_cohesive_extrinsic(runs, out):
    files = frames(runs, "cohesive_extrinsic", "extrinsic")
    idx = [4, 7, 10]
    meshes = []
    for k in idx:
        m = read(files[k])
        component(m, "displacement", 1, "u_y")
        meshes.append(warped(m, 1.0))
    return frame_row_2d(meshes, [f"step {10 * k}" for k in idx], "u_y", "displacement u_y", out,
                        "cohesive_extrinsic", edges=True, gap=0.3, symmetric=True)


@figure("cohesive_extrinsic_ig_tg")
def fig_cohesive_ig_tg(runs, out):
    files = frames(runs, "cohesive_extrinsic_ig_tg", "extrinsic")
    idx = [20, 50, 100]
    meshes = []
    for k in idx:
        m = read(files[k])
        s = magnitude(m)
        meshes.append(warped(m, 1.0))
    return frame_row_2d(meshes, [f"step {10 * k}" for k in idx], s, "displacement magnitude",
                        out, "cohesive_extrinsic_ig_tg", edges=True, gap=0.3)


@figure("scalability_test")
def fig_scalability(runs, out):
    m = read(frames(runs, "scalability_test", "extrinsic")[-1])
    s = magnitude(m)
    w = warped(m, 1.0).extract_surface()
    w.point_data[s] = w.point_data[s]
    p = plotter(900)
    add_field(p, w, s, clim_of([w], s), "displacement magnitude [m]", edges=True,
              lighting=True, ambient=0.45, diffuse=0.6, vertical=True)
    p.view_isometric()
    p.reset_camera()
    p.camera.zoom(0.95)
    return save(p, out, "scalability_test")


# --------------------------------------------------------------------------
# Figures: diffusion


@figure("heat_diffusion_static_2d")
def fig_heat_static(runs, out):
    files = frames(runs, "heat_diffusion_static_2d", "heat_diffusion_static_2d")
    meshes = [read(files[0]), read(files[-1])]
    return frame_row_2d(meshes, ["initial", "solution"], "temperature", "temperature [K]", out,
                        "heat_diffusion_static_2d", gap=0.1)


@figure("heat_diffusion_dynamics_2d")
def fig_heat_dyn_2d(runs, out):
    files = frames(runs, "heat_diffusion_dynamics_2d", "heat_diffusion_square2d")
    idx = [10, 50, 149]
    meshes = [read(files[k]) for k in idx]
    clim = [100.0, 300.0]
    gap = 0.1
    moved = [shifted(m, dx=i * (1 + gap)) for i, m in enumerate(meshes)]
    bounds = union_bounds(moved)
    reserve = (0.02, 0.02, 0.16, 0.08)
    p = plotter(height_for(bounds, reserve))
    for i, m in enumerate(moved):
        add_field(p, m, "temperature", clim, "temperature [K]", bar=(i == 0))
        iso = m.contour(isosurfaces=[110, 125, 150, 200, 250], scalars="temperature")
        if iso.n_points:
            p.add_mesh(iso, color=INK, line_width=1.2)
        b = m.bounds
        t = idx[i] * 100 * 0.12
        add_label(p, ((b[0] + b[1]) / 2, 1.06, 0), f"step {idx[i] * 100} (t = {t:.0f} s)")
    fit_2d(p, bounds, reserve)
    return save(p, out, "heat_diffusion_dynamics_2d")


@figure("heat_diffusion_dynamics_3d")
def fig_heat_dyn_3d(runs, out):
    files = frames(runs, "heat_diffusion_dynamics_3d", "heat_diffusion_cube3d")
    idx = [1, 4, 9]
    p = plotter(640)
    clim = [100.0, 300.0]
    for i, k in enumerate(idx):
        m = read(files[k])
        half = m.clip(normal=(0, -1, 0), origin=(0.5, 0.5, 0.5), crinkle=False)
        half = shifted(half.extract_surface(nonlinear_subdivision=2), dx=1.6 * i)
        add_field(p, half, "temperature", clim, "temperature [K]", bar=(i == 0), lighting=True,
                  ambient=0.5, diffuse=0.55)
        add_label(p, (0.5 + 1.6 * i, 0.5, 1.35), f"step {k * 100}")
    p.view_vector((0.45, -1.0, 0.55), viewup=(0, 0, 1))
    p.camera.parallel_projection = True
    p.reset_camera()
    p.camera.zoom(1.6)
    return save(p, out, "heat_diffusion_dynamics_3d")


# --------------------------------------------------------------------------
# Figures: phase field


@figure("phase_field_notch")
def fig_phase_field(runs, out):
    m = read(frames(runs, "phase_field_notch", "phase_notch")[-1])
    c0 = read(frames(runs, "phase_field_notch", "crack_0")[0])
    c1 = read(frames(runs, "phase_field_notch", "crack_1")[0])
    c0.cell_data["cluster"] = np.zeros(c0.n_cells)
    c1.cell_data["cluster"] = np.ones(c1.n_cells)
    dx = 1.15
    bounds = union_bounds([m, shifted(m, dx=dx)])
    reserve = (0.02, 0.02, 0.17, 0.08)
    p = plotter(height_for(bounds, reserve))
    add_field(p, m, "damage", clim_of([m], "damage", cell=True), "damage", edges=True)
    for c in (c0, c1):
        p.add_mesh(shifted(c, dx=dx), scalars="cluster", cmap=CMAP, clim=[-0.6, 1.6],
                   show_scalar_bar=False, show_edges=True, edge_color=EDGE, line_width=0.6)
        cc = c.center
        add_label(p, (cc[0] + dx, cc[1], 0.01), f"cluster {int(c.cell_data['cluster'][0])}", 18)
    fit_2d(p, bounds, reserve)
    add_label(p, (0.0, 0.56, 0), "damage, load step 900")
    add_label(p, (dx, 0.56, 0), "clusters (crack_0, crack_1)")
    return save(p, out, "phase_field_notch")


# --------------------------------------------------------------------------
# Figures: contact


def _hertz(runs, example, base, out, name, step_label):
    m = read(frames(runs, example, base)[-1])
    component(m, "stress", 3, "sigma_yy", cell=True)
    w = warped(m, 1.0)
    bounds = union_bounds([w, m])
    reserve = (0.02, 0.02, 0.16, 0.08)
    p = plotter(height_for(bounds, reserve))
    add_field(p, w, "sigma_yy", clim_of([w], "sigma_yy", cell=True, symmetric=True),
              "stress sigma_yy", edges=True)
    add_outline(p, m, color="#777777", width=1.2)
    # the contact force on the slave nodes, as arrows
    f = vec3(w.point_data["contact_force"])
    keep = np.linalg.norm(f, axis=1) > 0
    if keep.any():
        pts = pv.PolyData(w.points[keep] + np.array([0, 0, 1e-4]))
        pts["f"] = f[keep]
        scale = 0.012 / np.linalg.norm(f[keep], axis=1).max()
        p.add_mesh(pts.glyph(orient="f", scale="f", factor=scale), color=INK, lighting=False)
    fit_2d(p, bounds, reserve)
    add_label(p, ((bounds[0] + bounds[1]) / 2, bounds[3] + 0.04 * (bounds[3] - bounds[2]), 0),
              step_label)
    return save(p, out, name)


@figure("contact_explicit_static")
def fig_contact_static(runs, out):
    return _hertz(runs, "contact_explicit_static", "contact-explicit-static", out,
                  "contact_explicit_static", "load increment 100")


@figure("contact_explicit_dynamic")
def fig_contact_dynamic(runs, out):
    return _hertz(runs, "contact_explicit_dynamic", "contact-explicit-dynamic", out,
                  "contact_explicit_dynamic", "step 19 000")


@figure("cohesive_contact_explicit_dynamic")
def fig_cohesive_contact(runs, out):
    # deformed shape magnified 200 times
    files = frames(runs, "cohesive_contact_explicit_dynamic", "cohesive-contact-explicit-dynamic")
    idx = [14, 28, 42]
    meshes, outlines = [], []
    for k in idx:
        m = read(files[k])
        component(m, "displacement", 1, "u_y")
        meshes.append(warped(m, 200.0))
        outlines.append(m)
    # a dump every 1000 steps (25 000 steps apart, then 25 000 back together)
    return frame_row_2d(meshes, [f"step {k} 000" for k in idx], "u_y", "displacement u_y", out,
                        "cohesive_contact_explicit_dynamic", edges=True, gap=0.6,
                        symmetric=True, outlines=outlines, fmt="%.2g")


@figure("bloc_friction")
def fig_bloc_friction(runs, out):
    files = frames(runs, "bloc_friction", "bloc_friction")
    idx = [100, 200, 300]
    meshes = []
    for k in idx:
        m = read(files[k])
        component(m, "displacement", 0, "u_x")
        meshes.append(warped(m, 1.0))
    labels = ["end of pressing", "end of sliding", "end of the free phase"]
    return frame_row_2d(meshes, labels, "u_x", "displacement u_x [m]", out, "bloc_friction",
                        vertical_stack=True, gap=0.3)


@figure("steady_state")
def fig_steady_state(runs, out):
    # v_x minus the driving velocity +v/2 (slider) or -v/2 (base), v = 2e-3 m/s
    # (`shear_velocity` of ras_ss.in); the sign of v_x tells the body
    files = frames(runs, "steady_state", "steady_state")
    idx = [20, 50, 100]
    meshes = []
    for k in idx:
        m = read(files[k])
        vx = np.asarray(m.point_data["velocity"])[:, 0]
        m.point_data["dv_x"] = vx - np.sign(vx) * 1e-3
        meshes.append(m)
    return frame_row_2d(meshes, [f"step {10 * k}" for k in idx], "dv_x",
                        "velocity v_x - (+/-v/2) [m/s]", out, "steady_state", symmetric=True,
                        fmt="%.2g", vertical_stack=True, gap=0.3)


# --------------------------------------------------------------------------
# Figures: other


@figure("material_viscoelastic_maxwell_energies")
def fig_viscoelastic(runs, out):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    d = np.loadtxt(os.path.join(runs, "material_viscoelastic_maxwell_energies",
                                "material_viscoelastic_maxwell_output.out"))
    t, eps, epot, edis, work = d.T
    fig, ax = plt.subplots(figsize=(9.0, 3.6), dpi=150)
    ax.plot(t, work, color=INK, lw=1.4, label="work of the imposed strain $W$")
    ax.plot(t, epot, color="#3b4cc0", lw=1.6, label="potential energy $E_{pot}$")
    ax.plot(t, edis, color="#b40426", lw=1.6, label="dissipated energy $E_{diss}$")
    ax.axvline(10.0, color="#999999", lw=0.8, ls=":")
    ax.set_xlabel("time")
    ax.set_ylabel("energy")
    ax.grid(color="#dddddd", lw=0.6)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
    ax.legend(frameon=False, loc="center right")
    fig.tight_layout()
    path = os.path.join(out, "material_viscoelastic_maxwell_energies.png")
    fig.savefig(path, facecolor="white")
    plt.close(fig)
    optimise(path)
    return path


@figure("embedded")
def fig_embedded(runs, out):
    c = read(frames(runs, "embedded", "concrete")[-1])
    r = read(frames(runs, "embedded", "reinforcement")[-1])
    component(c, "stress", 0, "sigma_xx", cell=True)
    bounds = list(c.bounds)
    reserve = (0.02, 0.02, 0.2, 0.02)
    p = plotter(height_for(bounds, reserve))
    # colour scale clipped at the 99th percentile of |sigma_xx|: one element at the
    # anchorage of the bar (left end) reaches -4.9 kPa and would hide the rest
    a = float(np.percentile(np.abs(c.cell_data["sigma_xx"]), 99))
    add_field(p, c, "sigma_xx", [-a, a], "concrete stress sigma_xx [Pa]", edges=True)
    p.add_mesh(shifted(r, dz=0.01), color=INK, line_width=4)
    fit_2d(p, bounds, reserve)
    return save(p, out, "embedded")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("runs", help="directory holding one output directory per example")
    ap.add_argument("out", help="destination of the PNG files")
    ap.add_argument("--only", help="comma-separated figure names")
    args = ap.parse_args(argv)
    names = args.only.split(",") if args.only else list(FIGURES)
    os.makedirs(args.out, exist_ok=True)
    failed = []
    for name in names:
        try:
            path = FIGURES[name](args.runs, args.out)
            print(f"{name}: {os.path.getsize(path) // 1024} kB")
        except Exception as exc:  # report and go on with the other figures
            failed.append(name)
            print(f"{name}: FAILED: {exc!r}", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
