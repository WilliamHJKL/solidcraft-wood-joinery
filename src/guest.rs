//! The WebAssembly side: reads the entry's JSON, calls SolidCraft's commands, reports the result.

use serde_json::{json, Value};

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
fn pair(p: &Value) -> Result<(String, String, Aabb, Aabb), PluginError> {
    let first = body(p, "first")?;
    let second = body(p, "second")?;
    if first == second {
        return Err(invalid("second", "the joint needs two different bodies"));
    }
    let measured = run("inspect.measure", json!({ "bodies": [first, second] }))?;
    let (a, b) = (bbox(&measured, &first)?, bbox(&measured, &second)?);
    Ok((first, second, a, b))
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
    let rail = body(&p, "rail")?;
    let stile = body(&p, "stile")?;
    if rail == stile {
        return Err(invalid("stile", "the rail and the stile must be two different bodies"));
    }
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

    let measured = run("inspect.measure", json!({ "bodies": [rail, stile] }))?;
    let plan = plan(bbox(&measured, &rail)?, bbox(&measured, &stile)?, &options).map_err(|reason| invalid("rail", reason))?;

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
    })
    .to_string())
}

fn groove(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let board = body(&p, "body")?;
    let face = p
        .get("face")
        .and_then(Value::as_str)
        .and_then(groove::Face::parse)
        .ok_or_else(|| invalid("face", "expected +x, -x, +y, -y, +z or -z"))?;
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

fn half_lap(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let (first, second, a, b) = pair(&p)?;
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
    })
    .to_string())
}

fn dowels(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let (first, second, a, b) = pair(&p)?;
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
    })
    .to_string())
}

fn box_joint(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let (first, second, a, b) = pair(&p)?;
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
    })
    .to_string())
}
