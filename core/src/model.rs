//! Entity types, their fields, and the rules that turn loose input
//! (from the GUI, the CLI or an agent) into clean documents.

use crate::time::{parse_date_lenient, Clock};
use crate::util::{camel_to_snake, closest};
use crate::{Error, Result};
use serde_json::{json, Map, Number, Value};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// One-line text.
    Text,
    /// Multi-line text (may contain [[wikilinks]]).
    LongText,
    /// `YYYY-MM-DD` or null.
    Date,
    /// RFC 3339 timestamp or null.
    Ts,
    Bool,
    Int(i64, i64),
    Num,
    /// One of the values; the first is the default.
    Enum(&'static [&'static str]),
    /// One of the values or null.
    OptEnum(&'static [&'static str]),
    Tags,
    /// Id of an entity of the given type, or null.
    Ref(&'static str),
    /// List of ids of the given type.
    Refs(&'static str),
    /// Id of an idea, project, task or map.
    AnyRef,
    /// `{optimistic, likely, pessimistic}` in days, or null.
    Estimate,
    Json,
}

pub struct Field {
    pub name: &'static str,
    pub kind: Kind,
    pub aliases: &'static [&'static str],
    pub cn: &'static str,
    pub desc: &'static str,
}

pub struct TypeDef {
    pub name: &'static str,
    pub plural: &'static str,
    pub prefix: &'static str,
    pub cn: &'static str,
    pub title_field: &'static str,
    pub required: &'static [&'static str],
    pub fields: &'static [Field],
    pub desc: &'static str,
}

const fn f(name: &'static str, kind: Kind, aliases: &'static [&'static str], cn: &'static str, desc: &'static str) -> Field {
    Field { name, kind, aliases, cn, desc }
}

pub const IDEA_STATUS: &[&str] = &["inbox", "incubating", "converted", "archived"];
pub const PROJECT_STATUS: &[&str] = &["active", "paused", "done"];
pub const TASK_STATUS: &[&str] = &["todo", "doing", "done"];
pub const AREAS: &[&str] = &["work", "life"];
pub const COLORS: &[&str] = &["lav", "peach", "mint", "butter", "sky", "rose"];
pub const RELATIONS: &[&str] = &[
    "relates_to",
    "inspired_by",
    "derived_from",
    "part_of",
    "supports",
    "contradicts",
    "depends_on",
    "similar_to",
];
/// Relations that read the same in both directions.
pub const SYMMETRIC: &[&str] = &["relates_to", "similar_to", "contradicts"];
pub const LINK_STATUS: &[&str] = &["confirmed", "suggested"];

pub static IDEA: TypeDef = TypeDef {
    name: "idea",
    plural: "ideas",
    prefix: "idea",
    cn: "灵感",
    title_field: "text",
    required: &["text"],
    desc: "A captured thought. Ideas live in the inbox until they are incubated, converted into a task/project, or archived.",
    fields: &[
        f("text", Kind::LongText, &["title", "content", "body"], "内容", "the idea itself; first line is its title; may contain [[wikilinks]] and #tags"),
        f("note", Kind::LongText, &["notes", "description"], "备注", "longer notes; may contain [[wikilinks]]"),
        f("tags", Kind::Tags, &["tag", "labels"], "标签", "list of tags without '#'; a comma separated string also works"),
        f("status", Kind::Enum(IDEA_STATUS), &["state"], "状态", "inbox | incubating | converted | archived"),
        f("starred", Kind::Bool, &["star", "starred", "pinned"], "星标", "starred ideas are shown first"),
        f("project_id", Kind::Ref("project"), &["project"], "项目", "project this idea belongs to or became"),
        f("task_id", Kind::Ref("task"), &["task"], "任务", "task this idea became"),
        f("sample", Kind::Bool, &[], "示例", "true for example data created on first run"),
    ],
};

pub static PROJECT: TypeDef = TypeDef {
    name: "project",
    plural: "projects",
    prefix: "proj",
    cn: "项目",
    title_field: "name",
    required: &["name"],
    desc: "A goal with tasks. Work and life projects are kept apart by `area`.",
    fields: &[
        f("name", Kind::Text, &["title"], "名称", "project name"),
        f("area", Kind::Enum(AREAS), &[], "领域", "work | life"),
        f("color", Kind::OptEnum(COLORS), &["colour"], "颜色", "lav | peach | mint | butter | sky | rose, or null for the area default"),
        f("status", Kind::Enum(PROJECT_STATUS), &["state"], "状态", "active | paused | done"),
        f("goal", Kind::LongText, &["objective"], "目标", "what done looks like"),
        f("note", Kind::LongText, &["notes", "description"], "备注", "notes; may contain [[wikilinks]]"),
        f("start_date", Kind::Date, &["start", "starts"], "开始", "YYYY-MM-DD"),
        f("due_date", Kind::Date, &["due", "deadline", "end", "end_date"], "截止", "YYYY-MM-DD"),
        f("from_idea", Kind::Ref("idea"), &["idea", "idea_id"], "来源灵感", "idea this project came from"),
        f("from_map", Kind::Ref("map"), &["map", "map_id"], "来源导图", "mind map this project came from"),
        f("sample", Kind::Bool, &[], "示例", "true for example data created on first run"),
    ],
};

pub static TASK: TypeDef = TypeDef {
    name: "task",
    plural: "tasks",
    prefix: "task",
    cn: "任务",
    title_field: "title",
    required: &["title"],
    desc: "A unit of work. Tasks without project_id are loose tasks. Dependencies and three-point estimates drive the Gantt, PERT and burndown views.",
    fields: &[
        f("title", Kind::Text, &["name", "text"], "标题", "what to do"),
        f("project_id", Kind::Ref("project"), &["project"], "项目", "owning project, or null for a loose task"),
        f("status", Kind::Enum(TASK_STATUS), &["state"], "状态", "todo | doing | done; done_at is set automatically"),
        f("priority", Kind::Int(0, 3), &["prio", "p"], "优先级", "0 none, 1 high, 2 medium, 3 low"),
        f("start_date", Kind::Date, &["start", "starts"], "开始", "YYYY-MM-DD"),
        f("due_date", Kind::Date, &["due", "deadline", "end", "end_date"], "截止", "YYYY-MM-DD"),
        f("today_date", Kind::Date, &["today", "plan_date", "planned_for", "do_date"], "今日计划", "the day the user plans to do it (shows in Today); null to unplan"),
        f("note", Kind::LongText, &["notes", "description", "desc"], "备注", "notes; may contain [[wikilinks]]"),
        f("estimate", Kind::Estimate, &["est", "estimates"], "估时", "days: {optimistic, likely, pessimistic}; \"2,3,5\" or a single number also work"),
        f("depends_on", Kind::Refs("task"), &["deps", "after", "blocked_by", "dependencies"], "前置任务", "ids of tasks that must finish first; cycles are rejected"),
        f("rank", Kind::Num, &["order", "sort"], "排序", "manual order inside a board column"),
        f("done_at", Kind::Ts, &["completed_at", "finished_at"], "完成时间", "set automatically when status becomes done"),
        f("from_idea", Kind::Ref("idea"), &["idea", "idea_id"], "来源灵感", "idea this task came from"),
        f("sample", Kind::Bool, &[], "示例", "true for example data created on first run"),
    ],
};

pub static MAP: TypeDef = TypeDef {
    name: "map",
    plural: "maps",
    prefix: "map",
    cn: "导图",
    title_field: "title",
    required: &["title"],
    desc: "A mind map. Create one from an indented outline with `outline`; nodes can link to tasks, ideas or projects.",
    fields: &[
        f("title", Kind::Text, &["name"], "标题", "map title"),
        f("root_id", Kind::Text, &["root"], "根节点", "id of the root node (managed)"),
        f("nodes", Kind::Json, &[], "节点", "{node_id: {id, text, parent, children[], collapsed, color, link:{type,id}|null}} (prefer `outline`)"),
        f("idea_id", Kind::Ref("idea"), &["idea"], "灵感", "idea this map expands"),
        f("project_id", Kind::Ref("project"), &["project"], "项目", "project this map belongs to"),
        f("sample", Kind::Bool, &[], "示例", "true for example data created on first run"),
    ],
};

pub static LINK: TypeDef = TypeDef {
    name: "link",
    plural: "links",
    prefix: "link",
    cn: "关联",
    title_field: "relation",
    required: &["source", "target"],
    desc: "An explicit edge of the knowledge graph between two ideas, projects, tasks or maps. Agents should create links with status \"suggested\" unless the user asked for them.",
    fields: &[
        f("source", Kind::AnyRef, &["from", "src", "source_id"], "起点", "id (or exact title) of the source"),
        f("target", Kind::AnyRef, &["to", "dst", "target_id"], "终点", "id (or exact title) of the target"),
        f("relation", Kind::Enum(RELATIONS), &["rel", "kind", "relation_type"], "关系", "relates_to | inspired_by | derived_from | part_of | supports | contradicts | depends_on | similar_to"),
        f("note", Kind::LongText, &["reason", "why", "description"], "说明", "why these are related"),
        f("status", Kind::Enum(LINK_STATUS), &["state"], "状态", "confirmed | suggested (suggestions wait for the user to accept)"),
        f("confidence", Kind::Num, &["score"], "置信度", "0..1, for suggestions"),
        f("source_type", Kind::Text, &[], "起点类型", "managed"),
        f("target_type", Kind::Text, &[], "终点类型", "managed"),
    ],
};

pub static TYPES: [&TypeDef; 5] = [&IDEA, &PROJECT, &TASK, &MAP, &LINK];

pub const SYSTEM_FIELDS: &[&str] = &["id", "type", "created_at", "updated_at", "version", "created_by", "updated_by", "deleted_at"];

/// Accepts `task`, `tasks`, `Task`, the id prefix (`proj`) and the Chinese label.
pub fn type_def(name: &str) -> Option<&'static TypeDef> {
    let n = name.trim().to_lowercase();
    TYPES.iter().copied().find(|t| {
        t.name == n
            || t.plural == n
            || t.prefix == n
            || t.cn == n
            || (t.name == "idea" && (n == "想法" || n == "灵感" || n == "note"))
            || (t.name == "map" && (n == "mindmap" || n == "mind_map" || n == "mindmaps" || n == "思维导图"))
            || (t.name == "link" && (n == "edge" || n == "edges" || n == "relation" || n == "relations"))
    })
}

pub fn type_of_id(id: &str) -> Option<&'static TypeDef> {
    let p = id.split('_').next()?;
    if p.len() == id.len() {
        return None;
    }
    TYPES.iter().copied().find(|t| t.prefix == p)
}

pub fn field<'a>(td: &'a TypeDef, name: &str) -> Option<&'a Field> {
    td.fields.iter().find(|f| f.name == name)
}

/// Finds the field for an input key: exact name, alias, camelCase or kebab-case.
pub fn lookup_field<'a>(td: &'a TypeDef, key: &str) -> Option<&'a Field> {
    let k = key.trim();
    let snake = camel_to_snake(k);
    for cand in [k, snake.as_str()] {
        if let Some(f) = td.fields.iter().find(|f| f.name == cand) {
            return Some(f);
        }
        if let Some(f) = td.fields.iter().find(|f| f.aliases.contains(&cand)) {
            return Some(f);
        }
    }
    // the GUI's old names
    let legacy = match (td.name, k) {
        (_, "projectId") => "project_id",
        ("idea", "taskId") => "task_id",
        ("map", "ideaId") => "idea_id",
        ("map", "rootId") => "root_id",
        (_, "doneAt") => "done_at",
        (_, "todayDate") => "today_date",
        _ => "",
    };
    td.fields.iter().find(|f| f.name == legacy)
}

pub fn default_for(f: &Field) -> Value {
    match f.kind {
        Kind::Text | Kind::LongText => json!(""),
        Kind::Bool => json!(false),
        Kind::Int(min, _) => json!(min.max(0)),
        Kind::Enum(vals) => json!(vals[0]),
        Kind::Tags | Kind::Refs(_) => json!([]),
        _ => Value::Null,
    }
}

/// Words people (and models) use for statuses, areas and relations.
fn enum_alias(field: &str, v: &str) -> Option<&'static str> {
    let v = v.trim().to_lowercase().replace(['-', ' '], "_");
    let r = match (field, v.as_str()) {
        ("status", "待办" | "未开始" | "open" | "new" | "pending" | "to_do" | "backlog") => "todo",
        ("status", "进行中" | "进行" | "doing" | "in_progress" | "inprogress" | "wip" | "started" | "active_task") => "doing",
        ("status", "已完成" | "完成" | "done" | "completed" | "complete" | "finished" | "closed") => "done",
        ("status", "收件箱" | "inbox" | "new_idea") => "inbox",
        ("status", "孵化中" | "孵化" | "incubate" | "incubating" | "developing") => "incubating",
        ("status", "已转化" | "转化" | "converted") => "converted",
        ("status", "归档" | "已归档" | "archive" | "archived") => "archived",
        ("status", "暂停" | "paused" | "on_hold" | "hold") => "paused",
        ("status", "进行中项目" | "active" | "ongoing") => "active",
        ("status", "建议" | "suggest" | "suggested" | "pending_review") => "suggested",
        ("status", "确认" | "已确认" | "confirm" | "confirmed" | "accepted") => "confirmed",
        ("area", "工作" | "work" | "job" | "office") => "work",
        ("area", "生活" | "life" | "personal" | "home" | "private") => "life",
        ("relation", "related" | "related_to" | "relates" | "相关" | "关联") => "relates_to",
        ("relation", "inspired" | "启发" | "受启发") => "inspired_by",
        ("relation", "derived" | "来自" | "源自" | "衍生") => "derived_from",
        ("relation", "part" | "belongs_to" | "属于" | "部分") => "part_of",
        ("relation", "support" | "supports" | "支持" | "佐证") => "supports",
        ("relation", "contradict" | "conflicts_with" | "矛盾" | "冲突") => "contradicts",
        ("relation", "depends" | "requires" | "依赖" | "blocked_by") => "depends_on",
        ("relation", "similar" | "duplicate" | "相似" | "重复") => "similar_to",
        _ => return None,
    };
    Some(r)
}

fn bad(field: &str, msg: impl Into<String>, hint: impl Into<String>) -> Error {
    Error::new("invalid_value", format!("{}: {}", field, msg.into())).hint(hint).data(json!({ "field": field }))
}

fn num_of(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().trim_end_matches(['d', '天']).trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn to_num(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        json!(x as i64)
    } else {
        Number::from_f64(x).map(Value::Number).unwrap_or(Value::Null)
    }
}

pub fn parse_tags(v: &Value) -> Vec<String> {
    let raw: Vec<String> = match v {
        Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(String::from).or_else(|| x.as_i64().map(|n| n.to_string()))).collect(),
        Value::String(s) => s.split([',', '，', '、', ';', ' ', '\n']).map(String::from).collect(),
        _ => vec![],
    };
    let mut out: Vec<String> = vec![];
    for t in raw {
        let t = t.trim().trim_start_matches(['#', '＃']).trim().to_string();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t.chars().take(40).collect());
        }
    }
    out.truncate(20);
    out
}

/// Estimate input: object, `[o, m, p]`, `"2,3,5"`, `"3"` or a number.
pub fn parse_estimate(field: &str, v: &Value) -> Result<Value> {
    let hint = "use {\"optimistic\":2,\"likely\":3,\"pessimistic\":5} (days), \"2,3,5\", or a single number for the likely value";
    let triple: [Option<f64>; 3] = match v {
        Value::Null => return Ok(Value::Null),
        Value::Number(_) => [None, num_of(v), None],
        Value::String(s) if s.trim().is_empty() => return Ok(Value::Null),
        Value::String(s) => {
            let parts: Vec<&str> = s.split([',', '/', '，', ' ']).filter(|p| !p.trim().is_empty()).collect();
            let nums: Option<Vec<f64>> = parts.iter().map(|p| num_of(&json!(p))).collect();
            match nums {
                Some(n) if n.len() == 1 => [None, Some(n[0]), None],
                Some(n) if n.len() == 3 => [Some(n[0]), Some(n[1]), Some(n[2])],
                _ => return Err(bad(field, format!("cannot read estimate {:?}", s), hint)),
            }
        }
        Value::Array(a) if a.len() == 3 => [num_of(&a[0]), num_of(&a[1]), num_of(&a[2])],
        Value::Object(o) => {
            let g = |keys: &[&str]| keys.iter().find_map(|k| o.get(*k)).and_then(num_of);
            for k in o.keys() {
                if !["optimistic", "likely", "pessimistic", "o", "m", "p", "min", "max", "most_likely", "best", "worst", "expected"].contains(&k.as_str()) {
                    return Err(bad(field, format!("unknown estimate key {:?}", k), hint));
                }
            }
            [g(&["optimistic", "o", "min", "best"]), g(&["likely", "m", "most_likely", "expected"]), g(&["pessimistic", "p", "max", "worst"])]
        }
        _ => return Err(bad(field, "cannot read estimate", hint)),
    };
    if triple.iter().all(Option::is_none) {
        return Ok(Value::Null);
    }
    for x in triple.iter().flatten() {
        if *x < 0.0 || *x > 3650.0 {
            return Err(bad(field, "estimates are days between 0 and 3650", hint));
        }
    }
    let mut o = Map::new();
    for (k, x) in ["optimistic", "likely", "pessimistic"].iter().zip(triple) {
        o.insert(k.to_string(), x.map(to_num).unwrap_or(Value::Null));
    }
    Ok(Value::Object(o))
}

/// Resolves a reference to an id: `(expected type or "any", raw value) -> id`.
pub type Resolver<'a> = dyn FnMut(&str, &str) -> Result<String> + 'a;

/// Coerces one value for one field.
pub fn coerce(f: &Field, v: &Value, clock: &Clock, resolve: &mut Resolver) -> Result<Value> {
    let name = f.name;
    Ok(match f.kind {
        Kind::Text | Kind::LongText => match v {
            Value::Null => json!(""),
            Value::String(s) => {
                let s = if f.kind == Kind::Text { s.trim().replace(['\n', '\r'], " ") } else { s.trim_end().to_string() };
                let max = if f.kind == Kind::Text { 500 } else { 50_000 };
                if s.chars().count() > max {
                    return Err(bad(name, format!("longer than {} characters", max), "shorten it or put details in note"));
                }
                json!(s)
            }
            Value::Number(n) => json!(n.to_string()),
            Value::Bool(b) => json!(b.to_string()),
            _ => return Err(bad(name, "expected a string", "pass plain text")),
        },
        Kind::Date => match v {
            Value::Null => Value::Null,
            Value::String(s) if s.trim().is_empty() || s.trim() == "none" || s.trim() == "null" => Value::Null,
            Value::String(s) => match parse_date_lenient(s, &clock.today()) {
                Some(d) => json!(d),
                None => return Err(bad(name, format!("cannot read date {:?}", s), "use YYYY-MM-DD, today, tomorrow, +3d, +1w, fri, next mon, 周五 or 下周一")),
            },
            Value::Number(n) => {
                let ms = n.as_i64().unwrap_or(0);
                json!(clock.date_of(&clock.iso(ms)).unwrap_or_default())
            }
            _ => return Err(bad(name, "expected a date string", "use YYYY-MM-DD")),
        },
        Kind::Ts => match v {
            Value::Null => Value::Null,
            Value::String(s) if s.trim().is_empty() => Value::Null,
            Value::Number(n) => json!(clock.iso(n.as_f64().unwrap_or(0.0) as i64)),
            Value::String(s) => match crate::time::ms_from_iso(s, clock.offset_s) {
                Some(ms) => json!(clock.iso(ms)),
                None => match parse_date_lenient(s, &clock.today()) {
                    Some(d) => json!(clock.iso(crate::time::ms_from_iso(&d, clock.offset_s).unwrap_or(clock.now_ms))),
                    None => return Err(bad(name, format!("cannot read timestamp {:?}", s), "use RFC 3339, e.g. 2026-10-12T09:30:00+08:00")),
                },
            },
            _ => return Err(bad(name, "expected a timestamp", "use RFC 3339")),
        },
        Kind::Bool => match v {
            Value::Bool(b) => json!(*b),
            Value::Null => json!(false),
            Value::Number(n) => json!(n.as_f64().unwrap_or(0.0) != 0.0),
            Value::String(s) => match s.trim().to_lowercase().as_str() {
                "true" | "yes" | "y" | "1" | "on" | "是" => json!(true),
                "false" | "no" | "n" | "0" | "off" | "否" | "" => json!(false),
                _ => return Err(bad(name, format!("cannot read boolean {:?}", s), "use true or false")),
            },
            _ => return Err(bad(name, "expected true or false", "use true or false")),
        },
        Kind::Int(min, max) => {
            let n = match v {
                Value::Null => min.max(0) as f64,
                Value::String(s) => {
                    let t = s.trim().trim_start_matches(['p', 'P']);
                    match t.parse::<f64>() {
                        Ok(x) => x,
                        Err(_) => match (name, s.trim().to_lowercase().as_str()) {
                            ("priority", "high" | "高" | "urgent" | "紧急") => 1.0,
                            ("priority", "medium" | "mid" | "normal" | "中") => 2.0,
                            ("priority", "low" | "低") => 3.0,
                            ("priority", "none" | "无") => 0.0,
                            _ => return Err(bad(name, format!("cannot read number {:?}", s), format!("use an integer {}..{}", min, max))),
                        },
                    }
                }
                Value::Number(n) => n.as_f64().unwrap_or(0.0),
                _ => return Err(bad(name, "expected an integer", format!("use an integer {}..{}", min, max))),
            };
            if n.fract() != 0.0 || (n as i64) < min || (n as i64) > max {
                let extra = if name == "priority" { " (0 none, 1 high, 2 medium, 3 low)" } else { "" };
                return Err(bad(name, format!("{} is out of range", n), format!("use an integer {}..{}{}", min, max, extra)));
            }
            json!(n as i64)
        }
        Kind::Num => match v {
            Value::Null => Value::Null,
            Value::String(s) if s.trim().is_empty() => Value::Null,
            _ => match num_of(v) {
                Some(x) if x.is_finite() => to_num(x),
                _ => return Err(bad(name, "expected a number", "pass a number")),
            },
        },
        Kind::Enum(vals) | Kind::OptEnum(vals) => {
            let optional = matches!(f.kind, Kind::OptEnum(_));
            match v {
                Value::Null if optional => Value::Null,
                Value::String(s) if optional && (s.trim().is_empty() || s == "none" || s == "null" || s == "default") => Value::Null,
                Value::String(s) => {
                    let l = s.trim().to_lowercase();
                    if let Some(x) = vals.iter().find(|x| **x == l) {
                        json!(x)
                    } else if let Some(x) = enum_alias(name, &l).and_then(|a| {
                        let cross = match a {
                            "doing" => "active",
                            "active" => "doing",
                            _ => a,
                        };
                        [a, cross].into_iter().find(|c| vals.contains(c))
                    }) {
                        json!(x)
                    } else {
                        return Err(bad(name, format!("{:?} is not allowed", s), format!("use one of: {}", vals.join(", "))));
                    }
                }
                _ => return Err(bad(name, "expected a string", format!("use one of: {}", vals.join(", ")))),
            }
        }
        Kind::Tags => json!(parse_tags(v)),
        Kind::Ref(ty) => match v {
            Value::Null => Value::Null,
            Value::String(s) if s.trim().is_empty() || s == "none" || s == "null" => Value::Null,
            Value::String(s) => json!(resolve(ty, s.trim())?),
            _ => return Err(bad(name, "expected an id string", format!("pass a {} id", ty))),
        },
        Kind::AnyRef => match v {
            Value::String(s) if !s.trim().is_empty() => json!(resolve("any", s.trim())?),
            _ => return Err(bad(name, "expected an id", "pass the id (or exact title) of an idea, project, task or map")),
        },
        Kind::Refs(ty) => {
            let raw: Vec<String> = match v {
                Value::Null => vec![],
                Value::String(s) => s.split([',', ' ', '，']).map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
                Value::Array(a) => {
                    let mut out = vec![];
                    for x in a {
                        match x.as_str() {
                            Some(s) if !s.trim().is_empty() => out.push(s.trim().to_string()),
                            _ => return Err(bad(name, "expected a list of ids", format!("pass [\"{}_…\", …]", ty))),
                        }
                    }
                    out
                }
                _ => return Err(bad(name, "expected a list of ids", format!("pass [\"{}_…\", …]", ty))),
            };
            let mut ids: Vec<String> = vec![];
            for r in raw {
                let id = resolve(ty, &r)?;
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            json!(ids)
        }
        Kind::Estimate => parse_estimate(name, v)?,
        Kind::Json => v.clone(),
    })
}

/// Op parameters that are never document fields.
pub const RESERVED: &[&str] = &["op", "id", "ref", "if_version", "type", "cascade", "force", "set", "fields", "doc"];

/// Turns loose input into document fields. On create, missing fields get
/// defaults and required fields are checked. Unknown fields are errors with
/// a hint, so agents learn the schema from mistakes.
pub fn normalize(td: &TypeDef, input: &Map<String, Value>, create: bool, clock: &Clock, resolve: &mut Resolver, lenient: bool) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    let mut unknown: Vec<String> = vec![];
    for (k, v) in input {
        if RESERVED.contains(&k.as_str()) {
            continue;
        }
        let snake = camel_to_snake(k);
        if SYSTEM_FIELDS.contains(&snake.as_str()) || k == "updatedAt" || k == "createdAt" {
            if create && (snake == "created_at") {
                let ts = coerce(&Field { name: "created_at", kind: Kind::Ts, aliases: &[], cn: "", desc: "" }, v, clock, resolve)?;
                if !ts.is_null() {
                    out.insert("created_at".into(), ts);
                }
            }
            continue;
        }
        // GUI-only estimate fields
        if td.name == "task" && ["estO", "estM", "estP"].contains(&k.as_str()) {
            continue;
        }
        match lookup_field(td, k) {
            Some(f) => {
                if matches!(f.name, "source_type" | "target_type" | "root_id") && !lenient {
                    continue;
                }
                let val = coerce(f, v, clock, resolve)?;
                out.insert(f.name.to_string(), val);
            }
            None if lenient => {}
            None => unknown.push(k.clone()),
        }
    }
    if !unknown.is_empty() {
        let names: Vec<&str> = td.fields.iter().filter(|f| !matches!(f.name, "source_type" | "target_type" | "sample")).map(|f| f.name).collect();
        let sugg: Vec<String> = unknown
            .iter()
            .filter_map(|u| closest(u, names.iter().copied().chain(td.fields.iter().flat_map(|f| f.aliases.iter().copied()))).map(|c| format!("{} → {}", u, c)))
            .collect();
        let mut hint = format!("{} fields: {}", td.name, names.join(", "));
        if !sugg.is_empty() {
            hint = format!("did you mean {}? {}", sugg.join(", "), hint);
        }
        return Err(Error::new("unknown_field", format!("{} has no field {}", td.name, unknown.join(", "))).hint(hint).data(json!({ "fields": unknown })));
    }
    if create {
        for f in td.fields {
            if !out.contains_key(f.name) {
                out.insert(f.name.to_string(), default_for(f));
            }
        }
        for r in td.required.iter().filter(|_| !lenient) {
            let empty = match out.get(*r) {
                None | Some(Value::Null) => true,
                Some(Value::String(s)) => s.trim().is_empty(),
                _ => false,
            };
            if empty {
                return Err(Error::new("missing_field", format!("{} needs {}", td.name, r)).hint(format!("add \"{}\": …", r)).data(json!({ "field": r })));
            }
        }
    }
    Ok(out)
}

/// Display title of a document.
pub fn title_of(doc: &Map<String, Value>) -> String {
    let ty = doc.get("type").and_then(Value::as_str).unwrap_or("");
    let raw = match ty {
        "idea" => crate::util::first_line(crate::util::s(doc, "text")).to_string(),
        "project" => crate::util::s(doc, "name").to_string(),
        "link" => crate::util::s(doc, "relation").to_string(),
        _ => crate::util::s(doc, "title").to_string(),
    };
    crate::util::clip(&raw, 80)
}

pub fn status_cn(v: &str) -> &str {
    match v {
        "todo" => "待办",
        "doing" => "进行中",
        "done" => "已完成",
        "inbox" => "收件箱",
        "incubating" => "孵化中",
        "converted" => "已转化",
        "archived" => "归档",
        "active" => "进行中",
        "paused" => "暂停",
        "confirmed" => "已确认",
        "suggested" => "待确认",
        _ => v,
    }
}

pub fn relation_cn(v: &str) -> &str {
    match v {
        "relates_to" => "相关",
        "inspired_by" => "受启发于",
        "derived_from" => "来源于",
        "part_of" => "属于",
        "supports" => "支持",
        "contradicts" => "矛盾",
        "depends_on" => "依赖",
        "similar_to" => "相似",
        "mentions" => "提到",
        "tagged" => "标签",
        "converted_to" => "转化为",
        "expands" => "展开",
        _ => v,
    }
}

/// Schema description for agents (`shiyi schema`).
pub fn schema_json(only: Option<&str>) -> Value {
    let types: Vec<Value> = TYPES
        .iter()
        .filter(|t| only.map_or(true, |o| type_def(o).map_or(false, |d| d.name == t.name)))
        .map(|t| {
            let fields: Vec<Value> = t
                .fields
                .iter()
                .filter(|f| !matches!(f.name, "source_type" | "target_type"))
                .map(|f| {
                    let (kind, values): (String, Option<Vec<&str>>) = match f.kind {
                        Kind::Text => ("string".into(), None),
                        Kind::LongText => ("text".into(), None),
                        Kind::Date => ("date YYYY-MM-DD | null".into(), None),
                        Kind::Ts => ("timestamp | null".into(), None),
                        Kind::Bool => ("boolean".into(), None),
                        Kind::Int(a, b) => (format!("integer {}..{}", a, b), None),
                        Kind::Num => ("number | null".into(), None),
                        Kind::Enum(v) => ("enum".into(), Some(v.to_vec())),
                        Kind::OptEnum(v) => ("enum | null".into(), Some(v.to_vec())),
                        Kind::Tags => ("string[]".into(), None),
                        Kind::Ref(t) => (format!("{} id | null", t), None),
                        Kind::Refs(t) => (format!("{} id[]", t), None),
                        Kind::AnyRef => ("idea|project|task|map id".into(), None),
                        Kind::Estimate => ("{optimistic, likely, pessimistic} days | null".into(), None),
                        Kind::Json => ("json".into(), None),
                    };
                    let mut o = json!({ "name": f.name, "type": kind, "desc": f.desc, "default": default_for(f) });
                    if let Some(v) = values {
                        o["values"] = json!(v);
                    }
                    if !f.aliases.is_empty() {
                        o["aliases"] = json!(f.aliases);
                    }
                    if t.required.contains(&f.name) {
                        o["required"] = json!(true);
                    }
                    o
                })
                .collect();
            json!({ "type": t.name, "id_prefix": format!("{}_", t.prefix), "desc": t.desc, "fields": fields })
        })
        .collect();
    json!({
        "types": types,
        "system_fields": {
            "id": "string, prefixed by type (idea_, proj_, task_, map_, link_)",
            "type": "idea | project | task | map | link",
            "created_at": "timestamp", "updated_at": "timestamp",
            "version": "integer, +1 on every change; pass if_version to avoid overwriting someone else's change",
            "created_by": "user | agent:<name>", "updated_by": "user | agent:<name>",
            "deleted_at": "timestamp when soft-deleted, else null"
        },
        "ops": [
            {"op": "<type>.create", "params": "fields…, optional ref (name for later ops: \"$ref\"), optional id (client id, makes retries idempotent)", "example": {"op": "task.create", "title": "写周报", "project": "proj_…", "due": "fri", "estimate": "1,2,4"}},
            {"op": "<type>.update", "params": "id, fields… (or set:{…}), optional if_version", "example": {"op": "task.update", "id": "task_…", "status": "done"}},
            {"op": "<type>.delete", "params": "id; project.delete needs cascade:true when it still has tasks", "example": {"op": "idea.delete", "id": "idea_…"}},
            {"op": "<type>.restore", "params": "id", "example": {"op": "task.restore", "id": "task_…"}},
            {"op": "task.add_dep", "params": "id, on: task id or list", "example": {"op": "task.add_dep", "id": "task_b", "on": "task_a"}},
            {"op": "task.remove_dep", "params": "id, on", "example": {"op": "task.remove_dep", "id": "task_b", "on": "task_a"}},
            {"op": "idea.convert", "params": "id, to: task | project, optional project (for to=task), title/name, area", "example": {"op": "idea.convert", "id": "idea_…", "to": "project"}},
            {"op": "project.schedule", "params": "id, optional force (also reschedule tasks that already have dates)", "example": {"op": "project.schedule", "id": "proj_…"}},
            {"op": "map.create", "params": "title, outline (indented text or nested [{text, children}]), optional idea_id / project_id", "example": {"op": "map.create", "title": "发布会", "outline": "场地\n  预算\n嘉宾\n  邀请名单"}},
            {"op": "link.create", "params": "source, target, relation, note, status (agents: suggested)", "example": {"op": "link.create", "source": "idea_…", "target": "idea_…", "relation": "inspired_by", "status": "suggested", "note": "同一个用户痛点"}},
            {"op": "link.confirm", "params": "id", "example": {"op": "link.confirm", "id": "link_…"}},
            {"op": "undo", "params": "batch (bat_…) | op (op_…) | last:true; force:true ignores later edits", "example": {"op": "undo", "batch": "bat_…"}}
        ],
        "queries": ["today", "list", "get", "search", "graph", "log", "trash", "schema", "doctor", "status", "snapshot"],
        "conventions": [
            "A request is {\"ops\":[…]} (or one op, or an array). All ops in a request succeed or fail together.",
            "dry_run:true validates and shows the result without saving.",
            "Dates are YYYY-MM-DD; today, tomorrow, +3d, +1w, fri, next mon, 周五, 下周一 also work.",
            "References accept an id, the exact title, or \"$ref\" of an earlier op in the same request.",
            "Unknown fields are errors; error.hint lists the valid ones.",
            "Deletes are soft (restore brings them back). Every change can be undone with its batch id."
        ]
    })
}
