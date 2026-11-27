#!/usr/bin/env python3
"""Shape check of every joint: the same boards, turned any way, make the same joint.

Usage: tests/shapes.py /path/to/solidcraft-cli [joint ...]

Each joint starts from a setup that works (the same boards as tests/check.py). The setup is then
turned to 12 orientations: the three axes swapped in all 6 ways, each also mirrored across X, so
boards lie flat, stand on edge or stand upright along X, Y or Z. A joint is about the boards, not
about the world's axes, so in every orientation:

- the command must run;
- each body must lose or gain the volume it does in the first orientation;
- there must be as many bodies (a cut must never leave a board in pieces);
- the bodies must overlap as much as they do in the first orientation (usually not at all).

It prints one line per orientation that differs, and a summary per joint.
"""
import json
import subprocess
import sys
import tempfile
from itertools import permutations
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PLUGIN = ROOT / "dist" / "wood-joinery"


def box(name, corner, size):
    return (name, corner, size)


# joint: (boards, params). The boards are (name, corner, size); the first is made first.
SETUPS = {
    "mortise-tenon": ([box("Stile", [0, 0, 0], [35, 35, 830]), box("Rail", [35, 0, 400], [360, 18, 45])], {"rail": "Rail", "stile": "Stile"}),
    "loose-tenon": ([box("Stile", [0, 0, 0], [35, 35, 830]), box("Rail", [35, 0, 400], [360, 18, 45])], {"first": "Rail", "second": "Stile"}),
    "dowels": ([box("Stile", [0, 0, 0], [35, 35, 830]), box("Rail", [35, 0, 400], [360, 18, 45])], {"first": "Rail", "second": "Stile"}),
    "groove": ([box("Stile", [0, 0, 0], [35, 35, 830])], {"body": "Stile", "face": "+x"}),
    "half-lap": ([box("A", [0, 0, 0], [300, 45, 18]), box("B", [100, -100, 0], [45, 300, 18])], {"first": "A", "second": "B"}),
    "box-joint": ([box("Side", [18, 0, 0], [300, 18, 100]), box("Front", [0, 0, 0], [18, 300, 100])], {"first": "Side", "second": "Front"}),
    "dovetail": ([box("Side", [18, 0, 0], [300, 18, 100]), box("Front", [0, 0, 0], [18, 300, 100])], {"first": "Side", "second": "Front"}),
    "housing": ([box("Side", [0, 0, 0], [18, 250, 400]), box("Shelf", [18, 0, 200], [300, 250, 18])], {"first": "Shelf", "second": "Side"}),
    "screws": ([box("Side", [0, 0, 0], [18, 250, 400]), box("Shelf", [18, 0, 200], [300, 250, 18])], {"first": "Side", "second": "Shelf"}),
    "miter": ([box("A", [0, 0, 0], [300, 18, 100]), box("B", [0, 0, 0], [18, 300, 100])], {"first": "A", "second": "B"}),
    "tongue-groove": ([box("A", [0, 0, 0], [600, 100, 18]), box("B", [0, 100, 0], [600, 100, 18])], {"first": "A", "second": "B"}),
    "biscuits": ([box("A", [0, 0, 0], [600, 100, 18]), box("B", [0, 100, 0], [600, 100, 18])], {"first": "A", "second": "B"}),
    "scarf": ([box("A", [0, 0, 0], [400, 45, 18]), box("B", [256, 0, 0], [400, 45, 18])], {"first": "A", "second": "B"}),
}


def orientations():
    """(label, map) pairs: map turns a box (corner, size) into its turned copy."""
    for p in permutations(range(3)):
        for mirror in (False, True):
            def turn(corner, size, p=p, mirror=mirror):
                c = [corner[p[i]] for i in range(3)]
                s = [size[p[i]] for i in range(3)]
                if mirror:
                    c[0] = -(c[0] + s[0])
                return c, s
            label = "".join("xyz"[i] for i in p) + (" mirrored" if mirror else "")
            yield label, p, mirror, turn


def face(text, p, mirror):
    """A face such as `+x`, as it is after the turn."""
    sign, axis = text[0], "xyz".index(text[1])
    new = p.index(axis)
    if mirror and new == 0:
        sign = "-" if sign == "+" else "+"
    return sign + "xyz"[new]


def main():
    args = sys.argv[1:]
    if not args:
        raise SystemExit(__doc__)
    cli, only = str(Path(args[0]).resolve()), args[1:]
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)

        def run(steps):
            script, out = work / "s.json", work / "o.json"
            script.write_text(json.dumps(steps))
            r = subprocess.run([cli, "run", str(script), "--results", "--json-out", str(out)], capture_output=True, text=True)
            if r.returncode:
                text = " ".join((r.stdout + r.stderr).split())
                return None, text[text.find("Wood joinery") :][:220] if "Wood joinery" in text else text[-220:]
            return json.loads(out.read_text()), None

        approval = run([{"command": "plugin.inspect", "params": {"path": str(PLUGIN)}}])[0][0]["approval"]
        failed = False
        for joint, (boards, params) in SETUPS.items():
            if only and joint not in only:
                continue
            reference, bad = None, []
            for label, p, mirror, turn in orientations():
                steps = []
                for i, (name, corner, size) in enumerate(boards):
                    c, s = turn(corner, size)
                    q = {"corner": c, "length": s[0], "width": s[1], "height": s[2], "body_name": name}
                    if i:
                        q["operation"] = "new"
                    steps.append({"command": "solid.box", "params": q})
                # A board field picks: a script gives each board as a list of one name.
                given = {k: [v] if k in ("rail", "stile", "first", "second", "body") and isinstance(v, str) else v for k, v in params.items()}
                if "face" in given:
                    given["face"] = face(given["face"], p, mirror)
                steps += [
                    {"command": "inspect.measure", "params": {}},
                    {"command": "plugin.run", "params": {"path": str(PLUGIN), "approval": approval, "entry": joint, "params": given}},
                    {"command": "inspect.measure", "params": {}},
                    {"command": "inspect.interference", "params": {}},
                ]
                got, error = run(steps)
                if error:
                    bad.append(f"{label}: refused: {error}")
                    continue
                before = {b["name"]: b["volume_mm3"] for b in got[len(boards)]["bodies"]}
                after = {b["name"]: b["volume_mm3"] for b in got[-2]["bodies"]}
                change = {n: round(v - before.get(n, 0), 1) for n, v in after.items()}
                overlap = round(sum(i["volume_mm3"] for i in got[-1].get("interferences", [])), 1)
                shape = (change, overlap)
                if reference is None:
                    reference = shape
                    print(f"{joint}: {label}: {', '.join(f'{n} {v:+.1f}' for n, v in change.items())} mm3, overlap {overlap} mm3")
                    continue
                diffs = []
                if change.keys() != reference[0].keys():
                    diffs.append(f"bodies {sorted(change)} instead of {sorted(reference[0])}")
                else:
                    diffs += [f"{n} {change[n]:+.1f} instead of {reference[0][n]:+.1f}" for n in change if abs(change[n] - reference[0][n]) > 0.5]
                if abs(overlap - reference[1]) > 0.5:
                    diffs.append(f"overlap {overlap} instead of {reference[1]}")
                if diffs:
                    bad.append(f"{label}: " + "; ".join(diffs))
            for line in bad:
                print(f"  DIFFERS  {joint} {line}")
            print(f"{'FAIL' if bad else 'PASS'}  {joint}: {12 - len(bad)} of 12 orientations make the same joint")
            failed |= bool(bad)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
