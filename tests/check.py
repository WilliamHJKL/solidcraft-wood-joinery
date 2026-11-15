#!/usr/bin/env python3
"""End-to-end check of the packed plugin (dist/wood-joinery) in a real SolidCraft.

Usage: tests/check.py /path/to/solidcraft-cli [--keep DIR]

It builds a 35 x 35 x 830 mm stile and a 360 x 18 x 45 mm rail, runs the joint, and compares the
measured volumes with values worked out by hand. It also checks undo and redo, refusals, a saved
design reopened without the plugin, and writes a picture. With --keep, the files stay in DIR.
"""
import json
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
        c.refuse("unknown", "unknown body", mid, {"rail": "Nope", "stile": "Stile"}, "unknown body name")
        c.refuse("variant", "unknown variant", mid, {**both, "variant": "dovetail"}, "classic, haunched, through or bridle")

        # Grooves in the stile. Default: 6 wide, 8 deep, the whole 830 mm.
        c.groove("groove", "groove, default sizes", {"body": "Stile", "face": "+x"}, 6 * 8 * 830)
        # As on a cabinet leg: 6 mm from the edge, stopped 80 mm above the foot.
        c.groove("groove_stopped", "stopped groove in a low face", {"body": "Stile", "face": "-y", "offset": 6, "stop_start": 80}, 6 * 8 * 750)
        c.groove("rabbet", "rabbet at the edge", {"body": "Stile", "face": "+y", "offset": 0, "width": 10, "depth": 5}, 10 * 5 * 830)
        c.groove("dado", "dado across the top", {"body": "Stile", "face": "+z", "width": 12, "depth": 4}, 12 * 4 * 35)
        c.refuse_groove("groove_deep", "groove through the board", {"body": "Stile", "face": "+x", "depth": 35}, "cut through")
        c.refuse_groove("groove_face", "unknown face", {"body": "Stile", "face": "up"}, "+x, -x, +y, -y, +z or -z")

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
