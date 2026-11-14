#!/usr/bin/env python3
"""End-to-end check against a real SolidCraft: installs nothing, runs the packed plugin from dist/.

Usage: tests/check.py /path/to/solidcraft-cli

Builds a 35 x 35 x 830 mm stile and a 360 x 18 x 45 mm rail, runs each variant of the joint and
compares the two volumes with values worked out by hand.
"""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PLUGIN = ROOT / "dist" / "wood-joinery"
STILE, RAIL = 35 * 35 * 830, 360 * 18 * 45

# variant -> (rail lower edge Z, extra params, tenon volume, mortise volume)
CASES = {
    "classic": (400, {}, 6 * 33 * 25, 6 * 33 * 27),
    # Rail 20 mm under the top of the stile. Haunch 9 x 6, main tenon 30 wide.
    "haunched": (765, {}, 6 * (30 * 25 + 9 * 6), 6 * (30 * 27 + 9 * 6)),
    "through": (400, {}, 6 * 33 * 35, 6 * 33 * 35),
    # Rail flush with the top of the stile.
    "bridle": (785, {}, 6 * 45 * 35, 6 * 45 * 35),
}


def cli(binary, work, name, commands, *extra):
    script = work / f"{name}.json"
    out = work / f"{name}.out.json"
    script.write_text(json.dumps(commands))
    r = subprocess.run([binary, "run", str(script), "--results", "--json-out", str(out), *extra], capture_output=True, text=True, cwd=work)
    if r.returncode != 0:
        raise SystemExit(f"{name}: solidcraft-cli failed\n{r.stdout}\n{r.stderr}")
    return json.loads(out.read_text())


def main():
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    binary = sys.argv[1]
    if not (PLUGIN / "plugin.wasm").exists():
        raise SystemExit("run ./build.sh first")
    failed = False
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        inspected = cli(binary, work, "inspect", [{"command": "plugin.inspect", "params": {"path": str(PLUGIN)}}])
        approval = _find(inspected, "approval")
        if not approval:
            raise SystemExit(f"no approval hash in {json.dumps(inspected)[:400]}")
        for variant, (z, extra, tenon, mortise) in CASES.items():
            params = {"rail": ["Rail"], "stile": ["Stile"], "variant": variant, **extra}
            results = cli(binary, work, variant, [
                {"command": "solid.box", "params": {"corner": [0, 0, 0], "length": 35, "width": 35, "height": 830, "body_name": "Stile"}},
                {"command": "solid.box", "params": {"corner": [35, 0, z], "length": 360, "width": 18, "height": 45, "operation": "new", "body_name": "Rail"}},
                {"command": "plugin.run", "params": {"path": str(PLUGIN), "approval": approval, "entry": "mortise_tenon", "params": params}},
                {"command": "inspect.measure", "params": {}},
            ])
            bodies = {b["name"]: b["volume_mm3"] for b in _find(results, "bodies") or []}
            got = (bodies.get("Rail", 0) - RAIL, STILE - bodies.get("Stile", 0))
            ok = abs(got[0] - tenon) < 1 and abs(got[1] - mortise) < 1 and len(bodies) == 2
            failed |= not ok
            print(f"{'PASS' if ok else 'FAIL'} {variant}: tenon {got[0]:.1f} (expected {tenon}), mortise {got[1]:.1f} (expected {mortise}), {len(bodies)} bodies")
    raise SystemExit(1 if failed else 0)


def _find(value, key):
    """The last `key` anywhere in a nested result."""
    found = None
    if isinstance(value, dict):
        for k, v in value.items():
            if k == key:
                found = v
            else:
                found = _find(v, key) or found
    elif isinstance(value, list):
        for v in value:
            found = _find(v, key) or found
    return found


if __name__ == "__main__":
    main()
