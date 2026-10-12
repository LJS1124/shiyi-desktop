//! `shiyi` — the command line interface. JSON in, JSON out.

use crate::db::{default_db_path, Core};
use crate::model;
use crate::{Error, Result};
use serde_json::{json, Map, Value};
use std::io::{IsTerminal, Read, Write};

const HELP: &str = r#"十一遗 shiyi — ideas, projects, tasks and a knowledge graph, for humans and AI agents.
Output is JSON: {"ok":true,…} or {"ok":false,"error":{"code","message","hint"}}. Exit 0 ok, 1 error, 2 usage.

READ
  shiyi today                               what matters today (overdue, planned, doing, inbox, projects)
  shiyi list <type> [filters]               type: ideas | projects | tasks | maps | links
        --status todo,doing  --project <id|name|none>  --tag t  --text words  --due-before fri
        --today  --open  --starred  --sort smart|due|updated|created|priority  --limit 50  --full
  shiyi get <id|exact title>                one item with its relations, backlinks and stats
  shiyi search <words…> [--type task]       full-text search over everything
  shiyi graph [--center <id>] [--depth 2] [--types idea,project] [--no-tags]
  shiyi log [--actor agents|user|<name>] [--entity <id>] [--limit 20] [--detail]
  shiyi trash                               soft-deleted items
  shiyi schema [type]                       fields, ops and conventions
  shiyi status | doctor

WRITE (every write prints a batch id; `shiyi undo <batch>` reverts it)
  shiyi idea add <text…> [--tag t]… [--note n] [--project p] [--star]
  shiyi task add <title…> [--project p] [--due d] [--start d] [--priority 1-3] [--estimate 2,3,5] [--after <task>]… [--today]
  shiyi project add <name…> [--area work|life] [--due d] [--goal g]
  shiyi map add <title…> --outline <text|-|@file> [--idea id] [--project id]
  shiyi link add <source> <target> [--rel relates_to] [--note why] [--suggest]
  shiyi <type> set <id> --<field> <value>…  e.g. shiyi task set task_x --due +3d --status doing
  shiyi task done|start|reopen <id>…
  shiyi task dep <id> --on <task>… | --remove <task>…
  shiyi idea convert <id> --to task|project [--project p]
  shiyi idea archive <id>
  shiyi project schedule <id> [--force]
  shiyi link confirm|reject <id>
  shiyi <type> delete <id> [--cascade]   |   shiyi <type> restore <id>
  shiyi apply '<json>' | - | @file          batch: {"ops":[{"op":"task.create","ref":"t1",…},{"op":"task.add_dep","id":"…","on":"$t1"}]}
  shiyi undo [<batch>|<op>|last]
  shiyi query '<json>'                      e.g. {"q":"list","type":"task","status":"todo"}

OTHER
  shiyi skill [install [--dir <skills dir>]]   print or install the agent skill (SKILL.md)
  shiyi export [file]  |  shiyi import <file>

GLOBAL FLAGS
  --as <agent name>   who is acting (default: $SHIYI_AGENT or "cli"); shows up in 十一遗's 操作记录
  --dry-run           validate and preview without saving
  --db <path>         database file (default: $SHIYI_DB or the app's data folder)
  --pretty | --compact

Dates: YYYY-MM-DD, today, tomorrow, +3d, +1w, fri, next mon, 周五, 下周一.
References: an id, an exact title, or "$ref" of an earlier op in the same apply.
"#;

const BOOL_FLAGS: &[&str] = &[
    "today", "no-today", "force", "cascade", "full", "open", "starred", "star", "deleted", "detail", "suggest", "suggested", "confirm", "dry-run", "pretty", "compact",
    "no-tags", "no-suggested", "help", "unstar", "print",
];

struct Args {
    pos: Vec<String>,
    flags: Vec<(String, Value)>,
}

impl Args {
    fn parse(tokens: &[String]) -> Args {
        let mut pos = vec![];
        let mut flags = vec![];
        let mut i = 0;
        while i < tokens.len() {
            let t = &tokens[i];
            if let Some(name) = t.strip_prefix("--") {
                if name.is_empty() {
                    pos.extend(tokens[i + 1..].iter().cloned());
                    break;
                }
                if let Some((k, v)) = name.split_once('=') {
                    flags.push((k.to_string(), json!(v)));
                } else if BOOL_FLAGS.contains(&name) || i + 1 >= tokens.len() || tokens[i + 1].starts_with("--") {
                    flags.push((name.to_string(), json!(true)));
                } else {
                    flags.push((name.to_string(), json!(tokens[i + 1])));
                    i += 1;
                }
            } else {
                pos.push(t.clone());
            }
            i += 1;
        }
        Args { pos, flags }
    }
    fn get(&self, k: &str) -> Option<&Value> {
        self.flags.iter().rev().find(|(n, _)| n == k).map(|(_, v)| v)
    }
    fn str(&self, k: &str) -> Option<String> {
        self.get(k).and_then(|v| v.as_str().map(String::from))
    }
    fn has(&self, k: &str) -> bool {
        self.get(k).is_some()
    }
    fn all(&self, k: &str) -> Vec<Value> {
        self.flags.iter().filter(|(n, _)| n == k).map(|(_, v)| v.clone()).collect()
    }
    /// Remaining flags as a field map (kebab-case → snake_case, repeated flags → arrays).
    fn fields(&self, skip: &[&str]) -> Map<String, Value> {
        let mut m = Map::new();
        for (k, v) in &self.flags {
            if skip.contains(&k.as_str()) || ["as", "db", "dry-run", "pretty", "compact", "agent"].contains(&k.as_str()) {
                continue;
            }
            let key = k.replace('-', "_");
            match m.get_mut(&key) {
                Some(Value::Array(a)) => a.push(v.clone()),
                Some(prev) => {
                    let p = prev.clone();
                    *prev = json!([p, v]);
                }
                None => {
                    m.insert(key, v.clone());
                }
            }
        }
        m
    }
}

fn usage(msg: impl Into<String>) -> Error {
    Error::new("usage", msg).hint("run `shiyi help`")
}

fn read_input(src: &str) -> Result<String> {
    if src == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else if let Some(path) = src.strip_prefix('@') {
        Ok(std::fs::read_to_string(path)?)
    } else {
        Ok(src.to_string())
    }
}

fn parse_json(src: &str) -> Result<Value> {
    let text = read_input(src)?;
    serde_json::from_str(&text).map_err(|e| Error::new("invalid_json", e.to_string()).hint("pass valid JSON, - for stdin, or @file"))
}

pub struct Session {
    pub db: Option<String>,
    pub actor: String,
    pub dry_run: bool,
}

impl Session {
    fn open(&self) -> Result<Core> {
        match &self.db {
            Some(p) => Core::open(p),
            None => Core::open(default_db_path()),
        }
    }
    fn apply(&self, mut req: Value) -> Result<Value> {
        if self.dry_run {
            match &mut req {
                Value::Object(o) if o.contains_key("ops") => {
                    o.insert("dry_run".into(), json!(true));
                }
                Value::Object(o) => {
                    let op = Value::Object(o.clone());
                    req = json!({ "ops": [op], "dry_run": true });
                }
                Value::Array(a) => {
                    let a = a.clone();
                    req = json!({ "ops": a, "dry_run": true });
                }
                _ => {}
            }
        }
        self.open()?.apply(req, &self.actor)
    }
    fn query(&self, q: Value) -> Result<Value> {
        self.open()?.query(q)
    }
}

/// Entry point. `args` excludes the program name. Returns the exit code.
pub fn main_with_args(args: Vec<String>) -> i32 {
    let a = Args::parse(&args);
    let pretty = if a.has("compact") { false } else { a.has("pretty") || std::io::stdout().is_terminal() };
    if a.pos.is_empty() || a.pos[0] == "help" || (a.has("help") && a.pos.len() <= 1) {
        print!("{}", HELP);
        return 0;
    }
    let actor = a.str("as").or_else(|| a.str("agent")).or_else(|| std::env::var("SHIYI_AGENT").ok()).unwrap_or_else(|| "cli".into());
    let sess = Session { db: a.str("db"), actor, dry_run: a.has("dry-run") };
    let res = run(&sess, &a);
    let (out, code) = match res {
        Ok(Value::String(text)) => {
            print!("{}", text);
            return 0;
        }
        Ok(v) => (v, 0),
        Err(e) => {
            let code = if e.code == "usage" { 2 } else { 1 };
            (e.to_json(), code)
        }
    };
    let text = if pretty { serde_json::to_string_pretty(&out) } else { serde_json::to_string(&out) }.unwrap_or_default();
    let mut so = std::io::stdout().lock();
    let _ = writeln!(so, "{}", text);
    code
}

fn need<'a>(a: &'a Args, i: usize, what: &str) -> Result<&'a str> {
    a.pos.get(i).map(String::as_str).ok_or_else(|| usage(format!("missing {}", what)))
}

fn run(sess: &Session, a: &Args) -> Result<Value> {
    let cmd = a.pos[0].as_str();
    match cmd {
        "version" | "--version" | "-v" => Ok(json!({ "ok": true, "version": crate::VERSION })),
        "status" | "info" => sess.query(json!({ "q": "status" })),
        "doctor" => sess.query(json!({ "q": "doctor" })),
        "schema" => {
            let mut q = json!({ "q": "schema" });
            if let Some(t) = a.pos.get(1) {
                q["type"] = json!(t);
            }
            sess.query(q)
        }
        "today" | "agenda" => {
            let mut q = json!({ "q": "today" });
            if let Some(d) = a.str("date") {
                q["date"] = json!(d);
            }
            sess.query(q)
        }
        "list" | "ls" => {
            let ty = need(a, 1, "type (ideas, projects, tasks, maps, links)")?;
            let mut q = Value::Object(a.fields(&[]));
            q["q"] = json!("list");
            q["type"] = json!(ty);
            if a.pos.len() > 2 {
                q["text"] = json!(a.pos[2..].join(" "));
            }
            sess.query(q)
        }
        "trash" => {
            let mut q = Value::Object(a.fields(&[]));
            q["q"] = json!("trash");
            if let Some(t) = a.pos.get(1) {
                q["type"] = json!(t);
            }
            sess.query(q)
        }
        "get" | "show" => {
            let id = a.pos[1..].join(" ");
            if id.is_empty() {
                return Err(usage("missing id"));
            }
            sess.query(json!({ "q": "get", "id": id }))
        }
        "search" | "find" => {
            let text = a.pos[1..].join(" ");
            if text.is_empty() {
                return Err(usage("missing search words"));
            }
            let mut q = Value::Object(a.fields(&[]));
            q["q"] = json!("search");
            q["text"] = json!(text);
            sess.query(q)
        }
        "graph" => {
            let mut q = Value::Object(a.fields(&["no-tags", "no-suggested"]));
            q["q"] = json!("graph");
            if a.has("no-tags") {
                q["tags"] = json!(false);
            }
            if a.has("no-suggested") {
                q["suggested"] = json!(false);
            }
            if let Some(c) = a.pos.get(1) {
                q["center"] = json!(c);
            }
            for k in ["depth", "limit"] {
                if let Some(v) = a.str(k).and_then(|x| x.parse::<u64>().ok()) {
                    q[k] = json!(v);
                }
            }
            sess.query(q)
        }
        "log" | "history" | "ops" => {
            let mut q = Value::Object(a.fields(&[]));
            q["q"] = json!("log");
            if let Some(v) = a.str("limit").and_then(|x| x.parse::<u64>().ok()) {
                q["limit"] = json!(v);
            }
            sess.query(q)
        }
        "query" | "q" => {
            let src = need(a, 1, "query JSON")?;
            sess.query(parse_json(src)?)
        }
        "apply" => {
            let src = need(a, 1, "operations JSON (or - for stdin)")?;
            sess.apply(parse_json(src)?)
        }
        "undo" => {
            let target = a.pos.get(1).map(String::as_str).unwrap_or("last");
            let mut op = json!({ "op": "undo" });
            if target == "last" {
                op["last"] = json!(true);
            } else if target.starts_with("op_") {
                op["op_id"] = json!(target);
            } else {
                op["batch"] = json!(target);
            }
            if a.has("force") {
                op["force"] = json!(true);
            }
            sess.apply(json!({ "ops": [op] }))
        }
        "export" => {
            let snap = sess.query(json!({ "q": "snapshot" }))?;
            match a.pos.get(1) {
                Some(path) => {
                    std::fs::write(path, serde_json::to_string_pretty(&snap)?)?;
                    Ok(json!({ "ok": true, "file": path }))
                }
                None => Ok(snap),
            }
        }
        "import" => {
            let src = need(a, 1, "file")?;
            let data = parse_json(&if src.starts_with('@') || src == "-" { src.to_string() } else { format!("@{}", src) })?;
            sess.open()?.import(data, &sess.actor)
        }
        "skill" => skill(a),
        "permission" => sess.query(json!({ "q": "settings" })),
        _ => {
            if let Some(td) = model::type_def(cmd) {
                entity_cmd(sess, a, td)
            } else {
                Err(usage(format!("unknown command {:?}", cmd)))
            }
        }
    }
}

fn skill(a: &Args) -> Result<Value> {
    if a.pos.get(1).map(String::as_str) != Some("install") {
        return Ok(Value::String(crate::SKILL_MD.to_string()));
    }
    let dir = match a.str("dir") {
        Some(d) => std::path::PathBuf::from(d),
        None => {
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from).ok_or_else(|| Error::new("io_error", "HOME is not set").hint("pass --dir"))?;
            home.join(".claude").join("skills")
        }
    };
    let target = dir.join("shiyi");
    std::fs::create_dir_all(&target)?;
    let file = target.join("SKILL.md");
    std::fs::write(&file, crate::SKILL_MD)?;
    Ok(json!({ "ok": true, "installed": file.display().to_string() }))
}

fn entity_cmd(sess: &Session, a: &Args, td: &'static model::TypeDef) -> Result<Value> {
    let verb = a.pos.get(1).map(String::as_str).unwrap_or("list");
    let rest = || a.pos[2..].join(" ");
    let t = td.name;
    match verb {
        "list" | "ls" => {
            let mut q = Value::Object(a.fields(&[]));
            q["q"] = json!("list");
            q["type"] = json!(t);
            if a.pos.len() > 2 {
                q["text"] = json!(rest());
            }
            sess.query(q)
        }
        "get" | "show" => {
            let id = need(a, 2, "id")?;
            sess.query(json!({ "q": "get", "id": id }))
        }
        "add" | "create" | "new" => {
            let mut op = Map::new();
            op.insert("op".into(), json!(format!("{}.create", t)));
            let mut f = a.fields(&["today", "no-today", "star", "suggest", "suggested", "confirm", "outline", "ref", "id"]);
            if let Some(id) = a.str("id") {
                op.insert("id".into(), json!(id));
            }
            match t {
                "idea" => {
                    if a.pos.len() > 2 {
                        f.insert("text".into(), json!(rest()));
                    }
                    if a.has("star") {
                        f.insert("starred".into(), json!(true));
                    }
                }
                "task" => {
                    if a.pos.len() > 2 {
                        f.insert("title".into(), json!(rest()));
                    }
                    if a.has("today") {
                        f.insert("today_date".into(), json!("today"));
                    }
                }
                "project" => {
                    if a.pos.len() > 2 {
                        f.insert("name".into(), json!(rest()));
                    }
                }
                "map" => {
                    if a.pos.len() > 2 {
                        f.insert("title".into(), json!(rest()));
                    }
                    if let Some(o) = a.str("outline") {
                        op.insert("outline".into(), json!(read_input(&o)?));
                    }
                }
                "link" => {
                    let src = need(a, 2, "source")?;
                    let dst = need(a, 3, "target")?;
                    f.insert("source".into(), json!(src));
                    f.insert("target".into(), json!(dst));
                    if a.has("suggest") || a.has("suggested") {
                        f.insert("status".into(), json!("suggested"));
                    }
                }
                _ => {}
            }
            for (k, v) in f {
                op.insert(k, v);
            }
            sess.apply(json!({ "ops": [Value::Object(op)] }))
        }
        "set" | "update" | "edit" => {
            let id = need(a, 2, "id")?;
            let mut op = Map::new();
            op.insert("op".into(), json!(format!("{}.update", t)));
            op.insert("id".into(), json!(id));
            let mut f = a.fields(&["today", "no-today", "star", "unstar", "outline", "if-version"]);
            if a.has("today") {
                f.insert("today_date".into(), json!("today"));
            }
            if a.has("no-today") {
                f.insert("today_date".into(), Value::Null);
            }
            if a.has("star") {
                f.insert("starred".into(), json!(true));
            }
            if a.has("unstar") {
                f.insert("starred".into(), json!(false));
            }
            if let Some(o) = a.str("outline") {
                op.insert("outline".into(), json!(read_input(&o)?));
            }
            if let Some(v) = a.str("if-version").and_then(|x| x.parse::<i64>().ok()) {
                op.insert("if_version".into(), json!(v));
            }
            if a.pos.len() > 3 {
                return Err(usage(format!("unexpected {:?}; values go after --field flags, e.g. shiyi {} set {} --{} \"…\"", a.pos[3], t, id, td.title_field)));
            }
            for (k, v) in f {
                op.insert(k, v);
            }
            sess.apply(json!({ "ops": [Value::Object(op)] }))
        }
        "delete" | "rm" | "remove" | "restore" | "done" | "start" | "reopen" | "archive" | "confirm" | "reject" => {
            if a.pos.len() < 3 {
                return Err(usage("missing id"));
            }
            let ops: Vec<Value> = a.pos[2..]
                .iter()
                .map(|id| {
                    let v = match verb {
                        "rm" | "remove" => "delete",
                        v => v,
                    };
                    let mut o = json!({ "op": format!("{}.{}", t, v), "id": id });
                    if a.has("cascade") {
                        o["cascade"] = json!(true);
                    }
                    o
                })
                .collect();
            sess.apply(json!({ "ops": ops }))
        }
        "dep" | "deps" => {
            let id = need(a, 2, "task id")?;
            let mut ops = vec![];
            for on in a.all("on").into_iter().chain(a.all("after")) {
                ops.push(json!({ "op": "task.add_dep", "id": id, "on": on }));
            }
            for on in a.all("remove") {
                ops.push(json!({ "op": "task.remove_dep", "id": id, "on": on }));
            }
            if ops.is_empty() {
                return Err(usage("pass --on <task> to add or --remove <task> to drop a dependency"));
            }
            sess.apply(json!({ "ops": ops }))
        }
        "convert" => {
            let id = need(a, 2, "idea id")?;
            let mut op = Value::Object(a.fields(&[]));
            op["op"] = json!("idea.convert");
            op["id"] = json!(id);
            if op.get("to").is_none() {
                op["to"] = json!("task");
            }
            sess.apply(json!({ "ops": [op] }))
        }
        "schedule" => {
            let id = need(a, 2, "project id")?;
            sess.apply(json!({ "ops": [{ "op": "project.schedule", "id": id, "force": a.has("force") }] }))
        }
        other => Err(usage(format!("unknown {} command {:?}", t, other))),
    }
}
