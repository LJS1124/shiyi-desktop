use serde_json::{Map, Value};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn b36(mut n: u64, width: usize) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = vec![b'0'; width];
    for i in (0..width).rev() {
        out[i] = D[(n % 36) as usize];
        n /= 36;
    }
    String::from_utf8(out).unwrap()
}

pub fn rand_u64() -> u64 {
    let mut h = RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.write_i64(crate::time::now_ms());
    h.write_u32(std::process::id());
    h.finish()
}

/// `task_mgn3k2x9a7qp`: prefix, 8 base36 chars of time, 4 random.
pub fn new_id(prefix: &str) -> String {
    let ms = crate::time::now_ms().max(0) as u64;
    format!("{}_{}{}", prefix, b36(ms, 8), b36(rand_u64(), 4))
}

pub fn short_id() -> String {
    b36(rand_u64(), 7)
}

pub fn first_line(s: &str) -> &str {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("")
}

pub fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut o: String = s.chars().take(max.saturating_sub(1)).collect();
        o.push('…');
        o
    }
}

pub fn camel_to_snake(s: &str) -> String {
    let mut o = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                o.push('_');
            }
            o.push(c.to_ascii_lowercase());
        } else if c == '-' {
            o.push('_');
        } else {
            o.push(c);
        }
    }
    o
}

pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    prev[b.len()]
}

pub fn closest<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let w = word.to_lowercase();
    let mut best: Option<(usize, &str)> = None;
    for c in candidates {
        let d = if c.contains(w.as_str()) || w.contains(c) { 1 } else { edit_distance(&w, c) };
        if d <= 3 && best.map_or(true, |(bd, _)| d < bd) {
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}

pub fn s<'a>(o: &'a Map<String, Value>, k: &str) -> &'a str {
    o.get(k).and_then(Value::as_str).unwrap_or("")
}

pub fn str_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => vec![],
    }
}

pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map_or(false, |x| x != 0.0),
        Some(Value::String(s)) => matches!(s.to_lowercase().as_str(), "true" | "yes" | "1" | "y" | "on"),
        _ => false,
    }
}

/// Number parameter that may arrive as a JSON number or a string (CLI flags).
pub fn as_u64(v: Option<&Value>) -> Option<u64> {
    match v {
        Some(Value::Number(n)) => n.as_u64().or_else(|| n.as_f64().map(|x| x.max(0.0) as u64)),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

pub fn as_i64(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}
