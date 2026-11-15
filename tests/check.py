#!/usr/bin/env python3
"""End-to-end check of the packed plugin (dist/wood-joinery) in a real SolidCraft.

Usage: tests/check.py /path/to/solidcraft-cli [--keep DIR]

It builds a 35 x 35 x 830 mm stile and a 360 x 18 x 45 mm rail, runs each joint, and compares the
measured volumes with values worked out by hand: mortise and tenon, grooves, half laps, dowels,
box joints, loose tenons, housings, miters, tongue and groove, scarfs, dovetails and screws. It also checks undo and redo, refusals, a saved design reopened without the
plugin, and writes a picture. With --keep, the files stay in DIR.
"""
import json
import math
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PLUGIN = ROOT / "dist" / "wood-joinery"
STILE, RAIL = 35 * 35 * 830, 360 * 18 * 45


def box(name, corner, size, first=False):
    params = {"corner": corner, "length": size[0], "width": size[1], "height": size[2], "body_name": name}
    if not first:
        params["operation"] = "new"
    return {"command": "solid.box", "params": params}


def boards(rail_corner, rail_size=(360, 18, 45)):
    return [box("Stile", [0, 0, 0], (35, 35, 830), first=True), box("Rail", rail_corner, rail_size)]


MEASURE = {"command": "inspect.measure", "params": {}}


def select(*items):
    """Selects bodies (a name) and faces (a body name and a face index), in that order."""
    picked = [{"type": "body", "name": i} if isinstance(i, str) else {"type": "face", "body": i[0], "index": i[1]} for i in items]
    return {"command": "select.set", "params": {"items": picked}}


class Check:
    def __init__(self, binary, work):
        self.binary, self.work, self.failed, self.n = binary, work, False, 0

    def cli(self, name, commands, *extra, expect_error=False):
        script = self.work / f"{name}.json"
        out = self.work / f"{name}.out.json"
        script.write_text(json.dumps(commands))
        r = subprocess.run([self.binary, "run", str(script), "--results", "--json-out", str(out), *extra], capture_output=True, text=True, cwd=self.work)
        if expect_error:
            return r.returncode, r.stdout + r.stderr
        if r.returncode != 0:
            raise SystemExit(f"{name}: solidcraft-cli failed\n{r.stdout}\n{r.stderr}")
        return json.loads(out.read_text())

    def report(self, ok, text):
        self.failed |= not ok
        print(f"{'PASS' if ok else 'FAIL'}  {text}")

    def run(self, entry, params):
        return {"command": "plugin.run", "params": {"path": str(PLUGIN), "approval": self.approval, "entry": entry, "params": params}}

    def joint(self, params):
        return self.run("mortise-tenon", params)

    def groove(self, name, text, params, removed):
        """Volume a groove removes from the lone stile, in mm3."""
        results = self.cli(name, [box("Stile", [0, 0, 0], (35, 35, 830), first=True), self.run("groove", params), MEASURE])
        loss = STILE - results[-1]["total"]["volume_mm3"]
        self.report(abs(loss - removed) < 0.5 and len(results[-1]["bodies"]) == 1, f"{text}: {loss:.1f} mm3 removed (expected {removed})")

    def refuse_groove(self, name, text, params, reason):
        code, output = self.cli(name, [box("Stile", [0, 0, 0], (35, 35, 830), first=True), self.run("groove", params)], expect_error=True)
        self.report(code != 0 and reason in output, f"refused, {text}: {'ok' if reason in output else output.strip()[-300:]}")

    def change(self, name, text, setup, entry, params, expected):
        """Runs one entry on `setup` and compares each body's change of volume, in mm3, with `expected`.

        `expected` maps a body name to its change; a body the entry creates is named with its volume.
        """
        results = self.cli(name, [*setup, MEASURE, self.run(entry, params), MEASURE])
        before = {b["name"]: b["volume_mm3"] for b in results[len(setup)]["bodies"]}
        after = {b["name"]: b["volume_mm3"] for b in results[-1]["bodies"]}
        got = {n: v - before.get(n, 0) for n, v in after.items()}
        # A body the entry left alone is not listed.
        got = {n: v for n, v in got.items() if n in expected or abs(v) >= 0.5}
        ok = got.keys() == expected.keys() and all(abs(got[n] - expected[n]) < 0.5 for n in expected)
        shown = ", ".join(f"{n} {v:+.1f}" for n, v in got.items())
        self.report(ok, f"{text}: {shown} mm3" + ("" if ok else f" (expected {expected})"))
        return results

    def refuse_entry(self, name, text, setup, entry, params, reason):
        code, output = self.cli(name, [*setup, self.run(entry, params)], expect_error=True)
        self.report(code != 0 and reason in output, f"refused, {text}: {'ok' if reason in output else output.strip()[-300:]}")

    def volumes(self, name, setup, params, *extra):
        """Rail gain and stile loss, in mm3, after one joint."""
        results = self.cli(name, [*setup, self.joint(params), MEASURE], *extra)
        bodies = {b["name"]: b["volume_mm3"] for b in results[-1]["bodies"]}
        return bodies.get("Rail", 0) - RAIL, STILE - bodies.get("Stile", 0), len(bodies), results

    def expect(self, name, text, setup, params, tenon, mortise):
        gain, loss, count, _ = self.volumes(name, setup, params)
        ok = abs(gain - tenon) < 0.5 and abs(loss - mortise) < 0.5 and count == 2
        self.report(ok, f"{text}: tenon {gain:.1f} mm3 (expected {tenon}), mortise {loss:.1f} mm3 (expected {mortise}), {count} bodies")

    def refuse(self, name, text, setup, params, reason):
        code, output = self.cli(name, [*setup, self.joint(params)], expect_error=True)
        self.report(code != 0 and reason in output, f"refused, {text}: {'ok' if reason in output else output.strip()[-300:]}")


def main():
    args = sys.argv[1:]
    keep = None
    if "--keep" in args:
        i = args.index("--keep")
        keep = Path(args[i + 1]).resolve()
        del args[i : i + 2]
    if len(args) != 1:
        raise SystemExit(__doc__)
    if not (PLUGIN / "plugin.wasm").exists():
        raise SystemExit("run ./build.sh first")
    with tempfile.TemporaryDirectory() as tmp:
        work = keep or Path(tmp)
        work.mkdir(parents=True, exist_ok=True)
        # The CLI runs in the work folder, so a relative path to it must be made absolute first.
        binary = Path(args[0]).resolve()
        if not binary.is_file():
            raise SystemExit(f"no solidcraft-cli at {binary}")
        c = Check(str(binary), work)

        # 1. SolidCraft reads the manifest and the component without running anything.
        inspected = c.cli("inspect", [{"command": "plugin.inspect", "params": {"path": str(PLUGIN)}}])
        c.approval = inspected[0]["approval"]
        c.report(inspected[0]["manifest"]["id"] == "wood-joinery", "plugin.inspect accepts the manifest and the component")
        # The .zip is what Preferences > Plugins > Install plugin... takes. A dry run writes nothing.
        dry = c.cli("install", [{"command": "plugin.install", "params": {"path": str(ROOT / "dist" / "wood-joinery.zip"), "dry_run": True}}])
        c.report(dry[0]["id"] == "wood-joinery" and dry[0]["installed"] is False, "plugin.install accepts the .zip in a dry run")

        # Every form must be one SolidCraft 0.42.0 can draw: its dialog knows numbers, whole numbers,
        # checkboxes, text and `enum` lists, and refuses the whole entry for any other keyword.
        notes = {"type", "title", "description", "default", "unit"}
        allowed = {"number": {"minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum", "multipleOf"},
                   "integer": {"minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"}, "boolean": set(), "string": {"minLength", "maxLength", "enum"}}
        for package in [PLUGIN, *sorted((ROOT / "dist").glob("*/wood-joinery"))]:
            manifest = json.loads((package / "plugin.json").read_text())
            bad = [f"{e['name']}.{k}" for e in manifest["runtime"]["entries"] for k, prop in e["input_schema"]["properties"].items()
                   if prop.get("type") not in allowed or set(prop) - notes - allowed[prop["type"]]]
            bad += [e["name"] for e in manifest["runtime"]["entries"] if set(e["input_schema"]) - {"type", "properties", "required"}]
            c.report(not bad, f"every form of {package.parent.name}/{package.name} can be drawn by SolidCraft 0.42.0" + (f": not {bad}" if bad else ""))

        both = {"rail": "Rail", "stile": "Stile"}
        mid = boards([35, 0, 400])

        # Each translated package installs and runs as the English one does.
        for package in sorted((ROOT / "dist").glob("*/wood-joinery")):
            lang = package.parent.name
            dry = c.cli(f"install_{lang}", [{"command": "plugin.install", "params": {"path": str(ROOT / "dist" / f"wood-joinery-{lang}.zip"), "dry_run": True}}])
            approval = c.cli(f"inspect_{lang}", [{"command": "plugin.inspect", "params": {"path": str(package)}}])[0]["approval"]
            run = {"command": "plugin.run", "params": {"path": str(package), "approval": approval, "entry": "mortise-tenon", "params": both}}
            total = c.cli(f"run_{lang}", [*mid, run, MEASURE])[-1]["total"]["volume_mm3"]
            ok = dry[0]["id"] == "wood-joinery" and abs(total - (STILE + RAIL + 6 * 33 * 25 - 6 * 33 * 27)) < 0.5
            c.report(ok, f"the {lang} package installs in a dry run and makes the same joint ({dry[0]['name']})")

        # 2. The four variants. Tenon 6 thick; classic: 33 wide, 25 long, mortise 27 deep.
        c.expect("classic", "classic", mid, both, 6 * 33 * 25, 6 * 33 * 27)
        # Rail 20 mm under the top of the stile: haunch 9 wide and 6 deep, main tenon 30 wide.
        c.expect("haunched", "haunched", boards([35, 0, 765]), {**both, "variant": "haunched"}, 6 * (30 * 25 + 9 * 6), 6 * (30 * 27 + 9 * 6))
        c.expect("through", "through", mid, {**both, "variant": "through"}, 6 * 33 * 35, 6 * 33 * 35)
        # Rail flush with the top of the stile: tongue 45 wide, right through.
        c.expect("bridle", "bridle", boards([35, 0, 785]), {**both, "variant": "bridle"}, 6 * 45 * 35, 6 * 45 * 35)
        # Sizes given by hand: 8 thick, 20 long, 5 shoulders, 3 of glue relief, 0.25 clearance.
        custom = {**both, "thickness": 8, "length": 20, "shoulder": 5, "glue_relief": 3, "clearance": 0.25}
        c.expect("custom", "sizes set by hand", mid, custom, 8 * 35 * 20, 8.5 * 35.5 * 23)

        # The reply lists the sizes that were used, the defaults included.
        made = c.cli("sizes", [*mid, c.joint({**both, "variant": "haunched"})])[-1]["result"]
        want = {"tenon_thickness_mm": 6, "tenon_length_mm": 25, "tenon_width_mm": 30, "shoulder_mm": 6, "glue_relief_mm": 2, "mortise_depth_mm": 27, "haunch_width_mm": 9, "haunch_depth_mm": 6}
        c.report(all(abs(made.get(k, -1) - v) < 1e-9 for k, v in want.items()), f"the reply lists the sizes used: {({k: made.get(k) for k in want})}")
        # The form sends its shown defaults (glue relief 2, clearance 0, offset 0): same joint.
        c.expect("form_defaults", "the defaults the form shows", mid, {**both, "variant": "classic", "glue_relief": 2, "clearance": 0, "offset": 0}, 6 * 33 * 25, 6 * 33 * 27)
        c.expect("form_through", "a through tenon with the form defaults", mid, {**both, "variant": "through", "glue_relief": 2, "clearance": 0, "offset": 0}, 6 * 33 * 35, 6 * 33 * 35)

        # A translated package sends the value it shows, with the script value in brackets.
        c.expect("fr_value", "a translated choice, à renfort (haunched)", boards([35, 0, 765]), {**both, "variant": "à renfort (haunched)"}, 6 * (30 * 25 + 9 * 6), 6 * (30 * 27 + 9 * 6))
        c.groove("fr_face", "a translated face and direction", {"body": "Stile", "face": "droite (+x)", "along": "y"}, 6 * 8 * 35)
        french = json.loads((ROOT / "dist" / "fr" / "wood-joinery" / "plugin.json").read_text())["runtime"]["entries"][0]["input_schema"]["properties"]["variant"]
        c.report(french["enum"][1] == "à renfort (haunched)" and french["default"] == "classique (classic)", f"the fr package lists translated choices: {french['enum']}")

        # A rail wider than the stile: a 275 mm panel standing on a 250 mm one. The tenon runs over
        # the 250 mm they share, a shoulder (22 / 3) in from each end: 15 long, in a 17 mm mortise.
        panels = [box("Stile", [0, 25, 0], (350, 250, 22), first=True), box("Rail", [0, 12.5, 22], (22, 275, 250))]
        third = 22 / 3
        got = c.change("wide_rail", "a rail wider than the stile", panels, "mortise-tenon", both, {"Rail": third * (250 - 2 * third) * 15, "Stile": -third * (250 - 2 * third) * 17})

        # 3. Other directions: the rail on the -X side of the stile, then a rail along Y.
        c.expect("minus_x", "rail on the other side", boards([-360, 0, 400]), both, 6 * 33 * 25, 6 * 33 * 27)
        c.expect("along_y", "rail along Y", boards([0, 35, 400], (18, 360, 45)), both, 6 * 33 * 25, 6 * 33 * 27)
        # A script may also give each body as a list with one name.
        c.expect("lists", "bodies given as lists", mid, {"rail": ["Rail"], "stile": ["Stile"]}, 6 * 33 * 25, 6 * 33 * 27)
        c.refuse("empty", "empty body name", mid, {"rail": " ", "stile": "Stile"}, "name of one body")

        # 4. One undo removes the whole joint, one redo brings it back.
        steps = c.cli("undo", [*mid, c.joint({**both, "variant": "haunched"}), MEASURE, {"command": "edit.undo", "params": {}}, MEASURE, {"command": "edit.redo", "params": {}}, MEASURE])
        totals = [round(steps[i]["total"]["volume_mm3"]) for i in (3, 5, 7)]
        joined = STILE + RAIL + 6 * (30 * 25 + 9 * 6) - 6 * (30 * 27 + 9 * 6)
        c.report(totals == [joined, STILE + RAIL, joined], f"one undo and one redo: total volumes {totals}")

        # 5. Refusals leave a message that says what to change.
        c.refuse("apart", "rail 10 mm away from the stile", boards([45, 0, 400]), both, "does not lie on a face")
        c.refuse("too_long", "blind tenon as long as the stile", mid, {**both, "length": 34}, "come through")
        c.refuse("bridle_mid", "bridle away from the stile end", mid, {**both, "variant": "bridle"}, "flush with the end")
        c.refuse("same", "the same body twice", mid, {"rail": "Rail", "stile": "Rail"}, "two different bodies")
        c.refuse("unknown", "unknown body", mid, {"rail": "Nope", "stile": "Stile"}, "unknown body name `Nope`. The bodies of this design are: `Stile`, `Rail`")
        c.refuse("variant", "unknown variant", mid, {**both, "variant": "dovetail"}, "classic, haunched, through or bridle")

        # Grooves in the stile. Default: 6 wide, 8 deep, the whole 830 mm.
        c.groove("groove", "groove, default sizes", {"body": "Stile", "face": "+x"}, 6 * 8 * 830)
        # As on a cabinet leg: 6 mm from the edge, stopped 80 mm above the foot.
        c.groove("groove_stopped", "stopped groove in a low face", {"body": "Stile", "face": "-y", "offset": 6, "stop_start": 80}, 6 * 8 * 750)
        c.groove("rabbet", "rabbet at the edge", {"body": "Stile", "face": "+y", "offset": 0, "width": 10, "depth": 5}, 10 * 5 * 830)
        c.groove("dado", "dado across the top", {"body": "Stile", "face": "+z", "width": 12, "depth": 4}, 12 * 4 * 35)
        # The direction can be chosen: across the +X face, along Y, the groove is 35 mm long.
        c.groove("groove_across", "groove across the face, along Y", {"body": "Stile", "face": "+x", "along": "y"}, 6 * 8 * 35)
        c.groove("groove_along", "groove along Z, as the default", {"body": "Stile", "face": "+x", "along": "z"}, 6 * 8 * 830)
        c.refuse_groove("groove_into", "groove along the axis that goes into the board", {"body": "Stile", "face": "+x", "along": "x"}, "Choose Y or Z")
        c.refuse_groove("groove_deep", "groove through the board", {"body": "Stile", "face": "+x", "depth": 35}, "cut through")
        c.refuse_groove("groove_face", "unknown face", {"body": "Stile", "face": "up"}, "+x, -x, +y, -y, +z or -z")

        # Half laps: two 45 x 18 boards that cross flat. Each loses 45 x 45 x 9.
        two = {"first": "A", "second": "B"}
        cross = [box("A", [0, 0, 0], (300, 45, 18), first=True), box("B", [100, -100, 0], (45, 300, 18))]
        c.change("lap", "cross lap", cross, "half-lap", two, {"A": -45 * 45 * 9, "B": -45 * 45 * 9})
        c.change("lap_depth", "lap 6 mm deep, other side", cross, "half-lap", {**two, "depth": 6, "flip": True}, {"A": -45 * 45 * 6, "B": -45 * 45 * 12})
        corner = [box("A", [0, 0, 0], (300, 45, 18), first=True), box("B", [0, 0, 0], (45, 300, 18))]
        c.change("lap_corner", "corner lap", corner, "half-lap", two, {"A": -45 * 45 * 9, "B": -45 * 45 * 9})
        c.refuse_entry("lap_apart", "boards that do not overlap", [cross[0], box("B", [100, 45, 0], (45, 300, 18))], "half-lap", two, "do not overlap")

        # Dowels in the rail end: two of 8 x 40, 20 mm in each board, holes 1 mm deeper.
        rs = {"first": "Rail", "second": "Stile"}
        hole, peg = math.pi * 16 * 21, math.pi * 16 * 40
        c.change("dowels", "two dowels", mid, "dowels", rs, {"Rail": -2 * hole, "Stile": -2 * hole, "Dowel 1": peg, "Dowel 2": peg})
        # A second rail higher up: its dowels are numbered after the first two.
        three = [*mid, box("Rail 2", [35, 0, 600], (360, 18, 45)), c.run("dowels", rs)]
        c.change("dowels_again", "dowels in a second rail", three, "dowels", {"first": "Rail 2", "second": "Stile"}, {"Rail 2": -2 * hole, "Stile": -2 * hole, "Dowel 3": peg, "Dowel 4": peg})
        # One of 6 x 30, holes only.
        small = {**rs, "diameter": 6, "count": 1, "length": 30, "dowel_bodies": False}
        c.change("dowel_holes", "one 6 mm hole pair, no dowel body", mid, "dowels", small, {"Rail": -math.pi * 9 * 16, "Stile": -math.pi * 9 * 16})
        # An 80 mm dowel: 31 mm at most in the 35 mm stile, the other 49 in the rail.
        long = {**rs, "count": 1, "length": 80, "dowel_bodies": False}
        c.change("dowel_long", "long dowel, mostly in the rail", mid, "dowels", long, {"Rail": -math.pi * 16 * 50, "Stile": -math.pi * 16 * 32})
        c.refuse_entry("dowel_apart", "boards that do not touch", boards([45, 0, 400]), "dowels", rs, "do not touch")
        c.refuse_entry("dowel_fat", "dowel as thick as the rail", mid, "dowels", {**rs, "diameter": 17}, "no wood beside")

        # A box joint at a drawer corner: 18 x 100 boards, five divisions of 20, two fingers on the side.
        drawer = [box("Side", [18, 0, 0], (300, 18, 100), first=True), box("Front", [0, 0, 0], (18, 300, 100))]
        sf = {"first": "Side", "second": "Front"}
        c.change("box", "box joint", drawer, "box-joint", sf, {"Side": 2 * 20 * 18 * 18, "Front": -2 * 20 * 18 * 18})
        c.change("box_swap", "box joint starting with a finger", drawer, "box-joint", {**sf, "swap": True}, {"Side": 3 * 20 * 18 * 18, "Front": -3 * 20 * 18 * 18})
        c.change("box_four", "box joint in four divisions", drawer, "box-joint", {**sf, "count": 4}, {"Side": 2 * 25 * 18 * 18, "Front": -2 * 25 * 18 * 18})
        c.refuse_entry("box_apart", "boards that do not touch", [drawer[0], box("Front", [-20, 0, 0], (18, 300, 100))], "box-joint", sf, "does not lie on a face")

        # Boards picked in SolidCraft instead of typed: the first one picked is the first board.
        c.change("pick", "box joint on two selected bodies", [*drawer, select("Side", "Front")], "box-joint", {}, {"Side": 2 * 20 * 18 * 18, "Front": -2 * 20 * 18 * 18})
        c.change("pick_face", "box joint on a selected face and a body", [*drawer, select(("Side", 0), "Front")], "box-joint", {}, {"Side": 2 * 20 * 18 * 18, "Front": -2 * 20 * 18 * 18})
        c.change("pick_tenon", "mortise and tenon on the selection", [*mid, select("Rail", "Stile")], "mortise-tenon", {}, {"Rail": 6 * 33 * 25, "Stile": -6 * 33 * 27})
        c.change("pick_lap", "half lap on the selection", [*cross, select("A", "B")], "half-lap", {}, {"A": -45 * 45 * 9, "B": -45 * 45 * 9})
        c.refuse_entry("pick_one", "one body selected", [*drawer, select("Side")], "box-joint", {}, "select two bodies")
        c.refuse_entry("pick_none", "nothing selected", drawer, "dowels", {}, "select two bodies")
        # A groove in a picked face. Find the +X face of the stile by its normal first.
        stile = box("Stile", [0, 0, 0], (35, 35, 830), first=True)
        faces = c.cli("faces", [stile, *[x for i in range(6) for x in (select(("Stile", i)), {"command": "selection.get", "params": {}})]])
        plus_x = next(i for i in range(6) if faces[2 + 2 * i]["items"][0]["surface"]["normal"] == [1.0, 0.0, 0.0])
        c.change("pick_groove", "groove in a selected face", [stile, select(("Stile", plus_x))], "groove", {}, {"Stile": -6 * 8 * 830})
        c.change("pick_board", "groove in the selected board, face from the list", [stile, select("Stile")], "groove", {"face": "+x"}, {"Stile": -6 * 8 * 830})
        c.refuse_entry("pick_no_face", "groove with nothing selected", [stile], "groove", {}, "select one flat face")

        # Align: a side left 40 mm away, 25 mm in from the corner and 7 mm too high is moved into place.
        loose = [box("Side", [58, 25, 7], (300, 18, 100), first=True), drawer[1]]
        moved = c.change("align", "box joint that aligns the boards first", loose, "box-joint", {**sf, "align": True}, {"Side": 2 * 20 * 18 * 18, "Front": -2 * 20 * 18 * 18})
        side = next(b for b in moved[-1]["bodies"] if b["name"] == "Side")["bbox"]
        c.report(side["min"] == [0, 0, 0] and side["max"] == [318, 18, 100], f"the aligned side lies in the corner: {side['min']} to {side['max']}")
        c.refuse_entry("align_off", "the same boards without align", loose, "box-joint", sf, "does not lie on a face")
        undone = c.cli("align_undo", [*loose, c.run("box-joint", {**sf, "align": True}), {"command": "edit.undo", "params": {}}, MEASURE])
        side = next(b for b in undone[-1]["bodies"] if b["name"] == "Side")["bbox"]
        c.report(side["min"] == [58, 25, 7], f"one undo puts the side back: {side['min']}")

        # Align on the other joints. A rail 10 mm away, hanging 5 mm off the front of the stile.
        away = boards([45, -5, 400])
        got = c.change("align_tenon", "mortise and tenon that aligns the rail first", away, "mortise-tenon", {**both, "align": True}, {"Rail": 6 * 33 * 25, "Stile": -6 * 33 * 27})
        rail = next(b for b in got[-1]["bodies"] if b["name"] == "Rail")["bbox"]
        c.report(rail["min"] == [10, 0, 400] and rail["max"] == [395, 18, 445], f"the aligned rail and its tenon: {rail['min']} to {rail['max']}")
        c.change("align_dowels", "dowels that align the rail first", away, "dowels", {**rs, "align": True}, {"Rail": -2 * hole, "Stile": -2 * hole, "Dowel 1": peg, "Dowel 2": peg})
        placed = c.change("align_kept", "align leaves a rail that is in place", boards([35, 10, 400]), "dowels", {**rs, "align": True, "dowel_bodies": False}, {"Rail": -2 * hole, "Stile": -2 * hole})
        rail = next(b for b in placed[-1]["bodies"] if b["name"] == "Rail")["bbox"]
        # The round holes leave float noise in the bounding box, so compare within a micron.
        still = all(abs(got - want) < 1e-3 for got, want in zip(rail["min"], [35, 10, 400]))
        c.report(still, f"a rail already on the stile does not move: {[round(v, 3) for v in rail['min']]}")
        # A half lap whose first board lies 30 mm above the second one.
        above = [box("A", [0, 0, 30], (300, 45, 18), first=True), cross[1]]
        c.change("align_lap", "half lap that aligns the boards first", above, "half-lap", {**two, "align": True}, {"A": -45 * 45 * 9, "B": -45 * 45 * 9})
        c.refuse_entry("align_lap_off", "the same boards without align", above, "half-lap", two, "do not overlap")
        c.refuse_entry("align_lap_beside", "align on boards that do not cross", [box("A", [0, 300, 30], (300, 45, 18), first=True), cross[1]], "half-lap", {**two, "align": True}, "do not cross")

        # Two cabinet panels, taller than they are long. The side is drawn along X, turned 90 degrees
        # about Z, and left away from the back: align reads it by its thickness and brings it home.
        tall = [box("Side", [100, -100, 30], (300, 18, 400), first=True), box("Back", [-18, 0, 0], (350, 18, 400)),
                {"command": "solid.move", "params": {"bodies": ["Side"], "axis": [0, 0, 1], "angle": 90}}]
        sb = {"first": "Side", "second": "Back", "align": True, "count": 5}
        got = c.change("align_tall", "box joint on a panel turned 90 degrees", tall, "box-joint", sb, {"Side": 2 * 80 * 18 * 18, "Back": -2 * 80 * 18 * 18})
        side = next(b for b in got[-1]["bodies"] if b["name"] == "Side")["bbox"]
        home = all(abs(g - w) < 1e-3 for g, w in zip(side["min"] + side["max"], [-18, 0, 0, 0, 318, 400]))
        c.report(home, f"the turned side lies on the end of the back: {[round(v, 3) for v in side['min'] + side['max']]}")
        # A board in a component that was turned: the plugin cannot place the joint, and says so.
        turned = [box("Back", [-18, 0, 0], (350, 18, 400), first=True), {"command": "component.create", "params": {"name": "SideC", "grounded": False}},
                  box("Side", [100, -100, 30], (300, 18, 400)), {"command": "occurrence.move", "params": {"occurrence": 3, "angle": "90 deg", "axis": [0, 0, 1]}}]
        c.refuse_entry("component", "a board in a turned component", turned, "box-joint", sb, "did not land")

        # A loose tenon in the rail end: 6 x 33 x 30, 15 mm in each board, mortises 1 mm deeper.
        slot, key = 6 * 33 * 16, 6 * 33 * 30
        c.change("loose", "one loose tenon", mid, "loose-tenon", rs, {"Rail": -slot, "Stile": -slot, "Loose tenon 1": key})
        # Sizes by hand, mortises only: 8 x 20 x 40, 20 mm in each board.
        c.change("loose_holes", "loose tenon mortises only", mid, "loose-tenon", {**rs, "thickness": 8, "width": 20, "length": 40, "tenon_bodies": False}, {"Rail": -8 * 20 * 21, "Stile": -8 * 20 * 21})
        c.change("loose_align", "loose tenon that aligns the rail first", away, "loose-tenon", {**rs, "align": True}, {"Rail": -slot, "Stile": -slot, "Loose tenon 1": key})
        c.refuse_entry("loose_apart", "boards that do not touch", boards([45, 0, 400]), "loose-tenon", rs, "do not touch")
        c.refuse_entry("loose_fat", "tenon as thick as the rail", mid, "loose-tenon", {**rs, "thickness": 17}, "no cheek")

        # A housing: an 18 mm shelf let 6 mm into an 18 mm side. The shelf grows by as much.
        cabinet = [box("Side", [0, 0, 0], (18, 250, 400), first=True), box("Shelf", [18, 0, 200], (300, 250, 18))]
        ss = {"first": "Shelf", "second": "Side"}
        got = c.change("housing", "through housing", cabinet, "housing", ss, {"Shelf": 6 * 250 * 18, "Side": -6 * 250 * 18})
        shelf = next(b for b in got[-1]["bodies"] if b["name"] == "Shelf")["bbox"]
        c.report(shelf["min"] == [12, 0, 200] and shelf["max"] == [318, 250, 218], f"the shelf is 6 mm longer, into the side: {shelf['min']} to {shelf['max']}")
        c.change("housing_stopped", "housing stopped 20 mm from the front", cabinet, "housing", {**ss, "depth": 8, "stop_end": 20}, {"Shelf": 8 * 230 * 18, "Side": -8 * 230 * 18})
        c.change("housing_pick", "housing on the selection", [*cabinet, select("Shelf", "Side")], "housing", {}, {"Shelf": 6 * 250 * 18, "Side": -6 * 250 * 18})
        c.refuse_entry("housing_deep", "housing through the side", cabinet, "housing", {**ss, "depth": 18}, "cut through")
        c.refuse_entry("housing_apart", "a shelf away from the side", [cabinet[0], box("Shelf", [30, 0, 200], (300, 250, 18))], "housing", ss, "does not lie on a face")

        # A miter: two 18 x 100 boards drawn through each other at a corner. Each loses half of the
        # 18 x 18 corner over its 100 mm, and no tool body is left behind.
        frame = [box("A", [0, 0, 0], (300, 18, 100), first=True), box("B", [0, 0, 0], (18, 300, 100))]
        half = 18 * 18 / 2 * 100
        c.change("miter", "miter at 45 degrees", frame, "miter", two, {"A": -half, "B": -half})
        # A board that stops at the other one is first made to reach the corner: +32400 - 16200.
        butt = [box("A", [18, 0, 0], (282, 18, 100), first=True), frame[1]]
        got = c.change("miter_butt", "miter on a board that stops at the other", butt, "miter", two, {"A": 18 * 18 * 100 - half, "B": -half})
        a = next(b for b in got[-1]["bodies"] if b["name"] == "A")["bbox"]
        c.report(all(abs(g - w) < 1e-3 for g, w in zip(a["min"] + a["max"], [0, 0, 0, 300, 18, 100])), f"the extended board reaches the outer corner: {[round(v, 3) for v in a['min'] + a['max']]}")
        # Unequal boards at another corner: 12 mm along X, 20 mm along Y.
        other = [box("A", [-300, 282, 0], (300, 12, 100), first=True), box("B", [-20, -6, 0], (20, 300, 100))]
        c.change("miter_unequal", "miter between a 12 mm and a 20 mm board", other, "miter", two, {"A": -12 * 20 / 2 * 100, "B": -12 * 20 / 2 * 100})
        # A frame lying flat: the boards are 45 wide and 18 thick, and the corner runs along Z.
        flat = [box("A", [0, 0, 0], (300, 45, 18), first=True), box("B", [0, 0, 0], (45, 300, 18))]
        c.change("miter_flat", "miter of a flat frame", flat, "miter", two, {"A": -45 * 45 / 2 * 18, "B": -45 * 45 / 2 * 18})
        c.refuse_entry("miter_apart", "boards that do not meet", [frame[0], box("B", [60, 60, 0], (18, 300, 100))], "miter", two, "do not meet")
        undone = c.cli("miter_undo", [*frame, c.run("miter", two), {"command": "edit.undo", "params": {}}, MEASURE])
        back = {b["name"]: round(b["volume_mm3"]) for b in undone[-1]["bodies"]}
        c.report(back == {"A": 540000, "B": 540000}, f"one undo removes the whole miter: {back}")

        # Tongue and groove: two 600 x 100 x 18 boards edge to edge. Tongue 6 x 9, groove 1 mm deeper.
        planks = [box("A", [0, 0, 0], (600, 100, 18), first=True), box("B", [0, 100, 0], (600, 100, 18))]
        c.change("tongue", "tongue and groove", planks, "tongue-groove", two, {"A": 6 * 9 * 600, "B": -6 * 10 * 600})
        c.change("tongue_sizes", "tongue sizes set by hand", planks, "tongue-groove", {**two, "thickness": 8, "depth": 12, "glue_relief": 0}, {"A": 8 * 12 * 600, "B": -8 * 12 * 600})
        c.refuse_entry("tongue_apart", "boards that do not touch", [planks[0], box("B", [0, 120, 0], (600, 100, 18))], "tongue-groove", two, "do not touch")

        # A scarf: two 400 x 45 x 18 boards overlapping by 144 mm. Each loses half of the overlap.
        ends = [box("A", [0, 0, 0], (400, 45, 18), first=True), box("B", [256, 0, 0], (400, 45, 18))]
        slope = 144 * 18 / 2 * 45
        c.change("scarf", "scarf at 1:8", ends, "scarf", two, {"A": -slope, "B": -slope})
        c.change("scarf_flip", "scarf turned over", ends, "scarf", {**two, "flip": True}, {"A": -slope, "B": -slope})
        apart = [box("A", [-194, 10, 5], (400, 45, 18), first=True), ends[1]]
        got = c.change("scarf_align", "scarf that aligns the boards first", apart, "scarf", {**two, "align": True}, {"A": -slope, "B": -slope})
        total = max(b["bbox"]["max"][0] for b in got[-1]["bodies"]) - min(b["bbox"]["min"][0] for b in got[-1]["bodies"])
        c.report(abs(total - 656) < 1e-3, f"the two scarfed boards are {total:.1f} mm long together (expected 656)")
        c.refuse_entry("scarf_apart", "boards that do not overlap", [ends[0], box("B", [420, 0, 0], (400, 45, 18))], "scarf", two, "do not overlap")

        # Dovetails at a drawer corner: two tails, 41 mm wide at the end and 36.5 at the base.
        tail = (41 + 36.5) / 2 * 18 * 18
        got = c.change("dovetail", "two dovetails", drawer, "dovetail", sf, {"Side": 2 * tail, "Front": -2 * tail})
        c.report(len(got[-1]["bodies"]) == 2, f"no tool or blank body is left: {[b['name'] for b in got[-1]['bodies']]}")
        one = (91 + 91 - 2 * 18 / 6) / 2 * 18 * 18
        c.change("dovetail_one", "one tail at 1:6", drawer, "dovetail", {**sf, "count": 1, "slope": 6}, {"Side": one, "Front": -one})
        c.refuse_entry("dovetail_many", "too many tails", drawer, "dovetail", {**sf, "count": 9}, "under 3 mm")
        undone = c.cli("dovetail_undo", [*drawer, c.run("dovetail", sf), {"command": "edit.undo", "params": {}}, MEASURE])
        back = {b["name"]: round(b["volume_mm3"]) for b in undone[-1]["bodies"]}
        c.report(back == {"Side": 540000, "Front": 540000}, f"one undo removes the dovetails: {back}")

        # Half-blind dovetails: a 5 mm lap leaves the face of the front whole. Tails 13 mm long.
        blind = (41 + 41 - 2 * 13 / 8) / 2 * 13 * 18
        got = c.change("dovetail_blind", "half-blind dovetails", drawer, "dovetail", {**sf, "lap": 5}, {"Side": 2 * blind, "Front": -2 * blind})
        side = next(b for b in got[-1]["bodies"] if b["name"] == "Side")["bbox"]
        c.report(abs(side["min"][0] - 5) < 1e-3, f"the tails stop 5 mm short of the face: the side starts at X = {side['min'][0]:.3f}")

        # Screws: an 18 mm side screwed onto a shelf end. Two 4 x 45 screws: a 4.5 mm hole through
        # the side with a 90 degree countersink to 8 mm, and a 3 mm pilot hole 27 mm into the shelf.
        through = math.pi * 2.25 ** 2 * 18 + math.pi * 1.75 / 3 * (4 ** 2 + 4 * 2.25 + 2.25 ** 2) - math.pi * 2.25 ** 2 * 1.75
        pilot = math.pi * 1.5 ** 2 * 27
        fixed = {"first": "Side", "second": "Shelf"}
        c.change("screws", "two countersunk screws", cabinet, "screws", fixed, {"Side": -2 * through, "Shelf": -2 * pilot})
        plain = {**fixed, "diameter": 5, "length": 50, "count": 3, "countersink": False}
        c.change("screws_plain", "three 5 x 50 screws, no countersink", cabinet, "screws", plain, {"Side": -3 * math.pi * 2.75 ** 2 * 18, "Shelf": -3 * math.pi * 1.75 ** 2 * 32})
        c.refuse_entry("screws_wrong_way", "screws through the long board", cabinet, "screws", {"first": "Shelf", "second": "Side", "length": 45}, "does not reach through")

        # 6. A saved design reopens without the plugin: the joint is ordinary features.
        saved = work / "joint.sc"
        c.volumes("save", mid, both, "--save", str(saved))
        reopened = c.cli("reopen", [MEASURE], "--in", str(saved))
        bodies = {b["name"]: round(b["volume_mm3"]) for b in reopened[-1]["bodies"]}
        c.report(bodies == {"Rail": RAIL + 6 * 33 * 25, "Stile": STILE - 6 * 33 * 27}, f"saved design reopened without the plugin: {bodies}")

        # 7. A picture, for a human to look at.
        picture = work / "joint.png"
        cam = json.dumps({"target": [35, 9, 422], "yaw": 0.9, "pitch": 0.35, "distance": 260})
        r = subprocess.run([c.binary, "snapshot", "--in", str(saved), "--out", str(picture), "--no-grid", "--body", "Stile:opacity=0.3", "--camera", cam], capture_output=True, text=True)
        c.report(r.returncode == 0 and picture.exists(), f"picture written{f' to {picture}' if keep else ''}")

        raise SystemExit(1 if c.failed else 0)


if __name__ == "__main__":
    main()
