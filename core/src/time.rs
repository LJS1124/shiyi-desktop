//! Calendar dates (`YYYY-MM-DD`) and timestamps (RFC 3339 with the local
//! offset, e.g. `2026-10-12T16:30:05+08:00`) without external crates.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let d = d as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

fn valid_ymd(y: i64, m: u32, d: u32) -> bool {
    (1900..=9999).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m)
}

/// Strict `YYYY-MM-DD` to day number.
pub fn parse_date(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i64 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    if !valid_ymd(y, m, d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

pub fn is_date(s: &str) -> bool {
    parse_date(s).is_some()
}

pub fn fmt_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

pub fn add_days(s: &str, n: i64) -> Option<String> {
    parse_date(s).map(|d| fmt_date(d + n))
}

/// b - a in days.
pub fn day_diff(a: &str, b: &str) -> Option<i64> {
    Some(parse_date(b)? - parse_date(a)?)
}

/// 0 = Monday … 6 = Sunday.
pub fn weekday(days: i64) -> i64 {
    (days + 3).rem_euclid(7)
}

pub const WEEKDAY_CN: [&str; 7] = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];

pub fn iso_from_ms(ms: i64, offset_s: i64) -> String {
    let local = ms.div_euclid(1000) + offset_s;
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let sign = if offset_s < 0 { '-' } else { '+' };
    let off = offset_s.abs();
    if offset_s == 0 {
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, secs / 3600, secs % 3600 / 60, secs % 60)
    } else {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{:02}:{:02}",
            y,
            m,
            d,
            secs / 3600,
            secs % 3600 / 60,
            secs % 60,
            sign,
            off / 3600,
            off % 3600 / 60
        )
    }
}

/// Parses RFC 3339-ish timestamps: `YYYY-MM-DDTHH:MM[:SS[.fff]][Z|±HH:MM|±HHMM]`
/// (a space may replace `T`). Without an offset, `default_offset_s` applies.
/// A bare date means local midnight.
pub fn ms_from_iso(s: &str, default_offset_s: i64) -> Option<i64> {
    let s = s.trim();
    if s.len() < 10 {
        return None;
    }
    let days = parse_date(&s[..10])?;
    let rest = &s[10..];
    if rest.is_empty() {
        return Some((days * 86_400 - default_offset_s) * 1000);
    }
    let rest = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '))?;
    let b = rest.as_bytes();
    if b.len() < 5 || b[2] != b':' {
        return None;
    }
    let hh: i64 = rest[0..2].parse().ok()?;
    let mi: i64 = rest[3..5].parse().ok()?;
    let mut i = 5;
    let mut ss: i64 = 0;
    let mut frac_ms: i64 = 0;
    if b.len() >= 8 && b[5] == b':' {
        ss = rest[6..8].parse().ok()?;
        i = 8;
        if i < b.len() && b[i] == b'.' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let digits = &rest[start..j];
            if !digits.is_empty() {
                let mut d3 = digits.chars().take(3).collect::<String>();
                while d3.len() < 3 {
                    d3.push('0');
                }
                frac_ms = d3.parse().ok()?;
            }
            i = j;
        }
    }
    if hh > 23 || mi > 59 || ss > 60 {
        return None;
    }
    let tz = &rest[i..];
    let offset = if tz.is_empty() {
        default_offset_s
    } else if tz == "Z" || tz == "z" {
        0
    } else {
        let sign = match tz.as_bytes()[0] {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let t = tz[1..].replace(':', "");
        if t.len() != 4 {
            return None;
        }
        let oh: i64 = t[0..2].parse().ok()?;
        let om: i64 = t[2..4].parse().ok()?;
        sign * (oh * 3600 + om * 60)
    };
    let local = days * 86_400 + hh * 3600 + mi * 60 + ss;
    Some((local - offset) * 1000 + frac_ms)
}

/// Current time plus the local UTC offset (taken from SQLite, which asks the OS).
#[derive(Clone, Debug)]
pub struct Clock {
    pub now_ms: i64,
    pub offset_s: i64,
}

impl Clock {
    pub fn new(offset_s: i64) -> Self {
        Clock { now_ms: now_ms(), offset_s }
    }
    pub fn today_days(&self) -> i64 {
        (self.now_ms.div_euclid(1000) + self.offset_s).div_euclid(86_400)
    }
    pub fn today(&self) -> String {
        fmt_date(self.today_days())
    }
    pub fn now_iso(&self) -> String {
        iso_from_ms(self.now_ms, self.offset_s)
    }
    pub fn iso(&self, ms: i64) -> String {
        iso_from_ms(ms, self.offset_s)
    }
    /// Local calendar date of a stored timestamp.
    pub fn date_of(&self, iso: &str) -> Option<String> {
        let ms = ms_from_iso(iso, self.offset_s)?;
        Some(fmt_date((ms.div_euclid(1000) + self.offset_s).div_euclid(86_400)))
    }
}

fn weekday_index(w: &str) -> Option<i64> {
    let w = w.trim();
    let en = [
        ["mon", "monday"],
        ["tue", "tuesday"],
        ["wed", "wednesday"],
        ["thu", "thursday"],
        ["fri", "friday"],
        ["sat", "saturday"],
        ["sun", "sunday"],
    ];
    for (i, names) in en.iter().enumerate() {
        if names.contains(&w) {
            return Some(i as i64);
        }
    }
    if w == "tues" || w == "thur" || w == "thurs" {
        return Some(if w == "tues" { 1 } else { 3 });
    }
    let cn = ["一", "二", "三", "四", "五", "六", "日"];
    for prefix in ["周", "星期", "礼拜"] {
        if let Some(r) = w.strip_prefix(prefix) {
            if r == "天" {
                return Some(6);
            }
            if let Some(i) = cn.iter().position(|c| *c == r) {
                return Some(i as i64);
            }
        }
    }
    None
}

/// Lenient date input for humans and agents. Accepts `YYYY-MM-DD`,
/// `YYYY/M/D`, `M-D`, `M/D`, `today`, `tomorrow`, `yesterday`, `+3d`, `-2d`,
/// `+1w`, `3d`, weekday names (`fri`, `next mon`, `周五`, `下周一`) and the
/// Chinese words 今天/明天/后天/大后天/昨天. Datetimes keep their date part.
pub fn parse_date_lenient(input: &str, today: &str) -> Option<String> {
    let t = input.trim().to_lowercase();
    let today_d = parse_date(today)?;
    if t.is_empty() {
        return None;
    }
    let rel = |n: i64| Some(fmt_date(today_d + n));
    match t.as_str() {
        "today" | "now" | "今天" | "今日" => return rel(0),
        "tomorrow" | "tmr" | "明天" | "明日" => return rel(1),
        "yesterday" | "昨天" | "昨日" => return rel(-1),
        "后天" => return rel(2),
        "大后天" => return rel(3),
        "前天" => return rel(-2),
        _ => {}
    }
    // relative offsets: +3d, -2d, +1w, 3d, +3, 3天, 2周
    {
        let (sign, body) = if let Some(b) = t.strip_prefix('+') {
            (1, b)
        } else if let Some(b) = t.strip_prefix('-') {
            (-1, b)
        } else {
            (0, t.as_str())
        };
        let digits: String = body.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            let unit = body[digits.len()..].trim();
            let n: i64 = digits.parse().ok()?;
            let mult = match unit {
                "" if sign != 0 => Some(1),
                "d" | "day" | "days" | "天" | "日" => Some(1),
                "w" | "wk" | "week" | "weeks" | "周" | "星期" => Some(7),
                _ => None,
            };
            if let Some(mult) = mult {
                let s = if sign == 0 { 1 } else { sign };
                return rel(s * n * mult);
            }
        }
    }
    // weekdays
    let wd_today = weekday(today_d);
    let next_week = |w: i64| fmt_date(today_d - wd_today + 7 + w);
    for p in ["next ", "下周", "下星期", "下个星期", "下礼拜"] {
        if let Some(r) = t.strip_prefix(p) {
            let r = r.trim();
            let w = weekday_index(r).or_else(|| weekday_index(&format!("周{}", r)));
            if let Some(w) = w {
                return Some(next_week(w));
            }
        }
    }
    for p in ["this ", "本周", "这周", "这个星期"] {
        if let Some(r) = t.strip_prefix(p) {
            let r = r.trim();
            let w = weekday_index(r).or_else(|| weekday_index(&format!("周{}", r)));
            if let Some(w) = w {
                return Some(fmt_date(today_d - wd_today + w));
            }
        }
    }
    if let Some(w) = weekday_index(&t) {
        let delta = (w - wd_today).rem_euclid(7);
        return rel(delta);
    }
    // explicit dates
    let norm: String = t
        .chars()
        .map(|c| match c {
            '/' | '.' | '年' | '月' => '-',
            '日' | '号' => ' ',
            _ => c,
        })
        .collect();
    let norm = norm.trim();
    let datepart = norm.split(|c| c == 't' || c == ' ').next().unwrap_or("");
    let parts: Vec<&str> = datepart.split('-').filter(|p| !p.is_empty()).collect();
    let nums: Option<Vec<i64>> = parts.iter().map(|p| p.parse::<i64>().ok()).collect();
    let nums = nums?;
    let (y, m, d) = match nums.len() {
        3 => (nums[0], nums[1], nums[2]),
        2 => {
            let (cy, _, _) = civil_from_days(today_d);
            (cy, nums[0], nums[1])
        }
        _ => return None,
    };
    if m < 1 || d < 1 || m > 12 || d > 31 {
        return None;
    }
    if !valid_ymd(y, m as u32, d as u32) {
        return None;
    }
    Some(fmt_date(days_from_civil(y, m as u32, d as u32)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_roundtrip() {
        for d in [-1000, 0, 1, 365, 10_000, 20_738, 60_000] {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
        assert_eq!(fmt_date(0), "1970-01-01");
        assert_eq!(parse_date("2024-02-29").map(fmt_date).as_deref(), Some("2024-02-29"));
        assert!(parse_date("2023-02-29").is_none());
        assert_eq!(weekday(parse_date("2026-10-12").unwrap()), 0); // Monday
    }

    #[test]
    fn iso() {
        let ms = ms_from_iso("2026-10-12T16:30:05+08:00", 0).unwrap();
        assert_eq!(iso_from_ms(ms, 8 * 3600), "2026-10-12T16:30:05+08:00");
        assert_eq!(iso_from_ms(ms, 0), "2026-10-12T08:30:05Z");
        assert_eq!(ms_from_iso("2026-10-12T08:30:05.250Z", 0).unwrap(), ms + 250);
        assert_eq!(ms_from_iso("2026-10-12 16:30", 8 * 3600).unwrap(), ms - 5000);
        let c = Clock { now_ms: ms, offset_s: 8 * 3600 };
        assert_eq!(c.today(), "2026-10-12");
        assert_eq!(c.date_of("2026-10-11T20:00:00Z").as_deref(), Some("2026-10-12"));
    }

    #[test]
    fn lenient() {
        let t = "2026-10-12"; // Monday
        let p = |s: &str| parse_date_lenient(s, t);
        assert_eq!(p("today").as_deref(), Some("2026-10-12"));
        assert_eq!(p("明天").as_deref(), Some("2026-10-13"));
        assert_eq!(p("+3d").as_deref(), Some("2026-10-15"));
        assert_eq!(p("-2d").as_deref(), Some("2026-10-10"));
        assert_eq!(p("+1w").as_deref(), Some("2026-10-19"));
        assert_eq!(p("3天").as_deref(), Some("2026-10-15"));
        assert_eq!(p("fri").as_deref(), Some("2026-10-16"));
        assert_eq!(p("mon").as_deref(), Some("2026-10-12"));
        assert_eq!(p("周五").as_deref(), Some("2026-10-16"));
        assert_eq!(p("下周一").as_deref(), Some("2026-10-19"));
        assert_eq!(p("next fri").as_deref(), Some("2026-10-23"));
        assert_eq!(p("10/20").as_deref(), Some("2026-10-20"));
        assert_eq!(p("2026/1/5").as_deref(), Some("2026-01-05"));
        assert_eq!(p("2026年1月5日").as_deref(), Some("2026-01-05"));
        assert_eq!(p("2026-11-02T10:00:00Z").as_deref(), Some("2026-11-02"));
        assert_eq!(p("2026-02-30"), None);
        assert_eq!(p("someday"), None);
    }
}
