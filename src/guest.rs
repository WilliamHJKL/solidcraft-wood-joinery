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
use craft::plugin::types::{CommandFailure, FieldFailure, PluginError};

struct Plugin;

export!(Plugin);

impl exports::craft::plugin::entry::Guest for Plugin {
    fn execute(entry: String, params: String) -> Result<String, PluginError> {
        // Each run starts its own count of host calls, with no body names known yet.
        CALLS.with(|c| c.set(0));
        NAMES.with(|n| *n.borrow_mut() = None);
        match entry.as_str() {
            "update-joints" => return update_joints(&params),
            "edit-joint" => return edit_joint(&params),
            _ => {}
        }
        let mut kept = Joints::load()?;
        // A form opened by "Edit a joint" replaces that joint when it runs on the same boards.
        let editing = kept.take_editing();
        let at = editing.and_then(|marker| kept.edited_by(&marker, &entry, &params));
        if let Some(i) = at {
            kept.unmake(&[i])?;
        }
        let start = calls();
        let out = joint(&entry, &params)?;
        let folder = kept.keep_at(&entry, &params, at)?;
        kept.cost(at, calls() - start);
        kept.save()?;
        let mut out: Value = serde_json::from_str(&out).unwrap_or_else(|_| json!({}));
        if let (Some(out), Some(folder)) = (out.as_object_mut(), folder) {
            out.insert("folder".into(), json!(folder));
            if at.is_some() {
                out.insert("replaced".into(), json!(true));
            }
        }
        Ok(out.to_string())
    }
}

/// The English names of the joints, for the timeline folder that holds each one.
fn label(entry: &str) -> &str {
    match entry {
        "mortise-tenon" => "Mortise and tenon",
        "groove" => "Groove",
        "half-lap" => "Half lap",
        "dowels" => "Dowels",
        "box-joint" => "Box joint",
        "loose-tenon" => "Loose tenon",
        "housing" => "Housing",
        "miter" => "Miter",
        "tongue-groove" => "Tongue and groove",
        "scarf" => "Scarf",
        "dovetail" => "Dovetail",
        "screws" => "Screws",
        "biscuits" => "Biscuits",
        other => other,
    }
}

/// The joints this plugin made in the open design, kept in the design (capability `storage@0.1`)
/// so that "Update joints" can make them again on the boards as they are now: a joint's
/// features are plain boxes and cylinders at fixed sizes, so they do not follow the parameters.
/// Each joint's features sit in a timeline folder of their own. Each joint is kept as
/// `{entry, params, boards, folder, features, name}`; `boards` maps each field that names a
/// board (or a face) to what the joint used. While a form opened by "Edit a joint" is pending,
/// `editing` names that joint's folder and entry. An update that ran to its end leaves
/// `settled`, the fingerprint of what it made, so that the next one does nothing if nothing changed.
struct Joints {
    document: Value,
}

impl Joints {
    fn load() -> Result<Self, PluginError> {
        let got = run("storage.get", json!({}))?;
        let mut document = got.get("document").cloned().unwrap_or(Value::Null);
        if !document.get("joints").is_some_and(Value::is_array) {
            document = json!({ "joints": [] });
        }
        Ok(Self { document })
    }

    fn list(&mut self) -> &mut Vec<Value> {
        self.document["joints"].as_array_mut().expect("load makes a list")
    }

    fn take_editing(&mut self) -> Option<Value> {
        self.document.as_object_mut().and_then(|d| d.remove("editing"))
    }

    /// The joint that a pending "Edit a joint" form stands for, when this run of `entry` is on
    /// the same boards: that form was filled with the joint's own settings.
    fn edited_by(&mut self, marker: &Value, entry: &str, params: &str) -> Option<usize> {
        let p: Value = serde_json::from_str(params).ok()?;
        let folder = marker.get("folder")?.as_u64()?;
        let i = self.list().iter().position(|j| j.get("folder").and_then(Value::as_u64) == Some(folder))?;
        let joint = &self.list()[i];
        if joint.get("entry").and_then(Value::as_str) != Some(entry) {
            return None;
        }
        let same = joint.get("boards").and_then(Value::as_object).is_some_and(|boards| {
            !boards.is_empty()
                && boards.iter().all(|(field, value)| {
                    let given = p.get(field).and_then(one_board).map(str::trim);
                    given.is_some_and(|given| value.as_str().is_some_and(|v| choice(v) == choice(given)))
                })
        });
        same.then_some(i)
    }

    /// The joints whose features are still in the timeline, by index.
    fn live(&mut self) -> Result<Vec<usize>, PluginError> {
        let there = feature_ids()?;
        Ok(self
            .list()
            .iter()
            .enumerate()
            .filter(|(_, j)| features_of(j).iter().any(|id| there.contains(id)))
            .map(|(i, _)| i)
            .collect())
    }

    /// Deletes the features and the folders of these joints. Their records stay, for the caller
    /// to replace or drop.
    fn unmake(&mut self, joints: &[usize]) -> Result<(), PluginError> {
        let there = feature_ids()?;
        let mut doomed = Vec::new();
        let mut folders = Vec::new();
        for &i in joints {
            let joint = &self.list()[i];
            let mine: Vec<u64> = features_of(joint).into_iter().filter(|id| there.contains(id)).collect();
            if !mine.is_empty() {
                doomed.extend(mine);
                folders.extend(joint.get("folder").and_then(Value::as_u64));
            }
        }
        if !doomed.is_empty() {
            run("timeline.delete", json!({ "features": doomed }))?;
            // The folders are empty now. A folder taken apart by hand (Ungroup) is gone, and
            // deleting it would fail: the README says to delete a joint's folder, not to ungroup it.
            run("timeline.delete", json!({ "groups": folders }))?;
        }
        Ok(())
    }

    /// Puts the features the entry just made in a folder, and records the joint with the names
    /// it resolved from the selection, so that it can run again without a selection: at `at` in
    /// the list (the joint it replaces), or at the end.
    fn keep_at(&mut self, entry: &str, params: &str, at: Option<usize>) -> Result<Option<String>, PluginError> {
        let Some(made) = folder(entry, params)? else { return Ok(None) };
        let record = json!({
            "entry": entry,
            "params": made.params,
            "boards": made.boards,
            "folder": made.id,
            "features": made.features,
            "name": made.name,
        });
        match at {
            Some(i) => self.list()[i] = record,
            None => self.list().push(record),
        }
        Ok(Some(made.name))
    }

    /// Notes in the joint just kept (at `at`, or the last one) how many host calls it took, so
    /// that "Update joints" knows whether it still fits in the run.
    fn cost(&mut self, at: Option<usize>, calls: usize) {
        let list = self.list();
        let i = at.unwrap_or(list.len().saturating_sub(1));
        if let Some(joint) = list.get_mut(i).and_then(Value::as_object_mut) {
            joint.insert("calls".into(), json!(calls));
        }
    }

    /// The joints an update stopped before, for lack of host calls: the next update makes them.
    fn take_pending(&mut self) -> Vec<Value> {
        match self.document.as_object_mut().and_then(|d| d.remove("pending")) {
            Some(Value::Array(pending)) => pending,
            _ => Vec::new(),
        }
    }

    fn name(&mut self, i: usize) -> String {
        self.list()[i].get("name").and_then(Value::as_str).unwrap_or_default().to_owned()
    }

    /// The joint a user means: part of its folder's name, or, when that is empty, the joint of
    /// the selected boards.
    fn find(&mut self, text: Option<&str>) -> Result<usize, PluginError> {
        let live = self.live()?;
        let names: Vec<(usize, String)> = live.iter().map(|&i| (i, self.name(i))).collect();
        let listed = || {
            if names.is_empty() {
                "this design has no joint made by this plugin since version 0.12.0".to_owned()
            } else {
                format!("the joints are: {}", names.iter().map(|(_, n)| format!("`{n}`")).collect::<Vec<_>>().join(", "))
            }
        };
        let found: Vec<usize> = match text {
            Some(text) => {
                let lower = text.to_lowercase();
                let exact: Vec<usize> = names.iter().filter(|(_, n)| n.to_lowercase() == lower).map(|(i, _)| *i).collect();
                if exact.is_empty() {
                    names.iter().filter(|(_, n)| n.to_lowercase().contains(&lower)).map(|(i, _)| *i).collect()
                } else {
                    exact
                }
            }
            None => {
                let picked = selected_bodies()?;
                if picked.is_empty() {
                    return Err(invalid("joint", format!("select the boards of the joint, or type part of its folder name; {}", listed())));
                }
                live.iter()
                    .copied()
                    .filter(|&i| {
                        let boards: Vec<String> = self.list()[i]
                            .get("boards")
                            .and_then(Value::as_object)
                            .map(|b| b.iter().filter(|(f, _)| f.as_str() != "face").filter_map(|(_, v)| v.as_str().map(str::to_owned)).collect())
                            .unwrap_or_default();
                        picked.iter().all(|name| boards.contains(name))
                    })
                    .collect()
            }
        };
        match found.as_slice() {
            [i] => Ok(*i),
            [] => Err(invalid("joint", format!("no joint matches; {}", listed()))),
            many => Err(invalid(
                "joint",
                format!(
                    "{} joints match: {}. Type more of the folder name",
                    many.len(),
                    many.iter().map(|&i| format!("`{}`", self.name(i))).collect::<Vec<_>>().join(", ")
                ),
            )),
        }
    }

    fn save(&self) -> Result<(), PluginError> {
        run("storage.set", json!({ "document": self.document })).map(|_| ())
    }
}

fn features_of(joint: &Value) -> Vec<u64> {
    joint.get("features").and_then(Value::as_array).map(|ids| ids.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
}

/// A joint's timeline folder, made from the features the entry made.
struct Made {
    id: u64,
    features: Vec<u64>,
    name: String,
    /// The entry's params, with the boards and face it took from the selection.
    params: Value,
    /// Each field that names a board or a face, and what the joint used.
    boards: serde_json::Map<String, Value>,
}

/// Groups the features made since the entry began in one timeline folder.
fn folder(entry: &str, params: &str) -> Result<Option<Made>, PluginError> {
    let features = CREATED.with(|c| std::mem::take(&mut *c.borrow_mut()));
    let resolved = RESOLVED.with(|r| std::mem::take(&mut *r.borrow_mut()));
    if features.is_empty() {
        return Ok(None);
    }
    let mut stored: Value = serde_json::from_str(params).unwrap_or_else(|_| json!({}));
    if !stored.is_object() {
        stored = json!({});
    }
    let mut names = Vec::new();
    let mut boards = serde_json::Map::new();
    for (field, value) in resolved {
        if let Some(name) = value.as_str().filter(|_| field != "face") {
            names.push(name.to_owned());
        }
        stored[&field] = value.clone();
        boards.insert(field, value);
    }
    let name = format!("{}: {}", label(entry), names.join(" / "));
    let made = run("timeline.group", json!({ "name": name, "features": features }))?;
    let id = made
        .get("group")
        .and_then(Value::as_u64)
        .ok_or_else(|| PluginError::Trap("timeline.group returned no folder id".into()))?;
    Ok(Some(Made { id, features, name, params: stored, boards }))
}

/// The ids of the features in the timeline. A command that fails ends the plugin's whole run, so
/// what is still there is read, never tried.
fn feature_ids() -> Result<Vec<u64>, PluginError> {
    let doc = run(
        "document.inspect",
        json!({ "timeline": "summary", "bodies": false, "measure": false, "sketches": false, "joints": false }),
    )?;
    Ok(doc
        .get("timeline")
        .and_then(Value::as_array)
        .map(|features| features.iter().filter_map(|f| f.get("id").and_then(Value::as_u64)).collect())
        .unwrap_or_default())
}

/// What an update depends on, in one line: each joint's features, and the volume, area and box of
/// every body, rounded to a thousandth. Equal after a parameter changed back, different after
/// anything moved, grew or was cut. None when the bodies cannot be measured: then it is not known.
fn fingerprint(kept: &mut Joints) -> Option<String> {
    let measured = run("inspect.measure", json!({})).ok()?;
    let mut text = String::new();
    for joint in kept.list().iter() {
        text.push_str(&format!("{:?};", features_of(joint)));
    }
    let round = |v: Option<&Value>| v.and_then(Value::as_f64).map_or(String::from("-"), |x| format!("{:.3}", x));
    for body in measured.get("bodies").and_then(Value::as_array)? {
        text.push_str(body.get("name").and_then(Value::as_str).unwrap_or_default());
        text.push('|');
        text.push_str(&round(body.get("volume_mm3")));
        text.push('|');
        text.push_str(&round(body.get("area_mm2")));
        for corner in ["min", "max"] {
            for i in 0..3 {
                text.push('|');
                text.push_str(&round(body.get("bbox").and_then(|b| b.get(corner)).and_then(|c| c.get(i))));
            }
        }
        text.push(';');
    }
    // FNV-1a: a short, stable digest of that line, kept in the design instead of the line.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    Some(format!("{hash:016x}"))
}

/// Makes every joint again, in the order they were made, on the boards as they are now: deletes
/// each joint's features and folder, then runs its entry with the params it was made with. A
/// joint whose features were all deleted by hand (with its folder) is forgotten. All of it is one
/// undo step, and a joint that no longer fits stops it with that joint's name, changing nothing.
/// When nothing changed since the last update ran to its end, it does nothing, unless `force`.
fn update_joints(params: &str) -> Result<String, PluginError> {
    let p = serde_json::from_str::<Value>(params).unwrap_or(Value::Null);
    // The window sends its check box; a script that leaves it out never gets a window back.
    let reopen = p.get("reopen").and_then(Value::as_bool).unwrap_or(false);
    let force = p.get("force").and_then(Value::as_bool).unwrap_or(false);
    let mut kept = Joints::load()?;
    kept.take_editing();
    // An update that ran out of host calls left the rest of its joints here: finish those first,
    // in their order, without touching the ones it already made again.
    let pending = kept.take_pending();
    let settled = kept.document.as_object_mut().and_then(|d| d.remove("settled"));
    let (queue, forgotten) = if pending.is_empty() {
        let total = kept.list().len();
        let live = kept.live()?;
        // Nothing changed since the last update ran to its end: the same joints, all still there,
        // on bodies of the same sizes. Making them again would only take the design apart for a
        // while, run after run, to put it back as it was.
        if !force && live.len() == total {
            if let (Some(settled), Some(now)) = (settled.as_ref(), fingerprint(&mut kept)) {
                if settled.as_str() == Some(now.as_str()) {
                    craft::plugin::log::log(craft::plugin::log::Level::Info, "Nothing changed since the last update: no joint made again.");
                    return Ok(json!({ "updated": 0, "remaining": 0, "forgotten": 0, "joints": [], "unchanged": true }).to_string());
                }
            }
        }
        kept.unmake(&live)?;
        let joints: Vec<Value> = live.iter().map(|&i| kept.list()[i].clone()).collect();
        kept.list().clear();
        (joints, total - live.len())
    } else {
        (pending, 0)
    };
    let mut names = Vec::new();
    let mut left = Vec::new();
    for (k, joint) in queue.iter().enumerate() {
        let need = joint.get("calls").and_then(Value::as_u64).map_or(CALL_GUESS, |c| c as usize + 4);
        if k > 0 && calls() + need + CALL_RESERVE > CALL_BUDGET {
            left = queue[k..].to_vec();
            break;
        }
        let entry = joint.get("entry").and_then(Value::as_str).unwrap_or_default().to_owned();
        let params = joint.get("params").cloned().unwrap_or_else(|| json!({})).to_string();
        let was = joint.get("name").and_then(Value::as_str).unwrap_or(&entry).to_owned();
        let start = calls();
        joint_named(&entry, &params, &was)?;
        if let Some(name) = kept.keep_at(&entry, &params, None)? {
            kept.cost(None, calls() - start);
            names.push(name);
        }
    }
    if !left.is_empty() {
        craft::plugin::log::log(
            craft::plugin::log::Level::Warn,
            &format!("{} joints made again, {} left: run Update joints again to finish.", names.len(), left.len()),
        );
        kept.document["pending"] = json!(left);
    } else if let Some(now) = fingerprint(&mut kept) {
        kept.document["settled"] = json!(now);
    }
    kept.save()?;
    if reopen && !left.is_empty() {
        // The app shows only "finished": the window coming back is what says joints are left.
        craft::plugin::forms::open("update-joints", &json!({ "reopen": true }).to_string())?;
    }
    Ok(json!({ "updated": names.len(), "remaining": left.len(), "forgotten": forgotten, "joints": names }).to_string())
}

/// Opens a joint's own form, filled with the settings it was made with. Run in that form, on the
/// same boards, replaces the joint in place. A script cannot open a form: it gives the settings
/// to change in `settings`, and the joint is made again with them at once.
fn edit_joint(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let mut kept = Joints::load()?;
    kept.take_editing();
    let text = p.get("joint").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty());
    let i = kept.find(text)?;
    let joint = kept.list()[i].clone();
    let entry = joint.get("entry").and_then(Value::as_str).unwrap_or_default().to_owned();
    let name = kept.name(i);
    let mut stored = joint.get("params").cloned().unwrap_or_else(|| json!({}));
    if let Some(settings) = p.get("settings") {
        let Some(settings) = settings.as_object() else {
            return Err(invalid("settings", "expected an object of the joint's fields and their new values"));
        };
        for (field, value) in settings {
            stored[field] = value.clone();
        }
        let stored = stored.to_string();
        kept.unmake(&[i])?;
        joint_named(&entry, &stored, &name)?;
        let now = kept.keep_at(&entry, &stored, Some(i))?;
        kept.save()?;
        return Ok(json!({ "edited": now }).to_string());
    }
    kept.document["editing"] = json!({ "folder": joint.get("folder"), "entry": entry });
    kept.save()?;
    as_picks(&entry, &mut stored);
    craft::plugin::forms::open(&entry, &stored.to_string())?;
    Ok(json!({ "editing": name }).to_string())
}

/// The fields of an entry whose form picks a board in the view, as Extrude picks its profiles.
fn pick_fields(entry: &str) -> &'static [&'static str] {
    match entry {
        "mortise-tenon" => &["rail", "stile"],
        _ => &[],
    }
}

/// A joint keeps its boards by name. Its form takes a picked board, so the names go back into the
/// form as picks; a joint made before the form picked is read the same way.
fn as_picks(entry: &str, stored: &mut Value) {
    for field in pick_fields(entry) {
        let Some(name) = stored.get(*field).and_then(Value::as_str).map(str::trim).map(str::to_owned) else { continue };
        if name.is_empty() {
            if let Some(map) = stored.as_object_mut() {
                map.remove(*field);
            }
        } else {
            stored[*field] = json!([{ "type": "body", "name": name }]);
        }
    }
}

/// Runs one joint again, and names it in the error when it no longer fits.
fn joint_named(entry: &str, params: &str, name: &str) -> Result<String, PluginError> {
    joint(entry, params).map_err(|e| {
        let say = |reason: String| format!("`{name}` cannot be made again, so nothing was changed: {reason}");
        match e {
            PluginError::InvalidInput(f) => invalid(&f.field, say(f.reason)),
            PluginError::CommandFailed(f) => PluginError::CommandFailed(CommandFailure { command: f.command, reason: say(f.reason) }),
            other => other,
        }
    })
}

/// Runs one joint entry. Each run starts afresh: the bodies of the design, the features made
/// and the names taken from the selection are those of this joint only.
fn joint(entry: &str, params: &str) -> Result<String, PluginError> {
    BEFORE.with(|b| *b.borrow_mut() = None);
    CREATED.with(|c| c.borrow_mut().clear());
    RESOLVED.with(|r| r.borrow_mut().clear());
    match entry {
        "mortise-tenon" => mortise_tenon(params),
        "groove" => groove(params),
        "half-lap" => half_lap(params),
        "dowels" => dowels(params),
        "box-joint" => box_joint(params),
        "loose-tenon" => loose_tenon(params),
        "housing" => housing(params),
        "miter" => miter(params),
        "tongue-groove" => tongue_groove(params),
        "scarf" => scarf(params),
        "dovetail" => dovetail(params),
        "screws" => screws(params),
        "biscuits" => biscuits(params),
        other => Err(invalid("entry", format!("unknown entry `{other}`"))),
    }
}

/// A note in SolidCraft's plugin log: what was made, but against common practice.
fn warn(message: &str) {
    craft::plugin::log::log(craft::plugin::log::Level::Warn, message);
}

fn invalid(field: &str, reason: impl Into<String>) -> PluginError {
    PluginError::InvalidInput(FieldFailure { field: field.into(), reason: reason.into() })
}

/// The one board a field gives: a name, a list of one name, or what a selection field sends when
/// a board is picked in the view (a body by its name, a face or an edge by the body it belongs to).
fn one_board(v: &Value) -> Option<&str> {
    match v {
        Value::String(name) => Some(name),
        Value::Array(items) => match items.as_slice() {
            [Value::String(name)] => Some(name),
            [Value::Object(pick)] => {
                let key = if pick.get("type").and_then(Value::as_str) == Some("body") { "name" } else { "body" };
                pick.get(key).and_then(Value::as_str)
            }
            _ => None,
        },
        _ => None,
    }
}

/// One body name, typed or picked (see `one_board`).
fn body(p: &Value, field: &str) -> Result<String, PluginError> {
    match p.get(field).and_then(one_board).map(str::trim) {
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
        Some(Value::String(name)) if name.trim().is_empty() => Ok(None),
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
    resolved(first, json!(a));
    resolved(second, json!(b));
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
        // A script may send 2.0 for 2.
        Some(v) => v
            .as_u64()
            .or_else(|| v.as_f64().filter(|f| *f >= 0.0 && f.fract() == 0.0 && *f <= f64::from(u32::MAX)).map(|f| f as u64))
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
    // `whole` lists the bodies; the next joint of this run reuses that list.
    whole(checks.iter().map(|(name, _)| *name))?;
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
                "boards",
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

thread_local! {
    /// The bodies of the design when the entry began, to tell a board that a cut split in two.
    static BEFORE: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
    /// The features the joint made, in order: they go in the joint's timeline folder.
    static CREATED: std::cell::RefCell<Vec<u64>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The boards (and face) the joint took from the selection, by field, so that it can be
    /// made again without one.
    static RESOLVED: std::cell::RefCell<Vec<(String, Value)>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The body names after the last command that listed them in this run.
    static NAMES: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
    /// The commands this run has sent to SolidCraft: the host allows `CALL_BUDGET` in one run.
    static CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The host calls SolidCraft allows a plugin in one run (`MAX_CALLS` in craft-plugin-wasm).
const CALL_BUDGET: usize = 256;
/// Calls kept back for what "Update joints" does after the last joint: saving the list, the log.
const CALL_RESERVE: usize = 8;
/// What a joint is taken to cost when its record does not say: more than any entry uses.
const CALL_GUESS: usize = 40;

fn calls() -> usize {
    CALLS.with(std::cell::Cell::get)
}

fn resolved(field: &str, value: Value) {
    RESOLVED.with(|r| r.borrow_mut().push((field.to_owned(), value)));
}

fn body_names() -> Result<Vec<String>, PluginError> {
    let listed = run("inspect.measure", json!({ "names_only": true }))?;
    let names: Vec<String> = listed
        .get("bodies")
        .and_then(Value::as_array)
        .map(|bodies| bodies.iter().filter_map(|b| b.get("name").and_then(Value::as_str).map(str::to_owned)).collect())
        .unwrap_or_default();
    NAMES.with(|n| *n.borrow_mut() = Some(names.clone()));
    Ok(names)
}

/// Checks that no board was cut in two: SolidCraft names the pieces of a split body `Name (1)`,
/// `Name (2)`… A joint that would leave a board in pieces is refused, and nothing is changed.
fn whole<'a>(boards: impl Iterator<Item = &'a str>) -> Result<(), PluginError> {
    let Some(before) = BEFORE.with(|b| b.borrow().clone()) else { return Ok(()) };
    let now = body_names()?;
    for board in boards {
        let piece = format!("{board} (");
        if now.iter().any(|n| n.starts_with(&piece) && !before.contains(n)) {
            return Err(invalid(
                "boards",
                format!(
                    "the joint would cut `{board}` in two pieces, so nothing was changed. The boards are \
                     not placed as this joint needs: check which board is first and how they meet"
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
    // The names the last joint of this run left are still good, unless one is missing from them.
    let cached = NAMES.with(|n| n.borrow().clone()).filter(|listed| names.iter().all(|(_, name)| listed.iter().any(|l| l == name)));
    let listed = match cached {
        Some(listed) => listed,
        None => body_names()?,
    };
    BEFORE.with(|b| {
        b.borrow_mut().get_or_insert_with(|| listed.clone());
    });
    let bodies: Vec<&str> = listed.iter().map(String::as_str).collect();
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
    CALLS.with(|c| c.set(c.get() + 1));
    let out = commands::run(command, &args.to_string())?;
    let out: Value = serde_json::from_str(&out).map_err(|e| PluginError::Trap(format!("{command} returned invalid JSON: {e}")))?;
    if let Some(feature) = out.get("feature").and_then(Value::as_u64) {
        CREATED.with(|c| c.borrow_mut().push(feature));
    }
    Ok(out)
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

/// Cuts a mortise: a box, or a box and its two round ends.
fn cut_shape(shape: &loose::Shape, target: &str, name: &str) -> Result<(), PluginError> {
    match shape {
        loose::Shape::Square(block) => add(block, "cut", target, name),
        loose::Shape::Rounded { core, ends } => {
            add(core, "cut", target, name)?;
            for end in ends {
                bore(end, json!({ "operation": "cut", "targets": [target] }), &format!("{name} end"))?;
            }
            Ok(())
        }
    }
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
        drawbore: length(&p, "drawbore")?.unwrap_or(0.0),
        wedges: count(&p, "wedges")?.unwrap_or(0),
        wedge_thickness: length(&p, "wedge_thickness")?,
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
    if plan.twins > 1 {
        sizes["twin_tenons"] = json!(plan.twins);
    }
    if let Some((width, depth)) = plan.haunch {
        sizes["haunch_width_mm"] = json!(width);
        sizes["haunch_depth_mm"] = json!(depth);
    }
    // A haunch, or the stub between twin tenons, is named a haunch.
    let haunch = |i: usize| plan.short[i];
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
        match peg.tenon_base {
            None => bore(&hole, json!({ "operation": "cut", "targets": [stile, rail] }), "Peg hole")?,
            // Draw-bored: the hole in the tenon sits nearer the shoulder than the one in the stile.
            Some(at) => {
                bore(&hole, json!({ "operation": "cut", "targets": [stile] }), "Peg hole")?;
                let mut at = at;
                at[peg.axis] -= boards::OVERCUT;
                bore(&Bore { base: at, ..hole }, json!({ "operation": "cut", "targets": [rail] }), "Drawbore hole")?;
            }
        }
        let body = Bore { base: peg.base, axis: peg.axis, dir: 1.0, diameter: peg.diameter, height: peg.height };
        bore(&body, json!({ "operation": "new", "body_name": name }), "Peg")?;
    }
    sizes["pegs"] = json!(plan.pegs.len());
    // Each wedge: a block with its two sides sloped to a point, which then cuts its own slot in
    // the end of the tenon and stays as a body.
    let tools = free_names("Wedge tool", 2)?;
    for (wedge, name) in plan.wedges.iter().zip(free_names("Wedge", plan.wedges.len())?) {
        body_from(&wedge.blank, &name, "Wedge")?;
        for (side, tool) in wedge.sides.iter().zip(&tools) {
            slanted_cut(&name, side, tool, "Wedge side")?;
        }
        run("solid.combine", json!({ "target": rail, "tools": [name], "operation": "cut", "keep_tools": true, "name": "Wedge slot" }))?;
    }
    sizes["wedges"] = json!(plan.wedges.len());

    sizes["moved_rail_mm"] = json!(moved);
    for w in &plan.warnings {
        warn(w);
    }
    sizes["warnings"] = json!(plan.warnings);
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
    resolved("body", json!(board));
    resolved("face", json!(format!("{}{}", if face.high { '+' } else { '-' }, ['x', 'y', 'z'][face.axis])));
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
    let Pair { first, second, mut a, b, volume_a, volume_b, mut moved } = pair(&p, ("first", "second"), lap::align)?;
    // Boards edge to edge or end to end: the first slides over the second by the overlap.
    if let Some(by) = lap::slide(&a, &b, length(&p, "overlap")?, flag(&p, "align", false)?).map_err(|reason| invalid("overlap", reason))? {
        run("solid.move", json!({ "bodies": [first], "translate": by }))?;
        for i in 0..3 {
            a.min[i] += by[i];
            a.max[i] += by[i];
            moved[i] += by[i];
        }
        landed(&[(&first, Want::Bounds(a))])?;
    }
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
    let domino = match p.get("domino").and_then(Value::as_str).map(choice) {
        None | Some("") | Some("custom") => None,
        Some(name) => Some(loose::domino(name).ok_or_else(|| {
            let names: Vec<&str> = loose::DOMINOS.iter().map(|(n, _)| *n).collect();
            invalid("domino", format!("no Domino `{name}`; the sizes are {}", names.join(", ")))
        })?),
    };
    let fit = match p.get("domino_fit").and_then(Value::as_str).map(choice) {
        None | Some("") | Some("medium") => loose::Fit::Medium,
        Some("tight") => loose::Fit::Tight,
        Some("wide") => loose::Fit::Wide,
        Some(other) => return Err(invalid("domino_fit", format!("`{other}` is not a width: tight, medium or wide"))),
    };
    let options = loose::Options {
        thickness: length(&p, "thickness")?,
        width: length(&p, "width")?,
        length: length(&p, "length")?,
        count: count(&p, "count")?,
        glue_relief: length(&p, "glue_relief")?,
        domino,
        fit,
    };
    let with_bodies = flag(&p, "tenon_bodies", true)?;
    let plan = loose::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    let (label, body) = if domino.is_some() { ("Domino mortise", "Domino") } else { ("Loose tenon mortise", "Loose tenon") };
    for mortise in &plan.mortises_first {
        cut_shape(mortise, &first, label)?;
    }
    for mortise in &plan.mortises_second {
        cut_shape(mortise, &second, label)?;
    }
    landed(&[(&first, Want::Cut(volume_a)), (&second, Want::Cut(volume_b))])?;
    if with_bodies {
        for (tenon, name) in plan.tenons.iter().zip(free_names(body, plan.tenons.len())?) {
            match tenon {
                loose::Shape::Square(block) => body_from(block, &name, body)?,
                loose::Shape::Rounded { core, ends } => {
                    body_from(core, &name, body)?;
                    for end in ends {
                        bore(end, json!({ "operation": "join", "targets": [name] }), &format!("{body} end"))?;
                    }
                }
            }
        }
    }
    Ok(json!({
        "first": first,
        "second": second,
        "count": plan.tenons.len(),
        "thickness_mm": plan.thickness,
        "width_mm": plan.width,
        "width_second_mm": plan.width_second,
        "length_mm": plan.length,
        "domino": domino.map(|d| format!("{}x{}", d.thickness, d.length)),
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
    let keys = count(&p, "keys")?.unwrap_or(0);
    let options = miter::Options { spline: length(&p, "spline")?, spline_depth: length(&p, "spline_depth")?, keys };
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
    // A loose spline, or keys: each a body turned square to the cut, which then cuts its slot in
    // both boards.
    let names = free_names(if keys > 0 { "Miter key" } else { "Miter spline" }, plan.splines.len())?;
    for ((body, _), name) in plan.splines.iter().zip(names) {
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
    // The slots come after the landing check: they must not cut a board in two either.
    whole([first.as_str(), second.as_str()].into_iter())?;
    Ok(json!({
        "first": first,
        "second": second,
        "spline": !plan.splines.is_empty(),
        "keys": keys,
        "angle_deg": plan.angle,
        "extended_first": plan.extend_first.is_some(),
        "extended_second": plan.extend_second.is_some(),
        "moved_first_mm": moved,
    })
    .to_string())
}

fn tongue_groove(params: &str) -> Result<String, PluginError> {
    let p: Value = serde_json::from_str(params).map_err(|e| invalid("params", e.to_string()))?;
    let Pair { first, second, a, b, volume_a, volume_b, moved } = pair(&p, ("first", "second"), boards::align_end)?;
    let options = tongue::Options {
        thickness: length(&p, "thickness")?,
        depth: length(&p, "depth")?,
        glue_relief: length(&p, "glue_relief")?,
        spline: flag(&p, "spline", false)?,
    };
    let plan = tongue::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    let mut spline = None;
    add(&plan.groove, "cut", &second, "Groove")?;
    match &plan.groove_first {
        // A loose spline: both boards grooved, then the strip as a body of its own.
        Some(groove) => {
            add(groove, "cut", &first, "Groove")?;
            landed(&[(&first, Want::Cut(volume_a)), (&second, Want::Cut(volume_b))])?;
            let name = free_names("Spline", 1)?.remove(0);
            body_from(&plan.tongue, &name, "Spline")?;
            spline = Some(name);
        }
        None => {
            add(&plan.tongue, "join", &first, "Tongue")?;
            landed(&[(&first, Want::Bounds(grown(&a, &[plan.tongue]))), (&second, Want::Cut(volume_b))])?;
        }
    }
    Ok(json!({
        "first": first,
        "second": second,
        "spline": spline,
        "bastard": plan.bastard,
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
    let Pair { first, second, mut a, b, volume_a, volume_b, mut moved } = pair(&p, ("first", "second"), scarf::align)?;
    // Boards end to end that only meet: the first slides over the second by the slope's length.
    if let Some(by) = scarf::slide(&a, &b, length(&p, "slope")?, flag(&p, "align", false)?).map_err(|reason| invalid("slope", reason))? {
        run("solid.move", json!({ "bodies": [first], "translate": by }))?;
        for i in 0..3 {
            a.min[i] += by[i];
            a.max[i] += by[i];
            moved[i] += by[i];
        }
        landed(&[(&first, Want::Bounds(a))])?;
    }
    let options = scarf::Options { flip: flag(&p, "flip", false)?, key: flag(&p, "key", false)?, key_width: length(&p, "key_width")? };
    let plan = scarf::plan(a, b, &options).map_err(|reason| invalid("first", reason))?;
    match &plan.tabled {
        None => {
            let names = free_names("Scarf tool", 2)?;
            slanted_cut(&first, &plan.tool_first, &names[0], "Scarf")?;
            slanted_cut(&second, &plan.tool_second, &names[1], "Scarf")?;
        }
        // A tabled scarf: two steps cut from each board, and the key between them.
        Some(tabled) => {
            for block in &tabled.cuts_first {
                add(block, "cut", &first, "Scarf table")?;
            }
            for block in &tabled.cuts_second {
                add(block, "cut", &second, "Scarf table")?;
            }
        }
    }
    landed(&[(&first, Want::Cut(volume_a)), (&second, Want::Cut(volume_b))])?;
    if let Some(tabled) = &plan.tabled {
        body_from(&tabled.key, &free_names("Scarf key", 1)?.remove(0), "Scarf key")?;
    }
    Ok(json!({
        "first": first,
        "second": second,
        "length_mm": plan.length,
        "slope": plan.slope,
        "tabled": plan.tabled.is_some(),
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
    let Pair { first, second, mut a, b, volume_a, volume_b, mut moved } = pair(&p, ("first", "second"), boards::align_end)?;
    // Boards whose boxes overlap but whose wood does not are joined already (a tenon, a lap):
    // their boxes no longer tell where their faces meet.
    if (0..3).all(|i| a.min[i].max(b.min[i]) < a.max[i].min(b.max[i]) - 1e-6) {
        let found = run("inspect.interference", json!({ "bodies": [first, second] }))?;
        let shared: f64 = found.get("interferences").and_then(Value::as_array).into_iter().flatten().filter_map(|i| i.get("volume_mm3").and_then(Value::as_f64)).sum();
        if shared < 1.0 {
            return Err(invalid(
                "first",
                format!("`{first}` and `{second}` are already joined (a tenon, a lap…); screw them before cutting that joint, or pick two boards that lie face on face"),
            ));
        }
    }
    // Boards with their faces the same way, overlapping seen from above: the first is laid on the second.
    if let Some(by) = screw::stack(&a, &b, flag(&p, "align", false)?).map_err(|reason| invalid("first", reason))? {
        run("solid.move", json!({ "bodies": [first], "translate": by }))?;
        for i in 0..3 {
            a.min[i] += by[i];
            a.max[i] += by[i];
            moved[i] += by[i];
        }
        landed(&[(&first, Want::Bounds(a))])?;
    }
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
