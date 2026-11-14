# SPDX-License-Identifier: LGPL-3.0-or-later
"""Box-and-arrow diagrams written directly as SVG, in the style of the reports.

A diagram is laid out by hand on a pixel grid: boxes get explicit positions,
arrows connect named sides of two boxes. Nothing is computed automatically, so
the result is stable and easy to adjust.
"""
from __future__ import annotations

from dataclasses import dataclass
from html import escape

FONT = '"Helvetica Neue", Helvetica, Arial, sans-serif'
MONO = '"SF Mono", Menlo, Consolas, "DejaVu Sans Mono", monospace'
LABEL_SIZE = 11  # arrow labels and notes: legible at the A4 text width
INK = "#1a1a19"
INK2 = "#52514e"
INK3 = "#6f6e69"
RULE = "#c9c8c1"
SURFACE = "#ffffff"
TINT = "#f5f4f0"

# kind -> (fill, stroke, title colour). Blue and orange are the port and the C++
# library, as in the charts; boxes that are neither use the other kinds.
KINDS = {
    "default": ("#ffffff", "#8f8e88", INK),
    "blue": ("#e6f0fc", "#2a78d6", INK),
    "orange": ("#fdeee6", "#eb6834", INK),
    "aqua": ("#e3f5ee", "#1baf7a", INK),
    "pink": ("#fcecf3", "#e87ba4", INK),
    "violet": ("#ecebfa", "#6250d6", INK),
    "yellow": ("#fdf3d9", "#c98500", INK),
    "muted": ("#f5f4f0", "#b5b4ad", INK2),
    "dark": ("#1a1a19", "#1a1a19", "#ffffff"),
}


def _text_width(s: str, size: float) -> float:
    """Rough width of a Helvetica string (good enough for label plates)."""
    narrow = sum(c in "iljtf.,:;'| " for c in s)
    wide = sum(c in "mwMW" for c in s)
    return size * (0.56 * (len(s) - narrow - wide) + 0.28 * narrow + 0.85 * wide)


@dataclass
class _Box:
    x: float
    y: float
    w: float
    h: float

    def side(self, name: str, t: float = 0.5) -> tuple[float, float]:
        if name == "l":
            return (self.x, self.y + self.h * t)
        if name == "r":
            return (self.x + self.w, self.y + self.h * t)
        if name == "t":
            return (self.x + self.w * t, self.y)
        if name == "b":
            return (self.x + self.w * t, self.y + self.h)
        raise ValueError(name)


class Diagram:
    def __init__(self, width: int, height: int, title: str = "") -> None:
        self.w, self.h, self.title = width, height, title
        self._back: list[str] = []
        self._mid: list[str] = []
        self._front: list[str] = []
        self._boxes: dict[str, _Box] = {}
        self._markers: set[str] = set()

    # ----- containers and boxes -------------------------------------------
    def group(self, x, y, w, h, label: str, *, fill: str = TINT, stroke: str = "none",
              dashed: bool = False) -> None:
        dash = ' stroke-dasharray="5 4"' if dashed else ""
        self._back.append(
            f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="10" fill="{fill}" stroke="{stroke}"{dash}/>'
            f'<text x="{x + 12}" y="{y + 19}" font-size="11" font-weight="700" '
            f'letter-spacing="0.5" fill="{INK3}">{escape(label.upper())}</text>'
        )

    def box(self, key: str, x, y, w, h, title: str, sub: str | None = None, *,
            kind: str = "default", title_size: float = 12.5, sub_size: float = 11,
            dashed: bool = False, stack: bool = False, mono: bool = False) -> None:
        """A rounded box with a bold title and optional sub-lines ('\\n' splits lines).

        dashed: dotted outline (things outside the main subject); stack: two
        offset copies behind the box (several instances); mono: the title in
        a monospace face (crate and file names).
        """
        fill, stroke, tcol = KINDS[kind]
        self._boxes[key] = _Box(x, y, w, h)
        sub_lines = sub.split("\n") if sub else []
        title_lines = title.split("\n")
        line_h_t, line_h_s = title_size * 1.22, sub_size * 1.28
        block = len(title_lines) * line_h_t + len(sub_lines) * line_h_s + (3 if sub_lines else 0)
        ty = y + (h - block) / 2 + title_size * 0.92
        dash = ' stroke-dasharray="3 3"' if dashed else ""
        out = []
        if stack:
            for off in (8, 4):
                out.append(f'<rect x="{x + off}" y="{y - off}" width="{w}" height="{h}" rx="6" '
                           f'fill="{fill}" stroke="{stroke}" stroke-width="1.2"/>')
        out.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="{fill}" '
                   f'stroke="{stroke}" stroke-width="1.2"{dash}/>')
        cx = x + w / 2
        face = f' font-family=\'{MONO}\'' if mono else ""
        for line in title_lines:
            out.append(f'<text x="{cx}" y="{ty:.1f}" text-anchor="middle" font-size="{title_size}" '
                       f'font-weight="700" fill="{tcol}"{face}>{escape(line)}</text>')
            ty += line_h_t
        ty += 3 - (title_size - sub_size) * 0.2
        scol = "#d9d8d2" if kind == "dark" else INK2
        for line in sub_lines:
            out.append(f'<text x="{cx}" y="{ty:.1f}" text-anchor="middle" font-size="{sub_size}" '
                       f'fill="{scol}">{escape(line)}</text>')
            ty += line_h_s
        self._mid.append("".join(out))

    def text(self, x, y, s: str, *, size: float = LABEL_SIZE, anchor: str = "start",
             color: str = INK2, weight: int = 400, italic: bool = False) -> None:
        style = ' font-style="italic"' if italic else ""
        for i, line in enumerate(s.split("\n")):
            self._front.append(
                f'<text x="{x}" y="{y + i * size * 1.3:.1f}" text-anchor="{anchor}" font-size="{size}" '
                f'font-weight="{weight}" fill="{color}"{style}>{escape(line)}</text>')

    def diamond(self, key: str, cx, cy, w, h, title: str, *, kind: str = "default",
                title_size: float = 12) -> None:
        """A decision node; its sides l/r/t/b are the four corners."""
        fill, stroke, tcol = KINDS[kind]
        self._boxes[key] = _Box(cx - w / 2, cy - h / 2, w, h)
        pts = f"{cx},{cy - h / 2} {cx + w / 2},{cy} {cx},{cy + h / 2} {cx - w / 2},{cy}"
        self._mid.append(
            f'<polygon points="{pts}" fill="{fill}" stroke="{stroke}" stroke-width="1.2"/>'
            f'<text x="{cx}" y="{cy + title_size * 0.35:.1f}" text-anchor="middle" '
            f'font-size="{title_size}" font-weight="700" fill="{tcol}">{escape(title)}</text>')

    def swatch(self, x, y, label: str, *, kind: str = "default", dashed: bool = False,
               size: float = LABEL_SIZE) -> float:
        """A legend entry (small box + label); returns the x after it."""
        fill, stroke, _ = KINDS[kind]
        dash = ' stroke-dasharray="3 3"' if dashed else ""
        self._front.append(
            f'<rect x="{x}" y="{y - 9}" width="16" height="11" rx="2.5" fill="{fill}" '
            f'stroke="{stroke}" stroke-width="1.2"{dash}/>'
            f'<text x="{x + 22}" y="{y}" font-size="{size}" fill="{INK2}">{escape(label)}</text>')
        return x + 22 + _text_width(label, size) + 18

    def line_key(self, x, y, label: str, *, dashed: bool = False, size: float = LABEL_SIZE) -> float:
        """A legend entry for an arrow style; returns the x after it."""
        dash = ' stroke-dasharray="5 4"' if dashed else ""
        self._front.append(
            f'<path d="M{x},{y - 4} L{x + 26},{y - 4}" stroke="{INK2}" stroke-width="1.3"{dash} '
            f'marker-end="url(#ah-{INK2.strip("#")})"/>'
            f'<text x="{x + 34}" y="{y}" font-size="{size}" fill="{INK2}">{escape(label)}</text>')
        return x + 34 + _text_width(label, size) + 18

    # ----- arrows -----------------------------------------------------------
    def arrow(self, a: str, b: str, label: str | None = None, *, sa: str = "r", sb: str = "l",
              ta: float = 0.5, tb: float = 0.5, route: str = "straight", dashed: bool = False,
              both: bool = False, color: str = INK2, label_dx: float = 0, label_dy: float = -5,
              label_at: float = 0.5, label_anchor: str = "middle",
              plate: str = "none", head: bool = True) -> None:
        """Connect side `sa` of box `a` to side `sb` of box `b`.

        route: "straight", "hv" (horizontal then vertical), "vh", or "hvh"/"vhv"
        (three segments with the turn half-way).
        """
        x1, y1 = self._boxes[a].side(sa, ta)
        x2, y2 = self._boxes[b].side(sb, tb)
        if route == "straight":
            pts = [(x1, y1), (x2, y2)]
        elif route == "hv":
            pts = [(x1, y1), (x2, y1), (x2, y2)]
        elif route == "vh":
            pts = [(x1, y1), (x1, y2), (x2, y2)]
        elif route == "hvh":
            xm = (x1 + x2) / 2
            pts = [(x1, y1), (xm, y1), (xm, y2), (x2, y2)]
        elif route == "vhv":
            ym = (y1 + y2) / 2
            pts = [(x1, y1), (x1, ym), (x2, ym), (x2, y2)]
        else:
            raise ValueError(route)
        self.polyline(pts, label, dashed=dashed, both=both, color=color, label_dx=label_dx,
                      label_dy=label_dy, label_at=label_at, label_anchor=label_anchor,
                      plate=plate, head=head)

    def polyline(self, pts, label: str | None = None, *, dashed: bool = False, both: bool = False,
                 color: str = INK2, label_dx: float = 0, label_dy: float = -5,
                 label_at: float = 0.5, label_anchor: str = "middle",
                 plate: str = "none", head: bool = True, label_seg: int | None = None) -> None:
        """A line through `pts` with an arrowhead at the end (and the start if `both`).

        The label sits on the longest segment (or segment `label_seg`) at
        fraction `label_at`, offset by (label_dx, label_dy).
        """
        d = "M" + " L".join(f"{x:.1f},{y:.1f}" for x, y in pts)
        dash = ' stroke-dasharray="5 4"' if dashed else ""
        mid = "ah-" + color.strip("#")
        start = f' marker-start="url(#{mid}-s)"' if both else ""
        end = f' marker-end="url(#{mid})"' if head else ""
        self._mid.append(f'<path d="{d}" fill="none" stroke="{color}" stroke-width="1.3"{dash} '
                         f'{end}{start}/>')
        self._markers.add(color)
        if label:
            # Label position: along the longest segment.
            segs = list(zip(pts[:-1], pts[1:]))
            if label_seg is not None:
                (ax, ay), (bx, by) = segs[label_seg]
            else:
                (ax, ay), (bx, by) = max(segs, key=lambda s: abs(s[1][0] - s[0][0]) + abs(s[1][1] - s[0][1]))
            lx, ly = ax + (bx - ax) * label_at + label_dx, ay + (by - ay) * label_at + label_dy
            lines = label.split("\n")
            for i, line in enumerate(lines):
                yy = ly + (i - (len(lines) - 1)) * LABEL_SIZE * 1.25
                # A surface-coloured plate keeps the label readable where it crosses a line.
                tw = _text_width(line, LABEL_SIZE) + 6
                x0 = {"middle": lx - tw / 2, "start": lx - 3, "end": lx - tw + 3}[label_anchor]
                if plate != "none":
                    self._front.append(
                        f'<rect x="{x0:.1f}" y="{yy - 10:.1f}" width="{tw:.1f}" height="13.5" '
                        f'fill="{plate}"/>')
                self._front.append(
                    f'<text x="{lx:.1f}" y="{yy:.1f}" text-anchor="{label_anchor}" '
                    f'font-size="{LABEL_SIZE}" fill="{INK2}">{escape(line)}</text>')

    # ----- output -----------------------------------------------------------
    def svg(self) -> str:
        markers = []
        for color in sorted(self._markers | {INK2}):
            mid = "ah-" + color.strip("#")
            markers.append(
                f'<marker id="{mid}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" '
                f'markerHeight="7" orient="auto"><path d="M0,1 L9,5 L0,9 z" fill="{color}"/></marker>'
                f'<marker id="{mid}-s" viewBox="0 0 10 10" refX="1" refY="5" markerWidth="7" '
                f'markerHeight="7" orient="auto"><path d="M10,1 L1,5 L10,9 z" fill="{color}"/></marker>')
        title = f"<title>{escape(self.title)}</title>" if self.title else ""
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {self.w} {self.h}" '
            f'width="{self.w}" height="{self.h}" font-family=\'{FONT}\' role="img">{title}'
            f'<defs>{"".join(markers)}</defs>'
            f'<rect width="{self.w}" height="{self.h}" fill="{SURFACE}"/>'
            + "".join(self._back) + "".join(self._mid) + "".join(self._front) + "</svg>\n")

    def save(self, path) -> None:
        with open(path, "w", encoding="utf-8") as f:
            f.write(self.svg())
