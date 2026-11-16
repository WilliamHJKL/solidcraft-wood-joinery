//! The WebAssembly side: reads the entry's JSON, calls SolidCraft's commands, reports the result.

use serde_json::{json, Value};

use crate::boards;
use crate::biscuit;
use crate::dovetail;
use crate::dowel::{self, Bore};
use crate::finger;
use crate::groove;
use crate::housing;
use crate::joint::{plan, Aabb, Block, Options, Position, Variant};
use crate::lap;
use crate::loose;
use crate::miter;
use crate::scarf;
use crate::screw;
use crate::slant::Tool;
use crate::tongue;

wit_bindgen::generate!({ world: "command-plugin", path: "wit" });

use craft::plugin::commands;
use craft::plugin::types::{FieldFailure, PluginError};

struct Plugin;

export!(Plugin);

impl exports::craft::plugin::entry::Guest for Plugin {
    fn execute(entry: String, params: String) -> Result<String, PluginError> {
        match entry.as_str() {
            "mortise-tenon" => mortise_tenon(&params),
            "groove" => groove(&params),
            "half-lap" => half_lap(&params),
            "dowels" => dowels(&params),
            "box-joint" => box_joint(&params),
            "loose-tenon" => loose_tenon(&params),
            "housing" => housing(&params),
            "miter" => miter(&params),
            "tongue-groove" => tongue_groove(&params),
            "scarf" => scarf(&params),
            "dovetail" => dovetail(&params),
            "screws" => screws(&params),
            "biscuits" => biscuits(&params),
            other => Err(invalid("entry", format!("unknown entry `{other}`"))),
        }
    }
}

fn invalid(field: &str, reason: impl Into<String>) -> PluginError {
    PluginError::InvalidInput(FieldFailure { field: field.into(), reason: reason.into() })
}

/// One body name. The form sends plain text; a script may also send a list with one name.
fn body(p: &Value, field: &str) -> Result<String, PluginError> {
    let name = match p.get(field) {
        Some(Value::String(name)) => Some(name),
        Some(Value::Array(names)) => match names.as_slice() {
            [Value::String(name)] => Some(name),
            _ => None,
        },
        _ => None,
    };
    match name.map(|n| n.trim()) {
        Some(name) if !name.is_empty() => Ok(name.to_owned()),
        _ => Err(invalid(field, "expected the name of one body")),
    }
}

/// The script value of a choice. A translated package shows "classique (classic)": the value is
/// then the text in the last brackets.
fn choice(text: &str) -> &str {
    let text = text.trim();
    match (text.rfind('('), text.strip_suffix(')')) {
        (Some(open), Some(inner)) => inner[open + 1..].trim(),
        _ => text,
    }
}

/// A body name, or `None` when the field is left empty.
fn named(p: &Value, field: &str) -> Result<Option<String>, PluginError> {
    match p.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(name)) if name.is_empty() => Ok(None),
        Some(Value::Array(names)) if names.is_empty() => Ok(None),
        Some(_) => body(p, field).map(Some),
    }
}

/// What is selected in SolidCraft, in the order it was picked.
fn selection(detail: bool) -> Result<Vec<Value>, PluginError> {
    let got = run("selection.get", json!({ "detail": detail }))?;
    Ok(got.get("items").and_then(Value::as_array).cloned().unwrap_or_default())
}

/// The bodies that are selected, or that own a selected face or edge, each one once.
fn selected_bodies() -> Result<Vec<String>, PluginError> {
    let mut names: Vec<String> = Vec::new();
    for item in selection(false)? {
        let key = if item.get("type").and_then(Value::as_str) == Some("body") { "name" } else { "body" };
        if let Some(name) = item.get(key).and_then(Value::as_str) {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_owned());
            }
        }
    }
    Ok(names)
}

/// The two bodies of a joint: typed in the two fields, or the two that are selected, in order.
fn two(p: &Value, first: &str, second: &str) -> Result<(String, String), PluginError> {
    let (a, b) = match (named(p, first)?, named(p, second)?) {
        (Some(a), Some(b)) => (a, b),
        (None, None) => {
            let mut picked = selected_bodies()?;
            if picked.len() != 2 {
                return Err(invalid(
                    first,
                    format!(
                        "select two bodies, the {first} one first, or type both names ({} selected)",
                        picked.len()
                    ),
                ));
            }
            let b = picked.remove(1);
            (picked.remove(0), b)
        }
        (None, Some(_)) => return Err(invalid(first, "expected the name of one body")),
        (Some(_), None) => return Err(invalid(second, "expected the name of one body")),
    };
    if a == b {
        return Err(invalid(second, "the joint needs two different bodies"));
    }
    Ok((a, b))
}

fn length(p: &Value, field: &str) -> Result<Option<f64>, PluginError> {
    match p.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().map(Some).ok_or_else(|| invalid(field, "expected a number of millimetres")),
    }
}

fn count(p: &Value, field: &str) -> Result<Option<u32>, PluginError> {
    match p.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|k| u32::try_from(k).ok())
            .map(Some)
            .ok_or_else(|| invalid(field, "expected a whole number")),
    }
}

fn flag(p: &Value, field: &str, default: bool) -> Result<bool, PluginError> {
    match p.get(field) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_bool().ok_or_else(|| invalid(field, "expected true or false")),
    }
}

/// The two boards of a joint: their names, bounding boxes and volumes, and how far the first
/// one was moved.
struct Pair {
    first: String,
    second: String,
    a: Aabb,
    b: Aabb,
    volume_a: f64,
    volume_b: f64,
    moved: [f64; 3],
}

/// What a body must look like once the entry has run.
enum Want {
    /// Smaller than this volume: a cut took wood away.
    Cut(f64),
    /// This bounding box: the body was moved, or wood was added to it.
    Bounds(Aabb),
}

/// Checks that each body ended up as planned. SolidCraft takes the sizes of a new feature in
/// the frame of the active component, so a joint planned from world sizes can land beside the
/// boards. The error undoes the whole entry.
fn landed(checks: &[(&str, Want)]) -> Result<(), PluginError> {
    let names: Vec<&str> = checks.iter().map(|(name, _)| *name).collect();
    let measured = run("inspect.measure", json!({ "bodies": names }))?;
    for (name, want) in checks {
        let ok = match want {
            Want::Cut(before) => volume(&measured, name)? < before - 1e-6,
            Want::Bounds(want) => {
                let got = bbox(&measured, name)?;
                (0..3).all(|i| (got.min[i] - want.min[i]).abs() < 0.01 && (got.max[i] - want.max[i]).abs() < 0.01)
            }
        };
        if !ok {
            return Err(invalid(
                "body",
                format!(
                    "the joint did not land on `{name}` as planned, so nothing was changed. This happens when a \
                     component other than the top one is active, or when a board is in a component that was \
                     turned or moved. Activate the top component, or turn the body itself and not its \
                     component, then run the command again"
                ),
            ));
        }
    }
    Ok(())
}

/// The box around `body` and the blocks added to it.
fn grown(body: &Aabb, blocks: &[Block]) -> Aabb {
    let mut all = *body;
    for block in blocks {
        for i in 0..3 {
            all.min[i] = all.min[i].min(block.corner[i]);
            all.max[i] = all.max[i].max(block.corner[i] + block.size[i]);
        }
    }
    all
}

/// Checks that each name is a body of the design, and names the bodies when one is not: the
/// name may be mistyped, or a cut may have split the board in two bodies with new names.
fn known(names: &[(&str, &str)]) -> Result<(), PluginError> {
    let listed = run("inspect.measure", json!({ "names_only": true }))?;
    let bodies: Vec<&str> = listed
        .get("bodies")
        .and_then(Value::as_array)
        .map(|bodies| bodies.iter().filter_map(|b| b.get("name").and_then(Value::as_str)).collect())
        .unwrap_or_default();
    for (field, name) in names {
        if !bodies.contains(name) {
            let mut shown = bodies.iter().take(12).map(|b| format!("`{b}`")).collect::<Vec<_>>().join(", ");
            if bodies.len() > 12 {
                shown.push_str(&format!(" and {} more", bodies.len() - 12));
            }
            return Err(invalid(field, format!("unknown body name `{name}`. The bodies of this design are: {shown}")));
        }
    }
    Ok(())
}

/// The two boards of a joint. With `align` on, the first board is moved by what `shift` returns
/// before anything is cut.
fn pair(p: &Value, names: (&str, &str), shift: fn(&Aabb, &Aabb) -> Result<[f64; 3], String>) -> Result<Pair, PluginError> {
    let (first, second) = two(p, names.0, names.1)?;
    known(&[(names.0, &first), (names.1, &second)])?;
    let measured = run("inspect.measure", json!({ "bodies": [first, second] }))?;
    let (mut a, b) = (bbox(&measured, &first)?, bbox(&measured, &second)?);
    let (volume_a, volume_b) = (volume(&measured, &first)?, volume(&measured, &second)?);
    let mut moved = [0.0; 3];
    if flag(p, "align", false)? {
        let r = |v: f64| (v * 1e6).round() / 1e6;
        let by = shift(&a, &b).map_err(|reason| invalid("align", reason))?.map(r);
        if by.iter().any(|d| *d != 0.0) {
            run("solid.move", json!({ "bodies": [first], "translate": by }))?;
            for (i, d) in by.iter().enumerate() {
                a.min[i] += d;
                a.max[i] += d;
            }
            moved = by;
            landed(&[(&first, Want::Bounds(a))])?;
        }
    }
    Ok(Pair { first, second, a, b, volume_a, volume_b, moved })
}

fn run(command: &str, args: Value) -> Result<Value, PluginError> {
    let out = commands::run(command, &args.to_string())?;
    serde_json::from_str(&out).map_err(|e| PluginError::Trap(format!("{command} returned invalid JSON: {e}")))
}

fn measured_body<'a>(measured: &'a Value, name: &str) -> Result<&'a Value, PluginError> {
    measured
        .get("bodies")
        .and_then(Value::as_array)
        .and_then(|bodies| bodies.iter().find(|b| b.get("name").and_then(Value::as_str) == Some(name)))
        .ok_or_else(|| invalid("body", format!("no body named `{name}`")))
}

fn volume(measured: &Value, name: &str) -> Result<f64, PluginError> {
    measured_body(measured, name)?
        .get("volume_mm3")
        .and_then(Value::as_f64)
        .ok_or_else(|| PluginError::Trap(format!("no volume for `{name}`")))
}

fn bbox(measured: &Value, name: &str) -> Result<Aabb, PluginError> {
    let found = measured_body(measured, name)?;
    let corner = |key: &str| -> Option<[f64; 3]> {
        let v = found.get("bbox")?.get(key)?.as_array()?;
        Some([v.first()?.as_f64()?, v.get(1)?.as_f64()?, v.get(2)?.as_f64()?])
    };
    match (corner("min"), corner("max")) {
        (Some(min), Some(max)) => Ok(Aabb { min, max }),
        _ => Err(PluginError::Trap(format!("no bounding box for `{name}`"))),
    }
}

fn add(block: &Block, operation: &str, target: &str, name: &str) -> Result<(), PluginError> {
    // Six decimals: the box inputs are millimetres, and this keeps float noise out of the design.
    let r = |v: f64| (v * 1e6).round() / 1e6;
    run(
        "solid.box",
        json!({
            "corner": [r(block.corner[0]), r(block.corner[1]), r(block.corner[2])],
            "length": r(block.size[0]),
            "width": r(block.size[1]),
            "height": r(block.size[2]),
            "operation": operation,
            "targets": [target],
            "name": name,
        }),
    )
    .map(|_| ())
}

/// `count` body names that are free, as `Dowel 1`, `Dowel 2`... SolidCraft renames a body whose
/// name is taken, so the numbers go past the ones already there.
fn free_names(prefix: &str, count: usize) -> Result<Vec<String>, PluginError> {
    let listed = run("inspect.measure", json!({ "names_only": true }))?;
    let taken: Vec<&str> = listed
        .get("bodies")
        .and_then(Value::as_array)
        .map(|bodies| bodies.iter().filter_map(|b| b.get("name").and_then(Value::as_str)).collect())
        .unwrap_or_default();
    let mut names = Vec::new();
    let mut number = 0;
    while names.len() < count {
        number += 1;
        let name = format!("{prefix} {number}");
        if !taken.contains(&name.as_str()) {
            names.push(name);
        }
    }
    Ok(names)
}

/// A new body from a block.
fn body_from(block: &Block, body_name: &str, name: &str) -> Result<(), PluginError> {
    let r = |v: f64| (v * 1e6).round() / 1e6;
    run(
        "solid.box",
        json!({
            "corner": [r(block.corner[0]), r(block.corner[1]), r(block.corner[2])],
            "length": r(block.size[0]),
            "width": r(block.size[1]),
            "height": r(block.size[2]),
            "operation": "new",
            "body_name": body_name,
            "name": name,
        }),
    )
    .map(|_| ())
}

/// A cut at an angle: a tool box along the axes, turned about a line, then cut from `target`.
/// The cut uses the tool up.
fn slanted_cut(target: &str, tool: &Tool, tool_name: &str, label: &str) -> Result<(), PluginError> {
    let r = |v: f64| (v * 1e6).round() / 1e6;
    body_from(&tool.block, tool_name, &format!("{label} tool"))?;
    run(
        "solid.move",
        json!({ "bodies": [tool_name], "origin": tool.origin.map(r), "axis": tool.axis, "angle": r(tool.angle), "name": format!("{label} tool turn") }),
    )?;
    run("solid.combine", json!({ "target": target, "tools": [tool_name], "operation": "cut", "name": label })).map(|_| ())
}

fn bore(b: &Bore, operation: Value, name: &str) -> Result<(), PluginError> {
    let r = |v: f64| (v * 1e6).round() / 1e6;
    let mut axis = [0.0; 3];
    axis[b.axis] = b.dir;
    let mut args = json!({
        "base": [r(b.base[0]), r(b.base[1]), r(b.base[2])],
        "axis": axis,
        "diameter": r(b.diameter),
        "height": r(b.height),
        "name": name,
    });
    if let (Some(args), Value::Object(operation)) = (args.as_object_mut(), operation) {
        args.extend(operation);
    }
    run("solid.cylinder", args).map(|_| ())
}

fn mortise_tenon(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first: rail, second: stile, a: rail_box, b: stile_box, volume_b: stile_volume, moved, .. } =
        pair(&p, ("rail", "stile"), boards::align_end)?;
    let variant = match p.get("variant") {
        None | Some(Value::Null) => Variant::Classic,
        Some(v) => v
            .as_str()
            .map(choice)
            .and_then(Variant::parse)
            .ok_or_else(|| invalid("variant", "expected classic, haunched, through or bridle"))?,
    };
    let options = Options {
        variant,
        thickness: length(&p, "thickness")?,
        length: length(&p, "length")?,
        shoulder: length(&p, "shoulder")?,
        haunch_width: length(&p, "haunch_width")?,
        haunch_depth: length(&p, "haunch_depth")?,
        glue_relief: length(&p, "glue_relief")?,
        clearance: length(&p, "clearance")?,
        offset: length(&p, "offset")?,
        double: flag(&p, "double", false)?,
        position: match p.get("position").filter(|v| !v.is_null() && v.as_str() != Some("")) {
            None => Position::Centre,
            Some(v) => v.as_str().map(choice).and_then(Position::parse).ok_or_else(|| invalid("position", "expected centre, low or high"))?,
        },
        pegs: count(&p, "pegs")?.unwrap_or(0),
        peg_diameter: length(&p, "peg_diameter")?,
    };

    let plan = plan(rail_box, stile_box, &options).map_err(|reason| invalid("rail", reason))?;

    let mut sizes = json!({
        "variant": variant.name(),
        "rail": rail,
        "stile": stile,
        "tenon_thickness_mm": plan.thickness,
        "tenon_length_mm": plan.length,
        "tenon_width_mm": plan.width,
        "shoulder_mm": plan.shoulder,
        "glue_relief_mm": plan.glue_relief,
        "mortise_depth_mm": plan.mortise_depth,
    });
    if let Some((width, depth)) = plan.haunch {
        sizes["haunch_width_mm"] = json!(width);
        sizes["haunch_depth_mm"] = json!(depth);
    }
    // With a haunch, every second block is the haunch of the tenon before it.
    let haunch = |i: usize| plan.haunch.is_some() && i % 2 == 1;
    for (i, block) in plan.tenon.iter().enumerate() {
        add(block, "join", &rail, if haunch(i) { "Haunch" } else { "Tenon" })?;
    }
    for (i, block) in plan.mortise.iter().enumerate() {
        add(block, "cut", &stile, if haunch(i) { "Haunch mortise" } else { "Mortise" })?;
    }
    landed(&[(&rail, Want::Bounds(grown(&rail_box, &plan.tenon))), (&stile, Want::Cut(stile_volume))])?;
    // Each peg: a hole through the stile and the tenon, then the peg itself.
    for (peg, name) in plan.pegs.iter().zip(free_names("Peg", plan.pegs.len())?) {
        let mut out = peg.base;
        out[peg.axis] -= boards::OVERCUT;
        let hole = Bore { base: out, axis: peg.axis, dir: 1.0, diameter: peg.diameter, height: peg.height + 2.0 * boards::OVERCUT };
        bore(&hole, json!({ "operation": "cut", "targets": [stile, rail] }), "Peg hole")?;
        let body = Bore { base: peg.base, axis: peg.axis, dir: 1.0, diameter: peg.diameter, height: peg.height };
        bore(&body, json!({ "operation": "new", "body_name": name }), "Peg")?;
    }
    sizes["pegs"] = json!(plan.pegs.len());

    sizes["moved_rail_mm"] = json!(moved);
    Ok(sizes.to_string())
}

fn groove(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let typed = named(&p, "body")?;
    let (board, face) = match p.get("face").filter(|f| !f.is_null() && f.as_str() != Some("")) {
        Some(face) => {
            let face = face
                .as_str()
                .map(choice)
                .and_then(groove::Face::parse)
                .ok_or_else(|| invalid("face", "expected +x, -x, +y, -y, +z or -z"))?;
            let board = match typed {
                Some(board) => board,
                None => match selected_bodies()?.as_slice() {
                    [board] => board.clone(),
                    _ => return Err(invalid("body", "type the name of the board, or select it alone")),
                },
            };
            (board, face)
        }
        None => {
            let (board, face) = picked_face()?;
            if typed.is_some_and(|name| name != board) {
                return Err(invalid("face", "the selected face is not on the board that is named"));
            }
            (board, face)
        }
    };
    let options = groove::Options {
        width: length(&p, "width")?,
        depth: length(&p, "depth")?,
        offset: length(&p, "offset")?,
        stop_start: length(&p, "stop_start")?,
        stop_end: length(&p, "stop_end")?,
        along: match p.get("along").filter(|a| !a.is_null() && a.as_str() != Some("")) {
            None => None,
            Some(a) => Some(a.as_str().map(choice).and_then(groove::axis).ok_or_else(|| invalid("along", "expected x, y or z"))?),
        },
    };
    known(&[("body", &board)])?;
    let measured = run("inspect.measure", json!({ "bodies": [board] }))?;
    let plan = groove::plan(bbox(&measured, &board)?, face, &options).map_err(|reason| invalid("body", reason))?;
    add(&plan.cut, "cut", &board, "Groove")?;
    landed(&[(&board, Want::Cut(volume(&measured, &board)?))])?;
    Ok(json!({
        "body": board,
        "width_mm": plan.width,
        "depth_mm": plan.depth,
        "length_mm": plan.length,
    })
    .to_string())
}

/// The one face that is selected: its body, and which side of the bounding box it is.
fn picked_face() -> Result<(String, groove::Face), PluginError> {
    let hint = "choose a face in the list, or select one flat face of the board";
    let items = selection(true)?;
    let [item] = items.as_slice() else {
        return Err(invalid("face", format!("{hint} ({} items selected)", items.len())));
    };
    let board = item.get("body").and_then(Value::as_str);
    let normal = item.get("surface").and_then(|s| s.get("normal")).and_then(Value::as_array);
    let (Some("face"), Some(board), Some(normal)) = (item.get("type").and_then(Value::as_str), board, normal) else {
        return Err(invalid("face", hint));
    };
    let n: Vec<f64> = normal.iter().filter_map(Value::as_f64).collect();
    let axis = (0..n.len().min(3)).find(|&a| n[a].abs() > 0.999_999);
    match (n.len(), axis) {
        (3, Some(axis)) => Ok((board.to_owned(), groove::Face { axis, high: n[axis] > 0.0 })),
        _ => Err(invalid("face", "the selected face must be flat and square to the X, Y or Z axis")),
    }
}

fn half_lap(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_a, volume_b, moved } = pair(&p, ("first", "second"), lap::align)?;
    let options = lap::Options { depth: length(&p, "depth")?, flip: flag(&p, "flip", false)? };
    let plan = lap::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    add(&plan.cut_first, "cut", &first, "Half lap")?;
    add(&plan.cut_second, "cut", &second, "Half lap")?;
    landed(&[(&first, Want::Cut(volume_a)), (&second, Want::Cut(volume_b))])?;
    Ok(json!({
        "first": first,
        "second": second,
        "thickness_mm": plan.thickness,
        "depth_first_mm": plan.depth_first,
        "depth_second_mm": plan.depth_second,
        "moved_first_mm": moved,
    })
    .to_string())
}

fn dowels(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_a, volume_b, moved } = pair(&p, ("first", "second"), boards::align_end)?;
    let options = dowel::Options {
        diameter: length(&p, "diameter")?,
        length: length(&p, "length")?,
        count: count(&p, "count")?,
        glue_relief: length(&p, "glue_relief")?,
    };
    let with_bodies = flag(&p, "dowel_bodies", true)?;
    let plan = dowel::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    for hole in &plan.holes_first {
        bore(hole, json!({ "operation": "cut", "targets": [first] }), "Dowel hole")?;
    }
    for hole in &plan.holes_second {
        bore(hole, json!({ "operation": "cut", "targets": [second] }), "Dowel hole")?;
    }
    landed(&[(&first, Want::Cut(volume_a)), (&second, Want::Cut(volume_b))])?;
    if with_bodies {
        for (peg, name) in plan.dowels.iter().zip(free_names("Dowel", plan.dowels.len())?) {
            bore(peg, json!({ "operation": "new", "body_name": name }), "Dowel")?;
        }
    }
    Ok(json!({
        "first": first,
        "second": second,
        "count": plan.dowels.len(),
        "diameter_mm": plan.diameter,
        "length_mm": plan.length,
        "in_first_mm": plan.in_first,
        "in_second_mm": plan.in_second,
        "dowel_bodies": with_bodies,
        "moved_first_mm": moved,
    })
    .to_string())
}

fn box_joint(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_b, moved, .. } = pair(&p, ("first", "second"), finger::align)?;
    let options = finger::Options { count: count(&p, "count")?, swap: flag(&p, "swap", false)? };
    let plan = finger::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    for socket in &plan.sockets {
        add(socket, "cut", &second, "Finger socket")?;
    }
    for finger in &plan.fingers {
        add(finger, "join", &first, "Finger")?;
    }
    landed(&[(&first, Want::Bounds(grown(&a, &plan.fingers))), (&second, Want::Cut(volume_b))])?;
    Ok(json!({
        "first": first,
        "second": second,
        "count": plan.count,
        "fingers_on_first": plan.fingers.len(),
        "finger_width_mm": plan.finger_width,
        "finger_length_mm": plan.finger_length,
        "moved_first_mm": moved,
    })
    .to_string())
}

fn loose_tenon(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_a, volume_b, moved } = pair(&p, ("first", "second"), boards::align_end)?;
    let options = loose::Options {
        thickness: length(&p, "thickness")?,
        width: length(&p, "width")?,
        length: length(&p, "length")?,
        count: count(&p, "count")?,
        glue_relief: length(&p, "glue_relief")?,
    };
    let with_bodies = flag(&p, "tenon_bodies", true)?;
    let plan = loose::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    for mortise in &plan.mortises_first {
        add(mortise, "cut", &first, "Loose tenon mortise")?;
    }
    for mortise in &plan.mortises_second {
        add(mortise, "cut", &second, "Loose tenon mortise")?;
    }
    landed(&[(&first, Want::Cut(volume_a)), (&second, Want::Cut(volume_b))])?;
    if with_bodies {
        for (tenon, name) in plan.tenons.iter().zip(free_names("Loose tenon", plan.tenons.len())?) {
            body_from(tenon, &name, "Loose tenon")?;
        }
    }
    Ok(json!({
        "first": first,
        "second": second,
        "count": plan.tenons.len(),
        "thickness_mm": plan.thickness,
        "width_mm": plan.width,
        "length_mm": plan.length,
        "in_first_mm": plan.in_first,
        "in_second_mm": plan.in_second,
        "tenon_bodies": with_bodies,
        "moved_first_mm": moved,
    })
    .to_string())
}

fn housing(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_b, moved, .. } = pair(&p, ("first", "second"), boards::align_end)?;
    let options = housing::Options {
        depth: length(&p, "depth")?,
        stop_start: length(&p, "stop_start")?,
        stop_end: length(&p, "stop_end")?,
        slope: length(&p, "slope")?,
    };
    let plan = housing::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    match &plan.sides {
        None => {
            add(&plan.socket, "cut", &second, "Housing")?;
            add(&plan.tongue, "join", &first, "Housing tongue")?;
        }
        // A sliding dovetail: the tongue is a body of its own, its two sides cut at the slope.
        // It cuts its housing in the side, then becomes part of the shelf.
        Some((sides, _)) => {
            let name = free_names("Dovetail", 1)?.remove(0);
            body_from(&plan.tongue, &name, "Dovetail tongue")?;
            for (side, tool) in sides.iter().zip(free_names("Dovetail tool", 2)?) {
                slanted_cut(&name, side, &tool, "Dovetail side")?;
            }
            run("solid.combine", json!({ "target": second, "tools": [name], "operation": "cut", "keep_tools": true, "name": "Dovetail housing" }))?;
            run("solid.combine", json!({ "target": first, "tools": [name], "operation": "join", "name": "Dovetail tongue" }))?;
        }
    }
    landed(&[(&first, Want::Bounds(grown(&a, &[plan.tongue]))), (&second, Want::Cut(volume_b))])?;
    Ok(json!({
        "first": first,
        "second": second,
        "depth_mm": plan.depth,
        "width_mm": plan.width,
        "neck_mm": plan.sides.map_or(plan.width, |(_, neck)| neck),
        "length_mm": plan.length,
        "moved_first_mm": moved,
    })
    .to_string())
}

fn miter(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_a, volume_b, moved } = pair(&p, ("first", "second"), finger::align)?;
    let options = miter::Options { spline: length(&p, "spline")?, spline_depth: length(&p, "spline_depth")? };
    let plan = miter::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    let names = free_names("Miter tool", 2)?;
    let mut grown_by = [0.0; 2];
    let boards = [(&first, plan.extend_first, plan.tool_first), (&second, plan.extend_second, plan.tool_second)];
    for (i, (board, extend, tool)) in boards.into_iter().enumerate() {
        if let Some(block) = extend {
            add(&block, "join", board, "Miter extension")?;
            // The extension laps a hair into the board: only what lies outside it is new wood.
            grown_by[i] = block.size[0] * block.size[1] * block.size[2] * 0.9;
        }
        slanted_cut(board, &tool, &names[i], "Miter")?;
    }
    landed(&[(&first, Want::Cut(volume_a + grown_by[0])), (&second, Want::Cut(volume_b + grown_by[1]))])?;
    // A loose spline: a body turned square to the cut, which then cuts its slot in both boards.
    if let Some((body, _)) = &plan.spline {
        let name = free_names("Miter spline", 1)?.remove(0);
        let r = |v: f64| (v * 1e6).round() / 1e6;
        body_from(&body.block, &name, "Miter spline")?;
        run(
            "solid.move",
            json!({ "bodies": [name], "origin": body.origin.map(r), "axis": body.axis, "angle": r(body.angle), "name": "Miter spline turn" }),
        )?;
        for board in [&first, &second] {
            run("solid.combine", json!({ "target": board, "tools": [name], "operation": "cut", "keep_tools": true, "name": "Spline slot" }))?;
        }
    }
    Ok(json!({
        "first": first,
        "second": second,
        "spline": plan.spline.is_some(),
        "angle_deg": plan.angle,
        "extended_first": plan.extend_first.is_some(),
        "extended_second": plan.extend_second.is_some(),
        "moved_first_mm": moved,
    })
    .to_string())
}

fn tongue_groove(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_b, moved, .. } = pair(&p, ("first", "second"), boards::align_end)?;
    let options = tongue::Options {
        thickness: length(&p, "thickness")?,
        depth: length(&p, "depth")?,
        glue_relief: length(&p, "glue_relief")?,
    };
    let plan = tongue::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    add(&plan.groove, "cut", &second, "Groove")?;
    add(&plan.tongue, "join", &first, "Tongue")?;
    landed(&[(&first, Want::Bounds(grown(&a, &[plan.tongue]))), (&second, Want::Cut(volume_b))])?;
    Ok(json!({
        "first": first,
        "second": second,
        "thickness_mm": plan.thickness,
        "depth_mm": plan.depth,
        "groove_depth_mm": plan.groove_depth,
        "length_mm": plan.length,
        "moved_first_mm": moved,
    })
    .to_string())
}

fn scarf(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_a, volume_b, moved } = pair(&p, ("first", "second"), scarf::align)?;
    let options = scarf::Options { flip: flag(&p, "flip", false)? };
    let plan = scarf::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    let names = free_names("Scarf tool", 2)?;
    slanted_cut(&first, &plan.tool_first, &names[0], "Scarf")?;
    slanted_cut(&second, &plan.tool_second, &names[1], "Scarf")?;
    landed(&[(&first, Want::Cut(volume_a)), (&second, Want::Cut(volume_b))])?;
    Ok(json!({
        "first": first,
        "second": second,
        "length_mm": plan.length,
        "slope": plan.slope,
        "moved_first_mm": moved,
    })
    .to_string())
}

fn dovetail(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_b, moved, .. } = pair(&p, ("first", "second"), finger::align)?;
    let options =
        dovetail::Options { count: count(&p, "count")?, slope: length(&p, "slope")?, pin: length(&p, "pin")?, lap: length(&p, "lap")? };
    let plan = dovetail::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    let tools = free_names("Dovetail tool", 2)?;
    let blanks: Vec<Block> = plan.tails.iter().map(|tail| tail.blank).collect();
    for (tail, name) in plan.tails.iter().zip(free_names("Dovetail", plan.tails.len())?) {
        // The tail as a body of its own: a block, with its two sides cut at the slope.
        body_from(&tail.blank, &name, "Dovetail blank")?;
        for (side, tool) in tail.sides.iter().zip(&tools) {
            slanted_cut(&name, side, tool, "Dovetail side")?;
        }
        // It cuts its socket in the pin board, then becomes part of the tail board.
        run("solid.combine", json!({ "target": second, "tools": [name], "operation": "cut", "keep_tools": true, "name": "Dovetail socket" }))?;
        run("solid.combine", json!({ "target": first, "tools": [name], "operation": "join", "name": "Dovetail" }))?;
    }
    landed(&[(&first, Want::Bounds(grown(&a, &blanks))), (&second, Want::Cut(volume_b))])?;
    Ok(json!({
        "first": first,
        "second": second,
        "tails": plan.tails.len(),
        "slope": plan.slope,
        "pin_mm": plan.pin,
        "tail_wide_mm": plan.tail_wide,
        "tail_narrow_mm": plan.tail_narrow,
        "tail_length_mm": plan.tail_length,
        "moved_first_mm": moved,
    })
    .to_string())
}

fn screws(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_a, volume_b, moved } = pair(&p, ("first", "second"), boards::align_end)?;
    let options = screw::Options {
        diameter: length(&p, "diameter")?,
        length: length(&p, "length")?,
        count: count(&p, "count")?,
        countersink: Some(flag(&p, "countersink", true)?),
    };
    let plan = screw::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    let r = |v: f64| (v * 1e6).round() / 1e6;
    for hole in &plan.clearance {
        bore(hole, json!({ "operation": "cut", "targets": [first] }), "Screw clearance hole")?;
    }
    for sink in &plan.sinks {
        let mut axis = [0.0; 3];
        axis[sink.axis] = sink.dir;
        run(
            "solid.cone",
            json!({
                "base": sink.base.map(r),
                "axis": axis,
                "diameter": r(sink.diameter),
                "top_diameter": r(sink.top_diameter),
                "height": r(sink.height),
                "operation": "cut",
                "targets": [first],
                "name": "Countersink",
            }),
        )?;
    }
    for hole in &plan.pilots {
        bore(hole, json!({ "operation": "cut", "targets": [second] }), "Screw pilot hole")?;
    }
    landed(&[(&first, Want::Cut(volume_a)), (&second, Want::Cut(volume_b))])?;
    Ok(json!({
        "first": first,
        "second": second,
        "count": plan.pilots.len(),
        "diameter_mm": plan.diameter,
        "length_mm": plan.length,
        "clearance_diameter_mm": plan.clearance_diameter,
        "pilot_diameter_mm": plan.pilot_diameter,
        "pilot_depth_mm": plan.pilot_depth,
        "countersink_depth_mm": plan.sink_depth,
        "moved_first_mm": moved,
    })
    .to_string())
}

fn biscuits(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_a, volume_b, moved } = pair(&p, ("first", "second"), boards::align_end)?;
    let size = match p.get("size").filter(|v| !v.is_null() && v.as_str() != Some("")) {
        None => None,
        Some(v) => Some(
            v.as_u64()
                .or_else(|| v.as_str().map(choice).and_then(|s| s.parse().ok()))
                .and_then(|k| u32::try_from(k).ok())
                .ok_or_else(|| invalid("size", "expected 0, 10 or 20"))?,
        ),
    };
    let options = biscuit::Options { size, count: count(&p, "count")? };
    let with_bodies = flag(&p, "biscuit_bodies", true)?;
    let plan = biscuit::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    for slot in &plan.slots_first {
        bore(slot, json!({ "operation": "cut", "targets": [first] }), "Biscuit slot")?;
    }
    for slot in &plan.slots_second {
        bore(slot, json!({ "operation": "cut", "targets": [second] }), "Biscuit slot")?;
    }
    landed(&[(&first, Want::Cut(volume_a)), (&second, Want::Cut(volume_b))])?;
    if with_bodies {
        // A biscuit is what the two discs of the blade share.
        let names = free_names("Biscuit", plan.slots_first.len())?;
        for ((one, other), name) in plan.slots_first.iter().zip(&plan.slots_second).zip(names) {
            bore(one, json!({ "operation": "new", "body_name": name }), "Biscuit")?;
            bore(other, json!({ "operation": "intersect", "targets": [name] }), "Biscuit")?;
        }
    }
    Ok(json!({
        "first": first,
        "second": second,
        "count": plan.slots_first.len(),
        "size": plan.size,
        "depth_mm": plan.depth,
        "slot_length_mm": plan.slot_length,
        "biscuit_bodies": with_bodies,
        "moved_first_mm": moved,
    })
    .to_string())
}
