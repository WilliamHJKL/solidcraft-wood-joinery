//! The WebAssembly side: reads the entry's JSON, calls SolidCraft's commands, reports the result.

use serde_json::{json, Value};

use crate::joint::{plan, Aabb, Block, Options, Variant};

wit_bindgen::generate!({ world: "command-plugin", path: "wit" });

use craft::plugin::commands;
use craft::plugin::types::{FieldFailure, PluginError};

struct Plugin;

export!(Plugin);

impl exports::craft::plugin::entry::Guest for Plugin {
    fn execute(entry: String, params: String) -> Result<String, PluginError> {
        match entry.as_str() {
            "mortise-tenon" => mortise_tenon(&params),
            other => Err(invalid("entry", format!("unknown entry `{other}`"))),
        }
    }
}

fn invalid(field: &str, reason: impl Into<String>) -> PluginError {
    PluginError::InvalidInput(FieldFailure { field: field.into(), reason: reason.into() })
}

/// One body name: the field is a list, as every selection input of a plugin entry is.
fn body(p: &Value, field: &str) -> Result<String, PluginError> {
    let names = p.get(field).and_then(Value::as_array).ok_or_else(|| invalid(field, "expected a list with one body name"))?;
    match names.as_slice() {
        [Value::String(name)] if !name.trim().is_empty() => Ok(name.clone()),
        _ => Err(invalid(field, "expected exactly one body name")),
    }
}

fn length(p: &Value, field: &str) -> Result<Option<f64>, PluginError> {
    match p.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().map(Some).ok_or_else(|| invalid(field, "expected a number of millimetres")),
    }
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
        .ok_or_else(|| invalid("rail", format!("no body named `{name}`")))?;
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
