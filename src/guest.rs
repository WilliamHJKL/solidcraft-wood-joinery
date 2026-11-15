//! The WebAssembly side: reads the entry's JSON, calls SolidCraft's commands, reports the result.

use serde_json::{json, Value};

use crate::boards;
use crate::dowel::{self, Bore};
use crate::finger;
use crate::groove;
use crate::joint::{plan, Aabb, Block, Options, Variant};
use crate::lap;

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

/// The two boards of a joint, by name, and their bounding boxes.
///
/// With `align` on, the first board is moved by what `shift` returns before anything is cut. The
/// last value is how far it moved.
fn pair(
    p: &Value,
    names: (&str, &str),
    shift: fn(&Aabb, &Aabb) -> Result<[f64; 3], String>,
) -> Result<(String, String, Aabb, Aabb, [f64; 3]), PluginError> {
    let (first, second) = two(p, names.0, names.1)?;
    let measured = run("inspect.measure", json!({ "bodies": [first, second] }))?;
    let (mut a, b) = (bbox(&measured, &first)?, bbox(&measured, &second)?);
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
        }
    }
    Ok((first, second, a, b, moved))
}

fn run(command: &str, args: Value) -> Result<Value, PluginError> {
    let out = commands::run(command, &args.to_string())?;
    serde_json::from_str(&out).map_err(|e| PluginError::Trap(format!("{command} returned invalid JSON: {e}")))
}

fn bbox(measured: &Value, name: &str) -> Result<Aabb, PluginError> {
    let found = measured
        .get("bodies")
        .and_then(Value::as_array)
        .and_then(|bodies| bodies.iter().find(|b| b.get("name").and_then(Value::as_str) == Some(name)))
        .ok_or_else(|| invalid("body", format!("no body named `{name}`")))?;
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
    let (rail, stile, rail_box, stile_box, moved) = pair(&p, ("rail", "stile"), boards::align_end)?;
    let variant = match p.get("variant") {
        None | Some(Value::Null) => Variant::Classic,
        Some(v) => v
            .as_str()
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
    };

    let plan = plan(rail_box, stile_box, &options).map_err(|reason| invalid("rail", reason))?;

    for (i, block) in plan.tenon.iter().enumerate() {
        add(block, "join", &rail, if i == 0 { "Tenon" } else { "Haunch" })?;
    }
    for (i, block) in plan.mortise.iter().enumerate() {
        add(block, "cut", &stile, if i == 0 { "Mortise" } else { "Haunch mortise" })?;
    }

    Ok(json!({
        "variant": variant.name(),
        "rail": rail,
        "stile": stile,
        "tenon_thickness_mm": plan.thickness,
        "tenon_length_mm": plan.length,
        "tenon_width_mm": plan.width,
        "mortise_depth_mm": plan.mortise_depth,
        "moved_rail_mm": moved,
    })
    .to_string())
}

fn groove(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let typed = named(&p, "body")?;
    let (board, face) = match p.get("face").filter(|f| !f.is_null() && f.as_str() != Some("")) {
        Some(face) => {
            let face = face
                .as_str()
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
    };
    let measured = run("inspect.measure", json!({ "bodies": [board] }))?;
    let plan = groove::plan(bbox(&measured, &board)?, face, &options).map_err(|reason| invalid("body", reason))?;
    add(&plan.cut, "cut", &board, "Groove")?;
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
    let (first, second, a, b, moved) = pair(&p, ("first", "second"), lap::align)?;
    let options = lap::Options { depth: length(&p, "depth")?, flip: flag(&p, "flip", false)? };
    let plan = lap::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    add(&plan.cut_first, "cut", &first, "Half lap")?;
    add(&plan.cut_second, "cut", &second, "Half lap")?;
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
    let (first, second, a, b, moved) = pair(&p, ("first", "second"), boards::align_end)?;
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
    if with_bodies {
        // SolidCraft renames a body whose name is taken, so number the dowels past the ones there.
        let all = run("inspect.measure", json!({}))?;
        let taken: Vec<&str> = all
            .get("bodies")
            .and_then(Value::as_array)
            .map(|bodies| bodies.iter().filter_map(|b| b.get("name").and_then(Value::as_str)).collect())
            .unwrap_or_default();
        let mut number = 0;
        for peg in &plan.dowels {
            let name = loop {
                number += 1;
                let name = format!("Dowel {number}");
                if !taken.contains(&name.as_str()) {
                    break name;
                }
            };
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
    let (first, second, a, b, moved) = pair(&p, ("first", "second"), finger::align)?;
    let options = finger::Options { count: count(&p, "count")?, swap: flag(&p, "swap", false)? };
    let plan = finger::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    for socket in &plan.sockets {
        add(socket, "cut", &second, "Finger socket")?;
    }
    for finger in &plan.fingers {
        add(finger, "join", &first, "Finger")?;
    }
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
