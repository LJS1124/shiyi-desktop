//! "Possibly related" items without calling a model: TF-IDF cosine over
//! character bigrams (Chinese needs no word segmentation this way) and
//! words, plus shared tags and a shared project. The GUI runs the same
//! scoring in js/related.js.

use crate::graph::{self, Index};
use crate::model;
use crate::util::{s, str_list};
use crate::{Error, Result};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x3040..=0x30FF | 0xAC00..=0xD7AF)
}

/// Function-word bigrams that say nothing about the topic.
const STOP: &[&str] = &[
    "可以", "一个", "一下", "一些", "能不", "不能", "能否", "我们", "你们", "他们", "什么", "这个", "那个", "这些", "没有", "不是", "如果", "因为", "所以", "但是",
    "还是", "就是", "已经", "自己", "时候", "怎么", "为了", "或者", "然后", "现在", "这样", "那样", "需要", "考虑", "感觉", "觉得", "应该", "是不", "有没", "的时",
    "里的", "中的", "上的", "下的", "了一", "的一", "一次", "每次", "比较", "然后", "其实", "可能", "试试", "看看", "顺便",
    "the", "and", "for", "with", "this", "that", "from", "into", "are", "was", "can",
];

/// Features of a text: CJK character bigrams and latin/digit words (2+ chars).
pub fn features(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let lower = text.to_lowercase();
    let mut run: Vec<char> = vec![];
    let mut word = String::new();
    let flush_run = |run: &mut Vec<char>, out: &mut HashSet<String>| {
        for w in run.windows(2) {
            out.insert(w.iter().collect());
        }
        run.clear();
    };
    for c in lower.chars().chain(std::iter::once(' ')) {
        let c = if c.is_whitespace() { ' ' } else { c };
        if is_cjk(c) {
            if word.chars().count() >= 2 {
                out.insert(std::mem::take(&mut word));
            }
            word.clear();
            run.push(c);
        } else if c.is_alphanumeric() {
            flush_run(&mut run, &mut out);
            word.push(c);
        } else {
            flush_run(&mut run, &mut out);
            if word.chars().count() >= 2 {
                out.insert(std::mem::take(&mut word));
            }
            word.clear();
        }
    }
    out.retain(|w| !STOP.contains(&w.as_str()));
    out
}

/// Readable shared terms: adjacent shared bigrams of the query text merged
/// back into words ("路由" + "由器" → "路由器"), longest first.
pub fn shared_terms(query: &str, shared: &HashSet<String>) -> Vec<String> {
    let chars: Vec<char> = query.to_lowercase().chars().collect();
    let mut terms: Vec<String> = vec![];
    let mut i = 0;
    while i + 1 < chars.len() {
        let bg: String = chars[i..i + 2].iter().collect();
        if is_cjk(chars[i]) && is_cjk(chars[i + 1]) && shared.contains(&bg) {
            let mut j = i + 1;
            while j + 1 < chars.len() && is_cjk(chars[j + 1]) && shared.contains(&chars[j..j + 2].iter().collect::<String>()) {
                j += 1;
            }
            let t: String = chars[i..=j].iter().collect::<String>().trim_matches(|c: char| "的了和与在是把被就也都很".contains(c)).to_string();
            if t.chars().count() >= 2 && !terms.contains(&t) {
                terms.push(t);
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    for w in shared.iter().filter(|w| !w.chars().any(is_cjk)) {
        if !terms.contains(w) {
            terms.push(w.clone());
        }
    }
    terms.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()).then(a.cmp(b)));
    terms.truncate(4);
    terms
}

pub fn doc_text(d: &Map<String, Value>) -> String {
    let parts: Vec<&str> = match s(d, "type") {
        "idea" => vec![s(d, "text"), s(d, "note")],
        "project" => vec![s(d, "name"), s(d, "goal"), s(d, "note")],
        "task" => vec![s(d, "title"), s(d, "note")],
        "map" => vec![s(d, "title")],
        _ => vec![],
    };
    let mut t = parts.join("\n");
    if s(d, "type") == "map" {
        if let Some(nodes) = d.get("nodes").and_then(Value::as_object) {
            for n in nodes.values() {
                t.push('\n');
                t.push_str(n.get("text").and_then(Value::as_str).unwrap_or(""));
            }
        }
    }
    // [[wikilinks]] are explicit already; don't let their text count twice
    t.replace("[[", " ").replace("]]", " ")
}

fn candidate(d: &Map<String, Value>) -> bool {
    match s(d, "type") {
        "task" => s(d, "status") != "done",
        "idea" => s(d, "status") != "archived",
        "project" | "map" => true,
        _ => false,
    }
}

pub struct Hit {
    pub id: String,
    pub score: f64,
    pub shared: Vec<String>,
    pub tags: Vec<String>,
}

/// Ranks live items by similarity to `target` (an id) or to free text.
pub fn rank(ix: &Index, target_id: Option<&str>, text: Option<&str>, limit: usize, min_score: f64) -> Vec<Hit> {
    let docs: Vec<&Map<String, Value>> = ix.docs.iter().filter(|d| candidate(d) || target_id == Some(s(d, "id"))).collect();
    let feats: Vec<HashSet<String>> = docs.iter().map(|d| features(&doc_text(d))).collect();
    let mut df: HashMap<&str, usize> = HashMap::new();
    for f in &feats {
        for w in f {
            *df.entry(w.as_str()).or_default() += 1;
        }
    }
    let n = docs.len().max(1) as f64;
    let idf = |w: &str| ((n + 1.0) / (*df.get(w).unwrap_or(&0) as f64 + 1.0)).ln() + 1.0;
    let (q_text, q_feats, q_tags, q_project): (String, HashSet<String>, Vec<String>, String) = match target_id {
        Some(id) => match docs.iter().position(|d| s(d, "id") == id) {
            Some(i) => (doc_text(docs[i]), feats[i].clone(), str_list(docs[i].get("tags")), s(docs[i], "project_id").to_string()),
            None => return vec![],
        },
        None => (text.unwrap_or("").to_string(), features(text.unwrap_or("")), vec![], String::new()),
    };
    if q_feats.is_empty() && q_tags.is_empty() {
        return vec![];
    }
    // sum in sorted order so ties come out the same in every process
    let norm = |f: &HashSet<String>| {
        let mut v: Vec<&String> = f.iter().collect();
        v.sort();
        v.iter().map(|w| idf(w).powi(2)).sum::<f64>().sqrt()
    };
    let qn = norm(&q_feats);
    // already connected (any edge) items are not suggested again
    let mut connected: HashSet<String> = HashSet::new();
    if let Some(id) = target_id {
        let (edges, _) = graph::derive(ix, false);
        for e in edges {
            if e.source == id {
                connected.insert(e.target);
            } else if e.target == id {
                connected.insert(e.source);
            }
        }
        for l in ix.docs.iter().filter(|d| s(d, "type") == "link") {
            if s(l, "source") == id {
                connected.insert(s(l, "target").to_string());
            }
            if s(l, "target") == id {
                connected.insert(s(l, "source").to_string());
            }
        }
    }
    let mut hits = vec![];
    for (i, d) in docs.iter().enumerate() {
        let did = s(d, "id");
        if Some(did) == target_id || connected.contains(did) || !candidate(d) {
            continue;
        }
        let mut common: Vec<&String> = q_feats.iter().filter(|w| feats[i].contains(*w)).collect();
        common.sort();
        let dot: f64 = common.iter().map(|w| idf(w).powi(2)).sum();
        let dn = norm(&feats[i]);
        let mut score = if qn > 0.0 && dn > 0.0 { dot / (qn * dn) } else { 0.0 };
        let tags: Vec<String> = str_list(d.get("tags")).into_iter().filter(|t| q_tags.contains(t)).collect();
        score += (tags.len() as f64 * 0.12).min(0.24);
        if !q_project.is_empty() && s(d, "project_id") == q_project {
            score += 0.05;
        }
        if score < min_score {
            continue;
        }
        let set: HashSet<String> = common.into_iter().cloned().collect();
        hits.push(Hit { id: did.to_string(), score, shared: shared_terms(&q_text, &set), tags });
    }
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal).then(a.id.cmp(&b.id)));
    hits.truncate(limit);
    hits
}

pub fn query(conn: &Connection, o: &Map<String, Value>) -> Result<Value> {
    let ix = graph::load_index(conn)?;
    let id = o.get("id").and_then(Value::as_str).map(str::trim).filter(|x| !x.is_empty());
    let text = o.get("text").and_then(Value::as_str).map(str::trim).filter(|x| !x.is_empty());
    let id = match id {
        Some(i) if ix.get(i).is_some() => Some(i.to_string()),
        Some(i) => Some(ix.resolve_wikilink(i).ok_or_else(|| Error::new("not_found", format!("no entity {:?}", i)).hint("pass an id or an exact title"))?),
        None => None,
    };
    if id.is_none() && text.is_none() {
        return Err(Error::new("missing_field", "related needs an id or a text").hint("{\"q\":\"related\",\"id\":\"idea_…\"} or {\"q\":\"related\",\"text\":\"…\"}"));
    }
    let limit = crate::util::as_u64(o.get("limit")).unwrap_or(5).min(50) as usize;
    let min = o.get("min_score").and_then(Value::as_f64).unwrap_or(if id.is_some() { 0.12 } else { 0.08 });
    let hits = rank(&ix, id.as_deref(), text, limit, min);
    let items: Vec<Value> = hits
        .iter()
        .map(|h| {
            let d = ix.get(&h.id).unwrap();
            let mut v = json!({ "id": h.id, "type": d.get("type"), "title": model::title_of(d), "score": (h.score * 100.0).round() / 100.0, "shared": h.shared });
            if !h.tags.is_empty() {
                v["shared_tags"] = json!(h.tags);
            }
            v
        })
        .collect();
    Ok(json!({ "items": items, "method": "local text similarity (no AI); already connected items are left out" }))
}

#[cfg(test)]
mod tests {
    use super::features;
    #[test]
    fn bigrams_and_words() {
        let f = features("给 Agent 加回放面板, v2");
        assert!(f.contains("agent") && f.contains("回放") && f.contains("放面") && f.contains("v2"));
        assert!(!f.contains("给"));
        let shared: std::collections::HashSet<String> = ["路由", "由器", "信号"].iter().map(|x| x.to_string()).collect();
        assert_eq!(super::shared_terms("书房的路由器信号差", &shared), vec!["路由器", "信号"]);
    }
}
