//! The docked "Joints" panel (capability `ui@0.1`, SolidCraft 0.49 and later): the joints this
//! plugin made in the open design, with their state, and buttons to update them.
//!
//! The panel's layout lives in `plugin/panel.json`. The build copies it into the manifest's
//! `runtime.ui`, and `describe` reads the same file, so the two cannot differ: SolidCraft refuses
//! a plugin whose `describe` is not exactly what the user approved.

use serde_json::{json, Value};

use super::craft::plugin::types::{Datum, PluginError, Value as Cell};
use super::craft::plugin::ui;
use super::{invalid, Joints};

/// The panel's id in `plugin/panel.json`.
pub const PANEL: &str = "joints";

const DECLARATIONS: &str = include_str!("../plugin/panel.json");

/// What `ui-provider.describe` returns: `plugin/panel.json`, in the WIT's types.
pub fn declarations() -> ui::Declarations {
    let d: Value = serde_json::from_str(DECLARATIONS).unwrap_or(Value::Null);
    ui::Declarations {
        ui_version: d["ui_version"].as_u64().unwrap_or(1) as u32,
        panels: d["panels"].as_array().map(|p| p.iter().map(panel).collect()).unwrap_or_default(),
        workspaces: Vec::new(),
    }
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

/// A text the panel shows, in the package's language: the French package translates it from
/// plugin/locales/fr.json, as tools/localize.py translates the manifest's copy of the panel.
fn label(v: &Value) -> String {
    let english = text(v);
    #[cfg(feature = "fr")]
    {
        thread_local! {
            static WORDS: serde_json::Map<String, Value> = serde_json::from_str(include_str!("../plugin/locales/fr.json")).unwrap_or_default();
        }
        if let Some(french) = WORDS.with(|w| w.get(&english).and_then(Value::as_str).map(str::to_owned)) {
            return french;
        }
    }
    english
}

fn maybe(v: &Value) -> Option<String> {
    v.as_str().map(str::to_owned)
}

fn panel(p: &Value) -> ui::PanelSpec {
    let zone = |z: &Value| match z.as_str() {
        Some("left") => ui::DockZone::Left,
        Some("bottom") => ui::DockZone::Bottom,
        _ => ui::DockZone::Right,
    };
    ui::PanelSpec {
        id: text(&p["id"]),
        title: label(&p["title"]),
        zones: p["zones"].as_array().map(|z| z.iter().map(zone).collect()).unwrap_or_default(),
        layout: ui::Layout {
            root: p["layout"]["root"].as_u64().unwrap_or(0) as u32,
            nodes: p["layout"]["nodes"].as_array().map(|n| n.iter().map(|n| ui::Node { widget: widget(&n["widget"]) }).collect()).unwrap_or_default(),
        },
    }
}

/// The widgets this panel uses: a group, a toolbar and a table.
fn widget(w: &Value) -> ui::Widget {
    let button = |b: &Value| ui::ButtonSpec { label: label(&b["label"]), icon: maybe(&b["icon"]), entry: text(&b["entry"]) };
    if let Some(children) = w.get("group").and_then(Value::as_array) {
        return ui::Widget::Group(children.iter().filter_map(Value::as_u64).map(|c| c as u32).collect());
    }
    if let Some(buttons) = w.get("toolbar").and_then(Value::as_array) {
        return ui::Widget::Toolbar(buttons.iter().map(button).collect());
    }
    if let Some(t) = w.get("table") {
        let select = match t["select"].as_str() {
            Some("single") => ui::SelectionMode::Single,
            Some("multi") => ui::SelectionMode::Multi,
            _ => ui::SelectionMode::None,
        };
        return ui::Widget::Table(ui::TableSpec {
            bind: text(&t["bind"]),
            row_id: text(&t["row_id"]),
            columns: t["columns"]
                .as_array()
                .map(|c| c.iter().map(|c| ui::Column { key: text(&c["key"]), label: label(&c["label"]), unit: maybe(&c["unit"]) }).collect())
                .unwrap_or_default(),
            select,
            on_select: maybe(&t["on_select"]),
            on_edit: maybe(&t["on_edit"]),
        });
    }
    if let Some(f) = w.get("form") {
        let kind = |k: &Value| match k.as_str() {
            Some("number") => ui::FieldKind::Number,
            Some("length") => ui::FieldKind::Length,
            Some("angle") => ui::FieldKind::Angle,
            Some("toggle") => ui::FieldKind::Toggle,
            Some("choice") => ui::FieldKind::Choice,
            Some("selection") => ui::FieldKind::Selection,
            _ => ui::FieldKind::Text,
        };
        return ui::Widget::Form(ui::FormSpec {
            bind: text(&f["bind"]),
            fields: f["fields"]
                .as_array()
                .map(|fs| {
                    fs.iter()
                        .map(|x| ui::Field {
                            key: text(&x["key"]),
                            label: label(&x["label"]),
                            kind: kind(&x["kind"]),
                            choices: x["choices"].as_array().map(|c| c.iter().map(text).collect()).unwrap_or_default(),
                            on_change: maybe(&x["on_change"]),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        });
    }
    ui::Widget::Text(text(&w["text"]))
}

/// Fills the table with the joints of the open design: one row per joint, in the order they were
/// made. A joint whose folder was deleted by hand shows as gone; the joints an update stopped
/// before, for lack of host calls, show as waiting for the next update.
pub fn refresh() -> Result<usize, PluginError> {
    let mut kept = Joints::load()?;
    let live = kept.live()?;
    let mut rows: Vec<(String, String, &str)> = Vec::new();
    for i in 0..kept.list().len() {
        let joint = &kept.list()[i];
        let id = joint.get("folder").and_then(Value::as_u64).map_or_else(|| format!("j{i}"), |f| f.to_string());
        let state = if live.contains(&i) { "made" } else { "gone" };
        let entry = joint.get("entry").and_then(Value::as_str).unwrap_or_default().to_owned();
        rows.push((id, format!("{}: {}", super::tr(short(&entry)), boards_of(&kept.name(i))), state));
    }
    if let Some(Value::Array(pending)) = kept.document.get("pending") {
        for (k, joint) in pending.iter().enumerate() {
            let name = joint.get("name").and_then(Value::as_str).unwrap_or_default().to_owned();
            rows.push((format!("p{k}"), name, "to update"));
        }
    }
    // Whether the boards changed since the last update that ran to its end (the fingerprint
    // "Update joints" keeps): the panel cannot tell which joint, only that one may be out of date.
    let status = match (kept.document.get("settled").and_then(Value::as_str).map(str::to_owned), rows.is_empty()) {
        (_, true) => "No joint yet.".to_owned(),
        (_, false) if kept.document.get("pending").is_some() => "Unfinished: press Update joints.".to_owned(),
        (Some(settled), false) => match super::fingerprint(&mut kept) {
            Some(now) if now == settled => "Up to date.".to_owned(),
            Some(_) => "Changed: press Update joints.".to_owned(),
            None => String::new(),
        },
        (None, false) => "Not updated yet.".to_owned(),
    };
    let mut data = vec![Datum { path: "/status/text".into(), value: Cell::Text(super::tr(&status)) }];
    for (r, (id, name, state)) in rows.iter().enumerate() {
        data.push(Datum { path: format!("/joints/{r}/id"), value: Cell::Text(id.clone()) });
        // A folder is named "Mortise and tenon: Rail / Stile": the kind, then the boards.
        let (kind, boards) = name.split_once(": ").unwrap_or((name.as_str(), ""));
        data.push(Datum { path: format!("/joints/{r}/kind"), value: Cell::Text(kind.to_owned()) });
        data.push(Datum { path: format!("/joints/{r}/boards"), value: Cell::Text(boards.to_owned()) });
        data.push(Datum { path: format!("/joints/{r}/state"), value: Cell::Text(super::tr(state)) });
    }
    ui::clear_data(PANEL)?;
    if !data.is_empty() {
        ui::set_data(PANEL, &data)?;
    }
    Ok(rows.len())
}

/// The joint's short name, as on the ribbon.
fn short(entry: &str) -> &str {
    match entry {
        "mortise-tenon" => "Tenon",
        "loose-tenon" => "Loose tenon",
        "tongue-groove" => "Tongue",
        "half-lap" => "Half lap",
        "box-joint" => "Box joint",
        "groove" => "Groove",
        "housing" => "Housing",
        "miter" => "Miter",
        "scarf" => "Scarf",
        "dovetail" => "Dovetail",
        "dowels" => "Dowels",
        "biscuits" => "Biscuits",
        "screws" => "Screws",
        other => other,
    }
}

/// The boards in a folder name "Mortise and tenon: Rail / Stile".
fn boards_of(name: &str) -> &str {
    name.split_once(": ").map_or(name, |(_, boards)| boards)
}

thread_local! {
    /// The row of the panel the last click was on, until the entry it runs reads it.
    static CLICKED: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Keeps the row a panel click was on: the entry the click runs comes next, in the same instance.
pub fn clicked(row: Option<String>) {
    CLICKED.with(|c| *c.borrow_mut() = row);
}

/// The joint of the row that was double clicked, by its folder id, when the run comes from a
/// click in the panel.
pub fn clicked_joint(kept: &mut Joints) -> Result<Option<usize>, PluginError> {
    let Some(row) = CLICKED.with(|c| c.borrow_mut().take()) else { return Ok(None) };
    let folder: u64 = row.parse().map_err(|_| invalid("row", "this row is not a joint made in this design: press Update joints first"))?;
    let i = (0..kept.list().len())
        .find(|&i| kept.list()[i].get("folder").and_then(Value::as_u64) == Some(folder))
        .ok_or_else(|| invalid("row", "this joint is no longer in the design: press Refresh"))?;
    Ok(Some(i))
}

/// The reply of the panel's own entries.
pub fn reply(rows: usize) -> String {
    json!({ "joints": rows }).to_string()
}
