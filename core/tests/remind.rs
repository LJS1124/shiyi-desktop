use serde_json::{json, Value};
use shiyi_core::remind::{daily_message, is_due, minutes_of_day, normalize_hhmm, parse_hhmm};

// 2026-10-12 10:00 +08:00
const NOW: i64 = 1_791_770_400_000;

// ---------- parse_hhmm / normalize_hhmm ----------

#[test]
fn parse_hhmm_accepts_padded_and_unpadded_times() {
    assert_eq!(parse_hhmm("09:00"), Some(540));
    assert_eq!(parse_hhmm("9:00"), Some(540));
    assert_eq!(parse_hhmm("21:30"), Some(1290));
    assert_eq!(parse_hhmm(" 7:05 "), Some(425));
}

#[test]
fn parse_hhmm_rejects_non_times() {
    for s in ["24:00", "12:60", "abc", "", "9"] {
        assert_eq!(parse_hhmm(s), None, "input {s:?}");
    }
}

#[test]
fn normalize_hhmm_pads_and_falls_back_to_default() {
    assert_eq!(normalize_hhmm("9:5"), "09:05");
    assert_eq!(normalize_hhmm("bad"), "09:00");
}

// ---------- minutes_of_day ----------

#[test]
fn minutes_of_day_uses_the_utc_offset() {
    assert_eq!(minutes_of_day(NOW, 8 * 3600), 600);
    // 02:00 UTC
    assert_eq!(minutes_of_day(NOW, 0), 120);
    // a negative offset wraps back to the previous day: 02:00 UTC - 5h = 21:00
    assert_eq!(minutes_of_day(NOW, -5 * 3600) as i64, (2 * 60 - 5 * 60 + 1440) % 1440);
    assert_eq!(minutes_of_day(NOW, -5 * 3600), 1260);
}

// ---------- is_due ----------

#[test]
fn is_due_waits_for_the_chosen_time() {
    assert!(!is_due(539, "09:00", "2026-10-12", None));
    assert!(is_due(540, "09:00", "2026-10-12", None));
    // after the time, still due (opening the laptop late still shows it)
    assert!(is_due(600, "09:00", "2026-10-12", None));
}

#[test]
fn is_due_skips_a_missed_morning_late_at_night() {
    assert!(is_due(20 * 60 + 59, "09:00", "2026-10-12", None));
    assert!(!is_due(21 * 60, "09:00", "2026-10-12", None));
    assert!(!is_due(23 * 60 + 59, "09:00", "2026-10-12", None));
    // a summary chosen for the evening still comes
    assert!(is_due(22 * 60, "21:30", "2026-10-12", None));
    assert!(!is_due(21 * 60 + 29, "21:30", "2026-10-12", None));
}

#[test]
fn is_due_only_once_per_day() {
    assert!(!is_due(600, "09:00", "2026-10-12", Some("2026-10-12")));
    assert!(is_due(600, "09:00", "2026-10-12", Some("2026-10-11")));
}

#[test]
fn is_due_invalid_time_falls_back_to_nine() {
    assert!(!is_due(539, "x", "2026-10-12", None));
    assert!(is_due(540, "x", "2026-10-12", None));
}

// ---------- daily_message ----------

fn sample() -> Value {
    json!({
        "date": "2026-10-12",
        "weekday": "周一",
        "overdue": [{"id": "t1", "title": "交物业费"}],
        "today": [{"id": "t2", "title": "写打分脚本"}, {"id": "t1", "title": "交物业费"}]
    })
}

#[test]
fn daily_message_is_none_when_nothing_is_due() {
    let empty = json!({"date": "2026-10-12", "weekday": "周一", "overdue": [], "today": []});
    assert_eq!(daily_message(&empty), None);
    assert_eq!(daily_message(&json!({})), None);
}

#[test]
fn daily_message_title_has_month_day_and_weekday() {
    let (title, _) = daily_message(&sample()).unwrap();
    assert_eq!(title, "拾忆 · 10月12日 周一");
}

#[test]
fn daily_message_counts_a_task_in_both_lists_once() {
    let (_, body) = daily_message(&sample()).unwrap();
    assert!(body.starts_with("今天要做 1 件，逾期 1 件"), "{body}");
    let late = body.find("交物业费").expect("overdue title listed");
    let today = body.find("写打分脚本").expect("planned title listed");
    assert!(late < today, "{body}");
    assert_eq!(body.matches("交物业费").count(), 1, "{body}");
}

#[test]
fn daily_message_only_overdue() {
    let v = json!({
        "date": "2026-10-12",
        "weekday": "周一",
        "overdue": [{"id": "a", "title": "A"}, {"id": "b", "title": "B"}],
        "today": []
    });
    let (_, body) = daily_message(&v).unwrap();
    assert!(body.starts_with("有 2 件任务已经逾期"), "{body}");
}

#[test]
fn daily_message_only_today() {
    let v = json!({
        "date": "2026-10-12",
        "weekday": "周一",
        "overdue": [],
        "today": [{"id": "a", "title": "A"}]
    });
    let (_, body) = daily_message(&v).unwrap();
    assert!(body.starts_with("今天要做 1 件"), "{body}");
}

#[test]
fn daily_message_lists_three_titles_then_etc() {
    let v = json!({
        "date": "2026-10-12",
        "weekday": "周一",
        "overdue": [{"id": "a", "title": "甲"}],
        "today": [{"id": "b", "title": "乙"}, {"id": "c", "title": "丙"}, {"id": "d", "title": "丁"}]
    });
    let (_, body) = daily_message(&v).unwrap();
    assert!(body.contains("甲、乙、丙"), "{body}");
    assert!(!body.contains("丁"), "{body}");
    assert!(body.ends_with(" 等"), "{body}");
}

#[test]
fn daily_message_exactly_three_titles_has_no_etc() {
    let v = json!({
        "date": "2026-10-12",
        "weekday": "周一",
        "overdue": [],
        "today": [{"id": "a", "title": "甲"}, {"id": "b", "title": "乙"}, {"id": "c", "title": "丙"}]
    });
    let (_, body) = daily_message(&v).unwrap();
    assert!(body.ends_with("：甲、乙、丙"), "{body}");
}

#[test]
fn daily_message_clips_long_titles_at_18_chars() {
    let long = "甲".repeat(25);
    let v = json!({
        "date": "2026-10-12",
        "weekday": "周一",
        "overdue": [],
        "today": [{"id": "a", "title": long}]
    });
    let (_, body) = daily_message(&v).unwrap();
    assert!(body.ends_with(&format!("{}…", "甲".repeat(18))), "{body}");
    assert!(!body.contains(&"甲".repeat(19)), "{body}");
}

#[test]
fn daily_message_title_without_date_is_generic() {
    let v = json!({"overdue": [{"id": "a", "title": "A"}], "today": []});
    let (title, _) = daily_message(&v).unwrap();
    assert_eq!(title, "拾忆 · 今日安排");
}

#[test]
fn daily_message_blank_titles_do_not_add_etc() {
    let v = json!({
        "date": "2026-10-12",
        "weekday": "周一",
        "overdue": [{"id": "a", "title": "   "}],
        "today": []
    });
    let (_, body) = daily_message(&v).unwrap();
    assert_eq!(body, "有 1 件任务已经逾期", "{body}");
}

// ---------- carried over ----------

#[test]
fn daily_message_counts_tasks_left_from_an_earlier_plan() {
    let v = json!({
        "date": "2026-10-12", "weekday": "周一",
        "overdue": [], "today": [{"id": "t1", "title": "写周报"}],
        "carried_over": [{"id": "t2", "title": "交物业费"}, {"id": "t3", "title": "买墨盒"}]
    });
    let (_, body) = daily_message(&v).unwrap();
    assert!(body.starts_with("今天要做 1 件，之前没做完 2 件："), "{body}");
    assert!(body.contains("写周报") && body.contains("交物业费"), "{body}");
}

#[test]
fn daily_message_with_only_carried_over_tasks() {
    let v = json!({ "date": "2026-10-12", "weekday": "周一", "overdue": [], "today": [], "carried_over": [{"id": "t2", "title": "交物业费"}] });
    let (_, body) = daily_message(&v).unwrap();
    assert_eq!(body, "之前计划的 1 件还没做完：交物业费");
}
