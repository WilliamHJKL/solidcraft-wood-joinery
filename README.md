# Wood joinery for SolidCraft

A plugin for [SolidCraft](https://github.com/tangency-labs/solidcraft) that makes woodworking
joints between two boards. This first version makes one joint: the mortise and tenon.

Pick the rail and the stile. The plugin adds the tenon to the end of the rail and cuts the
matching mortise in the stile, in one undo step. The rail's bounding box then includes its
tenons, so a cut list reads the true length of the blank.

The result is two ordinary box features in the timeline. A design made with the plugin opens
and rebuilds without it.

## Variants

| `variant` | What it makes |
|---|---|
| `classic` (default) | A blind mortise. The tenon has a shoulder on each edge. |
| `haunched` | A blind mortise with a short haunch, on the rail edge nearest the end of the stile. |
| `through` | A tenon through the whole stile. |
| `bridle` | An open slot at the end of the stile. The tongue runs the full width of the rail. |

## Defaults

Every size is optional, in millimetres. Left out, it follows common cabinetmaking practice:

| Input | Default |
|---|---|
| `thickness` | A third of the rail thickness |
| `length` | Two thirds of the stile, to the nearest 5 mm. The whole stile for `through` and `bridle` |
| `shoulder` | The cheek width, at most a fifth of the rail width. None for `bridle` |
| `haunch_width`, `haunch_depth` | A fifth of the rail width; the tenon thickness |
| `glue_relief` | 2 mm under a blind tenon |
| `clearance` | 0 |
| `offset` | 0: the tenon is centred in the rail thickness |

Example: an 18 x 45 mm rail against a 35 x 35 mm stile gets a 6 x 33 x 25 mm tenon in a
27 mm deep mortise.

The plugin refuses a joint it cannot make, and says why: a blind tenon that would come through
the stile, a tenon as thick as the rail, a rail that does not touch the stile, a bridle away
from the end of the stile.

## Limits of this version

- Both boards must lie along the X, Y or Z axis. The plugin reads each body's bounding box, so
  a board at an angle is not supported.
- The stile depth is its bounding box along the joint, so a stile with a foot or a return is
  measured too deep. Set `length` yourself in that case.
- Sizes are numbers. They do not follow a parameter: after a change, undo the joint and run
  the plugin again. A joint that rebuilds by itself needs the parametric plugin features that
  SolidCraft's plugin RFC plans for a later phase.

## Build

Needs Rust with the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
and Python 3.

```sh
./build.sh
```

This writes `dist/wood-joinery/` (the plugin folder) and `dist/wood-joinery.zip`.

## Install and run

In SolidCraft: Preferences › Plugins › Install plugin…, and choose `dist/wood-joinery.zip`.

From a script, with `solidcraft-cli`: inspect the plugin, then pass the `approval` value it
returns to `plugin.run`.

```json
[{"command": "plugin.inspect", "params": {"path": "dist/wood-joinery"}}]
```

```json
[{"command": "plugin.run", "params": {
  "path": "dist/wood-joinery",
  "approval": "THE_APPROVAL_FROM_INSPECT",
  "entry": "mortise-tenon",
  "params": {"rail": ["Rail"], "stile": ["Stile"], "variant": "haunched"}
}}]
```

The plugin calls two SolidCraft commands, `inspect.measure` and `solid.box`. It asks for no
file, clock or network access.

## Test

```sh
cargo test                                   # the joint arithmetic
tests/check.py /path/to/solidcraft-cli       # the packed plugin in a real SolidCraft
```

## Licence

MIT OR Apache-2.0, like SolidCraft. `wit/plugin.wit` is SolidCraft's plugin interface
(`craft:plugin@0.1.0`), copied from its repository under the same licence.
