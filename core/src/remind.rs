//! The daily summary notification: when it is due and what it says. The
//! desktop shell runs the timer and shows the notification; the decisions live
//! here so they can be tested.

use serde_json::Value;
use std::collections::HashSet;

pub const DEFAULT_TIME: &str = "09:00";
/// meta key holding the date the summary was last shown
pub const META_LAST: &str = "notify_daily_last";

/// `9:00` / `09:00` / `21:30` → minutes after midnight.
pub fn parse_hhmm(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Normalised `HH:MM`, or the default when the input is not a time.
pub fn normalize_hhmm(s: &str) -> String {
    let m = parse_hhmm(s).or_else(|| parse_hhmm(DEFAULT_TIME)).unwrap_or(540);
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// Local minutes after midnight for a timestamp and UTC offset.
pub fn minutes_of_day(now_ms: i64, offset_s: i64) -> u32 {
    ((now_ms.div_euclid(1000) + offset_s).rem_euclid(86_400) / 60) as u32
}

/// After this, a morning summary that was missed is skipped for the day.
pub const LATE: u32 = 21 * 60;

/// Show today's summary now? Once a day, at or after the chosen time
/// (opening the laptop at 10:30 with a 09:00 summary still shows it), but
/// not late at night unless that is the chosen time.
pub fn is_due(now_minutes: u32, at: &str, today: &str, last_sent: Option<&str>) -> bool {
    let at = parse_hhmm(at).or_else(|| parse_hhmm(DEFAULT_TIME)).unwrap_or(540);
    last_sent != Some(today) && now_minutes >= at && (now_minutes < LATE || at >= LATE)
}

fn clip(s: &str, n: usize) -> String {
    let c: Vec<char> = s.trim().chars().collect();
    if c.len() <= n {
        c.into_iter().collect()
    } else {
        c[..n].iter().collect::<String>() + "…"
    }
}

/// Title and body from a `today` query result; `None` when nothing is due
/// today or overdue (no reason to interrupt).
pub fn daily_message(today: &Value) -> Option<(String, String)> {
    let items = |k: &str| today[k].as_array().cloned().unwrap_or_default();
    let overdue = items("overdue");
    let late: HashSet<String> = overdue.iter().filter_map(|t| t["id"].as_str().map(str::to_string)).collect();
    // a task planned for today that is also overdue counts once, as overdue
    let planned: Vec<Value> = items("today").into_iter().filter(|t| t["id"].as_str().map_or(true, |id| !late.contains(id))).collect();
    if overdue.is_empty() && planned.is_empty() {
        return None;
    }
    let head = match (planned.len(), overdue.len()) {
        (p, 0) => format!("今天要做 {p} 件"),
        (0, o) => format!("有 {o} 件任务已经逾期"),
        (p, o) => format!("今天要做 {p} 件，逾期 {o} 件"),
    };
    let titles: Vec<String> = overdue.iter().chain(planned.iter()).filter_map(|t| t["title"].as_str()).filter(|t| !t.trim().is_empty()).take(3).map(|t| clip(t, 18)).collect();
    let total = overdue.len() + planned.len();
    let mut list = titles.join("、");
    if total > titles.len() && !titles.is_empty() {
        list.push_str(" 等");
    }
    let date = today["date"].as_str().unwrap_or("");
    let title = match (date.get(5..7).and_then(|m| m.parse::<u32>().ok()), date.get(8..10).and_then(|d| d.parse::<u32>().ok())) {
        (Some(m), Some(d)) => format!("拾忆 · {m}月{d}日 {}", today["weekday"].as_str().unwrap_or("")).trim_end().to_string(),
        _ => "拾忆 · 今日安排".to_string(),
    };
    let body = if list.is_empty() { head } else { format!("{head}：{list}") };
    Some((title, body))
}
