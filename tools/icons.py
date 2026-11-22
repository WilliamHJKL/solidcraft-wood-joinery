#!/usr/bin/env python3
"""Draws the command icons: plugin/icons/<entry>.svg, one per entry of plugin/plugin.json.

Usage: tools/icons.py

SolidCraft 0.44 draws a plugin command with the SVG its input schema names in
"x-solidcraft-icon". The file must be in the app's icon subset: a 64 x 64 view box, absolute
M L Z paths, and only the colours of the app's icon palette, so the icon follows the light and
dark themes. Each icon here is a few boards in an isometric view, made of prisms: the boards in
grey, the part that makes the joint in blue, as the app's own icons mark what a tool changes.

The prisms are drawn in the order given, so each list goes from the back to the front.
"""
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "plugin" / "icons"

INK = "#34465b"
STROKE = 'stroke="#34465b" stroke-width="2.2" stroke-linejoin="round" stroke-linecap="round"'
# Top, left (+y) and right (+x) faces, from the palette's passive and active roles.
GREY = ("#e2e8ef", "#b8c3cf", "#8e9faf")
BLUE = ("#b9ddf8", "#328bd3", "#236ead")


def project(p):
    x, y, z = p
    return ((x - y) * math.cos(math.pi / 6), (x + y) * 0.5 - z)


class Prism:
    """A convex polygon `poly` in the plane of the two axes other than `axis`, from `lo` to `hi` along `axis`."""

    def __init__(self, poly, axis, lo, hi, colours=GREY):
        self.colours = colours
        others = [a for a in range(3) if a != axis]

        def point(uv, w):
            p = [0.0, 0.0, 0.0]
            p[others[0]], p[others[1]], p[axis] = uv[0], uv[1], w
            return tuple(p)

        bottom = [point(uv, lo) for uv in poly]
        top = [point(uv, hi) for uv in poly]
        n = len(poly)
        self.faces = [bottom[::-1], top]
        self.faces += [[bottom[i], bottom[(i + 1) % n], top[(i + 1) % n], top[i]] for i in range(n)]

    def visible(self):
        out = []
        for face in self.faces:
            a, b, c = face[0], face[1], face[2]
            u = [b[i] - a[i] for i in range(3)]
            v = [c[i] - a[i] for i in range(3)]
            normal = (u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0])
            # The viewer looks from (1, 1, 1): a face turned that way is seen.
            if sum(normal) <= 1e-9:
                continue
            length = math.sqrt(sum(c * c for c in normal))
            nx, ny, nz = (c / length for c in normal)
            shade = 0 if nz >= max(nx, ny) else (1 if ny >= nx else 2)
            out.append((face, self.colours[shade]))
        return out


def box(x0, y0, z0, x1, y1, z1, colours=GREY):
    return Prism([(x0, y0), (x1, y0), (x1, y1), (x0, y1)], 2, z0, z1, colours)


def cylinder(cu, cv, r, lo, hi, axis=2, colours=BLUE, sides=12):
    """A cylinder along `axis`, centred on (cu, cv) in the other two axes, in their order."""
    poly = [(cu + r * math.cos(2 * math.pi * i / sides), cv + r * math.sin(2 * math.pi * i / sides)) for i in range(sides)]
    return Prism(poly, axis, lo, hi, colours)


def svg(title, prisms):
    faces = [(project_face(f), c) for p in prisms for f, c in p.visible()]
    xs = [x for f, _ in faces for x, _ in f]
    ys = [y for f, _ in faces for _, y in f]
    size = 56.0
    scale = size / max(max(xs) - min(xs), max(ys) - min(ys))
    ox = 32 - (max(xs) + min(xs)) / 2 * scale
    oy = 32 - (max(ys) + min(ys)) / 2 * scale
    lines = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">', f"  <title>{title}</title>"]
    for face, colour in faces:
        d = "M" + "L".join(f"{ox + x * scale:.2f} {oy + y * scale:.2f}" for x, y in face) + "Z"
        lines.append(f'  <path d="{d}" fill="{colour}" {STROKE}/>')
    lines.append("</svg>")
    return "\n".join(lines) + "\n"


def project_face(face):
    return [project(p) for p in face]


ICONS = {
    # "Update joints": a post and its rail put together, two blue arrows turning about the joint.
    "update-joints": ("Update joints", [
        box(0, 0, 0, 7, 7, 32),
        box(7, 0, 14, 30, 7, 26),
        box(10, 10, 30, 26, 14, 33, BLUE),
        box(22, 2, 30, 26, 14, 33, BLUE),
        box(0, -6, 4, 4, 4, 7, BLUE),
    ]),
    # "Edit joint": a post and a rail whose tenon (blue) is half out of the mortise, to be changed.
    "edit-joint": ("Edit joint", [
        box(0, 0, 0, 7, 7, 32),
        box(4, 2, 16, 14, 5, 24, BLUE),
        box(14, 0, 14, 34, 7, 26),
    ]),
    # A post, and a rail ending in its tenon (blue), pulled out of the mortise.
    "mortise-tenon": ("Mortise and tenon", [
        box(0, 0, 0, 7, 7, 32),
        box(13, 2, 16, 21, 5, 24, BLUE),
        box(21, 0, 14, 38, 7, 26),
    ]),
    # Two boards end to end, pulled apart, and the loose tenon (blue) between them.
    "loose-tenon": ("Loose tenon", [
        box(0, 0, 0, 12, 7, 10),
        box(14, 2, 3, 22, 5, 8, BLUE),
        box(24, 0, 0, 36, 7, 10),
    ]),
    # A board with a groove along it; the groove's floor in blue.
    "groove": ("Groove", [
        box(0, 0, 0, 30, 20, 3),
        box(0, 0, 3, 30, 6, 6),
        box(0, 6, 3, 30, 14, 3.01, BLUE),
        box(0, 14, 3, 30, 20, 6),
    ]),
    # Two boards edge to edge, pulled apart: the tongue (blue) out of the back one.
    "tongue-groove": ("Tongue and groove", [
        box(0, 0, 0, 26, 9, 8),
        box(0, 9, 2, 26, 15, 6, BLUE),
        box(0, 21, 0, 26, 30, 8),
    ]),
    # A side with a housing across it, and the shelf (blue) above it.
    "housing": ("Housing joint", [
        box(0, 0, 0, 30, 20, 3),
        box(0, 0, 3, 12, 20, 6),
        box(12, 0, 3, 17, 20, 3.01, BLUE),
        box(17, 0, 3, 30, 20, 6),
        box(12, 2, 12, 17, 18, 28, BLUE),
    ]),
    # Two boards crossing, the upper one (blue) lifted: each loses half its thickness.
    "half-lap": ("Half lap", [
        box(0, 10, 0, 10, 18, 6),
        box(10, 10, 0, 18, 18, 3),
        box(18, 10, 0, 32, 18, 6),
        box(10, 0, 16, 18, 10, 22, BLUE),
        box(10, 10, 19, 18, 18, 22, BLUE),
        box(10, 18, 16, 18, 28, 22, BLUE),
    ]),
    # Two boards meeting at 45 degrees, the second in blue.
    "miter": ("Miter joint", [
        Prism([(0, 0), (24, 0), (16, 8), (0, 8)], 2, 0, 5),
        Prism([(26, 2), (26, 30), (18, 30), (18, 10)], 2, 0, 5, BLUE),
    ]),
    # Two boards end to end, the second (blue) lifted off the long slope between them.
    "scarf": ("Scarf joint", [
        Prism([(0, 0), (26, 0), (10, 10), (0, 10)], 1, 0, 5),
        Prism([(31, 4), (40, 4), (40, 14), (15, 14)], 1, 0, 5, BLUE),
    ]),
    # A corner: the fingers of one board (grey) between those of the other (blue).
    "box-joint": ("Box joints", [
        box(0, 6, 0, 6, 26, 5),
        box(0, 0, 0, 6, 6, 5),
        box(6, 0, 0, 26, 6, 5, BLUE),
        box(0, 0, 5, 6, 6, 10, BLUE),
        box(6, 0, 5, 26, 6, 10, BLUE),
        box(0, 6, 5, 6, 26, 10),
        box(0, 0, 10, 6, 6, 15),
        box(0, 6, 10, 6, 26, 15),
        box(6, 0, 10, 26, 6, 15, BLUE),
    ]),
    # A board ending in two tails (blue) that flare out.
    "dovetail": ("Dovetails", [
        box(0, 0, 0, 16, 24, 6),
        Prism([(16, 2), (25, 0), (25, 10), (16, 8)], 2, 0, 6, BLUE),
        Prism([(16, 16), (25, 14), (25, 24), (16, 22)], 2, 0, 6, BLUE),
    ]),
    # Two boards end to end, pulled apart, and two dowels (blue) between them.
    "dowels": ("Dowels", [
        box(0, 0, 0, 9, 24, 6),
        cylinder(6, 3, 2, 7, 19, axis=0),
        cylinder(18, 3, 2, 7, 19, axis=0),
        box(20, 0, 0, 29, 24, 6),
    ]),
    # Two boards edge to edge, pulled apart, and a biscuit (blue) between them.
    "biscuits": ("Biscuits", [
        box(0, 0, 0, 28, 9, 5),
        Prism([(14 + 9 * math.cos(a), 13 + 5 * math.sin(a)) for a in [2 * math.pi * i / 16 for i in range(16)]], 2, 2, 3, BLUE),
        box(0, 17, 0, 28, 26, 5),
    ]),
    # Two boards, one on the other, and a screw (blue) above them.
    "screws": ("Screws", [
        box(0, 0, 0, 26, 18, 6),
        box(0, 0, 6, 26, 18, 12),
        cylinder(13, 9, 1.2, 14, 24),
        cylinder(13, 9, 3.5, 24, 26),
    ]),
}


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("*.svg"):
        old.unlink()
    for entry, (title, prisms) in ICONS.items():
        (OUT / f"{entry}.svg").write_text(svg(title, prisms), encoding="utf-8")
    print(f"{len(ICONS)} icons in {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
