#!/usr/bin/env python3
"""End-to-end check of the packed plugin (dist/wood-joinery) in a real SolidCraft.

Usage: tests/check.py /path/to/solidcraft-cli [--keep DIR]

It builds a 35 x 35 x 830 mm stile and a 360 x 18 x 45 mm rail, runs each joint, and compares the
measured volumes with values worked out by hand: mortise and tenon, grooves, half laps, dowels,
box joints, loose tenons, housings, miters, tongue and groove, scarfs, dovetails, screws and biscuits. It also checks undo and redo, refusals, a saved design reopened without the
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

# The fields that pick a board. A form sends a picked board as a list of one reference; a script
# gives a list of one name. The scenarios below type plain names, wrapped here.
PICK_FIELDS = ("rail", "stile", "first", "second", "body")


def picks(params):
    return {k: [v] if k in PICK_FIELDS and isinstance(v, str) else v for k, v in params.items()}


def select(*items):
    """Selects bodies (a name) and faces (a body name and a face index), in that order."""
    picked = [{"type": "body", "name": i} if isinstance(i, str) else {"type": "face", "body": i[0], "index": i[1]} for i in items]
    return {"command": "select.set", "params": {"items": picked}}


class Check:
    def __init__(self, binary, work):
        self.binary, self.work, self.failed, self.n = binary, work, False, 0
        self.package = PLUGIN

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
        return {"command": "plugin.run", "params": {"path": str(self.package), "approval": self.approval, "entry": entry, "params": picks(params)}}

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
        version = subprocess.run([str(binary), "--version"], capture_output=True, text=True).stdout.split()[-1]
        c.conditions = tuple(int(n) for n in version.split("-")[0].split(".")[:2]) >= (0, 46)
        print(f"SolidCraft {version}: {'fields hide by condition' if c.conditions else 'no field conditions (before 0.46)'}")

        # Each condition names a field of its entry, and a value that field can take, in every
        # package (the French one translates the choices).
        for path in sorted((ROOT / "dist").glob("**/plugin.json")):
            bad = []
            for e in json.loads(path.read_text(encoding="utf-8"))["runtime"]["entries"]:
                props = e["input_schema"].get("properties", {})

                def walk(cond, field):
                    for inner in cond.get("all", []) + cond.get("any", []) + ([cond["not"]] if "not" in cond else []):
                        walk(inner, field)
                    if "field" in cond:
                        target = props.get(cond["field"])
                        values = [cond["eq"]] if "eq" in cond else cond.get("in", [])
                        if target is None or ("enum" in target and any(v not in target["enum"] for v in values)):
                            bad.append(f"{e['name']}.{field}: {cond}")

                for field, prop in props.items():
                    if "x-solidcraft-visible-when" in prop:
                        walk(prop["x-solidcraft-visible-when"], field)
            c.report(not bad, f"field conditions name real fields and values in {path.relative_to(ROOT)}: {bad or 'ok'}")

        # 1. SolidCraft reads the manifest and the component without running anything.
        inspected = c.cli("inspect", [{"command": "plugin.inspect", "params": {"path": str(PLUGIN)}}])
        c.approval = inspected[0]["approval"]
        c.report(inspected[0]["manifest"]["id"] == "wood-joinery", "plugin.inspect accepts the manifest and the component")
        # The .zip is what Preferences > Plugins > Install plugin... takes. A dry run writes nothing.
        dry = c.cli("install", [{"command": "plugin.install", "params": {"path": str(ROOT / "dist" / "wood-joinery-en.zip"), "dry_run": True}}])
        c.report(dry[0]["id"] == "wood-joinery" and dry[0]["installed"] is False, "plugin.install accepts the .zip in a dry run")

        # Every form must be one the dialog can draw: numbers, whole numbers, checkboxes, text,
        # `enum` lists, fields shown or greyed by condition, and fields that pick a board in the
        # view (SolidCraft 0.48), which take exactly `maxItems` and the `x-solidcraft-selection`
        # note. The dialog refuses the whole entry for any other keyword.
        notes = {"type", "title", "description", "default", "unit", "x-solidcraft-visible-when", "x-solidcraft-enabled-when"}
        allowed = {"number": {"minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum", "multipleOf"},
                   "integer": {"minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"}, "boolean": set(), "string": {"minLength", "maxLength", "enum"},
                   "array": {"minItems", "maxItems", "x-solidcraft-selection"}}
        for package in [PLUGIN, *sorted((ROOT / "dist").glob("*/wood-joinery"))]:
            manifest = json.loads((package / "plugin.json").read_text())
            # The docked panel's own entries (panel-refresh, panel-update, panel-edit) have no
            # button and no form: the panel runs them.
            manifest["runtime"]["entries"] = [e for e in manifest["runtime"]["entries"] if e.get("slots")]
            bad = [f"{e['name']}.{k}" for e in manifest["runtime"]["entries"] for k, prop in e["input_schema"]["properties"].items()
                   if prop.get("type") not in allowed or set(prop) - notes - allowed[prop["type"]]]
            # At the top of a schema, 0.42.0 to 0.49.0 refuse only $ref, oneOf, anyOf, allOf, if and
            # not; they ignore the icon that 0.44.0 draws and the field order that 0.49.1 follows.
            top = {"type", "properties", "required", "x-solidcraft-icon", "x-solidcraft-field-order"}
            bad += [e["name"] for e in manifest["runtime"]["entries"] if set(e["input_schema"]) - top]
            c.report(not bad, f"every form of {package.parent.name}/{package.name} can be drawn by its dialog" + (f": not {bad}" if bad else ""))
            # From 0.49.1, a form shows its fields in the order the manifest lists them: the boards,
            # then the sizes, then the options, and "Align the boards first" last in every form.
            unordered = [e["name"] for e in manifest["runtime"]["entries"]
                         if e["input_schema"].get("x-solidcraft-field-order") != "declared"
                         or ("align" in e["input_schema"]["properties"] and [k for k in e["input_schema"]["properties"] if k != "replaces"][-1] != "align")
                         or ("replaces" in e["input_schema"]["properties"] and list(e["input_schema"]["properties"])[-1] != "replaces")]
            c.report(not unordered, f"every form of {package.parent.name}/{package.name} keeps its declared field order" + (f": not {unordered}" if unordered else ""))
            icons = [e["input_schema"].get("x-solidcraft-icon") for e in manifest["runtime"]["entries"]]
            missing = [i for i in icons if not i or not (package / i).is_file()]
            c.report(not missing, f"every command of {package.parent.name}/{package.name} has its icon in the package" + (f": not {missing}" if missing else ""))

        both = {"rail": "Rail", "stile": "Stile"}
        mid = boards([35, 0, 400])

        # Each translated package installs and runs as the English one does.
        for package in sorted((ROOT / "dist").glob("*/wood-joinery")):
            lang = package.parent.name
            dry = c.cli(f"install_{lang}", [{"command": "plugin.install", "params": {"path": str(ROOT / "dist" / f"wood-joinery-{lang}.zip"), "dry_run": True}}])
            approval = c.cli(f"inspect_{lang}", [{"command": "plugin.inspect", "params": {"path": str(package)}}])[0]["approval"]
            run = {"command": "plugin.run", "params": {"path": str(package), "approval": approval, "entry": "mortise-tenon", "params": picks(both)}}
            total = c.cli(f"run_{lang}", [*mid, run, MEASURE])[-1]["total"]["volume_mm3"]
            ok = dry[0]["id"] == "wood-joinery" and abs(total - (STILE + RAIL + 6 * 33 * 25 - 6 * 33 * 27)) < 0.5
            c.report(ok, f"the {lang} package installs in a dry run and makes the same joint ({dry[0]['name']})")
            # Its messages are in its language: a 25 mm tenon does not fit in the 18 mm rail.
            if (ROOT / "plugin" / "messages" / f"{lang}.json").is_file():
                thick = {"command": "plugin.run", "params": {"path": str(package), "approval": approval, "entry": "mortise-tenon", "params": picks({**both, "thickness": 25})}}
                code, output = c.cli(f"refusal_{lang}", [*mid, thick], expect_error=True)
                c.report(code != 0 and "25,000 mm" in output and "rail thickness" not in output, f"the {lang} package refuses in its language: {output.strip().splitlines()[-1][-160:]}")

        # 2. The four variants. Tenon 6 thick; classic: 33 wide, 25 long, mortise 27 deep.
        c.expect("classic", "classic", mid, both, 6 * 33 * 25, 6 * 33 * 27)
        # Rail 20 mm under the top of the stile: haunch 9 wide and 6 deep, main tenon 30 wide.
        c.expect("haunched", "haunched", boards([35, 0, 765]), {**both, "variant": "haunched"}, 6 * (33 * 25 + 6 * 6), 6 * (33 * 27 + 6 * 6))
        c.expect("through", "through", mid, {**both, "variant": "through"}, 6 * 33 * 35, 6 * 33 * 35)
        # Rail flush with the top of the stile: tongue 45 wide, right through.
        c.expect("bridle", "bridle", boards([35, 0, 785]), {**both, "variant": "bridle"}, 6 * 45 * 35, 6 * 45 * 35)
        # Sizes given by hand: 8 thick, 20 long, 5 shoulders, 3 of glue relief, 0.25 clearance.
        custom = {**both, "thickness": 8, "length": 20, "shoulder": 5, "glue_relief": 3, "clearance": 0.25}
        c.expect("custom", "sizes set by hand", mid, custom, 8 * 35 * 20, 8.5 * 35.5 * 23)

        # The reply lists the sizes that were used, the defaults included.
        made = c.cli("sizes", [*mid, c.joint({**both, "variant": "haunched"})])[-1]["result"]
        want = {"tenon_thickness_mm": 6, "tenon_length_mm": 25, "tenon_width_mm": 33, "shoulder_mm": 6, "glue_relief_mm": 2, "mortise_depth_mm": 27, "haunch_width_mm": 6, "haunch_depth_mm": 6}
        c.report(all(abs(made.get(k, -1) - v) < 1e-9 for k, v in want.items()), f"the reply lists the sizes used: {({k: made.get(k) for k in want})}")
        # The rail is flush with a face of the 35 mm stile: a 6 mm cheek, under the usual 8, is
        # warned of in the reply, and the joint is still made.
        c.report(any("6.0 mm cheek" in w for w in made.get("warnings", [])), f"a thin cheek is warned of: {made.get('warnings')}")
        # A 120 mm rail 20 mm under the top: a 6 mm haunch, a 6 mm shoulder, and twin tenons of
        # (120 - 12) / 3 = 36 mm with a 36 mm stub between them, 6 deep. The rail stays one body.
        twins = c.change("twins", "twin haunched tenons in a 120 mm rail", boards([35, 0, 690], (360, 18, 120)), "mortise-tenon",
                         {**both, "variant": "haunched"}, {"Rail": 6 * (2 * 36 * 25 + 36 * 6 + 6 * 6), "Stile": -6 * (2 * 36 * 27 + 36 * 6 + 6 * 6)})
        c.report(len(twins[-1]["bodies"]) == 2, f"twin tenons leave one rail: {[b['name'] for b in twins[-1]['bodies']]}")
        # A draw-bored peg: the hole in the tenon is 1.5 mm nearer the shoulder, so the peg runs
        # into the tenon; with a plain peg it does not.
        overlap = lambda name, extra: sum(i["volume_mm3"] for i in c.cli(name, [*mid, c.joint({**both, "pegs": 1, **extra}), {"command": "inspect.interference", "params": {}}])[-1].get("interferences", []))
        plain, drawn = overlap("peg_plain", {}), overlap("peg_drawbore", {"drawbore": 1.5})
        c.report(plain < 0.5 and drawn > 1, f"a draw-bored peg bears on the tenon ({drawn:.1f} mm3), a plain one does not ({plain:.1f})")
        # The form sends its shown defaults (glue relief 2, clearance 0, offset 0): same joint.
        c.expect("form_defaults", "the defaults the form shows", mid, {**both, "variant": "classic", "glue_relief": 2, "clearance": 0, "offset": 0}, 6 * 33 * 25, 6 * 33 * 27)
        c.expect("form_through", "a through tenon with the form defaults", mid, {**both, "variant": "through", "glue_relief": 2, "clearance": 0, "offset": 0}, 6 * 33 * 35, 6 * 33 * 35)

        # A translated package sends the value it shows, with the script value in brackets. Since
        # 0.46, SolidCraft checks a choice that a condition names, so only the French package takes it.
        english = c.approval
        c.package = ROOT / "dist" / "fr" / "wood-joinery"
        c.approval = c.cli("inspect_fr", [{"command": "plugin.inspect", "params": {"path": str(c.package)}}])[0]["approval"]
        c.expect("fr_value", "a translated choice, à renfort (haunched)", boards([35, 0, 765]), {**both, "variant": "à renfort (haunched)"}, 6 * (33 * 25 + 6 * 6), 6 * (33 * 27 + 6 * 6))
        c.groove("fr_face", "a translated face and direction", {"body": "Stile", "face": "droite (+x)", "along": "y"}, 6 * 8 * 35)
        c.package, c.approval = PLUGIN, english
        french = json.loads((ROOT / "dist" / "fr" / "wood-joinery" / "plugin.json").read_text())["runtime"]["entries"][0]["input_schema"]["properties"]["variant"]
        c.report(french["enum"][1] == "à renfort (haunched)" and french["default"] == "classique (classic)", f"the fr package lists translated choices: {french['enum']}")

        # A rail wider than the stile: a 275 mm panel standing on a 250 mm one. The tenons run over
        # the 250 mm they share, a shoulder (22 / 3) in from each end: too wide for one tenon, so
        # twin tenons a third of it each, 15 long in 17 mm mortises, with a 6 mm stub between them
        # (2 x 15 + 6 = 36 on the rail, 2 x 17 + 6 = 40 in the stile).
        panels = [box("Stile", [0, 25, 0], (350, 250, 22), first=True), box("Rail", [0, 12.5, 22], (22, 275, 250))]
        third = 22 / 3
        twin = (250 - 2 * third) / 3
        got = c.change("wide_rail", "a rail wider than the stile", panels, "mortise-tenon", both, {"Rail": third * twin * 36, "Stile": -third * twin * 40})

        # 3. Other directions: the rail on the -X side of the stile, then a rail along Y.
        c.expect("minus_x", "rail on the other side", boards([-360, 0, 400]), both, 6 * 33 * 25, 6 * 33 * 27)
        c.expect("along_y", "rail along Y", boards([0, 35, 400], (18, 360, 45)), both, 6 * 33 * 25, 6 * 33 * 27)
        # The boards are picked: a script gives a list of one name, or the reference a pick sends.
        # SolidCraft checks the input against the form, so a plain name is refused.
        c.expect("picked", "boards given as picks", mid, {"rail": [{"type": "body", "name": "Rail"}], "stile": [{"type": "body", "name": "Stile"}]}, 6 * 33 * 25, 6 * 33 * 27)
        code, output = c.cli("plain_name", [*mid, {"command": "plugin.run", "params": {"path": str(c.package), "approval": c.approval, "entry": "mortise-tenon", "params": {"rail": "Rail", "stile": "Stile"}}}], expect_error=True)
        c.report(code != 0 and "must be of type array" in output, f"refused, a plain name in a pick field: {'ok' if 'must be of type array' in output else output.strip()[-300:]}")
        c.refuse("empty", "empty body name", mid, {"rail": " ", "stile": "Stile"}, "name of one body")

        # 4. One undo removes the whole joint, one redo brings it back.
        steps = c.cli("undo", [*mid, c.joint({**both, "variant": "haunched"}), MEASURE, {"command": "edit.undo", "params": {}}, MEASURE, {"command": "edit.redo", "params": {}}, MEASURE])
        totals = [round(steps[i]["total"]["volume_mm3"]) for i in (3, 5, 7)]
        joined = STILE + RAIL + 6 * (33 * 25 + 6 * 6) - 6 * (33 * 27 + 6 * 6)
        c.report(totals == [joined, STILE + RAIL, joined], f"one undo and one redo: total volumes {totals}")

        # 5. Refusals leave a message that says what to change.
        c.refuse("apart", "rail 10 mm away from the stile", boards([45, 0, 400]), both, "does not lie on a face")
        c.refuse("too_long", "blind tenon as long as the stile", mid, {**both, "length": 34}, "come through")
        c.refuse("bridle_mid", "bridle away from the stile end", mid, {**both, "variant": "bridle"}, "flush with the end")
        c.refuse("same", "the same body twice", mid, {"rail": "Rail", "stile": "Rail"}, "two different bodies")
        c.refuse("unknown", "unknown body", mid, {"rail": "Nope", "stile": "Stile"}, "unknown body name `Nope`. The bodies of this design are: `Stile`, `Rail`")
        # Before 0.46 the plugin refuses it; since, SolidCraft does, from the form's list.
        c.refuse("variant", "unknown variant", mid, {**both, "variant": "dovetail"}, '"classic", "haunched", "through", "bridle"' if c.conditions else "classic, haunched, through or bridle")

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
        # Every form has a condition (its greyed "Joint being edited" field), so SolidCraft checks
        # the input against the form before the plugin does, and gives its own message.
        c.refuse_groove("groove_face", "unknown face", {"body": "Stile", "face": "up"}, '"+x", "-x", "+y", "-y", "+z", "-z"' if c.conditions else "+x, -x, +y, -y, +z or -z")

        # Half laps: two 45 x 18 boards that cross flat. Each loses 45 x 45 x 9.
        two = {"first": "A", "second": "B"}
        cross = [box("A", [0, 0, 0], (300, 45, 18), first=True), box("B", [100, -100, 0], (45, 300, 18))]
        c.change("lap", "cross lap", cross, "half-lap", two, {"A": -45 * 45 * 9, "B": -45 * 45 * 9})
        c.change("lap_depth", "lap 6 mm deep, other side", cross, "half-lap", {**two, "depth": 6, "flip": True}, {"A": -45 * 45 * 6, "B": -45 * 45 * 12})
        corner = [box("A", [0, 0, 0], (300, 45, 18), first=True), box("B", [0, 0, 0], (45, 300, 18))]
        c.change("lap_corner", "corner lap", corner, "half-lap", two, {"A": -45 * 45 * 9, "B": -45 * 45 * 9})
        c.refuse_entry("lap_apart", "boards that do not overlap", [cross[0], box("B", [100, 45, 0], (45, 300, 18))], "half-lap", two, "do not overlap")
        # Boards end to end that only touch: the first slides over the second by its width, 45.
        ends = [box("A", [0, 0, 0], (300, 45, 18), first=True), box("B", [300, 0, 0], (300, 45, 18))]
        got = c.change("lap_butt", "end lap of boards that only touch", ends, "half-lap", two, {"A": -45 * 45 * 9, "B": -45 * 45 * 9})
        c.report(got[-2]["result"]["moved_first_mm"] == [45, 0, 0], f"the first board slid 45 mm: {got[-2]['result']['moved_first_mm']}")
        # Two 220 x 22 planks lying flat edge to edge (a shiplap): by default they overlap by half
        # the thickness, 11 mm, along the whole 1360; with an overlap of 10, by 10.
        planks = [box("A", [0, 0, 0], (220, 1360, 22), first=True), box("B", [220, 0, 0], (220, 1360, 22))]
        c.change("shiplap", "shiplap of planks edge to edge", planks, "half-lap", two, {"A": -11 * 1360 * 11, "B": -11 * 1360 * 11})
        c.change("shiplap_10", "shiplap with an overlap of 10", planks, "half-lap", {**two, "overlap": 10}, {"A": -10 * 1360 * 11, "B": -10 * 1360 * 11})
        c.refuse_entry("shiplap_wide", "an overlap as wide as the planks", planks, "half-lap", {**two, "overlap": 220}, "less than the 220")
        # An egg-crate divider: two 300 x 18 x 100 boards on edge, crossing. Each loses half its
        # height (50) over the other's thickness (18): never a cut through its thickness.
        crate = [box("A", [0, 141, 0], (300, 18, 100), first=True), box("B", [141, 0, 0], (18, 300, 100))]
        got = c.change("lap_on_edge", "egg-crate lap of boards on edge", crate, "half-lap", two, {"A": -18 * 18 * 50, "B": -18 * 18 * 50})
        c.report(len(got[-1]["bodies"]) == 2, f"no board is cut in two: {[b['name'] for b in got[-1]['bodies']]}")

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
        thin = [box("A", [0, 0, 0], (300, 10, 200), first=True), box("B", [0, 10, 0], (300, 10, 200))]
        c.refuse_entry("dowel_thin", "dowels in 10 mm boards", thin, "dowels", two, "too thin for dowels")
        # More than one call can make before SolidCraft's 30 s limit: refused before any cut.
        c.refuse_entry("dowels_too_many", "31 dowels, one over the limit", mid, "dowels", {**rs, "count": 31}, "at most 30" if c.conditions else "30 or fewer")

        # A box joint at a drawer corner: 18 x 100 boards, five divisions of 20, two fingers on the side.
        drawer = [box("Side", [18, 0, 0], (300, 18, 100), first=True), box("Front", [0, 0, 0], (18, 300, 100))]
        sf = {"first": "Side", "second": "Front"}
        c.change("box", "box joint", drawer, "box-joint", sf, {"Side": 2 * 20 * 18 * 18, "Front": -2 * 20 * 18 * 18})
        # Two 100 x 18 boards lying flat in an L, the side's end against the front's edge: a comb
        # of three 6 mm layers through the thickness, the middle one 100 x 100 into the front.
        flat = [box("Side", [100, 0, 0], (300, 100, 18), first=True), box("Front", [0, 0, 0], (100, 300, 18))]
        got = c.change("box_flat", "box joint of boards lying flat (a comb)", flat, "box-joint", sf, {"Side": 100 * 100 * 6, "Front": -100 * 100 * 6})
        c.report(len(got[-1]["bodies"]) == 2, f"the front of the flat comb is in one piece: {[b['name'] for b in got[-1]['bodies']]}")
        c.refuse_entry("dovetail_flat", "dovetails on boards lying flat", flat, "dovetail", sf, "box joint or a half lap")
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
        c.refuse_entry("align_lap_off", "the same boards without align", above, "half-lap", two, "Align the boards first")
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
        # Festool Dominos: Festool's sizes and half round ends. A 6 x 40 is 19.8 mm wide, 20 mm in each
        # board, in mortises 21 mm deep; the second (the stile) at the middle setting, 6 mm wider.
        oval = lambda w, d: (w - d) * d + math.pi * d * d / 4
        c.change("domino", "one 6 x 40 Domino, second mortise medium", mid, "loose-tenon", {**rs, "domino": "6x40"},
                 {"Rail": -oval(19.8, 6) * 21, "Stile": -oval(25.8, 6) * 21, "Domino 1": oval(19.8, 6) * 40})
        c.change("domino_tight", "a tight 8 x 40 Domino, mortises only", mid, "loose-tenon", {**rs, "domino": "8x40", "domino_fit": "tight", "tenon_bodies": False},
                 {"Rail": -oval(21.9, 8) * 21, "Stile": -oval(21.9, 8) * 21})
        c.refuse_entry("domino_fat", "a 14 mm Domino in an 18 mm rail", mid, "loose-tenon", {**rs, "domino": "14x100"}, "no cheek")
        c.refuse_entry("domino_unknown", "a Domino Festool does not make", mid, "loose-tenon", {**rs, "domino": "9x99"}, "must be one of")

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
        # A bastard tongue: an 18 mm board level with a 27 mm one on their low face. The tongue is
        # half the thin board (9), against its high face; the groove 9 x 10 in the thick one.
        uneven = [box("A", [0, 0, 0], (600, 100, 18), first=True), box("B", [0, 100, 0], (600, 100, 27))]
        got = c.change("tongue_bastard", "bastard tongue, boards of 18 and 27", uneven, "tongue-groove", two, {"A": 9 * 9 * 600, "B": -9 * 10 * 600})
        c.report(got[-2]["result"]["bastard"] is True, "the reply says the tongue is a bastard one")
        # A loose spline: a 6 x 10 groove in each board, and an 18 x 6 strip, 2 mm short of them.
        got = c.change("tongue_spline", "loose spline", planks, "tongue-groove", {**two, "spline": True}, {"A": -6 * 10 * 600, "B": -6 * 10 * 600, "Spline 1": 6 * 18 * 600})
        c.change("tongue_sizes", "tongue sizes set by hand", planks, "tongue-groove", {**two, "thickness": 8, "depth": 12, "glue_relief": 0}, {"A": 8 * 12 * 600, "B": -8 * 12 * 600})
        c.refuse_entry("tongue_apart", "boards that do not touch", [planks[0], box("B", [0, 120, 0], (600, 100, 18))], "tongue-groove", two, "do not touch")

        # A scarf: two 400 x 45 x 18 boards overlapping by 144 mm. Each loses half of the overlap.
        ends = [box("A", [0, 0, 0], (400, 45, 18), first=True), box("B", [256, 0, 0], (400, 45, 18))]
        slope = 144 * 18 / 2 * 45
        c.change("scarf", "scarf at 1:8", ends, "scarf", two, {"A": -slope, "B": -slope})
        c.change("scarf_flip", "scarf turned over", ends, "scarf", {**two, "flip": True}, {"A": -slope, "B": -slope})
        # Boards that only touch end to end: the first slides 144 mm (1:8) over the second, then
        # the same scarf; at 1:6 it slides 108 mm and each loses 108 x 18 / 2 x 45.
        butt = [box("A", [0, 0, 0], (400, 45, 18), first=True), box("B", [400, 0, 0], (400, 45, 18))]
        got = c.change("scarf_butt", "scarf of boards that only touch", butt, "scarf", two, {"A": -slope, "B": -slope})
        c.report(got[-2]["result"]["moved_first_mm"] == [144, 0, 0], f"the first board slid 144 mm: {got[-2]['result']['moved_first_mm']}")
        c.change("scarf_butt_6", "scarf of boards that only touch, at 1:6", butt, "scarf", {**two, "slope": 6}, {"A": -108 * 9 * 45, "B": -108 * 9 * 45})
        apart = [box("A", [-194, 10, 5], (400, 45, 18), first=True), ends[1]]
        got = c.change("scarf_align", "scarf that aligns the boards first", apart, "scarf", {**two, "align": True}, {"A": -slope, "B": -slope})
        total = max(b["bbox"]["max"][0] for b in got[-1]["bodies"]) - min(b["bbox"]["min"][0] for b in got[-1]["bodies"])
        c.report(abs(total - 656) < 1e-3, f"the two scarfed boards are {total:.1f} mm long together (expected 656)")
        # 20 mm apart end to end, they slide together too; off to the side, they are refused.
        c.change("scarf_gap", "scarf of boards 20 mm apart", [ends[0], box("B", [420, 0, 0], (400, 45, 18))], "scarf", two, {"A": -slope, "B": -slope})
        c.refuse_entry("scarf_apart", "boards that are not in line", [ends[0], box("B", [420, 60, 0], (400, 45, 18))], "scarf", two, "do not overlap")

        # Dovetails at a drawer corner: two tails at 1:6, 41 mm wide at the end and 35 at the base.
        tail = (41 + 35) / 2 * 18 * 18
        got = c.change("dovetail", "two dovetails", drawer, "dovetail", sf, {"Side": 2 * tail, "Front": -2 * tail})
        c.report(len(got[-1]["bodies"]) == 2, f"no tool or blank body is left: {[b['name'] for b in got[-1]['bodies']]}")
        one = (91 + 91 - 2 * 18 / 6) / 2 * 18 * 18
        c.change("dovetail_one", "one tail at 1:6", drawer, "dovetail", {**sf, "count": 1, "slope": 6}, {"Side": one, "Front": -one})
        c.refuse_entry("dovetail_many", "too many tails", drawer, "dovetail", {**sf, "count": 9}, "under 3 mm")
        undone = c.cli("dovetail_undo", [*drawer, c.run("dovetail", sf), {"command": "edit.undo", "params": {}}, MEASURE])
        back = {b["name"]: round(b["volume_mm3"]) for b in undone[-1]["bodies"]}
        c.report(back == {"Side": 540000, "Front": 540000}, f"one undo removes the dovetails: {back}")

        # Half-blind dovetails: a 5 mm lap leaves the face of the front whole. Tails 13 mm long.
        blind = (41 + 41 - 2 * 13 / 6) / 2 * 13 * 18
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
        c.refuse_entry("screws_wrong_way", "screws through the long board", cabinet, "screws", {"first": "Shelf", "second": "Side", "length": 45}, "the long way")
        # Two 220 x 1360 x 22 planks lying flat, drawn in one plane, overlapping by 205 mm: the
        # first is laid on the second, and the default screw is 40 mm (2 x 22 - 3, to 5 mm), not 55.
        flat = [box("Bottom", [843, 0, 0], (220, 1360, 22), first=True), box("Top", [858, 0, 0], (220, 1360, 22))]
        got = c.cli("screws_planks", [*flat, c.run("screws", {"first": "Top", "second": "Bottom"}), MEASURE])
        r = got[-2]["result"]
        tops = [b for b in got[-1]["bodies"] if b["name"] == "Top"]
        c.report(r["moved_first_mm"] == [0, 0, 22] and r["length_mm"] == 40 and abs(tops[0]["bbox"]["min"][2] - 22) < 1e-6,
                 f"planks in one plane: the top one laid on the other ({r['moved_first_mm']}, bottom at {tops[0]['bbox']['min'][2]:.3f}), {r['count']} screws of {r['length_mm']} mm")
        c.refuse_entry("screws_edge", "screws edge to edge through a plank's width",
                       [flat[0], box("Top", [1063, 0, 0], (220, 1360, 22))], "screws", {"first": "Top", "second": "Bottom", "length": 20}, "the long way")

        # Mortise and tenon variants. A double tenon: two tenons 4 thick, each in its own mortise.
        c.expect("double", "double tenon", mid, {**both, "double": True, "thickness": 4, "shoulder": 6}, 2 * 4 * 33 * 25, 2 * 4 * 33 * 27)
        # A tenon level with a face of the rail. The rail stands 10 mm in, so the mortise keeps a cheek.
        inset = boards([35, 10, 400])
        got = c.change("barefaced", "tenon level with a face of the rail", inset, "mortise-tenon", {**both, "position": "low"}, {"Rail": 6 * 33 * 25, "Stile": -6 * 33 * 27})
        english = c.approval
        c.package = ROOT / "dist" / "fr" / "wood-joinery"
        c.approval = c.cli("inspect_fr2", [{"command": "plugin.inspect", "params": {"path": str(c.package)}}])[0]["approval"]
        c.change("barefaced_fr", "the same, from the French package", inset, "mortise-tenon", {**both, "position": "à fleur, côté opposé (high)"}, {"Rail": 6 * 33 * 25, "Stile": -6 * 33 * 27})
        c.package, c.approval = PLUGIN, english
        c.refuse("barefaced_flush", "level tenon on a rail flush with the stile", mid, {**both, "position": "low"}, "leaves no cheek")
        # One 6 mm peg through the 35 mm stile: 29 mm of stile around the 6 mm tenon.
        peg = math.pi * 9
        got = c.change("pegged", "pegged tenon", mid, "mortise-tenon", {**both, "pegs": 1}, {"Rail": 6 * 33 * 25 - peg * 6, "Stile": -6 * 33 * 27 - peg * 29, "Peg 1": peg * 35})
        c.refuse("pegs_many", "three pegs", mid, {**both, "pegs": 3}, "at most 2" if c.conditions else "0, 1 or 2")

        # A sliding dovetail: the housing at 1:6. The tongue is 18 wide at the bottom, 16 at the neck.
        slid = (18 + 16) / 2 * 6 * 250
        got = c.change("sliding", "sliding dovetail housing", cabinet, "housing", {**ss, "slope": 6}, {"Shelf": slid, "Side": -slid})
        c.report(len(got[-1]["bodies"]) == 2, f"no tool or tongue body is left: {[b['name'] for b in got[-1]['bodies']]}")

        # A miter with a loose spline: 4 thick, 6 mm into each board, over the 100 mm of the boards.
        slot = 4 * 6 * 100
        c.change("miter_spline", "miter with a loose spline", frame, "miter", {**two, "spline": 4}, {"A": -half - slot, "B": -half - slot, "Miter spline 1": 2 * slot})
        # Two keys (loose tenons) in place of the spline: 25 mm each, at a quarter of each half.
        key = 4 * 6 * 25
        c.change("miter_keys", "miter with two keys", frame, "miter", {**two, "spline": 4, "keys": 2}, {"A": -half - 2 * key, "B": -half - 2 * key, "Miter key 1": 2 * key, "Miter key 2": 2 * key})
        c.refuse_entry("miter_spline_deep", "a spline too deep", frame, "miter", {**two, "spline": 4, "spline_depth": 12}, "at most")

        # Biscuits: four size 20 biscuits along a 600 mm joint. Each slot is the segment a 100 mm
        # blade leaves 12 mm deep, 4 mm thick; a biscuit is two of them.
        seg = (2500 * math.acos(38 / 50) - 38 * math.sqrt(1200 - 144)) * 4
        got = c.change("biscuits", "four biscuits", planks, "biscuits", two, {"A": -4 * seg, "B": -4 * seg, **{f"Biscuit {i}": 2 * seg for i in range(1, 5)}})
        small = (2500 * math.acos(42 / 50) - 42 * math.sqrt(800 - 64)) * 4
        c.change("biscuits_small", "two size 0 slots, no biscuit body", planks, "biscuits", {**two, "size": "0", "count": 2, "biscuit_bodies": False}, {"A": -2 * small, "B": -2 * small})
        c.refuse_entry("biscuits_many", "too many biscuits", planks, "biscuits", {**two, "count": 12}, "do not fit")

        # Dowels in a corner that fingers already hold: the side's box reaches through the front,
        # yet the boards are read as touching on the inner face of the front.
        fingered = [*drawer, c.run("box-joint", sf)]
        got = c.change("dowels_after", "dowels after a box joint", fingered, "dowels", {**sf, "count": 1, "dowel_bodies": False}, {"Side": -math.pi * 16 * 27, "Front": -math.pi * 16 * 15})

        # A wedged through tenon: two wedges 4 thick and 26.25 deep, in slots of their own shape.
        wedge = 4 * 26.25 / 2 * 6
        got = c.change("wedged", "wedged through tenon", mid, "mortise-tenon", {**both, "variant": "through", "wedges": 2},
                       {"Rail": 6 * 33 * 35 - 2 * wedge, "Stile": -6 * 33 * 35, "Wedge 1": wedge, "Wedge 2": wedge})
        c.report(len(got[-1]["bodies"]) == 4, f"no wedge tool is left: {[b['name'] for b in got[-1]['bodies']]}")
        # A field shown only for some choices (x-solidcraft-visible-when): SolidCraft 0.46 and later
        # leave it out of the input when it is hidden, so the joint is made without it; before 0.46
        # the plugin gets it and refuses it.
        hidden = [
            ("wedged_blind", "wedges in a blind tenon", mid, "mortise-tenon", {**both, "wedges": 1}, {**both}, "through tenon"),
            ("drawbore_no_pegs", "a drawbore without pegs", mid, "mortise-tenon", {**both, "drawbore": 1.5}, {**both}, "needs pegs"),
            ("keys_no_spline", "miter keys without a spline", frame, "miter", {**two, "spline": 0, "keys": 2}, {**two, "spline": 0}, "spline thickness"),
        ]
        for name, text, setup, entry, params, plain, reason in hidden:
            if c.conditions:
                with_field = c.cli(name, [*setup, c.run(entry, params), MEASURE])[-1]["bodies"]
                without = c.cli(f"{name}_plain", [*setup, c.run(entry, plain), MEASURE])[-1]["bodies"]
                same = [(b["name"], round(b["volume_mm3"], 1)) for b in with_field] == [(b["name"], round(b["volume_mm3"], 1)) for b in without]
                c.report(same, f"hidden field ignored, {text}: {[b['name'] for b in with_field]}")
            else:
                c.refuse_entry(name, text, setup, entry, params, reason)

        # A tabled scarf: each board keeps a hook, and a 9 x 4.5 key locks the two steps.
        table = (76.5 * 11.25 + 67.5 * 6.75) * 45
        got = c.change("scarf_key", "tabled scarf with a key", ends, "scarf", {**two, "key": True}, {"A": -table, "B": -table, "Scarf key 1": 9 * 4.5 * 45})
        whole = sum(b["volume_mm3"] for b in got[-1]["bodies"])
        c.report(abs(whole - (2 * 400 - 144) * 45 * 18) < 0.5, f"the two boards and the key fill a 656 mm board exactly: {whole:.1f} mm3")
        c.change("scarf_key_flip", "tabled scarf turned over", ends, "scarf", {**two, "key": True, "flip": True}, {"A": -table, "B": -table, "Scarf key 1": 9 * 4.5 * 45})
        c.refuse_entry("scarf_key_short", "an overlap too short for a tabled scarf", [ends[0], box("B", [370, 0, 0], (400, 45, 18))], "scarf", {**two, "key": True}, "three thicknesses")

        # 6. A saved design reopens without the plugin: the joint is ordinary features.
        saved = work / "joint.sc"
        c.volumes("save", mid, both, "--save", str(saved))
        reopened = c.cli("reopen", [MEASURE], "--in", str(saved))
        bodies = {b["name"]: round(b["volume_mm3"]) for b in reopened[-1]["bodies"]}
        c.report(bodies == {"Rail": RAIL + 6 * 33 * 25, "Stile": STILE - 6 * 33 * 27}, f"saved design reopened without the plugin: {bodies}")

        # 6b. Regressions from the code review of v0.10.0, one per bug.
        L = [box("A", [0, 0, 0], (300, 18, 100), first=True), box("B", [0, 0, 0], (18, 300, 100))]
        c.refuse_entry("rev_miter_spline", "a 6 mm spline 10 mm deep in 18 mm boards", L, "miter", {**two, "spline": 6, "spline_depth": 10}, "at most 7.728")
        tenoned = [box("Stile", [0, 0, 0], (45, 35, 830), first=True), box("Rail", [45, 0, 400], (360, 18, 45)), c.joint(both)]
        c.refuse_entry("rev_screws_joined", "screws between boards joined by a tenon", tenoned, "screws", {"first": "Stile", "second": "Rail"}, "already joined")
        thick = [box("A", [0, 127, 0], (300, 45, 18), first=True), box("B", [127, 0, 0], (45, 300, 30))]
        c.change("rev_lap_thick", "half lap of 18 and 30 mm boards, swapped: open, no tunnel", thick, "half-lap", {**two, "flip": True}, {"A": -45 * 45 * 9, "B": -45 * 45 * 21})
        c.refuse_entry("rev_scarf_thick", "a scarf of 18 and 40 mm boards", [box("A", [0, 0, 0], (400, 45, 18), first=True), box("B", [256, 0, 0], (260, 45, 40))], "scarf", {**two, "flip": True}, "one thickness")
        flat_l = [box("A", [120, 15, 30], (300, 100, 18), first=True), box("B", [0, 0, 0], (100, 300, 18))]
        got = c.cli("rev_box_flat_align", [*flat_l, c.run("box-joint", {**two, "align": True}), MEASURE])
        c.report(got[-2]["result"]["moved_first_mm"] == [-20, -15, -30] and len(got[-1]["bodies"]) == 2, f"a box joint aligns boards lying flat into an L: {got[-2]['result']['moved_first_mm']}")
        loose_ends = [box("A", [0, 10, 5], (400, 45, 18), first=True), box("B", [420, 0, 0], (400, 45, 18))]
        got = c.cli("rev_scarf_align_slope", [*loose_ends, c.run("scarf", {**two, "align": True, "slope": 6})])
        c.report(got[-1]["result"]["length_mm"] == 108, f"a scarf aligned at 1:6 overlaps by 108 mm: {got[-1]['result']['length_mm']}")
        thin11 = [box("A", [0, 0, 11], (300, 45, 11), first=True), box("B", [0, 0, 0], (300, 45, 11))]
        got = c.cli("rev_screws_11", [*thin11, c.run("screws", two)])
        c.report(got[-1]["result"]["length_mm"] == 19, f"two 11 mm boards get a {got[-1]['result']['length_mm']} mm screw")
        c.refuse_entry("rev_through_short", "a 20 mm through tenon in a 35 mm stile", mid, "mortise-tenon", {**both, "variant": "through", "length": 20, "wedges": 1}, "runs through the whole")
        c.refuse_entry("rev_drawbore", "a 20 mm drawbore", mid, "mortise-tenon", {**both, "pegs": 1, "drawbore": 20}, "too much")
        far = [box("A", [0, 0, 0], (220, 1360, 22), first=True), box("B", [330, 0, 0], (220, 1360, 22))]
        c.refuse_entry("rev_lap_far", "a shiplap of planks 110 mm apart, without align", far, "half-lap", two, "110 mm apart")
        c.change("rev_lap_far_align", "the same planks with align", far, "half-lap", {**two, "align": True}, {"A": -11 * 1360 * 11, "B": -11 * 1360 * 11})

        # 8. Update joints: a joint's features are boxes at fixed sizes. The plugin keeps each joint
        # in the design and puts its features in a timeline folder; "Update joints" deletes the
        # folders and makes the joints again on the boards as they are now.
        def parametric(height, x="35"):
            """The stile and a rail whose height and start are parameters."""
            return [{"command": "parameters.add", "params": {"name": "H", "expression": str(height)}},
                    {"command": "parameters.add", "params": {"name": "X", "expression": x}},
                    box("Stile", [0, 0, 0], (35, 35, 830), first=True),
                    {"command": "solid.box", "params": {"corner": [0, 0, 400], "length": 360, "width": 18, "height": "H", "body_name": "Rail", "operation": "new"}},
                    {"command": "solid.move", "params": {"bodies": ["Rail"], "translate": ["X", 0, 0]}}]

        def bodies_of(result):
            return {b["name"]: round(b["volume_mm3"], 1) for b in result["bodies"]}

        update = c.run("update-joints", {})
        set_h = lambda h: {"command": "parameters.change", "params": {"name": "H", "expression": str(h)}}
        stored = {"command": "storage.get", "params": {"plugin": "wood-joinery"}}
        fresh = bodies_of(c.cli("upd_fresh", [*parametric(60), c.joint(both), MEASURE])[-1])
        got = c.cli("upd_param", [*parametric(45), c.joint(both), set_h(60), MEASURE, update, MEASURE, stored])
        stale, made, after, kept = bodies_of(got[-4]), got[-3]["result"], bodies_of(got[-2]), got[-1]["document"]["joints"]
        c.report(got[-6]["result"].get("folder") == "Mortise and tenon: Rail / Stile", f"a joint goes in its own timeline folder: {got[-6]['result'].get('folder')}")
        c.report(stale != fresh, f"a rail made 60 mm high after its joint keeps the old tenon: Rail {stale.get('Rail')} (made at 60 mm: {fresh['Rail']})")
        c.report(after == fresh and made["updated"] == 1, f"Update joints makes the tenon of a 60 mm rail: {after} (made at 60 mm: {fresh})")
        c.report(len(kept) == 1 and kept[0]["params"]["rail"] == "Rail" and kept[0]["entry"] == "mortise-tenon", f"the design keeps one joint record: {[(k['entry'], k['name']) for k in kept]}")
        # An update with nothing changed since the last one does nothing; a change makes the joints
        # again; force makes them again anyway.
        force = c.run("update-joints", {"force": True})
        got = c.cli("upd_settled", [*parametric(45), c.joint(both), update, update, set_h(60), update, MEASURE, force, MEASURE])
        a, b, d, e = got[-7]["result"], got[-6]["result"], got[-4]["result"], got[-2]["result"]
        c.report(a["updated"] == 1 and b.get("unchanged") is True and b["updated"] == 0,
                 f"a second update with nothing changed makes no joint again: {a['updated']} then {b}")
        c.report(d["updated"] == 1 and bodies_of(got[-3]) == fresh, f"after a change, the next update makes the joint again: {d['updated']}")
        c.report(e["updated"] == 1 and not e.get("unchanged") and bodies_of(got[-1]) == fresh, f"force makes the joint again with nothing changed: {e['updated']}")
        got = c.cli("upd_undo", [*parametric(45), c.joint(both), set_h(60), MEASURE, update, {"command": "edit.undo", "params": {}}, MEASURE])
        c.report(bodies_of(got[-1]) == bodies_of(got[-4]), "one undo takes Update joints back whole")

        # A Domino is made again with its size, rounded ends and second mortise width.
        dom = c.run("loose-tenon", {"first": "Rail", "second": "Stile", "domino": "6x40", "domino_fit": "wide"})
        fresh_dom = bodies_of(c.cli("upd_dom_fresh", [*parametric(60), dom, MEASURE])[-1])
        got = c.cli("upd_dom", [*parametric(45), dom, set_h(60), update, MEASURE])
        c.report(bodies_of(got[-1]) == fresh_dom and "Domino 1" in fresh_dom, f"a Domino updates with its size and fit: {sorted(bodies_of(got[-1]))}")

        # A joint made from the selection is made again without it.
        pick = [select("Rail", "Stile"), c.joint({}), {"command": "select.set", "params": {"items": []}}]
        got = c.cli("upd_selected", [*parametric(45), *pick, set_h(60), update, MEASURE])
        c.report(bodies_of(got[-1]) == fresh, f"a joint made from the selection updates with no selection: {bodies_of(got[-1])}")

        # Two joints on one stile, one of them adding bodies: each is made again once.
        fresh2 = c.cli("upd_two_fresh", [*parametric(60), c.joint(both), box("Rail2", [35, 0, 100], (360, 18, 45)), c.run("dowels", {"first": "Rail2", "second": "Stile"}), MEASURE])[-1]
        got = c.cli("upd_two", [*parametric(45), c.joint(both), box("Rail2", [35, 0, 100], (360, 18, 45)), c.run("dowels", {"first": "Rail2", "second": "Stile"}), set_h(60), update, MEASURE])
        c.report(bodies_of(got[-1]) == bodies_of(fresh2), f"a tenon and dowels update, the dowels made once: {sorted(bodies_of(got[-1]))}")

        # A joint whose folder was deleted by hand is forgotten.
        got = c.cli("upd_deleted", [*parametric(45), c.joint(both), stored])
        folder = got[-1]["document"]["joints"][0]["folder"]
        got = c.cli("upd_deleted2", [*parametric(45), c.joint(both), {"command": "timeline.delete", "params": {"groups": [folder]}}, update, MEASURE, stored])
        c.report(got[-3]["result"]["forgotten"] == 1 and bodies_of(got[-2]) == {"Stile": STILE, "Rail": RAIL} and got[-1]["document"]["joints"] == [],
                 f"a joint deleted by hand is forgotten: {got[-3]['result']}")

        # The old folder goes: an update leaves no empty folder behind.
        code, output = c.cli("upd_old_folder", [*parametric(45), c.joint(both), set_h(60), update, {"command": "timeline.inspect", "params": {"group": folder}}], expect_error=True)
        c.report(code != 0 and f"no timeline folder {folder}" in output, f"Update joints deletes the joint's old folder: {output.strip()[-80:]}")

        # A joint that no longer fits stops the update, naming it, and changes nothing.
        code, output = c.cli("upd_far", [*parametric(45), c.joint(both), {"command": "parameters.change", "params": {"name": "X", "expression": "300"}}, update], expect_error=True)
        c.report(code != 0 and "cannot be made again" in output and "Mortise and tenon: Rail / Stile" in output, f"a rail moved 265 mm away stops Update joints: {output.strip()[-200:]}")

        # Edit a joint: a script gives the settings to change; the joint is made again in place.
        edit = lambda params: c.run("edit-joint", params)
        short = bodies_of(c.cli("edit_fresh", [*parametric(45), c.joint({**both, "length": 20}), MEASURE])[-1])
        got = c.cli("edit_settings", [*parametric(45), c.joint(both), edit({"joint": "rail / stile", "settings": {"length": 20}}), MEASURE, stored])
        kept = got[-1]["document"]["joints"]
        c.report(bodies_of(got[-2]) == short and len(kept) == 1 and kept[0]["params"]["length"] == 20,
                 f"Edit joint makes a 25 mm tenon 20 mm long, in place: {bodies_of(got[-2])} ({len(kept)} joint kept)")
        got = c.cli("edit_selected", [*parametric(45), c.joint(both), select("Stile", "Rail"), edit({"settings": {"length": 20}}), MEASURE])
        c.report(bodies_of(got[-1]) == short, f"Edit joint finds the joint of the two selected boards: {bodies_of(got[-1])}")
        # The edited joint keeps its place: an update after it gives the same result.
        both2 = {"rail": "Rail2", "stile": "Stile"}
        two_joints = [*parametric(45), c.joint(both), box("Rail2", [35, 0, 100], (360, 18, 45)), c.joint(both2)]
        fresh3 = bodies_of(c.cli("edit_two_fresh", [*parametric(60), c.joint({**both, "length": 20}), box("Rail2", [35, 0, 100], (360, 18, 45)), c.joint(both2), MEASURE])[-1])
        got = c.cli("edit_two", [*two_joints, edit({"joint": "Rail / Stile", "settings": {"length": 20}}), set_h(60), update, MEASURE, stored])
        order = [k["name"] for k in got[-1]["document"]["joints"]]
        c.report(bodies_of(got[-2]) == fresh3 and order == ["Mortise and tenon: Rail / Stile", "Mortise and tenon: Rail2 / Stile"],
                 f"an edited joint keeps its place, and updates with its new settings: {order}")
        code, output = c.cli("edit_form", [*parametric(45), c.joint(both), edit({"joint": "Rail"})], expect_error=True)
        c.report(code != 0 and "interactive" in output.lower(), f"without settings, a script is told the form needs the window: {output.strip()[-160:]}")
        code, output = c.cli("edit_two_match", [*two_joints, edit({"joint": "Stile", "settings": {"length": 20}})], expect_error=True)
        c.report(code != 0 and "2 joints match" in output, f"a name matching two joints is refused: {output.strip()[-160:]}")
        code, output = c.cli("edit_none", [*parametric(45), c.joint(both), edit({"joint": "Shelf", "settings": {}})], expect_error=True)
        c.report(code != 0 and "no joint matches" in output and "Rail / Stile" in output, f"an unknown joint is refused, naming the joints: {output.strip()[-160:]}")

        # SolidCraft allows a plugin 256 host calls in one run. An update with more joints than
        # that makes what fits, keeps the rest, and the next update finishes them.
        many = [box("Stile", [0, 0, 0], (35, 35, 830), first=True)]
        many += [c.run("groove", {"body": "Stile", "face": "+x", "along": "z", "width": 0.3, "depth": 1, "offset": 1 + 0.5 * k}) for k in range(60)]
        before = bodies_of(c.cli("upd_many_fresh", [*many, MEASURE])[-1])
        got = c.cli("upd_many", [*many, update, update, update, MEASURE])
        first, second, third = got[-4]["result"], got[-3]["result"], got[-2]["result"]
        c.report(first["remaining"] > 0 and first["updated"] + first["remaining"] == 60 and second["remaining"] == 0
                 and second["updated"] == first["remaining"] and bodies_of(got[-1]) == before,
                 f"60 grooves update in two runs, within the host's 256 calls: {first['updated']} then {second['updated']}")
        c.report(third.get("unchanged") is True and bodies_of(got[-1]) == before, f"a third run, with nothing left or changed, takes nothing apart: {third}")

        # The joints are saved with the design.
        saved_upd = work / "update.sc"
        c.cli("upd_save", [*parametric(45), c.joint(both)], "--save", str(saved_upd))
        got = c.cli("upd_reopen", [set_h(60), update, MEASURE], "--in", str(saved_upd))
        c.report(bodies_of(got[-1]) == fresh, f"a saved design reopened updates its joints: {bodies_of(got[-1])}")

        # 7. A picture, for a human to look at.
        picture = work / "joint.png"
        cam = json.dumps({"target": [35, 9, 422], "yaw": 0.9, "pitch": 0.35, "distance": 260})
        r = subprocess.run([c.binary, "snapshot", "--in", str(saved), "--out", str(picture), "--no-grid", "--body", "Stile:opacity=0.3", "--camera", cam], capture_output=True, text=True)
        c.report(r.returncode == 0 and picture.exists(), f"picture written{f' to {picture}' if keep else ''}")

        raise SystemExit(1 if c.failed else 0)


if __name__ == "__main__":
    main()
