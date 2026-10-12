//! Write path: every change is an operation inside a batch. Batches are
//! atomic, logged with before/after snapshots, and undoable.

use crate::db::{delete_row, docs, load_doc, write_doc, Core, Deleted};
use crate::model::{self, TypeDef};
use crate::time::Clock;
use crate::util::{clip, first_line, new_id, s, short_id, str_list, truthy};
use crate::{Error, Result};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub enum Perm {
    Read,
    Write,
    Full,
}

impl Perm {
    pub fn parse(s: &str) -> Option<Perm> {
        match s.trim().to_lowercase().as_str() {
            "read" | "readonly" | "read_only" | "只读" => Some(Perm::Read),
            "write" | "readwrite" | "可写" => Some(Perm::Write),
            "full" | "all" | "完全" => Some(Perm::Full),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Perm::Read => "read",
            Perm::Write => "write",
            Perm::Full => "full",
        }
    }
}

pub const MAX_OPS: usize = 500;

/// Normalizes an actor name: `user`, or `agent:<name>`.
pub fn actor_name(raw: &str) -> String {
    let r = raw.trim();
    if r == "user" {
        return "user".into();
    }
    let name = r.strip_prefix("agent:").unwrap_or(r);
    let clean: String = name.chars().filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ')).take(40).collect();
    let clean = clean.trim();
    format!("agent:{}", if clean.is_empty() { "unknown" } else { clean })
}

fn parse_request(req: Value) -> Result<(Vec<Map<String, Value>>, bool, Option<String>)> {
    let mut dry = false;
    let mut note = None;
    let list: Vec<Value> = match req {
        Value::Array(a) => a,
        Value::Object(mut o) => {
            dry = truthy(o.get("dry_run"));
            note = o.get("note").and_then(Value::as_str).map(|x| clip(x, 200));
            match o.remove("ops") {
                Some(Value::Array(a)) => a,
                Some(_) => return Err(Error::new("invalid_request", "\"ops\" must be an array").hint("{\"ops\":[{\"op\":\"task.create\",\"title\":\"…\"}]}")),
                None if o.contains_key("op") => {
                    o.remove("dry_run");
                    vec![Value::Object(o)]
                }
                None => return Err(Error::new("invalid_request", "no operations").hint("send {\"ops\":[…]} or a single {\"op\":…}; see `shiyi schema`")),
            }
        }
        _ => return Err(Error::new("invalid_request", "expected a JSON object or array").hint("{\"ops\":[{\"op\":\"idea.create\",\"text\":\"…\"}]}")),
    };
    if list.is_empty() {
        return Err(Error::new("invalid_request", "no operations").hint("\"ops\" is empty"));
    }
    let mut ops = vec![];
    for (i, v) in list.into_iter().enumerate() {
        match v {
            Value::Object(o) if o.get("op").and_then(Value::as_str).is_some() => ops.push(o),
            _ => return Err(Error::new("invalid_request", format!("op #{} needs an \"op\" name", i)).hint("e.g. {\"op\":\"task.update\",\"id\":\"task_…\",\"status\":\"done\"}")),
        }
    }
    Ok((ops, dry, note))
}

pub fn apply(core: &mut Core, req: Value, actor: &str) -> Result<Value> {
    let (ops, dry_run, note) = parse_request(req)?;
    if ops.len() > MAX_OPS {
        return Err(Error::new("too_many_ops", format!("at most {} operations per request", MAX_OPS)).hint("split the work into several requests"));
    }
    run_batch(core, ops, dry_run, note, actor)
}

/// Runs operations as one transaction (no size limit; used by import).
pub(crate) fn run_batch(core: &mut Core, ops: Vec<Map<String, Value>>, dry_run: bool, note: Option<String>, actor: &str) -> Result<Value> {
    run_batch_mode(core, ops, dry_run, note, actor, false)
}

pub(crate) fn run_batch_mode(core: &mut Core, ops: Vec<Map<String, Value>>, dry_run: bool, note: Option<String>, actor: &str, import: bool) -> Result<Value> {
    let actor = actor_name(actor);
    let clock = core.clock();
    let perm = Perm::parse(&core.permission()).unwrap_or(Perm::Write);
    let tx = core.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut cx = Ctx { tx: &tx, clock, actor, perm, batch: new_id("bat"), refs: HashMap::new(), changes: vec![], note, logged: 0, task_deps: None, import };
    let mut results = vec![];
    for (i, op) in ops.iter().enumerate() {
        match cx.run(op) {
            Ok(r) => results.push(r),
            Err(mut e) => {
                let mut d = e.data.take().unwrap_or_else(|| json!({}));
                if !d.is_object() {
                    d = json!({ "detail": d });
                }
                d["op_index"] = json!(i);
                d["op"] = op.get("op").cloned().unwrap_or(Value::Null);
                if ops.len() > 1 {
                    e.hint = Some(format!("{}{}nothing was saved; fix op #{} and resend the request", e.hint.clone().unwrap_or_default(), if e.hint.is_some() { " — " } else { "" }, i));
                }
                e.data = Some(d);
                return Err(e);
            }
        }
    }
    let changed = cx.logged;
    let batch = cx.batch.clone();
    let changes = std::mem::take(&mut cx.changes);
    drop(cx);
    if dry_run {
        tx.rollback()?;
    } else {
        tx.commit()?;
    }
    let mut out = json!({ "ok": true, "dry_run": dry_run, "changed": changed, "results": results, "changes": changes });
    if changed > 0 && !dry_run {
        out["batch"] = json!(batch);
        out["undo"] = json!(format!("shiyi undo {}", batch));
    }
    Ok(out)
}

struct Ctx<'a> {
    tx: &'a Transaction<'a>,
    clock: Clock,
    actor: String,
    perm: Perm,
    batch: String,
    refs: HashMap<String, String>,
    changes: Vec<Value>,
    note: Option<String>,
    logged: usize,
    /// cached task -> depends_on for cycle checks
    task_deps: Option<HashMap<String, Vec<String>>>,
    /// bulk import: references may point forward or in circles
    import: bool,
}

fn canonical_verb(v: &str) -> &str {
    match v {
        "create" | "add" | "new" | "insert" => "create",
        "update" | "set" | "edit" | "patch" | "modify" | "change" => "update",
        "delete" | "remove" | "rm" | "del" | "trash" => "delete",
        "restore" | "undelete" | "recover" | "untrash" => "restore",
        "put" | "upsert" | "replace" => "put",
        "add_dep" | "dep" | "depend" | "add_dependency" | "add_deps" => "add_dep",
        "remove_dep" | "rm_dep" | "remove_dependency" | "del_dep" => "remove_dep",
        "convert" | "promote" => "convert",
        "schedule" | "auto_schedule" | "autoschedule" => "schedule",
        "confirm" | "accept" | "approve" => "confirm",
        "reject" | "dismiss" => "reject",
        "done" | "complete" | "finish" => "done",
        "start" | "begin" => "start",
        "reopen" | "undone" => "reopen",
        "archive" => "archive",
        other => other,
    }
}

impl<'a> Ctx<'a> {
    fn is_user(&self) -> bool {
        self.actor == "user"
    }

    /// The GUI and bulk imports send whole documents whose references may not resolve yet.
    fn lenient(&self) -> bool {
        self.is_user() || self.import
    }

    fn require_write(&self) -> Result<()> {
        if !self.is_user() && self.perm == Perm::Read {
            return Err(Error::new("permission_denied", "agents may only read 十一遗 right now (permission: read)")
                .hint("ask the user to change 设置 → AI 接入 → 权限 to 可写 (write)"));
        }
        Ok(())
    }

    /// Agents with write permission may only suggest links.
    fn may_confirm_links(&self) -> bool {
        self.is_user() || self.perm == Perm::Full
    }

    fn require_delete(&self, doc: &Map<String, Value>) -> Result<()> {
        self.require_write()?;
        if self.is_user() || self.perm == Perm::Full {
            return Ok(());
        }
        let by = s(doc, "created_by");
        let suggestion = s(doc, "type") == "link" && s(doc, "status") == "suggested";
        if by.starts_with("agent:") || suggestion {
            return Ok(());
        }
        Err(Error::new("permission_denied", format!("{} was created by the user; agents with write permission can only delete what agents created", s(doc, "id")))
            .hint("archive it instead (ideas: status=archived, projects: status=paused/done), or ask the user to raise 设置 → AI 接入 → 权限 to 完全 (full)"))
    }

    fn run(&mut self, op: &Map<String, Value>) -> Result<Value> {
        let name = op.get("op").and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
        if name == "undo" || name == "redo" {
            return self.undo(op);
        }
        if name == "settings.set" || name == "setting.set" || name == "set_setting" {
            return self.settings_set(op);
        }
        let (tpart, vpart) = match name.split_once('.') {
            Some((a, b)) => (a.to_string(), b.to_string()),
            None => match name.split_once('_') {
                Some((a, b)) if model::type_def(b).is_some() => (b.to_string(), a.to_string()),
                _ => (op.get("type").and_then(Value::as_str).unwrap_or("entity").to_string(), name.clone()),
            },
        };
        let verb = canonical_verb(&vpart).to_string();
        let td: Option<&'static TypeDef> = if tpart == "entity" || tpart == "entities" || tpart.is_empty() {
            op.get("type").and_then(Value::as_str).and_then(model::type_def)
        } else {
            Some(model::type_def(&tpart).ok_or_else(|| {
                Error::new("unknown_op", format!("unknown type {:?} in op {:?}", tpart, name)).hint("types: idea, project, task, map, link; ops look like task.create, idea.update, link.create, undo")
            })?)
        };
        match verb.as_str() {
            "create" => {
                let td = td.ok_or_else(|| Error::new("invalid_request", "create needs a type").hint("use task.create, idea.create, …"))?;
                self.create(td, op)
            }
            "update" => self.update_op(td, op),
            "delete" => self.delete_op(td, op, tpart != "entity"),
            "reject" if td.map_or(false, |t| t.name == "link") => self.delete_op(td, op, true),
            "restore" => self.restore_op(td, op),
            "put" => self.put_op(td, op),
            "add_dep" | "remove_dep" => self.dep_op(op, verb == "add_dep"),
            "convert" => self.convert_op(op),
            "schedule" => self.schedule_op(op),
            "confirm" => self.status_shortcut(td, op, "link", "confirmed"),
            "done" => self.status_shortcut(td, op, "task", "done"),
            "start" => self.status_shortcut(td, op, "task", "doing"),
            "reopen" => self.status_shortcut(td, op, "task", "todo"),
            "archive" => self.status_shortcut(td, op, "idea", "archived"),
            _ => Err(Error::new("unknown_op", format!("unknown op {:?}", name))
                .hint("ops: <type>.create|update|delete|restore, task.add_dep|remove_dep|done|start|reopen, idea.convert|archive, project.schedule, link.confirm|reject, undo — see `shiyi schema`")),
        }
    }

    // ---------- references ----------

    fn resolve(&self, expected: &str, raw: &str) -> Result<String> {
        let v = raw.trim();
        if let Some(r) = v.strip_prefix('$') {
            return self.refs.get(r).cloned().ok_or_else(|| {
                Error::new("not_found", format!("no earlier op in this request has ref {:?}", r)).hint("give an earlier create op \"ref\": \"name\" and refer to it as \"$name\"")
            });
        }
        if let Some(doc) = load_doc(self.tx, v)? {
            let t = s(&doc, "type");
            let ok = if expected == "any" { t != "link" } else { t == expected };
            if !ok {
                return Err(Error::new("invalid_ref", format!("{} is a {}, expected {}", v, t, if expected == "any" { "an idea, project, task or map" } else { expected })));
            }
            if !self.lenient() && doc.get("deleted_at").map_or(false, |d| !d.is_null()) {
                return Err(Error::new("deleted", format!("{} is deleted", v)).hint(format!("restore it first: {{\"op\":\"{}.restore\",\"id\":\"{}\"}}", t, v)));
            }
            return Ok(v.to_string());
        }
        if self.lenient() {
            return Ok(v.to_string());
        }
        if model::type_of_id(v).is_some() {
            return Err(Error::new("not_found", format!("no {} with id {}", if expected == "any" { "entity" } else { expected }, v)).hint("check the id with `shiyi search` or `shiyi list`"));
        }
        self.find_by_title(expected, v)
    }

    fn find_by_title(&self, expected: &str, v: &str) -> Result<String> {
        let types: Vec<&str> = if expected == "any" { vec!["idea", "project", "task", "map"] } else { vec![expected] };
        let placeholders = types.iter().map(|t| format!("'{}'", t)).collect::<Vec<_>>().join(",");
        let find = |sql: &str, arg: &str| -> Result<Vec<(String, String, String)>> {
            let mut st = self.tx.prepare(sql)?;
            let rows = st.query_map([arg], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<String>>(2)?.unwrap_or_default())))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        };
        let exact = find(
            &format!("SELECT id,type,title FROM entities WHERE deleted_at IS NULL AND type IN ({}) AND lower(title)=lower(?1) LIMIT 6", placeholders),
            v,
        )?;
        let cands = if exact.is_empty() {
            find(
                &format!("SELECT id,type,title FROM entities WHERE deleted_at IS NULL AND type IN ({}) AND instr(lower(title), lower(?1))>0 ORDER BY updated_at DESC LIMIT 6", placeholders),
                v,
            )?
        } else {
            exact
        };
        match cands.len() {
            1 => Ok(cands[0].0.clone()),
            0 => Err(Error::new("not_found", format!("no {} matches {:?}", if expected == "any" { "idea, project, task or map" } else { expected }, v))
                .hint("pass an id; find it with `shiyi search <words>`")),
            _ => Err(Error::new("ambiguous", format!("{:?} matches several {}s", v, if expected == "any" { "entitie" } else { expected }))
                .hint("pass one of the ids")
                .data(json!({ "candidates": cands.iter().map(|(i, t, ti)| json!({"id": i, "type": t, "title": ti})).collect::<Vec<_>>() }))),
        }
    }

    fn target(&self, td: Option<&'static TypeDef>, op: &Map<String, Value>) -> Result<(Map<String, Value>, &'static TypeDef)> {
        let raw = op.get("id").and_then(Value::as_str).ok_or_else(|| Error::new("missing_field", "this op needs \"id\"").hint("pass the entity id, e.g. \"id\":\"task_…\""))?;
        let expected = td.map(|t| t.name).unwrap_or("any");
        let id = if raw.starts_with('$') || model::type_of_id(raw).is_some() || load_doc(self.tx, raw)?.is_some() || self.lenient() {
            if let Some(r) = raw.strip_prefix('$') {
                self.refs.get(r).cloned().ok_or_else(|| Error::new("not_found", format!("no earlier op has ref {:?}", r)))?
            } else {
                raw.to_string()
            }
        } else {
            self.find_by_title(if expected == "link" { "link" } else { expected }, raw)?
        };
        let doc = load_doc(self.tx, &id)?.ok_or_else(|| Error::new("not_found", format!("no {} with id {}", expected, id)).hint("check the id with `shiyi search` or `shiyi list`"))?;
        let ty = s(&doc, "type").to_string();
        if let Some(t) = td {
            if t.name != ty {
                return Err(Error::new("invalid_ref", format!("{} is a {}, not a {}", id, ty, t.name)).hint(format!("use {}.… ops for it", ty)));
            }
        }
        let td = model::type_def(&ty).ok_or_else(|| Error::new("db_error", format!("unknown stored type {}", ty)))?;
        Ok((doc, td))
    }

    fn check_version(&self, op: &Map<String, Value>, doc: &Map<String, Value>) -> Result<()> {
        if let Some(want) = op.get("if_version").and_then(Value::as_i64) {
            let have = doc.get("version").and_then(Value::as_i64).unwrap_or(1);
            if want != have {
                return Err(Error::new("conflict", format!("{} changed since you read it (version {} now, you had {})", s(doc, "id"), have, want))
                    .hint("read it again with `shiyi get`, then retry with the new version")
                    .data(json!({ "current_version": have, "updated_by": doc.get("updated_by"), "updated_at": doc.get("updated_at") })));
            }
        }
        Ok(())
    }

    fn register_ref(&mut self, op: &Map<String, Value>, id: &str) {
        if let Some(r) = op.get("ref").and_then(Value::as_str) {
            self.refs.insert(r.trim_start_matches('$').to_string(), id.to_string());
        }
    }

    // ---------- logging ----------

    fn title_for(&self, id: &str) -> String {
        load_doc(self.tx, id).ok().flatten().map(|d| model::title_of(&d)).unwrap_or_else(|| id.to_string())
    }

    fn summarize(&self, verb: &str, td: &TypeDef, before: Option<&Map<String, Value>>, after: Option<&Map<String, Value>>) -> String {
        let doc = after.or(before);
        let title = doc.map(model::title_of).unwrap_or_default();
        if td.name == "link" {
            if let Some(d) = doc {
                let line = format!("「{}」{}「{}」", clip(&self.title_for(s(d, "source")), 24), model::relation_cn(s(d, "relation")), clip(&self.title_for(s(d, "target")), 24));
                return match verb {
                    "create" => format!("{}关联 {}", if s(d, "status") == "suggested" { "建议" } else { "新建" }, line),
                    "delete" => format!("删除关联 {}", line),
                    "restore" => format!("恢复关联 {}", line),
                    "undo" => format!("撤销关联变更 {}", line),
                    _ => format!("修改关联 {}{}", line, if s(d, "status") == "confirmed" && before.map_or(false, |b| s(b, "status") == "suggested") { "：已确认" } else { "" }),
                };
            }
        }
        match verb {
            "create" => format!("新建{}「{}」", td.cn, title),
            "delete" => format!("删除{}「{}」", td.cn, title),
            "restore" => format!("恢复{}「{}」", td.cn, title),
            "undo" => match (before, after) {
                (Some(_), None) => format!("撤销：移除{}「{}」", td.cn, title),
                (None, Some(_)) => format!("撤销：找回{}「{}」", td.cn, title),
                _ => format!("撤销：{}「{}」恢复原状", td.cn, title),
            },
            _ => {
                let (Some(b), Some(a)) = (before, after) else { return format!("修改{}「{}」", td.cn, title) };
                let mut parts = vec![];
                for f in td.fields {
                    let (x, y) = (b.get(f.name).unwrap_or(&Value::Null), a.get(f.name).unwrap_or(&Value::Null));
                    if x == y {
                        continue;
                    }
                    let show = |v: &Value| match v {
                        Value::Null => "无".to_string(),
                        Value::String(t) if t.is_empty() => "无".to_string(),
                        Value::String(t) => clip(model::status_cn(t), 16),
                        Value::Number(n) => n.to_string(),
                        _ => String::new(),
                    };
                    let part = match f.name {
                        "status" | "due_date" | "start_date" | "today_date" | "priority" | "area" => format!("{} {}→{}", f.cn, show(x), show(y)),
                        "nodes" => describe_node_changes(x, y),
                        _ => f.cn.to_string(),
                    };
                    parts.push(part);
                }
                if parts.is_empty() {
                    format!("修改{}「{}」", td.cn, title)
                } else {
                    format!("修改{}「{}」：{}", td.cn, title, parts.join("、"))
                }
            }
        }
    }

    fn log(&mut self, verb: &str, td: &TypeDef, id: &str, before: Option<&Map<String, Value>>, after: Option<&Map<String, Value>>) -> Result<()> {
        let summary = self.summarize(verb, td, before, after);
        let enc = |d: Option<&Map<String, Value>>| -> Result<Option<String>> { Ok(match d { Some(m) => Some(serde_json::to_string(m)?), None => None }) };
        self.tx.execute(
            "INSERT INTO ops(id,batch,ts,ts_ms,actor,op,entity_type,entity_id,before,after,summary,note) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                new_id("op"),
                self.batch,
                self.clock.now_iso(),
                self.clock.now_ms,
                self.actor,
                format!("{}.{}", td.name, verb),
                td.name,
                id,
                enc(before)?,
                enc(after)?,
                summary,
                self.note
            ],
        )?;
        self.logged += 1;
        self.changes.push(json!({ "op": format!("{}.{}", td.name, verb), "id": id, "summary": summary }));
        if td.name == "task" {
            self.task_deps = None;
        }
        Ok(())
    }

    fn stamp_new(&self, td: &TypeDef, id: &str, fields: Map<String, Value>) -> Map<String, Value> {
        let mut d = Map::new();
        d.insert("id".into(), json!(id));
        d.insert("type".into(), json!(td.name));
        let created = fields.get("created_at").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| json!(self.clock.now_iso()));
        for (k, v) in fields {
            if k != "created_at" {
                d.insert(k, v);
            }
        }
        d.insert("created_at".into(), created);
        d.insert("updated_at".into(), json!(self.clock.now_iso()));
        d.insert("version".into(), json!(1));
        d.insert("created_by".into(), json!(self.actor));
        d.insert("updated_by".into(), json!(self.actor));
        d.insert("deleted_at".into(), Value::Null);
        d
    }

    fn bump(&self, d: &mut Map<String, Value>) {
        let v = d.get("version").and_then(Value::as_i64).unwrap_or(1);
        d.insert("version".into(), json!(v + 1));
        d.insert("updated_at".into(), json!(self.clock.now_iso()));
        d.insert("updated_by".into(), json!(self.actor));
    }

    fn brief(&self, d: &Map<String, Value>) -> Value {
        json!({ "id": d.get("id"), "type": d.get("type"), "title": model::title_of(d), "version": d.get("version") })
    }

    // ---------- create / update ----------

    fn fields_of(op: &Map<String, Value>) -> Map<String, Value> {
        let mut m = Map::new();
        for (k, v) in op {
            if !model::RESERVED.contains(&k.as_str()) {
                m.insert(k.clone(), v.clone());
            }
        }
        for key in ["set", "fields", "doc"] {
            if let Some(Value::Object(o)) = op.get(key) {
                for (k, v) in o {
                    if k != "id" && k != "type" {
                        m.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        m
    }

    fn create(&mut self, td: &'static TypeDef, op: &Map<String, Value>) -> Result<Value> {
        self.require_write()?;
        let client_id = op.get("id").and_then(Value::as_str).or_else(|| op.get("doc").and_then(|d| d.get("id")).and_then(Value::as_str)).map(str::trim);
        let id = match client_id {
            Some(id) => {
                if id.is_empty() || id.len() > 64 || id.chars().any(|c| c.is_whitespace() || c == '$') {
                    return Err(Error::new("invalid_id", format!("bad id {:?}", id)).hint("ids are 1-64 characters without spaces; leave id out to get a generated one"));
                }
                if !self.lenient() && !id.starts_with(&format!("{}_", td.prefix)) {
                    return Err(Error::new("invalid_id", format!("{} ids must start with {}_", td.name, td.prefix)).hint(format!("e.g. {}_my-key, or leave id out", td.prefix)));
                }
                if let Some(ex) = load_doc(self.tx, id)? {
                    if s(&ex, "type") != td.name {
                        return Err(Error::new("conflict", format!("id {} is already used by a {}", id, s(&ex, "type"))));
                    }
                    self.register_ref(op, id);
                    let mut r = self.brief(&ex);
                    r["existed"] = json!(true);
                    if ex.get("deleted_at").map_or(false, |d| !d.is_null()) {
                        r["deleted"] = json!(true);
                    }
                    return Ok(r);
                }
                id.to_string()
            }
            None => new_id(td.prefix),
        };
        let mut input = Self::fields_of(op);
        let outline = if td.name == "map" { input.remove("outline") } else { None };
        if let (Some(o), false) = (&outline, input.contains_key("title") || input.contains_key("name")) {
            let (root, nodes) = outline_to_nodes("", o)?;
            let t = nodes.get(&root).and_then(|n| n.get("text")).and_then(Value::as_str).unwrap_or("").to_string();
            input.insert("title".into(), json!(clip(&t, 80)));
        }
        let lenient = self.lenient();
        let mut review = false;
        let mut f = {
            let me: &Self = self;
            model::normalize(td, &input, true, &me.clock, &mut |t, v| me.resolve(t, v), lenient)?
        };
        match td.name {
            "task" => {
                if s(&f, "status") == "done" && f.get("done_at").map_or(true, Value::is_null) {
                    f.insert("done_at".into(), json!(self.clock.now_iso()));
                }
                if s(&f, "status") != "done" {
                    f.insert("done_at".into(), Value::Null);
                }
                let deps = str_list(f.get("depends_on"));
                self.check_deps(&id, &deps)?;
            }
            "link" => {
                // under write permission an agent can only suggest; the user confirms
                if !self.may_confirm_links() && s(&f, "status") != "suggested" {
                    f.insert("status".into(), json!("suggested"));
                    review = true;
                }
                if let Some(existing) = self.prepare_link(&id, &mut f)? {
                    self.register_ref(op, s(&existing, "id"));
                    let upgrade = s(&existing, "status") == "suggested" && s(&f, "status") == "confirmed";
                    if upgrade {
                        let before = existing.clone();
                        let mut after = existing.clone();
                        after.insert("status".into(), json!("confirmed"));
                        self.bump(&mut after);
                        write_doc(self.tx, &after)?;
                        self.log("update", td, s(&after, "id"), Some(&before), Some(&after))?;
                        let mut r = self.brief(&after);
                        r["existed"] = json!(true);
                        r["confirmed"] = json!(true);
                        return Ok(r);
                    }
                    let mut r = self.brief(&existing);
                    r["existed"] = json!(true);
                    return Ok(r);
                }
            }
            "map" => {
                let title = s(&f, "title").to_string();
                let has_nodes = f.get("nodes").map_or(false, |n| n.as_object().map_or(false, |o| !o.is_empty()));
                if let Some(o) = outline {
                    let (root, nodes) = outline_to_nodes(&title, &o)?;
                    f.insert("root_id".into(), json!(root));
                    f.insert("nodes".into(), Value::Object(nodes));
                } else if has_nodes {
                    if !lenient {
                        validate_nodes(&f)?;
                    }
                } else {
                    let (root, nodes) = outline_to_nodes(&title, &json!(""))?;
                    f.insert("root_id".into(), json!(root));
                    f.insert("nodes".into(), Value::Object(nodes));
                }
            }
            _ => {}
        }
        let doc = self.stamp_new(td, &id, f);
        write_doc(self.tx, &doc)?;
        self.log("create", td, &id, None, Some(&doc))?;
        self.register_ref(op, &id);
        let mut r = self.brief(&doc);
        r["created"] = json!(true);
        if td.name == "link" {
            r["status"] = json!(s(&doc, "status"));
            if review {
                r["needs_review"] = json!(true);
            }
        }
        Ok(r)
    }

    /// Validates a new link; returns an existing equivalent link if there is one.
    fn prepare_link(&self, id: &str, f: &mut Map<String, Value>) -> Result<Option<Map<String, Value>>> {
        let (src, dst) = (s(f, "source").to_string(), s(f, "target").to_string());
        if src == dst {
            return Err(Error::new("invalid_value", "a link needs two different ends").hint("source and target are the same"));
        }
        let st = load_doc(self.tx, &src)?.map(|d| s(&d, "type").to_string()).unwrap_or_default();
        let tt = load_doc(self.tx, &dst)?.map(|d| s(&d, "type").to_string()).unwrap_or_default();
        f.insert("source_type".into(), json!(st));
        f.insert("target_type".into(), json!(tt));
        let rel = s(f, "relation").to_string();
        let symmetric = model::SYMMETRIC.contains(&rel.as_str());
        for l in docs(self.tx, Some("link"), Deleted::Exclude)? {
            if s(&l, "id") == id || s(&l, "relation") != rel {
                continue;
            }
            let same = (s(&l, "source") == src && s(&l, "target") == dst) || (symmetric && s(&l, "source") == dst && s(&l, "target") == src);
            if same {
                return Ok(Some(l));
            }
        }
        Ok(None)
    }

    fn update_op(&mut self, td: Option<&'static TypeDef>, op: &Map<String, Value>) -> Result<Value> {
        let (before, td) = self.target(td, op)?;
        if before.get("deleted_at").map_or(false, |d| !d.is_null()) {
            return Err(Error::new("deleted", format!("{} is deleted", s(&before, "id"))).hint(format!("restore it first: {{\"op\":\"{}.restore\",\"id\":\"{}\"}}", td.name, s(&before, "id"))));
        }
        self.require_write()?;
        self.check_version(op, &before)?;
        let mut input = Self::fields_of(op);
        let outline = if td.name == "map" { input.remove("outline") } else { None };
        if input.is_empty() && outline.is_none() {
            return Err(Error::new("invalid_request", "nothing to update").hint(format!("pass fields to change, e.g. {{\"op\":\"{}.update\",\"id\":\"…\",\"{}\":…}}", td.name, td.title_field)));
        }
        let lenient = self.lenient();
        let mut part = {
            let me: &Self = self;
            model::normalize(td, &input, false, &me.clock, &mut |t, v| me.resolve(t, v), lenient)?
        };
        if let Some(o) = outline {
            let title = part.get("title").and_then(Value::as_str).unwrap_or(s(&before, "title")).to_string();
            let (root, nodes) = outline_to_nodes(&title, &o)?;
            part.insert("root_id".into(), json!(root));
            part.insert("nodes".into(), Value::Object(nodes));
        }
        self.update_doc(td, before, part)
    }

    fn update_doc(&mut self, td: &'static TypeDef, before: Map<String, Value>, part: Map<String, Value>) -> Result<Value> {
        let mut after = before.clone();
        let explicit_done_at = part.contains_key("done_at");
        for (k, v) in part {
            after.insert(k, v);
        }
        let id = s(&before, "id").to_string();
        match td.name {
            "task" => {
                let (was, now) = (s(&before, "status").to_string(), s(&after, "status").to_string());
                if now == "done" && (was != "done" || after.get("done_at").map_or(true, Value::is_null)) && !explicit_done_at {
                    after.insert("done_at".into(), json!(self.clock.now_iso()));
                }
                if now != "done" {
                    after.insert("done_at".into(), Value::Null);
                }
                let old: HashSet<String> = str_list(before.get("depends_on")).into_iter().collect();
                let new = str_list(after.get("depends_on"));
                if new.iter().any(|d| !old.contains(d)) {
                    self.check_deps(&id, &new)?;
                }
            }
            "link" => {
                if !self.may_confirm_links() && s(&before, "status") != "confirmed" && s(&after, "status") == "confirmed" {
                    return Err(Error::new("permission_denied", "only the user can accept a suggested link")
                        .hint("leave it as a suggestion; the user accepts it in 十一遗 (知识图谱 → 待确认的关联), or raises 设置 → AI 接入 → 权限 to 完全 (full)"));
                }
                if before.get("source") != after.get("source") || before.get("target") != after.get("target") || before.get("relation") != after.get("relation") {
                    if let Some(ex) = self.prepare_link(&id, &mut after)? {
                        return Err(Error::new("conflict", format!("an equivalent link already exists: {}", s(&ex, "id"))));
                    }
                }
            }
            _ => {}
        }
        let same = td.fields.iter().all(|f| before.get(f.name) == after.get(f.name));
        if same {
            let mut r = self.brief(&before);
            r["unchanged"] = json!(true);
            return Ok(r);
        }
        self.bump(&mut after);
        write_doc(self.tx, &after)?;
        self.log("update", td, &id, Some(&before), Some(&after))?;
        let changed: Vec<&str> = td.fields.iter().filter(|f| before.get(f.name) != after.get(f.name)).map(|f| f.name).collect();
        let mut r = self.brief(&after);
        r["updated"] = json!(changed);
        Ok(r)
    }

    fn status_shortcut(&mut self, td: Option<&'static TypeDef>, op: &Map<String, Value>, expect: &str, status: &str) -> Result<Value> {
        let td = td.or_else(|| model::type_def(expect));
        if td.map_or(true, |t| t.name != expect) {
            return Err(Error::new("unknown_op", format!("this shortcut works on {}s", expect)));
        }
        let mut o = Map::new();
        o.insert("id".into(), op.get("id").cloned().unwrap_or(Value::Null));
        if let Some(v) = op.get("if_version") {
            o.insert("if_version".into(), v.clone());
        }
        o.insert("status".into(), json!(status));
        self.update_op(td, &o)
    }

    // ---------- delete / restore / put ----------

    fn soft_delete(&mut self, td: &'static TypeDef, before: Map<String, Value>) -> Result<()> {
        let mut after = before.clone();
        after.insert("deleted_at".into(), json!(self.clock.now_iso()));
        self.bump(&mut after);
        write_doc(self.tx, &after)?;
        self.log("delete", td, s(&before, "id"), Some(&before), Some(&after))
    }

    fn strip_dependents(&mut self, task_id: &str) -> Result<usize> {
        let mut n = 0;
        for t in docs(self.tx, Some("task"), Deleted::Exclude)? {
            let deps = str_list(t.get("depends_on"));
            if deps.iter().any(|d| d == task_id) {
                let mut part = Map::new();
                part.insert("depends_on".into(), json!(deps.into_iter().filter(|d| d != task_id).collect::<Vec<_>>()));
                self.update_doc(&model::TASK, t, part)?;
                n += 1;
            }
        }
        Ok(n)
    }

    fn delete_op(&mut self, td: Option<&'static TypeDef>, op: &Map<String, Value>, guarded: bool) -> Result<Value> {
        let (before, td) = self.target(td, op)?;
        let id = s(&before, "id").to_string();
        if before.get("deleted_at").map_or(false, |d| !d.is_null()) {
            let mut r = self.brief(&before);
            r["already_deleted"] = json!(true);
            return Ok(r);
        }
        self.require_delete(&before)?;
        self.check_version(op, &before)?;
        let mut extra = vec![];
        match td.name {
            "project" => {
                let tasks: Vec<Map<String, Value>> = docs(self.tx, Some("task"), Deleted::Exclude)?.into_iter().filter(|t| s(t, "project_id") == id).collect();
                if !tasks.is_empty() && guarded && !self.is_user() {
                    if !truthy(op.get("cascade")) {
                        return Err(Error::new("has_tasks", format!("project {} still has {} tasks", id, tasks.len()))
                            .hint("pass \"cascade\":true to delete its tasks too, or move them first (task.update project_id)")
                            .data(json!({ "tasks": tasks.len() })));
                    }
                    for t in &tasks {
                        self.require_delete(t)?;
                    }
                }
                if truthy(op.get("cascade")) {
                    for t in tasks {
                        let tid = s(&t, "id").to_string();
                        self.soft_delete(&model::TASK, t)?;
                        extra.push(tid);
                    }
                }
            }
            "task" => {
                self.strip_dependents(&id)?;
            }
            _ => {}
        }
        let before = load_doc(self.tx, &id)?.unwrap_or(before);
        self.soft_delete(td, before.clone())?;
        let mut r = self.brief(&before);
        r["deleted"] = json!(true);
        if !extra.is_empty() {
            r["deleted_tasks"] = json!(extra);
        }
        Ok(r)
    }

    fn restore_op(&mut self, td: Option<&'static TypeDef>, op: &Map<String, Value>) -> Result<Value> {
        let (before, td) = self.target(td, op)?;
        let id = s(&before, "id").to_string();
        let Some(when) = before.get("deleted_at").and_then(Value::as_str).map(String::from) else {
            let mut r = self.brief(&before);
            r["not_deleted"] = json!(true);
            return Ok(r);
        };
        self.require_write()?;
        let mut after = before.clone();
        after.insert("deleted_at".into(), Value::Null);
        self.bump(&mut after);
        write_doc(self.tx, &after)?;
        self.log("restore", td, &id, Some(&before), Some(&after))?;
        let mut restored = vec![];
        if td.name == "project" {
            for t in docs(self.tx, Some("task"), Deleted::Only)? {
                if s(&t, "project_id") == id && s(&t, "deleted_at") == when {
                    let mut a = t.clone();
                    a.insert("deleted_at".into(), Value::Null);
                    self.bump(&mut a);
                    write_doc(self.tx, &a)?;
                    self.log("restore", &model::TASK, s(&t, "id"), Some(&t), Some(&a))?;
                    restored.push(s(&t, "id").to_string());
                }
            }
        }
        let mut r = self.brief(&after);
        r["restored"] = json!(true);
        if !restored.is_empty() {
            r["restored_tasks"] = json!(restored);
        }
        Ok(r)
    }

    /// GUI write: create with the given id, or replace the fields of an existing one.
    fn put_op(&mut self, td: Option<&'static TypeDef>, op: &Map<String, Value>) -> Result<Value> {
        let doc_in: Map<String, Value> = match op.get("doc") {
            Some(Value::Object(o)) => o.clone(),
            _ => Self::fields_of(op),
        };
        let id = op.get("id").and_then(Value::as_str).or_else(|| doc_in.get("id").and_then(Value::as_str)).map(String::from);
        let td = td
            .or_else(|| doc_in.get("type").and_then(Value::as_str).and_then(model::type_def))
            .or_else(|| id.as_deref().and_then(model::type_of_id))
            .ok_or_else(|| Error::new("invalid_request", "put needs a type").hint("pass \"type\":\"task\""))?;
        let Some(id) = id else {
            let mut o = doc_in.clone();
            o.insert("op".into(), json!(format!("{}.create", td.name)));
            return self.create(td, &o);
        };
        match load_doc(self.tx, &id)? {
            None => {
                let mut o = doc_in.clone();
                o.insert("id".into(), json!(id));
                self.create(td, &o)
            }
            Some(before) => {
                if s(&before, "type") != td.name {
                    return Err(Error::new("conflict", format!("id {} belongs to a {}", id, s(&before, "type"))));
                }
                self.require_write()?;
                let mut cur = before.clone();
                if before.get("deleted_at").map_or(false, |d| !d.is_null()) {
                    let mut o = Map::new();
                    o.insert("id".into(), json!(id));
                    self.restore_op(Some(td), &o)?;
                    cur = load_doc(self.tx, &id)?.unwrap_or(before);
                }
                let part = {
                    let me: &Self = self;
                    let mut full = model::normalize(td, &doc_in, true, &me.clock, &mut |t, v| me.resolve(t, v), me.lenient())?;
                    full.remove("created_at");
                    full
                };
                self.update_doc(td, cur, part)
            }
        }
    }

    // ---------- tasks ----------

    fn deps_map(&mut self) -> Result<&HashMap<String, Vec<String>>> {
        if self.task_deps.is_none() {
            let mut m = HashMap::new();
            for t in docs(self.tx, Some("task"), Deleted::Exclude)? {
                m.insert(s(&t, "id").to_string(), str_list(t.get("depends_on")));
            }
            self.task_deps = Some(m);
        }
        Ok(self.task_deps.as_ref().unwrap())
    }

    fn check_deps(&mut self, task_id: &str, deps: &[String]) -> Result<()> {
        if deps.iter().any(|d| d == task_id) {
            return Err(Error::new("cycle", "a task cannot depend on itself"));
        }
        if deps.is_empty() {
            return Ok(());
        }
        let graph = self.deps_map()?.clone();
        for d in deps {
            // does d (transitively) depend on task_id?
            let mut stack = vec![(d.clone(), vec![d.clone()])];
            let mut seen = HashSet::new();
            while let Some((x, path)) = stack.pop() {
                if x == task_id {
                    let mut names: Vec<String> = vec![task_id.to_string()];
                    names.extend(path.iter().cloned());
                    return Err(Error::new("cycle", format!("adding {} → {} would create a dependency loop", task_id, d))
                        .hint("remove one of the dependencies in the loop first")
                        .data(json!({ "loop": names })));
                }
                if !seen.insert(x.clone()) {
                    continue;
                }
                for y in graph.get(&x).cloned().unwrap_or_default() {
                    let mut p = path.clone();
                    p.push(y.clone());
                    stack.push((y, p));
                }
            }
        }
        Ok(())
    }

    fn dep_op(&mut self, op: &Map<String, Value>, add: bool) -> Result<Value> {
        let (before, td) = self.target(Some(&model::TASK), op)?;
        self.require_write()?;
        let raw = op.get("on").or_else(|| op.get("depends_on")).or_else(|| op.get("dep")).or_else(|| op.get("after")).cloned().ok_or_else(|| {
            Error::new("missing_field", "needs \"on\": the task id(s) it depends on").hint("{\"op\":\"task.add_dep\",\"id\":\"task_b\",\"on\":\"task_a\"}")
        })?;
        let ids = {
            let me: &Self = self;
            model::coerce(model::field(&model::TASK, "depends_on").unwrap(), &raw, &me.clock, &mut |t, v| me.resolve(t, v))?
        };
        let ids = str_list(Some(&ids));
        let mut deps = str_list(before.get("depends_on"));
        if add {
            for d in ids {
                if !deps.contains(&d) {
                    deps.push(d);
                }
            }
        } else {
            deps.retain(|d| !ids.contains(d));
        }
        let mut part = Map::new();
        part.insert("depends_on".into(), json!(deps));
        self.update_doc(td, before, part)
    }

    fn schedule_op(&mut self, op: &Map<String, Value>) -> Result<Value> {
        let (project, _) = self.target(Some(&model::PROJECT), op)?;
        self.require_write()?;
        let pid = s(&project, "id").to_string();
        let force = truthy(op.get("force"));
        let today = self.clock.today();
        let base = match project.get("start_date").and_then(Value::as_str) {
            Some(st) if crate::time::is_date(st) && st > today.as_str() => st.to_string(),
            _ => today.clone(),
        };
        let tasks: Vec<Map<String, Value>> = docs(self.tx, Some("task"), Deleted::Exclude)?.into_iter().filter(|t| s(t, "project_id") == pid && s(t, "status") != "done").collect();
        let ids: HashSet<String> = tasks.iter().map(|t| s(t, "id").to_string()).collect();
        let by_id: HashMap<String, Map<String, Value>> = tasks.iter().map(|t| (s(t, "id").to_string(), t.clone())).collect();
        let mut indeg: HashMap<String, usize> = ids.iter().map(|i| (i.clone(), 0)).collect();
        let mut succ: HashMap<String, Vec<String>> = HashMap::new();
        for t in &tasks {
            for d in str_list(t.get("depends_on")).into_iter().filter(|d| ids.contains(d)) {
                *indeg.get_mut(s(t, "id")).unwrap() += 1;
                succ.entry(d).or_default().push(s(t, "id").to_string());
            }
        }
        let mut queue: Vec<Map<String, Value>> = tasks.iter().filter(|t| indeg[s(t, "id")] == 0).cloned().collect();
        queue.sort_by(task_order);
        let mut q: std::collections::VecDeque<String> = queue.iter().map(|t| s(t, "id").to_string()).collect();
        let mut order = vec![];
        while let Some(id) = q.pop_front() {
            order.push(id.clone());
            for x in succ.get(&id).cloned().unwrap_or_default() {
                let e = indeg.get_mut(&x).unwrap();
                *e -= 1;
                if *e == 0 {
                    q.push_back(x);
                }
            }
        }
        let mut ef: HashMap<String, i64> = HashMap::new();
        let mut changed = vec![];
        for id in &order {
            let t = &by_id[id];
            let d = duration_days(t);
            let es = str_list(t.get("depends_on")).iter().filter_map(|x| ef.get(x)).copied().max().unwrap_or(0).max(0);
            let start = t.get("start_date").and_then(Value::as_str).filter(|x| crate::time::is_date(x));
            if !force {
                if let Some(st) = start {
                    let off = crate::time::day_diff(&base, st).unwrap_or(0).max(0);
                    let len = match t.get("due_date").and_then(Value::as_str).filter(|x| crate::time::is_date(x)) {
                        Some(due) => (crate::time::day_diff(st, due).unwrap_or(0) + 1).max(1),
                        None => d,
                    };
                    ef.insert(id.clone(), off + len);
                    continue;
                }
            }
            let ns = crate::time::add_days(&base, es).unwrap_or_else(|| base.clone());
            let mut nd = crate::time::add_days(&ns, d - 1).unwrap_or_else(|| ns.clone());
            // keep an existing deadline when the work fits before it
            if let (false, Some(due)) = (force, t.get("due_date").and_then(Value::as_str).filter(|x| crate::time::is_date(x))) {
                if due > nd.as_str() {
                    nd = due.to_string();
                }
            }
            if t.get("start_date").and_then(Value::as_str) != Some(ns.as_str()) || t.get("due_date").and_then(Value::as_str) != Some(nd.as_str()) {
                let mut part = Map::new();
                part.insert("start_date".into(), json!(ns));
                part.insert("due_date".into(), json!(nd));
                let cur = load_doc(self.tx, id)?.unwrap_or_else(|| t.clone());
                self.update_doc(&model::TASK, cur, part)?;
                changed.push(json!({ "id": id, "title": s(t, "title"), "start_date": ns, "due_date": nd }));
            }
            ef.insert(id.clone(), es + d);
        }
        Ok(json!({ "id": pid, "scheduled": changed.len(), "tasks": changed, "cyclic": order.len() < tasks.len(), "base": base }))
    }

    // ---------- ideas ----------

    fn convert_op(&mut self, op: &Map<String, Value>) -> Result<Value> {
        let (idea, _) = self.target(Some(&model::IDEA), op)?;
        self.require_write()?;
        let iid = s(&idea, "id").to_string();
        let to = op.get("to").and_then(Value::as_str).unwrap_or("task").to_lowercase();
        let text = s(&idea, "text").to_string();
        let head = clip(first_line(&text), 140);
        let rest: String = text.lines().skip_while(|l| l.trim().is_empty()).skip(1).collect::<Vec<_>>().join("\n").trim().to_string();
        let note = [rest.as_str(), s(&idea, "note")].iter().filter(|x| !x.trim().is_empty()).cloned().collect::<Vec<_>>().join("\n\n");
        let (td, mut o): (&'static TypeDef, Map<String, Value>) = match to.as_str() {
            "task" | "任务" => {
                if let Some(t) = idea.get("task_id").and_then(Value::as_str).filter(|_| s(&idea, "status") == "converted") {
                    return Ok(json!({ "id": iid, "already_converted": true, "task_id": t }));
                }
                let mut o = Map::new();
                o.insert("title".into(), op.get("title").cloned().unwrap_or(json!(head)));
                if let Some(p) = op.get("project").or_else(|| op.get("project_id")) {
                    o.insert("project_id".into(), p.clone());
                }
                for k in ["due_date", "due", "priority", "start_date", "today_date", "estimate"] {
                    if let Some(v) = op.get(k) {
                        o.insert(k.into(), v.clone());
                    }
                }
                o.insert("note".into(), json!(note));
                (&model::TASK, o)
            }
            "project" | "项目" => {
                if let Some(p) = idea.get("project_id").and_then(Value::as_str).filter(|_| s(&idea, "status") == "converted") {
                    return Ok(json!({ "id": iid, "already_converted": true, "project_id": p }));
                }
                let mut o = Map::new();
                o.insert("name".into(), op.get("name").or_else(|| op.get("title")).cloned().unwrap_or(json!(head)));
                o.insert("area".into(), op.get("area").cloned().unwrap_or(json!("work")));
                o.insert("goal".into(), op.get("goal").cloned().unwrap_or(json!(rest)));
                o.insert("note".into(), json!(s(&idea, "note")));
                o.insert("start_date".into(), json!(self.clock.today()));
                if let Some(v) = op.get("due_date").or_else(|| op.get("due")) {
                    o.insert("due_date".into(), v.clone());
                }
                (&model::PROJECT, o)
            }
            _ => return Err(Error::new("invalid_value", format!("cannot convert an idea to {:?}", to)).hint("\"to\": \"task\" or \"project\"")),
        };
        o.insert("from_idea".into(), json!(iid));
        o.insert("op".into(), json!(format!("{}.create", td.name)));
        let made = self.create(td, &o)?;
        let new_id = made["id"].as_str().unwrap_or("").to_string();
        let mut part = Map::new();
        part.insert("status".into(), json!("converted"));
        part.insert(if td.name == "task" { "task_id" } else { "project_id" }.into(), json!(new_id));
        let cur = load_doc(self.tx, &iid)?.unwrap_or(idea);
        self.update_doc(&model::IDEA, cur, part)?;
        self.register_ref(op, &new_id);
        Ok(json!({ "id": iid, "converted_to": { "type": td.name, "id": new_id, "title": made["title"] } }))
    }

    // ---------- undo ----------

    fn undo(&mut self, op: &Map<String, Value>) -> Result<Value> {
        let force = truthy(op.get("force"));
        let target: (bool, String) = if let Some(b) = op.get("batch").and_then(Value::as_str) {
            (true, b.to_string())
        } else if let Some(o) = op.get("op_id").and_then(Value::as_str) {
            (false, o.to_string())
        } else if let Some(id) = op.get("id").and_then(Value::as_str).or_else(|| op.get("target").and_then(Value::as_str)) {
            if id.starts_with("op_") {
                (false, id.to_string())
            } else if id == "last" {
                (true, self.last_batch()?)
            } else {
                (true, id.to_string())
            }
        } else if op.get("last").map_or(false, |v| truthy(Some(v)) || v.as_str() == Some("last")) {
            (true, self.last_batch()?)
        } else {
            return Err(Error::new("missing_field", "undo needs a batch, an op id, or last:true").hint("{\"op\":\"undo\",\"batch\":\"bat_…\"} — batch ids are in apply results and `shiyi log`"));
        };
        let (by_batch, key) = target;
        let sql = if by_batch {
            "SELECT seq,actor,entity_type,entity_id,before,after,undone_by FROM ops WHERE batch=?1 ORDER BY seq"
        } else {
            "SELECT seq,actor,entity_type,entity_id,before,after,undone_by FROM ops WHERE id=?1 ORDER BY seq"
        };
        type Row = (i64, String, String, String, Option<String>, Option<String>, Option<String>);
        let rows: Vec<Row> = {
            let mut st = self.tx.prepare(sql)?;
            let it = st.query_map([&key], |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<String>>(2)?.unwrap_or_default(), r.get::<_, Option<String>>(3)?.unwrap_or_default(), r.get(4)?, r.get(5)?, r.get(6)?)))?;
            it.collect::<std::result::Result<Vec<_>, _>>()?
        };
        if rows.is_empty() {
            return Err(Error::new("not_found", format!("no batch or op {}", key)).hint("see `shiyi log` for batch ids"));
        }
        let live: Vec<&Row> = rows.iter().filter(|r| r.6.is_none()).collect();
        if live.is_empty() {
            return Err(Error::new("already_undone", format!("{} was already undone by {}", key, rows[0].6.clone().unwrap_or_default())).hint("undo that batch to redo it"));
        }
        if !self.is_user() {
            self.require_write()?;
            if live.iter().any(|r| !r.1.starts_with("agent:")) && self.perm < Perm::Full {
                return Err(Error::new("permission_denied", "agents with write permission can only undo changes made by agents").hint("ask the user to undo it in 十一遗 (操作记录), or to raise the permission to 完全 (full)"));
            }
        }
        // per entity: earliest before, latest after
        let mut order: Vec<String> = vec![];
        let mut first_before: HashMap<String, Option<String>> = HashMap::new();
        let mut last_after: HashMap<String, Option<String>> = HashMap::new();
        let mut etype: HashMap<String, String> = HashMap::new();
        for r in &live {
            if !first_before.contains_key(&r.3) {
                order.push(r.3.clone());
                first_before.insert(r.3.clone(), r.4.clone());
                etype.insert(r.3.clone(), r.2.clone());
            }
            last_after.insert(r.3.clone(), r.5.clone());
        }
        let ver = |t: &Option<String>| -> Option<i64> { t.as_ref().and_then(|x| serde_json::from_str::<Value>(x).ok()).and_then(|v| v.get("version").and_then(Value::as_i64)) };
        let content = |d: &Map<String, Value>| -> Map<String, Value> {
            d.iter().filter(|(k, _)| !matches!(k.as_str(), "version" | "updated_at" | "updated_by")).map(|(k, v)| (k.clone(), v.clone())).collect()
        };
        let mut conflicts = vec![];
        for e in &order {
            if etype[e] == "setting" {
                continue;
            }
            let cur = load_doc(self.tx, e)?;
            let cur_v = cur.as_ref().and_then(|d| d.get("version").and_then(Value::as_i64));
            let exp_v = ver(&last_after[e]);
            let expected = last_after[e].as_ref().map(|x| crate::db::parse_doc(x)).transpose()?;
            let same = match (&cur, &expected) {
                (None, None) => true,
                (Some(a), Some(b)) => cur_v == exp_v || content(a) == content(b),
                _ => false,
            };
            if !same {
                conflicts.push(json!({
                    "id": e,
                    "title": cur.as_ref().map(model::title_of),
                    "expected_version": exp_v,
                    "current_version": cur_v,
                    "changed_by": cur.as_ref().and_then(|d| d.get("updated_by").cloned())
                }));
            }
        }
        if !conflicts.is_empty() && !force {
            return Err(Error::new("conflict", format!("{} item(s) changed after {}; undoing would overwrite those later changes", conflicts.len(), key))
                .hint("undo the later batches first, or pass force:true to restore anyway")
                .data(json!({ "conflicts": conflicts })));
        }
        let mut touched = vec![];
        for e in order.iter().rev() {
            if etype[e] == "setting" {
                if !self.is_user() {
                    return Err(Error::new("permission_denied", "only the user can change settings"));
                }
                let val = first_before[e].as_ref().and_then(|b| serde_json::from_str::<Value>(b).ok()).and_then(|v| v.get("value").and_then(Value::as_str).map(String::from));
                if let Some(v) = val {
                    Core::meta_set(self.tx, e, &v)?;
                }
                continue;
            }
            let td = model::type_def(&etype[e]).ok_or_else(|| Error::new("db_error", "unknown type in log"))?;
            let cur = load_doc(self.tx, e)?;
            match &first_before[e] {
                None => {
                    if let Some(c) = cur {
                        delete_row(self.tx, e)?;
                        self.log("undo", td, e, Some(&c), None)?;
                    }
                }
                Some(b) => {
                    let mut restored = crate::db::parse_doc(b)?;
                    let v = cur.as_ref().and_then(|d| d.get("version").and_then(Value::as_i64)).unwrap_or_else(|| restored.get("version").and_then(Value::as_i64).unwrap_or(1));
                    restored.insert("version".into(), json!(v));
                    self.bump(&mut restored);
                    write_doc(self.tx, &restored)?;
                    self.log("undo", td, e, cur.as_ref(), Some(&restored))?;
                }
            }
            touched.push(json!(e));
        }
        let col = if by_batch { "batch" } else { "id" };
        self.tx.execute(&format!("UPDATE ops SET undone_by=?1 WHERE {}=?2 AND undone_by IS NULL AND batch<>?1", col), params![self.batch, key])?;
        Ok(json!({ "undone": key, "entities": touched, "forced": force && !conflicts.is_empty() }))
    }

    fn last_batch(&self) -> Result<String> {
        let b: Option<String> = self
            .tx
            .query_row(
                "SELECT batch FROM ops WHERE actor=?1 AND undone_by IS NULL AND op NOT LIKE '%.undo' ORDER BY seq DESC LIMIT 1",
                [&self.actor],
                |r| r.get(0),
            )
            .optional()?;
        b.ok_or_else(|| Error::new("not_found", format!("{} has no change left to undo", self.actor)))
    }

    // ---------- settings ----------

    fn settings_set(&mut self, op: &Map<String, Value>) -> Result<Value> {
        if !self.is_user() {
            return Err(Error::new("permission_denied", "only the user can change settings").hint("ask the user to change it in 设置 → AI 接入"));
        }
        let key = op.get("key").and_then(Value::as_str).unwrap_or("");
        let val = op.get("value").and_then(Value::as_str).unwrap_or("");
        match key {
            "agent_permission" => {
                let p = Perm::parse(val).ok_or_else(|| Error::new("invalid_value", "agent_permission must be read, write or full"))?;
                let old: Option<String> = self.tx.query_row("SELECT value FROM meta WHERE key=?1", [key], |r| r.get(0)).optional()?;
                Core::meta_set(self.tx, key, p.as_str())?;
                self.tx.execute(
                    "INSERT INTO ops(id,batch,ts,ts_ms,actor,op,entity_type,entity_id,before,after,summary,note) VALUES(?1,?2,?3,?4,?5,'settings.set','setting',?6,?7,?8,?9,?10)",
                    params![
                        new_id("op"),
                        self.batch,
                        self.clock.now_iso(),
                        self.clock.now_ms,
                        self.actor,
                        key,
                        json!({ "value": old }).to_string(),
                        json!({ "value": p.as_str() }).to_string(),
                        format!("AI 权限改为「{}」", match p { Perm::Read => "只读", Perm::Write => "可写", Perm::Full => "完全" }),
                        self.note
                    ],
                )?;
                self.logged += 1;
                Ok(json!({ "key": key, "value": p.as_str() }))
            }
            _ => Err(Error::new("invalid_value", format!("unknown setting {:?}", key)).hint("settings: agent_permission")),
        }
    }
}

pub(crate) fn task_order(a: &Map<String, Value>, b: &Map<String, Value>) -> std::cmp::Ordering {
    let so = |t: &Map<String, Value>| match s(t, "status") {
        "doing" => 0,
        "todo" => 1,
        _ => 2,
    };
    let pr = |t: &Map<String, Value>| match t.get("priority").and_then(Value::as_i64).unwrap_or(0) {
        0 => 9,
        p => p,
    };
    let due = |t: &Map<String, Value>| t.get("due_date").and_then(Value::as_str).unwrap_or("9999").to_string();
    so(a).cmp(&so(b)).then(pr(a).cmp(&pr(b))).then(due(a).cmp(&due(b))).then(s(a, "created_at").cmp(s(b, "created_at")))
}

/// PERT expected duration (days) and variance.
pub(crate) fn pert(t: &Map<String, Value>) -> Option<(f64, f64)> {
    let e = t.get("estimate")?.as_object()?;
    let g = |k: &str| e.get(k).and_then(Value::as_f64);
    let (o, m, p) = (g("optimistic"), g("likely"), g("pessimistic"));
    if o.is_none() && m.is_none() && p.is_none() {
        return None;
    }
    let known: Vec<f64> = [o, p].iter().flatten().copied().collect();
    let mm = m.unwrap_or_else(|| known.iter().sum::<f64>() / known.len() as f64);
    let oo = o.unwrap_or(mm);
    let pp = p.unwrap_or(mm);
    Some((((oo + 4.0 * mm + pp) / 6.0).max(0.0), ((pp - oo) / 6.0).powi(2)))
}

pub(crate) fn duration_days(t: &Map<String, Value>) -> i64 {
    pert(t).map(|(te, _)| te.round() as i64).unwrap_or(1).max(1)
}

fn node(text: &str, parent: Option<&str>) -> (String, Value) {
    let id = format!("n{}", short_id());
    (id.clone(), json!({ "id": id, "text": text, "parent": parent, "children": [], "collapsed": false, "color": null, "link": null }))
}

/// Describes how a mind map's `nodes` changed, for the operation log (e.g. `添加节点「预算」`).
fn describe_node_changes(before: &Value, after: &Value) -> String {
    const FIELDS: [&str; 7] = ["id", "text", "parent", "children", "collapsed", "color", "link"];
    let empty = Map::new();
    let b = before.as_object().unwrap_or(&empty);
    let a = after.as_object().unwrap_or(&empty);
    let text = |n: &Value| n.get("text").and_then(Value::as_str).unwrap_or("").to_string();
    let label = |n: &Value| {
        let t = clip(&text(n), 16);
        if t.is_empty() {
            "未命名".to_string()
        } else {
            t
        }
    };
    let mut ids: Vec<&String> = b.keys().chain(a.keys()).collect();
    ids.sort();
    ids.dedup();
    let (mut added, mut removed, mut renamed, mut moved) = (vec![], vec![], vec![], vec![]);
    // per node: which of FIELDS differ (only consulted when no text or parent change exists)
    let mut shapes: Vec<Vec<&str>> = vec![];
    for id in ids {
        match (b.get(id), a.get(id)) {
            (None, Some(n)) => added.push(label(n)),
            (Some(n), None) => removed.push(label(n)),
            (Some(x), Some(y)) => {
                if text(x) != text(y) {
                    renamed.push((label(x), label(y)));
                }
                if x.get("parent").unwrap_or(&Value::Null) != y.get("parent").unwrap_or(&Value::Null) {
                    moved.push(label(y));
                }
                let diff: Vec<&str> = FIELDS.iter().copied().filter(|k| x.get(*k).unwrap_or(&Value::Null) != y.get(*k).unwrap_or(&Value::Null)).collect();
                if !diff.is_empty() {
                    shapes.push(diff);
                }
            }
            (None, None) => {}
        }
    }
    let mut parts: Vec<String> = vec![];
    match added.len() {
        0 => {}
        1 => parts.push(format!("添加节点「{}」", added[0])),
        n => parts.push(format!("添加 {} 个节点", n)),
    }
    match removed.len() {
        0 => {}
        1 => parts.push(format!("删除节点「{}」", removed[0])),
        n => parts.push(format!("删除 {} 个节点", n)),
    }
    match renamed.as_slice() {
        [] => {}
        [(from, to)] => parts.push(format!("「{}」改为「{}」", from, to)),
        r => parts.push(format!("改了 {} 个节点的文字", r.len())),
    }
    match moved.len() {
        0 => {}
        1 => parts.push(format!("移动节点「{}」", moved[0])),
        n => parts.push(format!("移动 {} 个节点", n)),
    }
    if !parts.is_empty() {
        return parts.join("、");
    }
    if shapes.iter().any(|d| matches!(d.as_slice(), ["collapsed"])) {
        "折叠或展开节点".to_string()
    } else if shapes.iter().any(|d| d.contains(&"link")) {
        "关联到任务或灵感".to_string()
    } else {
        "调整节点".to_string()
    }
}

/// Builds mind map nodes from an indented outline (text or nested JSON).
pub(crate) fn outline_to_nodes(title: &str, outline: &Value) -> Result<(String, Map<String, Value>)> {
    // flatten to (depth, text)
    let mut items: Vec<(usize, String)> = vec![];
    fn walk(v: &Value, depth: usize, out: &mut Vec<(usize, String)>) -> Result<()> {
        match v {
            Value::String(s) => out.push((depth, s.trim().to_string())),
            Value::Array(a) => {
                for x in a {
                    walk(x, depth, out)?;
                }
            }
            Value::Object(o) => {
                let t = o.get("text").or_else(|| o.get("title")).and_then(Value::as_str).unwrap_or("").trim().to_string();
                out.push((depth, t));
                if let Some(c) = o.get("children") {
                    walk(c, depth + 1, out)?;
                }
            }
            Value::Null => {}
            _ => return Err(Error::new("invalid_value", "outline items must be text or {text, children}")),
        }
        Ok(())
    }
    match outline {
        Value::String(text) => {
            let mut widths: Vec<usize> = vec![];
            for line in text.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                let indent: usize = line.chars().take_while(|c| c.is_whitespace()).map(|c| if c == '\t' { 4 } else { 1 }).sum();
                while widths.last().map_or(false, |w| *w > indent) {
                    widths.pop();
                }
                if widths.last().map_or(true, |w| *w < indent) {
                    widths.push(indent);
                }
                let depth = widths.len() - 1;
                let mut t = line.trim();
                for b in ["- ", "* ", "+ ", "• ", "· "] {
                    if let Some(r) = t.strip_prefix(b) {
                        t = r.trim();
                    }
                }
                let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
                if digits > 0 && (t[digits..].starts_with(". ") || t[digits..].starts_with("、")) {
                    t = t[digits..].trim_start_matches(['.', '、']).trim();
                }
                items.push((depth, t.to_string()));
            }
        }
        other => walk(other, 0, &mut items)?,
    }
    items.retain(|(_, t)| !t.is_empty());
    if items.len() > 2000 {
        return Err(Error::new("invalid_value", "an outline can have at most 2000 nodes"));
    }
    let mut nodes = Map::new();
    let tops = items.iter().filter(|(d, _)| *d == 0).count();
    let title = title.trim();
    let (root_id, root_val, shift) = if !title.is_empty() && !(tops == 1 && items.first().map_or(false, |(_, t)| t == title)) {
        let (id, v) = node(title, None);
        (id, v, 1usize)
    } else if let Some((_, t)) = items.first().filter(|_| tops == 1) {
        let (id, v) = node(t, None);
        items.remove(0);
        (id, v, 0usize)
    } else {
        let (id, v) = node(if title.is_empty() { "中心主题" } else { title }, None);
        (id, v, 1usize)
    };
    nodes.insert(root_id.clone(), root_val);
    // stack[depth] = node id at that depth (root at depth 0 of the tree)
    let mut stack: Vec<String> = vec![root_id.clone()];
    for (d, text) in items {
        let depth = d + shift; // tree depth of this node (>=1)
        let depth = depth.max(1).min(stack.len());
        stack.truncate(depth);
        let parent = stack.last().cloned().unwrap_or_else(|| root_id.clone());
        let (id, v) = node(&text, Some(&parent));
        nodes.insert(id.clone(), v);
        if let Some(Value::Array(ch)) = nodes.get_mut(&parent).and_then(|p| p.get_mut("children")) {
            ch.push(json!(id));
        }
        stack.push(id);
    }
    Ok((root_id, nodes))
}

fn validate_nodes(f: &Map<String, Value>) -> Result<()> {
    let root = s(f, "root_id");
    let nodes = f.get("nodes").and_then(Value::as_object).ok_or_else(|| Error::new("invalid_value", "nodes must be an object"))?;
    if root.is_empty() || !nodes.contains_key(root) {
        return Err(Error::new("invalid_value", "nodes need root_id pointing at the root node").hint("easier: pass \"outline\" as indented text instead of nodes"));
    }
    for (k, n) in nodes {
        if n.get("text").and_then(Value::as_str).is_none() || n.get("id").and_then(Value::as_str) != Some(k.as_str()) {
            return Err(Error::new("invalid_value", format!("node {} needs id and text", k)).hint("easier: pass \"outline\" as indented text instead of nodes"));
        }
    }
    Ok(())
}
