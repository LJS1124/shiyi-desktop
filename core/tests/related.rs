use serde_json::{json, Value};
use shiyi_core::Core;

// 2026-10-12 10:00 +08:00
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

/// id of the i-th created item in a batch response
fn id(v: &Value, i: usize) -> String {
    v["results"][i]["id"].as_str().unwrap().to_string()
}

fn related(c: &Core, target: &str) -> Value {
    ok(c.query(json!({"q": "related", "id": target, "limit": 10})))
}

fn ids(v: &Value) -> Vec<String> {
    v["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap().to_string()).collect()
}

fn related_ids(c: &Core, target: &str) -> Vec<String> {
    ids(&related(c, target))
}

#[test]
fn finds_the_obvious_match() {
    let mut c = core();
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "idea.create", "ref": "a", "text": "书房的路由器信号差，考虑加一个有线回程"},
            {"op": "idea.create", "ref": "b", "text": "家里路由器换成 Mesh，书房信号太差"},
            {"op": "idea.create", "text": "学一下日语五十音"},
            {"op": "idea.create", "text": "周报模板想整理一下"},
            {"op": "idea.create", "text": "大模型推理成本怎么压"}
        ]}),
        "user",
    ));
    let (a, b) = (id(&r, 0), id(&r, 1));
    let v = related(&c, &a);
    let top = &v["items"][0];
    assert_eq!(top["id"], b.as_str(), "{}", v);
    assert_eq!(top["type"], "idea");
    let shared: Vec<&str> = top["shared"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
    assert!(shared.contains(&"路由器") && shared.contains(&"书房"), "{:?}", shared);
}

#[test]
fn unrelated_text_gets_no_hits() {
    let mut c = core();
    ok(c.apply(
        json!({"ops": [
            {"op": "idea.create", "text": "家里路由器换成 Mesh，书房信号太差"},
            {"op": "idea.create", "text": "大模型推理成本怎么压"},
            {"op": "task.create", "title": "周报模板想整理一下"}
        ]}),
        "user",
    ));
    let v = ok(c.query(json!({"q": "related", "text": "学一下日语五十音"})));
    assert_eq!(v["items"], json!([]));
    // the same holds for an item whose text shares nothing with the corpus
    let j = id(&ok(c.apply(json!({"op": "idea.create", "text": "学一下日语五十音"}), "user")), 0);
    assert_eq!(related_ids(&c, &j), Vec::<String>::new());
}

#[test]
fn items_already_connected_are_not_suggested_again() {
    let mut c = core();
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "idea.create", "ref": "a", "text": "书房的路由器信号差，考虑加一个有线回程"},
            {"op": "idea.create", "ref": "b", "text": "家里路由器换成 Mesh，书房信号太差"},
            {"op": "project.create", "ref": "p", "name": "网络改造"},
            {"op": "task.create", "ref": "t1", "title": "书房信号差改有线回程", "project": "$p"},
            {"op": "task.create", "ref": "t2", "title": "测试书房信号", "project": "$p"}
        ]}),
        "user",
    ));
    let (a, b, t1, t2) = (id(&r, 0), id(&r, 1), id(&r, 3), id(&r, 4));
    // before any link, both pairs are suggested
    assert!(related_ids(&c, &a).contains(&b));
    assert!(related_ids(&c, &t2).contains(&t1));
    // a link between the two ideas hides each from the other
    ok(c.apply(json!({"op": "link.create", "source": a, "target": b, "relation": "relates_to"}), "user"));
    assert!(!related_ids(&c, &a).contains(&b));
    assert!(!related_ids(&c, &b).contains(&a));
    // a wikilink from t2 to t1 connects them, so t1 is no longer suggested
    ok(c.apply(json!({"op": "task.update", "id": t2, "note": "先看 [[书房信号差改有线回程]]"}), "user"));
    assert!(!related_ids(&c, &t2).contains(&t1));
}

#[test]
fn done_tasks_and_archived_ideas_are_never_suggested() {
    let mut c = core();
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "idea.create", "ref": "a", "text": "书房路由器信号差"},
            {"op": "idea.create", "ref": "b", "text": "书房路由器想换 Mesh"},
            {"op": "idea.create", "ref": "c", "text": "书房路由器信号差，考虑有线回程"},
            {"op": "task.create", "ref": "d", "title": "书房路由器换 Mesh"}
        ]}),
        "user",
    ));
    let (a, b, c_idea, d) = (id(&r, 0), id(&r, 1), id(&r, 2), id(&r, 3));
    // before: the near-duplicates are suggested
    let before = related_ids(&c, &a);
    assert!(before.contains(&c_idea) && before.contains(&d), "{:?}", before);
    ok(c.apply(json!({"op": "idea.archive", "id": c_idea}), "user"));
    ok(c.apply(json!({"op": "task.done", "id": d}), "user"));
    let after = related_ids(&c, &a);
    assert!(after.contains(&b), "{:?}", after);
    assert!(!after.contains(&c_idea) && !after.contains(&d), "{:?}", after);
    // free text does not bring them back either
    let text = ids(&ok(c.query(json!({"q": "related", "text": "书房路由器信号差"}))));
    assert!(text.contains(&b));
    assert!(!text.contains(&c_idea) && !text.contains(&d), "{:?}", text);
}

#[test]
fn shared_tags_alone_make_an_item_related() {
    let mut c = core();
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "idea.create", "ref": "a", "text": "咖啡豆怎么选", "tags": ["采购", "生活"]},
            {"op": "idea.create", "ref": "b", "text": "周末去哪买手冲壶", "tags": ["生活", "采购"]},
            {"op": "idea.create", "ref": "c", "text": "电机轴承温度偏高", "tags": ["运维"]}
        ]}),
        "user",
    ));
    let (a, b, c_idea) = (id(&r, 0), id(&r, 1), id(&r, 2));
    let v = related(&c, &a);
    let item = v["items"].as_array().unwrap().iter().find(|i| i["id"] == b.as_str()).expect("the idea with the same tags should be suggested");
    assert_eq!(item["shared_tags"], json!(["生活", "采购"]));
    assert_eq!(item["shared"], json!([]));
    assert!(!ids(&v).contains(&c_idea), "{}", v);
}

#[test]
fn free_text_needs_no_id() {
    let mut c = core();
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "idea.create", "text": "家里路由器换成 Mesh，书房信号太差"},
            {"op": "idea.create", "text": "学一下日语五十音"},
            {"op": "task.create", "title": "周报模板"}
        ]}),
        "user",
    ));
    let b = id(&r, 0);
    let v = ok(c.query(json!({"q": "related", "text": "书房路由器信号差"})));
    assert_eq!(v["items"][0]["id"], b.as_str(), "{}", v);
    assert!(v["items"][0]["shared"].as_array().unwrap().iter().any(|x| x == "路由器"), "{}", v);
    let one = ok(c.query(json!({"q": "related", "text": "书房路由器信号差", "limit": 1})));
    assert_eq!(ids(&one).len(), 1);
}

#[test]
fn unknown_id_and_missing_input_are_errors() {
    let c = core();
    let e = err(c.query(json!({"q": "related", "id": "idea_nope", "limit": 5})));
    assert_eq!(e.code, "not_found");
    let e = err(c.query(json!({"q": "related"})));
    assert_eq!(e.code, "missing_field");
    let e = err(c.query(json!({"q": "related", "id": "  ", "text": "  "})));
    assert_eq!(e.code, "missing_field");
}

#[test]
fn id_argument_also_accepts_an_exact_title() {
    let mut c = core();
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "task.create", "ref": "t", "title": "书房信号差改有线回程"},
            {"op": "idea.create", "text": "家里路由器换成 Mesh，书房信号太差"}
        ]}),
        "user",
    ));
    let t = id(&r, 0);
    let by_id = related(&c, &t);
    assert!(!ids(&by_id).is_empty(), "{}", by_id);
    let by_title = ok(c.query(json!({"q": "related", "id": "书房信号差改有线回程", "limit": 10})));
    assert_eq!(by_title["items"], by_id["items"]);
}

#[test]
fn agent_links_from_suggestions_need_review_until_the_user_confirms() {
    let mut c = core();
    let r = ok(c.apply(
        json!({"ops": [
            {"op": "idea.create", "ref": "a", "text": "书房的路由器信号差，考虑加一个有线回程"},
            {"op": "idea.create", "ref": "b", "text": "家里路由器换成 Mesh，书房信号太差"},
            {"op": "idea.create", "ref": "c", "text": "同一个路由器的信号问题，电力猫试试"}
        ]}),
        "user",
    ));
    let (a, b, c_idea) = (id(&r, 0), id(&r, 1), id(&r, 2));
    // the agent takes the top related item and proposes a link to it
    let top = ids(&ok(c.query(json!({"q": "related", "id": a.as_str()}))))[0].clone();
    assert_eq!(top, b);
    let l = ok(c.apply(json!({"op": "link.create", "source": a, "target": top, "relation": "relates_to"}), "agent:x"));
    assert_eq!(l["results"][0]["status"], "suggested");
    assert_eq!(l["results"][0]["needs_review"], true);
    let lid = id(&l, 0);
    // a pending suggestion counts as a connection: it is not suggested again
    assert!(!related_ids(&c, &a).contains(&b));
    // the agent cannot accept it, by either route
    let e = err(c.apply(json!({"op": "link.confirm", "id": lid}), "agent:x"));
    assert_eq!(e.code, "permission_denied");
    let e = err(c.apply(json!({"op": "link.update", "id": lid, "status": "confirmed"}), "agent:x"));
    assert_eq!(e.code, "permission_denied");
    // the user accepts it
    ok(c.apply(json!({"op": "link.confirm", "id": lid}), "user"));
    // with full permission an agent's new link is confirmed straight away
    ok(c.apply(json!({"op": "settings.set", "key": "agent_permission", "value": "full"}), "user"));
    let l2 = ok(c.apply(json!({"op": "link.create", "source": a, "target": c_idea, "relation": "supports"}), "agent:x"));
    assert_eq!(l2["results"][0]["status"], "confirmed");
    assert!(l2["results"][0].get("needs_review").is_none());
}
