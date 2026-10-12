use serde_json::{json, Value};
use shiyi_core::Core;

// 2026-10-12 10:00 +08:00 (a Monday)
const NOW: i64 = 1_791_770_400_000;

fn core() -> Core {
    let mut c = Core::open_in_memory().unwrap();
    c.set_fixed_clock(NOW, 8 * 3600);
    c
}

fn ok(v: shiyi_core::Result<Value>) -> Value {
    match v {
        Ok(v) => v,
        Err(e) => panic!("unexpected error: {}", e),
    }
}

fn err(v: shiyi_core::Result<Value>) -> shiyi_core::Error {
    match v {
        Ok(v) => panic!("expected an error, got {}", v),
        Err(e) => e,
    }
}

fn id(v: &Value, i: usize) -> String {
    v["results"][i]["id"].as_str().unwrap().to_string()
}

fn get(c: &Core, id: &str) -> Value {
    ok(c.query(json!({"q": "get", "id": id})))["item"].clone()
}

#[test]
fn clock_is_pinned() {
    let c = core();
    let s = ok(c.query(json!({"q": "status"})));
    assert_eq!(s["today"], "2026-10-12");
    assert_eq!(s["now"], "2026-10-12T10:00:00+08:00");
}

#[test]
fn batch_with_refs_and_lenient_input() {
    let mut c = core();
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "project.create", "ref": "p", "name": "发布会", "due": "+14d", "area": "工作"},
            {"op": "task.create", "ref": "a", "title": "订场地", "project": "$p", "estimate": "1,2,4", "priority": "P1"},
            {"op": "task.create", "title": "发邀请", "project_id": "$p", "deps": ["$a"], "due": "fri", "status": "进行中"},
            {"op": "idea.create", "text": "现场直播？", "tags": "#直播, 活动"}
        ]}),
        "agent:claude",
    ));
    assert_eq!(r["changed"], 4);
    let p = id(&r, 0);
    let a = id(&r, 1);
    let b = id(&r, 2);
    assert!(p.starts_with("proj_") && a.starts_with("task_"));
    let pd = get(&c, &p);
    assert_eq!(pd["due_date"], "2026-10-26");
    assert_eq!(pd["area"], "work");
    assert_eq!(pd["created_by"], "agent:claude");
    let bd = get(&c, &b);
    assert_eq!(bd["depends_on"], json!([a]));
    assert_eq!(bd["due_date"], "2026-10-16");
    assert_eq!(bd["status"], "doing");
    assert_eq!(get(&c, &a)["priority"], 1);
    assert_eq!(get(&c, &a)["estimate"], json!({"optimistic": 1, "likely": 2, "pessimistic": 4}));
    let idea = get(&c, &id(&r, 3));
    assert_eq!(idea["tags"], json!(["直播", "活动"]));
    // reference by exact title
    let r2 = ok(c.apply(json!({"op": "task.create", "title": "彩排", "project": "发布会"}), "agent:claude"));
    assert_eq!(get(&c, &id(&r2, 0))["project_id"], p);
}

#[test]
fn errors_teach_the_schema() {
    let mut c = core();
    let e = err(c.apply(json!({"op": "task.create", "title": "x", "deadline_at": "fri"}), "agent:a"));
    assert_eq!(e.code, "unknown_field");
    assert!(e.hint.unwrap().contains("due_date"));
    let e = err(c.apply(json!({"op": "task.create", "title": "x", "due": "someday"}), "agent:a"));
    assert_eq!(e.code, "invalid_value");
    let e = err(c.apply(json!({"op": "task.create", "title": "x", "status": "blocked"}), "agent:a"));
    assert!(e.hint.unwrap().contains("todo, doing, done"));
    let e = err(c.apply(json!({"op": "task.create"}), "agent:a"));
    assert_eq!(e.code, "missing_field");
    let e = err(c.apply(json!({"op": "task.fly", "id": "x"}), "agent:a"));
    assert_eq!(e.code, "unknown_op");
    let e = err(c.apply(json!({"op": "task.create", "title": "x", "project": "不存在的项目"}), "agent:a"));
    assert_eq!(e.code, "not_found");
    // a failing op rolls back the whole request
    let e = err(c.apply(json!({"ops": [{"op": "idea.create", "text": "ok"}, {"op": "idea.create"}]}), "agent:a"));
    assert_eq!(e.data.unwrap()["op_index"], 1);
    assert_eq!(ok(c.query(json!({"q": "list", "type": "idea"})))["total"], 0);
}

#[test]
fn dry_run_saves_nothing() {
    let mut c = core();
    let r = ok(c.apply(json!({"ops": [{"op": "idea.create", "text": "试试"}], "dry_run": true}), "agent:a"));
    assert_eq!(r["dry_run"], true);
    assert_eq!(r["changed"], 1);
    assert!(r.get("batch").is_none());
    assert_eq!(ok(c.query(json!({"q": "list", "type": "idea"})))["total"], 0);
    assert_eq!(ok(c.query(json!({"q": "log"})))["batches"], json!([]));
}

#[test]
fn update_status_and_done_at() {
    let mut c = core();
    let t = id(&ok(c.apply(json!({"op": "task.create", "title": "写周报"}), "agent:a")), 0);
    let r = ok(c.apply(json!({"op": "task.done", "id": t}), "agent:a"));
    assert_eq!(r["results"][0]["updated"], json!(["status", "done_at"]));
    let d = get(&c, &t);
    assert_eq!(d["done_at"], "2026-10-12T10:00:00+08:00");
    assert_eq!(d["version"], 2);
    ok(c.apply(json!({"op": "task.reopen", "id": t}), "agent:a"));
    assert_eq!(get(&c, &t)["done_at"], Value::Null);
    // no-op update is not logged
    let r = ok(c.apply(json!({"op": "task.update", "id": t, "title": "写周报"}), "agent:a"));
    assert_eq!(r["changed"], 0);
    assert_eq!(r["results"][0]["unchanged"], true);
    // optimistic concurrency
    let e = err(c.apply(json!({"op": "task.update", "id": t, "if_version": 1, "title": "x"}), "agent:a"));
    assert_eq!(e.code, "conflict");
}

#[test]
fn dependency_cycles_are_rejected() {
    let mut c = core();
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "task.create", "ref": "a", "title": "A"},
            {"op": "task.create", "ref": "b", "title": "B", "depends_on": ["$a"]},
            {"op": "task.create", "ref": "c", "title": "C", "depends_on": ["$b"]}
        ]}),
        "agent:a",
    ));
    let (a, cc) = (id(&r, 0), id(&r, 2));
    let e = err(c.apply(json!({"op": "task.add_dep", "id": a, "on": cc}), "agent:a"));
    assert_eq!(e.code, "cycle");
    let e = err(c.apply(json!({"op": "task.add_dep", "id": a, "on": a}), "agent:a"));
    assert_eq!(e.code, "cycle");
    // deleting a task strips it from dependents
    let b = id(&r, 1);
    ok(c.apply(json!({"op": "task.delete", "id": b}), "agent:a"));
    assert_eq!(get(&c, &cc)["depends_on"], json!([]));
}

#[test]
fn undo_and_redo() {
    let mut c = core();
    let r = ok(c.apply(json!({"ops": [{"op": "idea.create", "text": "一"}, {"op": "idea.create", "text": "二"}]}), "agent:a"));
    let batch = r["batch"].as_str().unwrap().to_string();
    let i1 = id(&r, 0);
    let r2 = ok(c.apply(json!({"op": "undo", "batch": batch}), "agent:a"));
    assert_eq!(r2["changed"], 2);
    assert_eq!(ok(c.query(json!({"q": "list", "type": "idea"})))["total"], 0);
    let e = err(c.apply(json!({"op": "undo", "batch": batch}), "agent:a"));
    assert_eq!(e.code, "already_undone");
    // undoing the undo brings them back
    let redo = r2["batch"].as_str().unwrap().to_string();
    ok(c.apply(json!({"op": "undo", "batch": redo}), "agent:a"));
    assert_eq!(ok(c.query(json!({"q": "list", "type": "idea"})))["total"], 2);
    assert_eq!(get(&c, &i1)["text"], "一");
}

#[test]
fn undo_detects_later_edits() {
    let mut c = core();
    let r = ok(c.apply(json!({"op": "task.create", "title": "原标题"}), "agent:a"));
    let t = id(&r, 0);
    let b1 = ok(c.apply(json!({"op": "task.update", "id": t, "title": "AI 改的"}), "agent:a"))["batch"].as_str().unwrap().to_string();
    ok(c.apply(json!({"op": "entity.patch", "type": "task", "id": t, "set": {"note": "用户后来加的"}}), "user"));
    let e = err(c.apply(json!({"op": "undo", "batch": b1}), "agent:a"));
    assert_eq!(e.code, "conflict");
    // the user may force it
    ok(c.apply(json!({"op": "undo", "batch": b1, "force": true}), "user"));
    let d = get(&c, &t);
    assert_eq!(d["title"], "原标题");
}

#[test]
fn undo_last_walks_back() {
    let mut c = core();
    let t = id(&ok(c.apply(json!({"op": "task.create", "title": "t"}), "agent:a")), 0);
    ok(c.apply(json!({"op": "task.update", "id": t, "status": "doing"}), "agent:a"));
    ok(c.apply(json!({"op": "task.update", "id": t, "status": "done"}), "agent:a"));
    ok(c.apply(json!({"op": "undo", "last": true}), "agent:a"));
    assert_eq!(get(&c, &t)["status"], "doing");
    ok(c.apply(json!({"op": "undo", "last": true}), "agent:a"));
    assert_eq!(get(&c, &t)["status"], "todo");
    // another agent's changes are not "last" for this one
    let e = err(c.apply(json!({"op": "undo", "last": true}), "agent:b"));
    assert_eq!(e.code, "not_found");
}

#[test]
fn permissions() {
    let mut c = core();
    let mine = id(&ok(c.apply(json!({"op": "idea.create", "text": "用户的想法"}), "user")), 0);
    let theirs = id(&ok(c.apply(json!({"op": "idea.create", "text": "AI 的想法"}), "agent:a")), 0);
    // write: may not delete the user's things or undo the user's batches
    let e = err(c.apply(json!({"op": "idea.delete", "id": mine}), "agent:a"));
    assert_eq!(e.code, "permission_denied");
    ok(c.apply(json!({"op": "idea.delete", "id": theirs}), "agent:a"));
    ok(c.apply(json!({"op": "idea.archive", "id": mine}), "agent:a"));
    // settings are the user's
    let e = err(c.apply(json!({"op": "settings.set", "key": "agent_permission", "value": "full"}), "agent:a"));
    assert_eq!(e.code, "permission_denied");
    ok(c.apply(json!({"op": "settings.set", "key": "agent_permission", "value": "read"}), "user"));
    let e = err(c.apply(json!({"op": "idea.create", "text": "x"}), "agent:a"));
    assert_eq!(e.code, "permission_denied");
    assert!(c.query(json!({"q": "today"})).is_ok());
    ok(c.apply(json!({"op": "settings.set", "key": "agent_permission", "value": "full"}), "user"));
    ok(c.apply(json!({"op": "idea.delete", "id": mine}), "agent:a"));
    // agents cannot pretend to be the user
    let r = ok(c.apply(json!({"op": "idea.create", "text": "y"}), "user2"));
    assert_eq!(get(&c, &id(&r, 0))["created_by"], "agent:user2");
}

#[test]
fn soft_delete_restore_and_cascade() {
    let mut c = core();
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "project.create", "ref": "p", "name": "P"},
            {"op": "task.create", "title": "t1", "project": "$p"},
            {"op": "task.create", "title": "t2", "project": "$p"}
        ]}),
        "agent:a",
    ));
    let p = id(&r, 0);
    let e = err(c.apply(json!({"op": "project.delete", "id": p}), "agent:a"));
    assert_eq!(e.code, "has_tasks");
    let d = ok(c.apply(json!({"op": "project.delete", "id": p, "cascade": true}), "agent:a"));
    assert_eq!(d["changed"], 3);
    assert_eq!(ok(c.query(json!({"q": "list", "type": "task"})))["total"], 0);
    assert_eq!(ok(c.query(json!({"q": "trash"})))["total"], 3);
    let e = err(c.apply(json!({"op": "task.create", "title": "x", "project": p}), "agent:a"));
    assert_eq!(e.code, "deleted");
    let rr = ok(c.apply(json!({"op": "project.restore", "id": p}), "agent:a"));
    assert_eq!(rr["results"][0]["restored_tasks"].as_array().unwrap().len(), 2);
    assert_eq!(ok(c.query(json!({"q": "list", "type": "task"})))["total"], 2);
}

#[test]
fn idempotent_client_ids() {
    let mut c = core();
    let op = json!({"op": "task.create", "id": "task_weekly-report-1012", "title": "周报"});
    ok(c.apply(op.clone(), "agent:a"));
    let r = ok(c.apply(op, "agent:a"));
    assert_eq!(r["results"][0]["existed"], true);
    assert_eq!(r["changed"], 0);
    let e = err(c.apply(json!({"op": "task.create", "id": "weird", "title": "x"}), "agent:a"));
    assert_eq!(e.code, "invalid_id");
}

#[test]
fn convert_schedule_and_maps() {
    let mut c = core();
    let idea = id(&ok(c.apply(json!({"op": "idea.create", "text": "做一个读书 App\n可以记录划线"}), "agent:a")), 0);
    let r = ok(c.apply(json!({"op": "idea.convert", "id": idea, "to": "project"}), "agent:a"));
    let p = r["results"][0]["converted_to"]["id"].as_str().unwrap().to_string();
    let pd = get(&c, &p);
    assert_eq!(pd["name"], "做一个读书 App");
    assert_eq!(pd["goal"], "可以记录划线");
    assert_eq!(pd["from_idea"], idea);
    assert_eq!(get(&c, &idea)["status"], "converted");
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "task.create", "ref": "a", "title": "调研", "project": p, "estimate": 2},
            {"op": "task.create", "ref": "b", "title": "原型", "project": p, "estimate": "1,3,5", "after": ["$a"]},
            {"op": "task.create", "title": "开发", "project": p, "estimate": 5, "after": "$b"},
            {"op": "project.schedule", "id": p}
        ]}),
        "agent:a",
    ));
    assert_eq!(r["results"][3]["scheduled"], 3);
    let tasks = ok(c.query(json!({"q": "list", "type": "task", "sort": "due"})))["items"].clone();
    let dates: Vec<(String, String)> = tasks.as_array().unwrap().iter().map(|t| (t["start_date"].as_str().unwrap().into(), t["due_date"].as_str().unwrap().into())).collect();
    assert_eq!(dates, vec![("2026-10-12".into(), "2026-10-13".into()), ("2026-10-14".into(), "2026-10-16".into()), ("2026-10-17".into(), "2026-10-21".into())]);
    // mind map from an outline
    let r = ok(c.apply(json!({"op": "map.create", "title": "读书 App", "outline": "功能\n  划线\n  笔记\n- 推广\n    - 社群", "project": p}), "agent:a"));
    let m = id(&r, 0);
    let g = ok(c.query(json!({"q": "get", "id": m})));
    assert_eq!(g["outline"], "读书 App\n  功能\n    划线\n    笔记\n  推广\n    社群\n");
    // outline JSON form and title from outline
    let r = ok(c.apply(json!({"op": "map.create", "outline": [{"text": "根", "children": ["甲", {"text": "乙", "children": ["丙"]}]}]}), "agent:a"));
    let g = ok(c.query(json!({"q": "get", "id": id(&r, 0)})));
    assert_eq!(g["outline"], "根\n  甲\n  乙\n    丙\n");
}

#[test]
fn knowledge_graph() {
    let mut c = core();
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "project.create", "ref": "p", "name": "网站改版"},
            {"op": "idea.create", "ref": "i1", "text": "暗色模式，参考 [[网站改版]] 和 [[不存在]]", "tags": ["设计"]},
            {"op": "idea.create", "ref": "i2", "text": "字体换成思源", "tags": ["设计"]},
            {"op": "link.create", "source": "$i1", "target": "$i2", "relation": "related", "status": "suggested", "note": "都是视觉"},
            {"op": "task.create", "title": "首页", "project": "$p"}
        ]}),
        "agent:a",
    ));
    let (p, i1, i2) = (id(&r, 0), id(&r, 1), id(&r, 2));
    // duplicate links are merged, confirming a suggestion
    let again = ok(c.apply(json!({"op": "link.create", "source": i2, "target": i1, "relation": "relates_to"}), "user"));
    assert_eq!(again["results"][0]["existed"], true);
    assert_eq!(again["results"][0]["confirmed"], true);
    let g = ok(c.query(json!({"q": "graph"})));
    let edges = g["edges"].as_array().unwrap();
    let has = |a: &str, b: &str, kind: &str| edges.iter().any(|e| e["source"] == a && e["target"] == b && e["kind"] == kind);
    assert!(has(&i1, &p, "mention"));
    assert!(has(&i1, &i2, "link"));
    assert!(has(&i1, "tag:设计", "tag"));
    assert!(edges.iter().any(|e| e["kind"] == "field" && e["relation"] == "part_of" && e["target"] == p));
    assert_eq!(g["unresolved_wikilinks"][0]["text"], "不存在");
    // local graph
    let g = ok(c.query(json!({"q": "graph", "center": i2, "depth": 1, "tags": false})));
    let ids: Vec<&str> = g["nodes"].as_array().unwrap().iter().map(|n| n["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&i1.as_str()) && !ids.contains(&p.as_str()));
    // backlinks in get
    let gp = ok(c.query(json!({"q": "get", "id": p})));
    assert!(gp["links_in"].as_array().unwrap().iter().any(|e| e["source"] == i1 && e["relation"] == "mentions"));
    assert_eq!(gp["stats"]["total"], 1);
    let doc = ok(c.query(json!({"q": "doctor"})));
    assert_eq!(doc["issues"][0]["problem"], "unresolved_wikilink");
}

#[test]
fn agents_only_suggest_links() {
    let mut c = core();
    let r = ok(c.apply(json!({"ops": [{"op": "idea.create", "ref": "a", "text": "甲"}, {"op": "idea.create", "ref": "b", "text": "乙"}]}), "user"));
    let (a, b) = (id(&r, 0), id(&r, 1));
    // write permission: forced to a suggestion, and the agent cannot accept it
    let l = ok(c.apply(json!({"op": "link.create", "source": a, "target": b, "relation": "supports"}), "agent:x"));
    assert_eq!(l["results"][0]["status"], "suggested");
    assert_eq!(l["results"][0]["needs_review"], true);
    let lid = id(&l, 0);
    let e = err(c.apply(json!({"op": "link.confirm", "id": lid}), "agent:x"));
    assert_eq!(e.code, "permission_denied");
    let e = err(c.apply(json!({"op": "link.update", "id": lid, "status": "confirmed"}), "agent:x"));
    assert_eq!(e.code, "permission_denied");
    // the agent may still edit its suggestion; the user accepts it
    ok(c.apply(json!({"op": "link.update", "id": lid, "note": "理由"}), "agent:x"));
    ok(c.apply(json!({"op": "link.confirm", "id": lid}), "user"));
    assert_eq!(get(&c, &lid)["status"], "confirmed");
    // full permission: the agent's links count as confirmed
    ok(c.apply(json!({"op": "settings.set", "key": "agent_permission", "value": "full"}), "user"));
    let l2 = ok(c.apply(json!({"op": "link.create", "source": b, "target": a, "relation": "inspired_by"}), "agent:x"));
    assert_eq!(l2["results"][0]["status"], "confirmed");
}

#[test]
fn search_list_and_log() {
    let mut c = core();
    ok(c.apply(
        json!({"ops": [
            {"op": "project.create", "ref": "p", "name": "旅行计划"},
            {"op": "task.create", "title": "订机票", "project": "$p", "due": "tomorrow", "today": "today"},
            {"op": "task.create", "title": "办签证", "project": "$p", "due": "-2d"},
            {"op": "idea.create", "text": "去京都看红叶", "note": "十一月最好", "tags": ["旅行"]}
        ]}),
        "agent:planner",
    ));
    let s = ok(c.query(json!({"q": "search", "text": "红叶"})));
    assert_eq!(s["items"][0]["type"], "idea");
    let l = ok(c.query(json!({"q": "list", "type": "task", "project": "旅行计划", "due_before": "today"})));
    assert_eq!(l["total"], 1);
    assert_eq!(l["items"][0]["title"], "办签证");
    assert_eq!(l["items"][0]["project"], "旅行计划");
    let t = ok(c.query(json!({"q": "today"})));
    assert_eq!(t["overdue"][0]["title"], "办签证");
    assert_eq!(t["today"][0]["title"], "订机票");
    let log = ok(c.query(json!({"q": "log", "actor": "agents"})));
    let b = &log["batches"][0];
    assert_eq!(b["actor"], "agent:planner");
    assert_eq!(b["ops"].as_array().unwrap().len(), 4);
    assert!(b["ops"][0]["summary"].as_str().unwrap().contains("旅行计划"));
}

#[test]
fn gui_writes_are_lenient() {
    let mut c = core();
    // the GUI sends whole documents with its own ids and may reference ids that do not exist yet
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "entity.put", "type": "task", "doc": {"id": "task_abc", "title": "", "depends_on": ["task_later"], "created_at": NOW - 86_400_000}},
            {"op": "entity.put", "type": "task", "doc": {"id": "task_later", "title": "later"}},
            {"op": "entity.patch", "type": "task", "id": "task_abc", "set": {"title": "now named", "status": "done"}}
        ]}),
        "user",
    ));
    assert_eq!(r["changed"], 3);
    let d = get(&c, "task_abc");
    assert_eq!(d["title"], "now named");
    assert_eq!(d["created_at"], "2026-10-11T10:00:00+08:00");
    assert_eq!(d["created_by"], "user");
    // delete then put again = restore with new content
    ok(c.apply(json!({"op": "entity.delete", "type": "task", "id": "task_abc"}), "user"));
    ok(c.apply(json!({"op": "entity.put", "type": "task", "doc": {"id": "task_abc", "title": "back"}}), "user"));
    assert_eq!(get(&c, "task_abc")["deleted_at"], Value::Null);
    assert_eq!(get(&c, "task_abc")["title"], "back");
}

#[test]
fn legacy_import() {
    let mut c = core();
    let old = json!({
        "app": "shiyi", "version": 2,
        "projects": [{"id": "p1", "name": "旧项目", "area": "life", "color": "mint", "status": "active", "start": "2026-10-01", "due": null, "createdAt": NOW - 1000}],
        "tasks": [
            {"id": "t1", "title": "旧任务", "projectId": "p1", "status": "done", "priority": 2, "estO": 1, "estM": "2", "estP": null, "deps": [], "doneAt": NOW - 500, "todayDate": null, "createdAt": NOW - 900},
            {"id": "t2", "title": "后续", "projectId": "p1", "status": "todo", "priority": 0, "deps": ["t1"], "createdAt": NOW - 800}
        ],
        "ideas": [{"id": "i1", "text": "旧想法", "tags": ["a"], "status": "converted", "star": true, "projectId": "p1", "createdAt": NOW - 700}],
        "maps": [{"id": "m1", "title": "旧导图", "rootId": "r", "nodes": {"r": {"id": "r", "text": "旧导图", "parent": null, "children": ["n"], "collapsed": false, "color": null, "link": null},
                                                               "n": {"id": "n", "text": "任务", "parent": "r", "children": [], "collapsed": false, "color": null, "link": {"type": "task", "id": "t1"}}}, "projectId": "p1"}]
    });
    let r = ok(c.import(old.clone(), "user"));
    assert_eq!(r["imported"], 5);
    let t2 = get(&c, "task_t2");
    assert_eq!(t2["depends_on"], json!(["task_t1"]));
    assert_eq!(t2["project_id"], "proj_p1");
    let t1 = get(&c, "task_t1");
    assert_eq!(t1["estimate"], json!({"optimistic": 1, "likely": 2, "pessimistic": null}));
    assert_eq!(t1["priority"], 2);
    assert_eq!(get(&c, "idea_i1")["starred"], true);
    assert_eq!(get(&c, "map_m1")["nodes"]["n"]["link"]["id"], "task_t1");
    // importing twice updates instead of duplicating
    ok(c.import(old, "user"));
    assert_eq!(ok(c.query(json!({"q": "list", "type": "task"})))["total"], 2);
    // snapshot round trip
    let snap = ok(c.query(json!({"q": "snapshot"})));
    let mut c2 = core();
    ok(c2.import(snap, "user"));
    assert_eq!(ok(c2.query(json!({"q": "list", "type": "idea"})))["total"], 1);
    assert_eq!(get(&c2, "task_t2")["depends_on"], json!(["task_t1"]));
}

#[test]
fn schema_is_complete() {
    let c = core();
    let s = ok(c.query(json!({"q": "schema"})));
    assert_eq!(s["types"].as_array().unwrap().len(), 5);
    let s = ok(c.query(json!({"q": "schema", "type": "tasks"})));
    assert_eq!(s["types"][0]["type"], "task");
}
