//! Read path. Every query returns plain JSON documents.

use crate::db::{docs, load_doc, Core, Deleted};
use crate::graph::{self, Index};
use crate::model;
use crate::ops::{pert, task_order};
use crate::time::{parse_date_lenient, Clock, WEEKDAY_CN};
use crate::util::{clip, s, str_list, truthy};
use crate::{Error, Result};
use rusqlite::{params_from_iter, Connection};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub fn query(core: &Core, req: Value) -> Result<Value> {
    let o: Map<String, Value> = match req {
        Value::Object(o) => o,
        Value::String(q) => {
            let mut m = Map::new();
            m.insert("q".into(), json!(q));
            m
        }
        _ => return Err(Error::new("invalid_request", "a query is a JSON object").hint("{\"q\":\"list\",\"type\":\"task\",\"status\":\"todo\"}")),
    };
    let kind = o.get("q").or_else(|| o.get("query")).or_else(|| o.get("kind")).and_then(Value::as_str).unwrap_or("").to_lowercase();
    let clock = core.clock();
    let conn = &core.conn;
    let mut out = match kind.as_str() {
        "get" | "show" => get(conn, &clock, &o)?,
        "list" | "ls" | "find" => list(conn, &clock, &o, Deleted::Exclude)?,
        "trash" | "deleted" => list(conn, &clock, &o, Deleted::Only)?,
        "search" => search(conn, &o)?,
        "today" | "agenda" => today(conn, &clock, &o)?,
        "graph" => graph::graph(conn, &o)?,
        "log" | "history" => log(conn, &o)?,
        "schema" => model::schema_json(o.get("type").and_then(Value::as_str)),
        "doctor" | "check" => doctor(conn)?,
        "status" | "info" => status(core, &clock)?,
        "snapshot" | "dump" | "export" => snapshot(core, &clock)?,
        "settings" => json!({ "agent_permission": core.permission() }),
        "" => return Err(Error::new("invalid_request", "which query?").hint("set \"q\" to one of: today, list, get, search, graph, log, trash, schema, doctor, status, snapshot")),
        other => return Err(Error::new("unknown_query", format!("unknown query {:?}", other)).hint("queries: today, list, get, search, graph, log, trash, schema, doctor, status, snapshot")),
    };
    if let Value::Object(m) = &mut out {
        m.insert("ok".into(), json!(true));
    }
    Ok(out)
}

/// Compact view of a document for lists: drops empty values, bookkeeping and big blobs.
pub fn brief(d: &Map<String, Value>, ix: Option<&Index>) -> Value {
    let mut m = Map::new();
    for (k, v) in d {
        let skip = match v {
            Value::Null => true,
            Value::String(x) => x.is_empty(),
            Value::Array(a) => a.is_empty(),
            Value::Bool(b) => !b,
            _ => false,
        };
        if skip || matches!(k.as_str(), "created_by" | "updated_by" | "deleted_at" | "nodes" | "root_id" | "source_type" | "target_type" | "created_at") {
            continue;
        }
        if k == "priority" && v.as_i64() == Some(0) {
            continue;
        }
        if k == "note" || k == "goal" {
            m.insert(k.clone(), json!(clip(v.as_str().unwrap_or(""), 160)));
            continue;
        }
        m.insert(k.clone(), v.clone());
    }
    if let Some(del) = d.get("deleted_at").filter(|x| !x.is_null()) {
        m.insert("deleted_at".into(), del.clone());
    }
    if s(d, "type") == "map" {
        m.insert("nodes".into(), json!(d.get("nodes").and_then(Value::as_object).map_or(0, |n| n.len())));
    }
    if let Some(ix) = ix {
        if let Some(p) = d.get("project_id").and_then(Value::as_str).and_then(|p| ix.get(p)) {
            m.insert("project".into(), json!(s(p, "name")));
        }
        if s(d, "type") == "task" {
            let open: Vec<&str> = str_list(d.get("depends_on")).iter().filter_map(|x| ix.get(x)).filter(|t| s(t, "status") != "done").map(|t| s(t, "id")).collect();
            if !open.is_empty() && s(d, "status") != "done" {
                m.insert("blocked_by".into(), json!(open));
            }
        }
        if s(d, "type") == "link" {
            m.insert("source_title".into(), json!(ix.get(s(d, "source")).map(model::title_of).unwrap_or_default()));
            m.insert("target_title".into(), json!(ix.get(s(d, "target")).map(model::title_of).unwrap_or_default()));
        }
    }
    Value::Object(m)
}

fn brief_ref(ix: &Index, id: &str) -> Value {
    match ix.get(id) {
        Some(d) => {
            let mut o = json!({ "id": id, "type": d.get("type"), "title": model::title_of(d) });
            if let Some(st) = d.get("status") {
                o["status"] = st.clone();
            }
            o
        }
        None => json!({ "id": id, "missing": true }),
    }
}

fn types_param(o: &Map<String, Value>) -> Result<Vec<&'static model::TypeDef>> {
    let raw = o.get("type").or_else(|| o.get("types"));
    let names: Vec<String> = match raw {
        None | Some(Value::Null) => return Ok(vec![]),
        Some(Value::String(x)) => x.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty() && t != "all").collect(),
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(String::from).collect(),
        _ => vec![],
    };
    names
        .iter()
        .map(|n| model::type_def(n).ok_or_else(|| Error::new("invalid_value", format!("unknown type {:?}", n)).hint("types: idea, project, task, map, link")))
        .collect()
}

fn opt_str<'a>(o: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| o.get(*k).and_then(Value::as_str)).map(str::trim).filter(|x| !x.is_empty())
}

fn get(conn: &Connection, clock: &Clock, o: &Map<String, Value>) -> Result<Value> {
    let id = opt_str(o, &["id", "target"]).ok_or_else(|| Error::new("missing_field", "get needs an id").hint("{\"q\":\"get\",\"id\":\"task_…\"}"))?;
    let ix = graph::load_index(conn)?;
    let id = if ix.get(id).is_some() || load_doc(conn, id)?.is_some() {
        id.to_string()
    } else {
        ix.resolve_wikilink(id).ok_or_else(|| Error::new("not_found", format!("no entity {:?}", id)).hint("search with `shiyi search <words>`"))?
    };
    let doc = match ix.get(&id) {
        Some(d) => d.clone(),
        None => load_doc(conn, &id)?.ok_or_else(|| Error::new("not_found", format!("no entity {}", id)))?,
    };
    let (edges, _) = graph::derive(&ix, true);
    let mut out = json!({ "item": Value::Object(doc.clone()) });
    let ty = s(&doc, "type").to_string();
    let outgoing: Vec<Value> = edges.iter().filter(|e| e.source == id && e.kind != "tag").map(|e| {
        let mut v = e.to_json();
        v["other"] = brief_ref(&ix, &e.target);
        v
    }).collect();
    let incoming: Vec<Value> = edges.iter().filter(|e| e.target == id).map(|e| {
        let mut v = e.to_json();
        v["other"] = brief_ref(&ix, &e.source);
        v
    }).collect();
    out["links_out"] = json!(outgoing);
    out["links_in"] = json!(incoming);
    match ty.as_str() {
        "task" => {
            if let Some((te, var)) = pert(&doc) {
                out["pert"] = json!({ "expected_days": (te * 100.0).round() / 100.0, "variance": (var * 1000.0).round() / 1000.0 });
            }
            let deps: Vec<Value> = str_list(doc.get("depends_on")).iter().map(|d| brief_ref(&ix, d)).collect();
            out["depends_on"] = json!(deps);
            let dependents: Vec<Value> = ix.docs.iter().filter(|t| s(t, "type") == "task" && str_list(t.get("depends_on")).contains(&id)).map(|t| brief_ref(&ix, s(t, "id"))).collect();
            out["dependents"] = json!(dependents);
        }
        "project" => {
            let mut tasks: Vec<&Map<String, Value>> = ix.docs.iter().filter(|t| s(t, "type") == "task" && s(t, "project_id") == id).collect();
            tasks.sort_by(|a, b| task_order(a, b));
            let today = clock.today();
            let count = |st: &str| tasks.iter().filter(|t| s(t, "status") == st).count();
            let overdue = tasks.iter().filter(|t| s(t, "status") != "done" && t.get("due_date").and_then(Value::as_str).map_or(false, |d| d < today.as_str())).count();
            out["stats"] = json!({ "total": tasks.len(), "todo": count("todo"), "doing": count("doing"), "done": count("done"), "overdue": overdue });
            out["tasks"] = json!(tasks.iter().map(|t| brief(t, Some(&ix))).collect::<Vec<_>>());
            out["ideas"] = json!(ix.docs.iter().filter(|d| s(d, "type") == "idea" && s(d, "project_id") == id).map(|d| brief(d, None)).collect::<Vec<_>>());
            out["maps"] = json!(ix.docs.iter().filter(|d| s(d, "type") == "map" && s(d, "project_id") == id).map(|d| brief(d, None)).collect::<Vec<_>>());
        }
        "map" => {
            out["outline"] = json!(map_outline(&doc));
        }
        _ => {}
    }
    Ok(out)
}

/// Indented text of a mind map.
pub fn map_outline(d: &Map<String, Value>) -> String {
    let Some(nodes) = d.get("nodes").and_then(Value::as_object) else { return String::new() };
    let mut out = String::new();
    fn walk(nodes: &Map<String, Value>, id: &str, depth: usize, out: &mut String, guard: &mut usize) {
        *guard += 1;
        if *guard > 5000 || depth > 50 {
            return;
        }
        let Some(n) = nodes.get(id) else { return };
        out.push_str(&"  ".repeat(depth));
        out.push_str(n.get("text").and_then(Value::as_str).unwrap_or(""));
        if let Some(l) = n.get("link").and_then(|l| l.get("id")).and_then(Value::as_str) {
            out.push_str(&format!("  ({})", l));
        }
        out.push('\n');
        for c in n.get("children").and_then(Value::as_array).cloned().unwrap_or_default() {
            if let Some(c) = c.as_str() {
                walk(nodes, c, depth + 1, out, guard);
            }
        }
    }
    let mut guard = 0;
    walk(nodes, s(d, "root_id"), 0, &mut out, &mut guard);
    out
}

fn list(conn: &Connection, clock: &Clock, o: &Map<String, Value>, deleted: Deleted) -> Result<Value> {
    let types = types_param(o)?;
    if types.is_empty() && deleted == Deleted::Exclude {
        return Err(Error::new("missing_field", "list needs a type").hint("type: idea | project | task | map | link (several: \"task,idea\")"));
    }
    let ix = graph::load_index(conn)?;
    let mut all: Vec<Map<String, Value>> = vec![];
    if types.is_empty() {
        all = docs(conn, None, deleted)?;
    } else {
        for t in &types {
            all.extend(docs(conn, Some(t.name), deleted)?);
        }
    }
    let today = clock.today();
    let date_param = |keys: &[&str]| -> Result<Option<String>> {
        match opt_str(o, keys) {
            None => Ok(None),
            Some(x) => parse_date_lenient(x, &today).map(Some).ok_or_else(|| Error::new("invalid_value", format!("cannot read date {:?}", x)).hint("YYYY-MM-DD, today, +7d, fri…")),
        }
    };
    let due_before = date_param(&["due_before", "before"])?;
    let due_after = date_param(&["due_after", "after"])?;
    let statuses: Vec<String> = match o.get("status") {
        Some(Value::String(x)) => x.split(',').map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect(),
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_lowercase).collect(),
        _ => vec![],
    };
    let not_done = truthy(o.get("open")) || truthy(o.get("not_done"));
    let project = match opt_str(o, &["project", "project_id"]) {
        None => None,
        Some("none") | Some("loose") => Some(String::new()),
        Some(p) => Some(if ix.get(p).is_some() {
            p.to_string()
        } else {
            let hit = ix.docs.iter().filter(|d| s(d, "type") == "project").find(|d| s(d, "name").to_lowercase() == p.to_lowercase()).or_else(|| {
                ix.docs.iter().filter(|d| s(d, "type") == "project").find(|d| s(d, "name").to_lowercase().contains(&p.to_lowercase()))
            });
            hit.map(|d| s(d, "id").to_string()).ok_or_else(|| Error::new("not_found", format!("no project {:?}", p)).hint("`shiyi list projects` shows them"))?
        }),
    };
    let tag = opt_str(o, &["tag"]).map(|t| t.trim_start_matches('#').to_lowercase());
    let text = opt_str(o, &["text", "contains", "search", "keyword"]).map(str::to_lowercase);
    let planned_today = truthy(o.get("today"));
    let starred = truthy(o.get("starred"));
    let of = opt_str(o, &["of", "node"]);
    let link_status = opt_str(o, &["link_status"]);
    let created_by = opt_str(o, &["created_by", "by"]);
    let mut rows: Vec<Map<String, Value>> = all
        .into_iter()
        .filter(|d| statuses.is_empty() || statuses.iter().any(|x| x == s(d, "status") || model::status_cn(s(d, "status")) == x))
        .filter(|d| !not_done || !matches!(s(d, "status"), "done" | "archived" | "converted"))
        .filter(|d| match &project {
            None => true,
            Some(p) if p.is_empty() => s(d, "project_id").is_empty(),
            Some(p) => s(d, "project_id") == p || (s(d, "type") == "project" && s(d, "id") == p),
        })
        .filter(|d| tag.as_ref().map_or(true, |t| str_list(d.get("tags")).iter().any(|x| x.to_lowercase() == *t)))
        .filter(|d| {
            text.as_ref().map_or(true, |q| {
                ["text", "title", "name", "note", "goal"].iter().any(|k| s(d, k).to_lowercase().contains(q.as_str()))
            })
        })
        .filter(|d| due_before.as_ref().map_or(true, |b| d.get("due_date").and_then(Value::as_str).map_or(false, |x| x <= b.as_str())))
        .filter(|d| due_after.as_ref().map_or(true, |a| d.get("due_date").and_then(Value::as_str).map_or(false, |x| x >= a.as_str())))
        .filter(|d| !planned_today || s(d, "today_date") == today)
        .filter(|d| !starred || d.get("starred") == Some(&json!(true)))
        .filter(|d| of.map_or(true, |x| s(d, "source") == x || s(d, "target") == x))
        .filter(|d| link_status.map_or(true, |x| s(d, "type") != "link" || s(d, "status") == x))
        .filter(|d| created_by.map_or(true, |x| s(d, "created_by") == x || (x == "agents" && s(d, "created_by").starts_with("agent:"))))
        .collect();
    let sort = opt_str(o, &["sort", "order_by"]).unwrap_or(if types.len() == 1 && types[0].name == "task" { "smart" } else { "updated" });
    match sort {
        "smart" | "status" => rows.sort_by(task_order),
        "due" | "due_date" => rows.sort_by(|a, b| a.get("due_date").and_then(Value::as_str).unwrap_or("9999").cmp(b.get("due_date").and_then(Value::as_str).unwrap_or("9999"))),
        "created" | "created_at" => rows.sort_by(|a, b| s(b, "created_at").cmp(s(a, "created_at"))),
        "priority" => rows.sort_by_key(|d| match d.get("priority").and_then(Value::as_i64).unwrap_or(0) { 0 => 9, p => p }),
        "rank" => rows.sort_by(|a, b| a.get("rank").and_then(Value::as_f64).unwrap_or(f64::MAX).partial_cmp(&b.get("rank").and_then(Value::as_f64).unwrap_or(f64::MAX)).unwrap_or(std::cmp::Ordering::Equal)),
        "title" | "name" => rows.sort_by_key(model::title_of),
        _ => rows.sort_by(|a, b| s(b, "updated_at").cmp(s(a, "updated_at"))),
    }
    let total = rows.len();
    let offset = crate::util::as_u64(o.get("offset")).unwrap_or(0) as usize;
    let limit = crate::util::as_u64(o.get("limit")).unwrap_or(100).min(2000) as usize;
    let full = truthy(o.get("full"));
    let items: Vec<Value> = rows.iter().skip(offset).take(limit).map(|d| if full { Value::Object(d.clone()) } else { brief(d, Some(&ix)) }).collect();
    let mut out = json!({ "total": total, "count": items.len(), "items": items });
    if offset + limit < total {
        out["next_offset"] = json!(offset + limit);
    }
    Ok(out)
}

fn search(conn: &Connection, o: &Map<String, Value>) -> Result<Value> {
    let q = opt_str(o, &["text", "query_text", "words", "keyword", "search"]).ok_or_else(|| Error::new("missing_field", "search needs text").hint("{\"q\":\"search\",\"text\":\"周报\"}"))?;
    let types = types_param(o)?;
    let ix = graph::load_index(conn)?;
    let words: Vec<String> = q.to_lowercase().split_whitespace().map(String::from).collect();
    let limit = crate::util::as_u64(o.get("limit")).unwrap_or(20) as usize;
    let mut hits: Vec<(i64, &Map<String, Value>, String)> = vec![];
    for d in &ix.docs {
        let ty = s(d, "type");
        if ty == "link" || (!types.is_empty() && !types.iter().any(|t| t.name == ty)) {
            continue;
        }
        let title = model::title_of(d).to_lowercase();
        let body: String = ["text", "note", "goal"].iter().map(|k| s(d, k)).collect::<Vec<_>>().join("\n");
        let mut extra = body.clone();
        if ty == "map" {
            extra.push('\n');
            extra.push_str(&map_outline(d));
        }
        let low = extra.to_lowercase();
        let tags: Vec<String> = str_list(d.get("tags")).iter().map(|t| t.to_lowercase()).collect();
        let mut score = 0i64;
        let mut all = true;
        for w in &words {
            let mut hit = false;
            if title == *w {
                score += 10;
                hit = true;
            } else if title.contains(w.as_str()) {
                score += 5;
                hit = true;
            }
            if tags.iter().any(|t| t == w.trim_start_matches('#')) {
                score += 4;
                hit = true;
            }
            if low.contains(w.as_str()) {
                score += 1;
                hit = true;
            }
            if s(d, "id") == w {
                score += 20;
                hit = true;
            }
            all &= hit;
        }
        if all && score > 0 {
            let snippet = words.iter().find_map(|w| {
                let lw = low.find(w.as_str())?;
                let start = low[..lw].char_indices().rev().nth(20).map_or(0, |(i, _)| i);
                Some(clip(&extra[start.min(extra.len())..].replace('\n', " "), 80))
            });
            hits.push((score, d, snippet.unwrap_or_default()));
        }
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(s(b.1, "updated_at").cmp(s(a.1, "updated_at"))));
    let items: Vec<Value> = hits
        .iter()
        .take(limit)
        .map(|(sc, d, snip)| {
            let mut v = brief_ref(&ix, s(d, "id"));
            v["score"] = json!(sc);
            if !snip.is_empty() {
                v["snippet"] = json!(snip);
            }
            if let Some(p) = d.get("project_id").and_then(Value::as_str).and_then(|p| ix.get(p)) {
                v["project"] = json!(s(p, "name"));
            }
            v
        })
        .collect();
    Ok(json!({ "total": hits.len(), "items": items }))
}

fn today(conn: &Connection, clock: &Clock, o: &Map<String, Value>) -> Result<Value> {
    let ix = graph::load_index(conn)?;
    let today = match opt_str(o, &["date"]) {
        Some(d) => parse_date_lenient(d, &clock.today()).ok_or_else(|| Error::new("invalid_value", "bad date"))?,
        None => clock.today(),
    };
    let week_end = crate::time::add_days(&today, 7).unwrap_or_default();
    let mut tasks: Vec<&Map<String, Value>> = ix.docs.iter().filter(|d| s(d, "type") == "task").collect();
    tasks.sort_by(|a, b| task_order(a, b));
    let open = |t: &Map<String, Value>| s(t, "status") != "done";
    let due = |t: &Map<String, Value>| t.get("due_date").and_then(Value::as_str).unwrap_or("").to_string();
    let b = |t: &&Map<String, Value>| brief(t, Some(&ix));
    let overdue: Vec<Value> = tasks.iter().filter(|t| open(t)).filter(|t| !due(t).is_empty() && due(t) < today).map(b).collect();
    let planned: Vec<Value> = tasks.iter().filter(|t| open(t)).filter(|t| s(t, "today_date") == today || due(t) == today).map(b).collect();
    let doing: Vec<Value> = tasks.iter().filter(|t| s(t, "status") == "doing" && s(t, "today_date") != today && due(t) != today && !(due(t) < today && !due(t).is_empty())).map(b).collect();
    let upcoming: Vec<Value> = tasks.iter().filter(|t| open(t)).filter(|t| due(t) > today && due(t) <= week_end && s(t, "today_date") != today).map(b).collect();
    let done_today: Vec<Value> = tasks
        .iter()
        .filter(|t| s(t, "status") == "done" && t.get("done_at").and_then(Value::as_str).and_then(|x| clock.date_of(x)).as_deref() == Some(today.as_str()))
        .map(b)
        .collect();
    let mut inbox: Vec<&Map<String, Value>> = ix.docs.iter().filter(|d| s(d, "type") == "idea" && s(d, "status") == "inbox").collect();
    inbox.sort_by(|a, b| s(b, "created_at").cmp(s(a, "created_at")));
    let projects: Vec<Value> = ix
        .docs
        .iter()
        .filter(|d| s(d, "type") == "project" && s(d, "status") == "active")
        .map(|p| {
            let pid = s(p, "id");
            let ts: Vec<&&Map<String, Value>> = tasks.iter().filter(|t| s(t, "project_id") == pid).collect();
            let done = ts.iter().filter(|t| s(t, "status") == "done").count();
            let ready = |t: &Map<String, Value>| str_list(t.get("depends_on")).iter().all(|d| ix.get(d).map_or(true, |x| s(x, "status") == "done"));
            let next = ts
                .iter()
                .find(|t| s(t, "status") != "done" && ready(t))
                .or_else(|| ts.iter().find(|t| s(t, "status") != "done"))
                .map(|t| json!({ "id": t.get("id"), "title": t.get("title") }));
            json!({ "id": pid, "name": s(p, "name"), "area": p.get("area"), "due_date": p.get("due_date"), "tasks": ts.len(), "done": done, "next": next })
        })
        .collect();
    let days = crate::time::parse_date(&today).unwrap_or(0);
    Ok(json!({
        "date": today,
        "weekday": WEEKDAY_CN[crate::time::weekday(days) as usize],
        "overdue": overdue,
        "today": planned,
        "doing": doing,
        "upcoming_7d": upcoming,
        "done_today": done_today,
        "inbox": { "count": inbox.len(), "latest": inbox.iter().take(5).map(|d| brief(d, None)).collect::<Vec<_>>() },
        "projects": projects,
        "suggested_links": ix.docs.iter().filter(|d| s(d, "type") == "link" && s(d, "status") == "suggested").count()
    }))
}

fn log(conn: &Connection, o: &Map<String, Value>) -> Result<Value> {
    let limit = crate::util::as_u64(o.get("limit")).unwrap_or(20).min(500) as i64;
    let mut cond: Vec<String> = vec![];
    let mut args: Vec<Value> = vec![];
    match opt_str(o, &["actor", "by"]) {
        Some("agents") | Some("agent") | Some("ai") => cond.push("actor LIKE 'agent:%'".into()),
        Some(a) => {
            cond.push("actor = ?".into());
            args.push(json!(if a == "user" || a.starts_with("agent:") { a.to_string() } else { format!("agent:{}", a) }));
        }
        None => {}
    }
    if let Some(e) = opt_str(o, &["entity", "id"]) {
        cond.push("entity_id = ?".into());
        args.push(json!(e));
    }
    if let Some(b) = opt_str(o, &["batch"]) {
        cond.push("batch = ?".into());
        args.push(json!(b));
    }
    if let Some(sq) = crate::util::as_i64(o.get("since_seq")) {
        cond.push("seq > ?".into());
        args.push(json!(sq));
    }
    let wh = if cond.is_empty() { "1=1".to_string() } else { cond.join(" AND ") };
    let sql_args: Vec<rusqlite::types::Value> = args
        .iter()
        .map(|v| match v {
            Value::Number(n) => rusqlite::types::Value::Integer(n.as_i64().unwrap_or(0)),
            other => rusqlite::types::Value::Text(other.as_str().unwrap_or("").to_string()),
        })
        .collect();
    let mut st = conn.prepare(&format!("SELECT batch, MAX(seq) m FROM ops WHERE {} GROUP BY batch ORDER BY m DESC LIMIT {}", wh, limit))?;
    let batches: Vec<String> = st.query_map(params_from_iter(sql_args.iter()), |r| r.get(0))?.collect::<std::result::Result<_, _>>()?;
    let detail = truthy(o.get("detail"));
    let mut out = vec![];
    for b in &batches {
        let mut st = conn.prepare("SELECT seq,id,ts,actor,op,entity_type,entity_id,summary,note,undone_by,before,after FROM ops WHERE batch=?1 ORDER BY seq")?;
        let rows = st.query_map([b], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, Option<String>>(11)?,
            ))
        })?;
        let rows: Vec<_> = rows.collect::<std::result::Result<_, _>>()?;
        let Some(first) = rows.first() else { continue };
        let undone_by = rows.iter().find_map(|r| r.9.clone());
        let all_undone = rows.iter().all(|r| r.9.is_some());
        let is_undo = rows.iter().all(|r| r.4.ends_with(".undo"));
        let undo_of: Option<String> = if is_undo { conn.query_row("SELECT batch FROM ops WHERE undone_by=?1 LIMIT 1", [b], |r| r.get(0)).ok() } else { None };
        let ops: Vec<Value> = rows
            .iter()
            .map(|r| {
                let mut v = json!({ "seq": r.0, "op_id": r.1, "op": r.4, "entity_type": r.5, "entity_id": r.6, "summary": r.7 });
                if r.9.is_some() {
                    v["undone"] = json!(true);
                }
                if detail {
                    v["before"] = r.10.as_ref().and_then(|x| serde_json::from_str(x).ok()).unwrap_or(Value::Null);
                    v["after"] = r.11.as_ref().and_then(|x| serde_json::from_str(x).ok()).unwrap_or(Value::Null);
                }
                v
            })
            .collect();
        let mut bj = json!({ "batch": b, "ts": first.2, "actor": first.3, "ops": ops, "undone": all_undone });
        if let Some(u) = undone_by {
            bj["undone_by"] = json!(u);
        }
        if let Some(n) = &first.8 {
            bj["note"] = json!(n);
        }
        if let Some(u) = undo_of {
            bj["undo_of"] = json!(u);
        }
        out.push(bj);
    }
    let max_seq: i64 = conn.query_row("SELECT COALESCE(MAX(seq),0) FROM ops", [], |r| r.get(0))?;
    Ok(json!({ "batches": out, "max_seq": max_seq }))
}

fn doctor(conn: &Connection) -> Result<Value> {
    let ix = graph::load_index(conn)?;
    let all = docs(conn, None, Deleted::Include)?;
    let exists: HashMap<String, bool> = all.iter().map(|d| (s(d, "id").to_string(), d.get("deleted_at").map_or(true, Value::is_null))).collect();
    let mut issues = vec![];
    let mut check_ref = |d: &Map<String, Value>, field: &str, v: &str| {
        if v.is_empty() {
            return;
        }
        match exists.get(v) {
            None => issues.push(json!({ "id": d.get("id"), "problem": "missing_ref", "field": field, "ref": v, "fix": { "op": format!("{}.update", s(d, "type")), "id": d.get("id"), field: Value::Null } })),
            Some(false) => issues.push(json!({ "id": d.get("id"), "problem": "deleted_ref", "field": field, "ref": v, "fix": format!("restore {} or clear {}", v, field) })),
            _ => {}
        }
    };
    for d in &ix.docs {
        for f in ["project_id", "task_id", "idea_id", "from_idea", "from_map"] {
            check_ref(d, f, s(d, f));
        }
        if s(d, "type") == "task" {
            for dep in str_list(d.get("depends_on")) {
                check_ref(d, "depends_on", &dep);
            }
        }
        if s(d, "type") == "link" {
            check_ref(d, "source", s(d, "source"));
            check_ref(d, "target", s(d, "target"));
        }
    }
    // dependency cycles
    let deps: HashMap<&str, Vec<String>> = ix.docs.iter().filter(|d| s(d, "type") == "task").map(|d| (s(d, "id"), str_list(d.get("depends_on")))).collect();
    let mut color: HashMap<&str, u8> = HashMap::new();
    fn dfs<'a>(n: &'a str, deps: &'a HashMap<&'a str, Vec<String>>, color: &mut HashMap<&'a str, u8>, found: &mut Vec<String>) {
        color.insert(n, 1);
        if let Some(ds) = deps.get(n) {
            for d in ds {
                if let Some((k, _)) = deps.get_key_value(d.as_str()) {
                    match color.get(k).copied().unwrap_or(0) {
                        1 => found.push(format!("{} → {}", n, d)),
                        0 => dfs(k, deps, color, found),
                        _ => {}
                    }
                }
            }
        }
        color.insert(n, 2);
    }
    let mut cycles = vec![];
    for k in deps.keys() {
        if color.get(k).copied().unwrap_or(0) == 0 {
            dfs(k, &deps, &mut color, &mut cycles);
        }
    }
    for c in cycles {
        issues.push(json!({ "problem": "dependency_cycle", "edge": c, "fix": "remove one dependency with task.remove_dep" }));
    }
    let (_, unresolved) = graph::derive(&ix, false);
    for u in unresolved {
        issues.push(json!({ "id": u["in"], "problem": "unresolved_wikilink", "text": u["text"], "fix": "create an entity with that exact title, or edit the text" }));
    }
    for d in ix.docs.iter().filter(|d| s(d, "type") == "map") {
        let ok = d.get("nodes").and_then(Value::as_object).map_or(false, |n| n.contains_key(s(d, "root_id")));
        if !ok {
            issues.push(json!({ "id": d.get("id"), "problem": "map_without_root" }));
        }
    }
    let integrity: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    Ok(json!({ "healthy": issues.is_empty() && integrity == "ok", "sqlite": integrity, "issues": issues }))
}

fn counts(conn: &Connection) -> Result<Value> {
    let mut m = Map::new();
    let mut st = conn.prepare("SELECT type, SUM(deleted_at IS NULL), SUM(deleted_at IS NOT NULL) FROM entities GROUP BY type")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?;
    let mut deleted = 0;
    for r in rows {
        let (t, alive, del) = r?;
        let plural = model::type_def(&t).map(|d| d.plural).unwrap_or("other");
        m.insert(plural.into(), json!(alive));
        deleted += del;
    }
    for t in model::TYPES.iter() {
        m.entry(t.plural.to_string()).or_insert(json!(0));
    }
    m.insert("deleted".into(), json!(deleted));
    Ok(Value::Object(m))
}

fn status(core: &Core, clock: &Clock) -> Result<Value> {
    Ok(json!({
        "app": "十一遗",
        "version": crate::VERSION,
        "db": core.path().map(|p| p.display().to_string()),
        "schema_version": core.meta_get("schema_version")?,
        "agent_permission": core.permission(),
        "today": clock.today(),
        "now": clock.now_iso(),
        "counts": counts(&core.conn)?,
        "last_seq": core.max_seq()
    }))
}

fn snapshot(core: &Core, clock: &Clock) -> Result<Value> {
    let mut out = json!({
        "format": "shiyi.snapshot",
        "version": 1,
        "exported_at": clock.now_iso(),
        "seq": core.max_seq(),
        "agent_permission": core.permission()
    });
    for t in model::TYPES.iter() {
        out[t.plural] = json!(docs(&core.conn, Some(t.name), Deleted::Exclude)?);
    }
    Ok(out)
}
