//! The knowledge graph: explicit links plus edges derived from fields,
//! [[wikilinks]] and shared tags.

use crate::db::{docs, Deleted};
use crate::model;
use crate::util::{s, str_list};
use crate::{Error, Result};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone, Debug)]
pub struct Edge {
    pub source: String,
    pub target: String,
    pub relation: String,
    /// field | link | mention | tag
    pub kind: &'static str,
    pub link_id: Option<String>,
    pub status: Option<String>,
    pub note: Option<String>,
}

impl Edge {
    pub fn to_json(&self) -> Value {
        let mut o = json!({ "source": self.source, "target": self.target, "relation": self.relation, "kind": self.kind });
        if let Some(l) = &self.link_id {
            o["link_id"] = json!(l);
        }
        if let Some(st) = &self.status {
            o["status"] = json!(st);
        }
        if let Some(n) = self.note.as_ref().filter(|n| !n.is_empty()) {
            o["note"] = json!(n);
        }
        o
    }
}

/// `[[target]]` or `[[target|label]]` occurrences in a text.
pub fn wikilinks(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut rest = text;
    while let Some(i) = rest.find("[[") {
        let after = &rest[i + 2..];
        match after.find("]]") {
            Some(j) => {
                let inner = &after[..j];
                if !inner.contains('\n') && !inner.contains("[[") {
                    let target = inner.split('|').next().unwrap_or("").trim();
                    if !target.is_empty() && !out.iter().any(|x: &String| x == target) {
                        out.push(target.to_string());
                    }
                }
                rest = &after[j + 2..];
            }
            None => break,
        }
    }
    out
}

/// Texts of a document that may contain wikilinks.
fn texts(d: &Map<String, Value>) -> Vec<String> {
    let mut v: Vec<String> = match s(d, "type") {
        "idea" => vec![s(d, "text").into(), s(d, "note").into()],
        "task" => vec![s(d, "title").into(), s(d, "note").into()],
        "project" => vec![s(d, "goal").into(), s(d, "note").into()],
        _ => vec![],
    };
    if s(d, "type") == "map" {
        if let Some(nodes) = d.get("nodes").and_then(Value::as_object) {
            for n in nodes.values() {
                if let Some(t) = n.get("text").and_then(Value::as_str) {
                    v.push(t.to_string());
                }
            }
        }
    }
    v
}

pub struct Index {
    pub docs: Vec<Map<String, Value>>,
    pub by_id: HashMap<String, usize>,
    by_title: HashMap<String, Vec<usize>>,
}

impl Index {
    pub fn new(docs: Vec<Map<String, Value>>) -> Index {
        let mut by_id = HashMap::new();
        let mut by_title: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, d) in docs.iter().enumerate() {
            by_id.insert(s(d, "id").to_string(), i);
            if s(d, "type") != "link" {
                let t = model::title_of(d).to_lowercase();
                if !t.is_empty() {
                    by_title.entry(t).or_default().push(i);
                }
            }
        }
        Index { docs, by_id, by_title }
    }
    pub fn get(&self, id: &str) -> Option<&Map<String, Value>> {
        self.by_id.get(id).map(|i| &self.docs[*i])
    }
    pub fn resolve_wikilink(&self, target: &str) -> Option<String> {
        if let Some(d) = self.get(target) {
            if s(d, "type") != "link" {
                return Some(target.to_string());
            }
        }
        let hits = self.by_title.get(&target.to_lowercase())?;
        // prefer projects, then ideas, tasks, maps
        let rank = |t: &str| match t {
            "project" => 0,
            "idea" => 1,
            "task" => 2,
            _ => 3,
        };
        hits.iter().map(|i| &self.docs[*i]).min_by_key(|d| rank(s(d, "type"))).map(|d| s(d, "id").to_string())
    }
}

pub fn load_index(conn: &Connection) -> Result<Index> {
    Ok(Index::new(docs(conn, None, Deleted::Exclude)?))
}

/// All edges among live documents. Returns (edges, unresolved wikilinks).
pub fn derive(ix: &Index, include_tags: bool) -> (Vec<Edge>, Vec<Value>) {
    let mut edges: Vec<Edge> = vec![];
    let mut pairs: HashSet<(String, String)> = HashSet::new();
    let mut unresolved = vec![];
    let alive = |id: &str| ix.get(id).is_some();
    let mut field = |edges: &mut Vec<Edge>, a: &str, b: &str, rel: &str| {
        if a.is_empty() || b.is_empty() || a == b || !alive(b) {
            return;
        }
        let key = if a < b { (a.to_string(), b.to_string()) } else { (b.to_string(), a.to_string()) };
        if rel != "depends_on" && !pairs.insert(key) {
            return;
        }
        edges.push(Edge { source: a.into(), target: b.into(), relation: rel.into(), kind: "field", link_id: None, status: None, note: None });
    };
    for d in &ix.docs {
        let id = s(d, "id");
        match s(d, "type") {
            "idea" => {
                let conv = s(d, "status") == "converted";
                field(&mut edges, id, s(d, "task_id"), "converted_to");
                field(&mut edges, id, s(d, "project_id"), if conv { "converted_to" } else { "part_of" });
            }
            "task" => {
                field(&mut edges, id, s(d, "project_id"), "part_of");
                for dep in str_list(d.get("depends_on")) {
                    field(&mut edges, id, &dep, "depends_on");
                }
                field(&mut edges, id, s(d, "from_idea"), "derived_from");
            }
            "project" => {
                field(&mut edges, id, s(d, "from_idea"), "derived_from");
                field(&mut edges, id, s(d, "from_map"), "derived_from");
            }
            "map" => {
                field(&mut edges, id, s(d, "idea_id"), "expands");
                field(&mut edges, id, s(d, "project_id"), "part_of");
                if let Some(nodes) = d.get("nodes").and_then(Value::as_object) {
                    for n in nodes.values() {
                        if let Some(l) = n.get("link").and_then(|l| l.get("id")).and_then(Value::as_str) {
                            field(&mut edges, id, l, "mentions");
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // wikilinks
    let mut seen_mentions: HashSet<(String, String)> = HashSet::new();
    for d in &ix.docs {
        let id = s(d, "id");
        if s(d, "type") == "link" {
            continue;
        }
        for t in texts(d) {
            for w in wikilinks(&t) {
                match ix.resolve_wikilink(&w) {
                    Some(target) if target != id => {
                        if seen_mentions.insert((id.to_string(), target.clone())) {
                            edges.push(Edge { source: id.into(), target, relation: "mentions".into(), kind: "mention", link_id: None, status: None, note: None });
                        }
                    }
                    Some(_) => {}
                    None => unresolved.push(json!({ "in": id, "text": w })),
                }
            }
        }
    }
    // explicit links
    for d in &ix.docs {
        if s(d, "type") != "link" {
            continue;
        }
        let (a, b) = (s(d, "source"), s(d, "target"));
        if !alive(a) || !alive(b) {
            continue;
        }
        edges.push(Edge {
            source: a.into(),
            target: b.into(),
            relation: s(d, "relation").into(),
            kind: "link",
            link_id: Some(s(d, "id").into()),
            status: Some(s(d, "status").into()),
            note: Some(s(d, "note").into()),
        });
    }
    if include_tags {
        for d in &ix.docs {
            if s(d, "type") != "idea" {
                continue;
            }
            for t in str_list(d.get("tags")) {
                edges.push(Edge { source: s(d, "id").into(), target: format!("tag:{}", t), relation: "tagged".into(), kind: "tag", link_id: None, status: None, note: None });
            }
        }
    }
    (edges, unresolved)
}

fn node_json(d: &Map<String, Value>) -> Value {
    let mut o = json!({ "id": d.get("id"), "type": d.get("type"), "label": model::title_of(d) });
    for k in ["status", "project_id", "area"] {
        if let Some(v) = d.get(k).filter(|v| !v.is_null() && v.as_str() != Some("")) {
            o[k] = v.clone();
        }
    }
    if let Some(t) = d.get("tags").filter(|t| t.as_array().map_or(false, |a| !a.is_empty())) {
        o["tags"] = t.clone();
    }
    o
}

pub fn graph(conn: &Connection, o: &Map<String, Value>) -> Result<Value> {
    let ix = load_index(conn)?;
    let include_tags = o.get("tags").map_or(true, |v| crate::util::truthy(Some(v)) || v.is_null());
    let include_suggested = o.get("suggested").map_or(true, |v| crate::util::truthy(Some(v)) || v.is_null());
    let types: Option<Vec<String>> = o.get("types").map(|v| match v {
        Value::String(x) => x.split(',').map(|t| model::type_def(t).map(|d| d.name.to_string()).unwrap_or_else(|| t.trim().to_string())).collect(),
        Value::Array(a) => a.iter().filter_map(Value::as_str).map(|t| model::type_def(t).map(|d| d.name.to_string()).unwrap_or_else(|| t.to_string())).collect(),
        _ => vec![],
    });
    let limit = crate::util::as_u64(o.get("limit")).unwrap_or(600) as usize;
    let (mut edges, unresolved) = derive(&ix, include_tags);
    if !include_suggested {
        edges.retain(|e| e.status.as_deref() != Some("suggested"));
    }
    let type_ok = |id: &str| -> bool {
        let t = if id.starts_with("tag:") { "tag" } else { ix.get(id).map(|d| s(d, "type")).unwrap_or("") };
        if t == "link" {
            return false;
        }
        types.as_ref().map_or(true, |ts| ts.is_empty() || ts.iter().any(|x| x == t))
    };
    edges.retain(|e| type_ok(&e.source) && type_ok(&e.target));
    let mut ids: Vec<String> = ix.docs.iter().map(|d| s(d, "id").to_string()).filter(|i| type_ok(i)).collect();
    if include_tags {
        let mut tags: Vec<String> = edges.iter().filter(|e| e.kind == "tag").map(|e| e.target.clone()).collect();
        tags.sort();
        tags.dedup();
        ids.extend(tags.into_iter().filter(|t| type_ok(t)));
    }
    if let Some(c) = o.get("center").and_then(Value::as_str).filter(|c| !c.is_empty()) {
        let center = if ix.get(c).is_some() || c.starts_with("tag:") {
            c.to_string()
        } else {
            ix.resolve_wikilink(c).ok_or_else(|| Error::new("not_found", format!("no entity {:?}", c)).hint("pass an id or an exact title"))?
        };
        let depth = crate::util::as_u64(o.get("depth")).unwrap_or(2).min(5) as usize;
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
        for e in &edges {
            adj.entry(e.source.as_str()).or_default().push(e.target.as_str());
            adj.entry(e.target.as_str()).or_default().push(e.source.as_str());
        }
        let mut seen: HashMap<String, usize> = HashMap::new();
        seen.insert(center.clone(), 0);
        let mut q = VecDeque::from([center.clone()]);
        while let Some(x) = q.pop_front() {
            let dx = seen[&x];
            if dx >= depth {
                continue;
            }
            // tags are hubs: pass through them only from the center
            if x.starts_with("tag:") && x != center {
                continue;
            }
            for y in adj.get(x.as_str()).cloned().unwrap_or_default() {
                if !seen.contains_key(y) {
                    seen.insert(y.to_string(), dx + 1);
                    q.push_back(y.to_string());
                }
            }
        }
        ids.retain(|i| seen.contains_key(i));
        edges.retain(|e| seen.contains_key(&e.source) && seen.contains_key(&e.target));
    }
    let truncated = ids.len() > limit;
    if truncated {
        let mut deg: HashMap<&str, usize> = HashMap::new();
        for e in &edges {
            *deg.entry(e.source.as_str()).or_default() += 1;
            *deg.entry(e.target.as_str()).or_default() += 1;
        }
        let mut ranked: Vec<(usize, String)> = ids.iter().map(|i| (deg.get(i.as_str()).copied().unwrap_or(0), i.clone())).collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0));
        let keep: HashSet<String> = ranked.into_iter().take(limit).map(|(_, i)| i).collect();
        ids.retain(|i| keep.contains(i));
        edges.retain(|e| keep.contains(&e.source) && keep.contains(&e.target));
    }
    let mut degree: HashMap<&str, usize> = HashMap::new();
    for e in &edges {
        *degree.entry(e.source.as_str()).or_default() += 1;
        *degree.entry(e.target.as_str()).or_default() += 1;
    }
    let nodes: Vec<Value> = ids
        .iter()
        .map(|i| {
            let mut n = match ix.get(i) {
                Some(d) => node_json(d),
                None => json!({ "id": i, "type": "tag", "label": format!("#{}", i.trim_start_matches("tag:")) }),
            };
            n["degree"] = json!(degree.get(i.as_str()).copied().unwrap_or(0));
            n
        })
        .collect();
    let mut by_kind: HashMap<&str, usize> = HashMap::new();
    for e in &edges {
        *by_kind.entry(e.kind).or_default() += 1;
    }
    let isolated = nodes.iter().filter(|n| n["degree"] == json!(0) && n["type"] != json!("tag")).count();
    Ok(json!({
        "nodes": nodes,
        "edges": edges.iter().map(Edge::to_json).collect::<Vec<_>>(),
        "unresolved_wikilinks": unresolved,
        "stats": { "nodes": ids.len(), "edges": edges.len(), "by_kind": by_kind, "isolated": isolated, "truncated": truncated }
    }))
}

#[cfg(test)]
mod tests {
    use super::wikilinks;
    #[test]
    fn parse_wikilinks() {
        assert_eq!(wikilinks("see [[A]] and [[B|bee]], [[A]] again, [[ broken"), vec!["A", "B"]);
        assert!(wikilinks("no links [x]").is_empty());
    }
}
