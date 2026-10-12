//! Imports the GUI's old `data.json` (camelCase, millisecond timestamps,
//! unprefixed ids) and snapshot exports.

use crate::db::Core;
use crate::model;
use crate::{Error, Result};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

fn arr<'a>(v: &'a Value, k: &str) -> Vec<&'a Map<String, Value>> {
    v.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_object).collect()).unwrap_or_default()
}

fn sanitize(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(48).collect()
}

/// Old id -> new prefixed id. Deterministic, so importing twice updates instead of duplicating.
fn map_id(prefix: &str, old: &str) -> String {
    if old.starts_with(&format!("{}_", prefix)) {
        old.to_string()
    } else {
        format!("{}_{}", prefix, sanitize(old))
    }
}

pub fn import(core: &mut Core, data: Value, actor: &str) -> Result<Value> {
    if !data.is_object() {
        return Err(Error::new("invalid_request", "import expects a JSON object").hint("a 十一遗 backup (.json)"));
    }
    let snapshot = data.get("format").and_then(Value::as_str) == Some("shiyi.snapshot")
        || data.get("links").is_some()
        || arr(&data, "tasks").iter().any(|t| t.contains_key("project_id") || t.contains_key("due_date"));
    let mut ops: Vec<Value> = vec![];
    if snapshot {
        for t in model::TYPES.iter() {
            for d in arr(&data, t.plural) {
                if d.get("deleted_at").map_or(false, |x| !x.is_null()) {
                    continue;
                }
                ops.push(json!({ "op": "entity.put", "type": t.name, "doc": d }));
            }
        }
    } else {
        let mut ids: HashMap<(String, String), String> = HashMap::new();
        for (col, prefix) in [("ideas", "idea"), ("projects", "proj"), ("tasks", "task"), ("maps", "map")] {
            for d in arr(&data, col) {
                if let Some(id) = d.get("id").and_then(|v| v.as_str().map(String::from).or_else(|| v.as_i64().map(|n| n.to_string()))) {
                    ids.insert((col.to_string(), id.clone()), map_id(prefix, &id));
                }
            }
        }
        let r = |col: &str, v: Option<&Value>| -> Value {
            match v.and_then(Value::as_str) {
                Some(x) if !x.is_empty() => json!(ids.get(&(col.to_string(), x.to_string())).cloned().unwrap_or_else(|| x.to_string())),
                _ => Value::Null,
            }
        };
        let g = |d: &Map<String, Value>, k: &str| d.get(k).cloned().unwrap_or(Value::Null);
        let idof = |col: &str, d: &Map<String, Value>| r(col, d.get("id"));
        for d in arr(&data, "projects") {
            ops.push(json!({ "op": "entity.put", "type": "project", "doc": {
                "id": idof("projects", d), "name": g(d, "name"), "area": g(d, "area"), "color": g(d, "color"), "status": g(d, "status"),
                "goal": g(d, "goal"), "note": g(d, "note"), "start_date": g(d, "start"), "due_date": g(d, "due"),
                "from_idea": r("ideas", d.get("fromIdea")), "created_at": g(d, "createdAt"), "sample": g(d, "sample")
            }}));
        }
        for d in arr(&data, "ideas") {
            ops.push(json!({ "op": "entity.put", "type": "idea", "doc": {
                "id": idof("ideas", d), "text": g(d, "text"), "note": g(d, "note"), "tags": g(d, "tags"), "status": g(d, "status"),
                "starred": g(d, "star"), "project_id": r("projects", d.get("projectId")), "task_id": r("tasks", d.get("taskId")),
                "created_at": g(d, "createdAt"), "sample": g(d, "sample")
            }}));
        }
        for d in arr(&data, "tasks") {
            let est = json!({ "optimistic": g(d, "estO"), "likely": g(d, "estM"), "pessimistic": g(d, "estP") });
            let deps: Vec<Value> = d.get("deps").and_then(Value::as_array).map(|a| a.iter().map(|x| r("tasks", Some(x))).filter(|x| !x.is_null()).collect()).unwrap_or_default();
            ops.push(json!({ "op": "entity.put", "type": "task", "doc": {
                "id": idof("tasks", d), "title": g(d, "title"), "project_id": r("projects", d.get("projectId")), "status": g(d, "status"),
                "priority": g(d, "priority"), "start_date": g(d, "start"), "due_date": g(d, "due"), "today_date": g(d, "todayDate"),
                "note": g(d, "note"), "estimate": est, "depends_on": deps, "rank": g(d, "rank"), "done_at": g(d, "doneAt"),
                "from_idea": r("ideas", d.get("fromIdea")), "created_at": g(d, "createdAt"), "sample": g(d, "sample")
            }}));
        }
        for d in arr(&data, "maps") {
            let mut nodes = d.get("nodes").cloned().unwrap_or(json!({}));
            if let Some(ns) = nodes.as_object_mut() {
                for n in ns.values_mut() {
                    if let Some(link) = n.get_mut("link").and_then(Value::as_object_mut) {
                        let col = match link.get("type").and_then(Value::as_str) {
                            Some("task") => "tasks",
                            Some("idea") => "ideas",
                            Some("project") => "projects",
                            _ => "",
                        };
                        if !col.is_empty() {
                            let nid = r(col, link.get("id"));
                            link.insert("id".into(), nid);
                        }
                    }
                }
            }
            ops.push(json!({ "op": "entity.put", "type": "map", "doc": {
                "id": idof("maps", d), "title": g(d, "title"), "root_id": g(d, "rootId"), "nodes": nodes,
                "idea_id": r("ideas", d.get("ideaId")), "project_id": r("projects", d.get("projectId")),
                "created_at": g(d, "createdAt"), "sample": g(d, "sample")
            }}));
        }
    }
    // drop nulls so defaults apply
    for op in ops.iter_mut() {
        if let Some(doc) = op.get_mut("doc").and_then(Value::as_object_mut) {
            doc.retain(|_, v| !v.is_null());
            if let Some(Value::Object(e)) = doc.get("estimate") {
                if e.values().all(Value::is_null) {
                    doc.remove("estimate");
                }
            }
            if doc.get("priority").map_or(false, |p| p.is_string()) {
                let n = doc["priority"].as_str().and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
                doc.insert("priority".into(), json!(n));
            }
        }
    }
    if ops.is_empty() {
        return Ok(json!({ "ok": true, "changed": 0, "imported": 0 }));
    }
    let total = ops.len();
    let ops: Vec<Map<String, Value>> = ops.into_iter().filter_map(|v| match v {
        Value::Object(o) => Some(o),
        _ => None,
    }).collect();
    let mut res = crate::ops::run_batch_mode(core, ops, false, Some("导入数据".into()), actor, true)?;
    res["imported"] = json!(total);
    if let Some(r) = res.as_object_mut() {
        r.remove("results");
        r.remove("changes");
    }
    Ok(res)
}
