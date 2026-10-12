use serde_json::{json, Map, Value};
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

fn id(v: &Value, i: usize) -> String {
    v["results"][i]["id"].as_str().unwrap().to_string()
}

fn get(c: &Core, id: &str) -> Value {
    ok(c.query(json!({"q": "get", "id": id})))["item"].clone()
}

/// Node id of the node whose text is `text`.
fn node(c: &Core, map: &str, text: &str) -> String {
    let nodes = get(c, map)["nodes"].as_object().unwrap().clone();
    nodes.iter().find(|(_, n)| n["text"] == text).map(|(k, _)| k.clone()).unwrap_or_else(|| panic!("no node {}", text))
}

/// Applies `edit` to the map's current nodes and saves them as the user.
fn edit(c: &mut Core, map: &str, edit: impl FnOnce(&mut Map<String, Value>)) {
    let mut nodes = get(c, map)["nodes"].as_object().unwrap().clone();
    edit(&mut nodes);
    ok(c.apply(json!({"op": "entity.patch", "type": "map", "id": map, "set": {"nodes": nodes}}), "user"));
}

fn new_node(id: &str, text: &str, parent: &str) -> Value {
    json!({"id": id, "text": text, "parent": parent, "children": [], "collapsed": false, "color": null, "link": null})
}

/// Adds `child` under `parent` in the nodes object.
fn attach(nodes: &mut Map<String, Value>, parent: &str, child: Value) {
    let cid = child["id"].as_str().unwrap().to_string();
    nodes.insert(cid.clone(), child);
    nodes.get_mut(parent).unwrap()["children"].as_array_mut().unwrap().push(json!(cid));
}

/// Moves `child` from its current parent to `to` (both children arrays updated).
fn reparent(nodes: &mut Map<String, Value>, child: &str, to: &str) {
    let from = nodes[child]["parent"].as_str().unwrap().to_string();
    nodes.get_mut(&from).unwrap()["children"].as_array_mut().unwrap().retain(|c| c.as_str() != Some(child));
    nodes.get_mut(to).unwrap()["children"].as_array_mut().unwrap().push(json!(child));
    nodes.get_mut(child).unwrap()["parent"] = json!(to);
}

fn last_summary(c: &Core) -> String {
    ok(c.query(json!({"q": "log", "limit": 1})))["batches"][0]["ops"][0]["summary"].as_str().unwrap().to_string()
}

/// A map from the outline 场地 > 预算, 嘉宾, returning (core, map id, root id).
fn setup() -> (Core, String, String) {
    let mut c = core();
    let r = ok(c.apply(json!({"op": "map.create", "title": "发布会", "outline": "场地\n  预算\n嘉宾"}), "user"));
    let m = id(&r, 0);
    let root = get(&c, &m)["root_id"].as_str().unwrap().to_string();
    (c, m, root)
}

#[test]
fn add_one_node() {
    let (mut c, m, root) = setup();
    edit(&mut c, &m, |n| attach(n, &root, new_node("n_budget2", "预算二", &root)));
    assert_eq!(last_summary(&c), "修改导图「发布会」：添加节点「预算二」");
}

#[test]
fn add_several_nodes() {
    let (mut c, m, root) = setup();
    edit(&mut c, &m, |n| {
        attach(n, &root, new_node("a1", "嘉宾甲", &root));
        attach(n, &root, new_node("a2", "嘉宾乙", &root));
        attach(n, &root, new_node("a3", "媒体", &root));
    });
    assert_eq!(last_summary(&c), "修改导图「发布会」：添加 3 个节点");
}

#[test]
fn long_and_blank_added_text_is_clipped_or_named() {
    let (mut c, m, root) = setup();
    edit(&mut c, &m, |n| attach(n, &root, new_node("long", "一二三四五六七八九十甲乙丙丁戊己庚", &root)));
    assert_eq!(last_summary(&c), "修改导图「发布会」：添加节点「一二三四五六七八九十甲乙丙丁戊…」");
    edit(&mut c, &m, |n| attach(n, &root, new_node("blank", "   ", &root)));
    assert_eq!(last_summary(&c), "修改导图「发布会」：添加节点「未命名」");
}

#[test]
fn delete_one_leaf() {
    let (mut c, m, root) = setup();
    let guests = node(&c, &m, "嘉宾");
    edit(&mut c, &m, |n| {
        n.remove(&guests);
        n.get_mut(&root).unwrap()["children"].as_array_mut().unwrap().retain(|x| x.as_str() != Some(guests.as_str()));
    });
    assert_eq!(last_summary(&c), "修改导图「发布会」：删除节点「嘉宾」");
}

#[test]
fn delete_several_nodes() {
    let (mut c, m, root) = setup();
    let budget = node(&c, &m, "预算");
    let venue = node(&c, &m, "场地");
    let guests = node(&c, &m, "嘉宾");
    edit(&mut c, &m, |n| {
        for k in [&budget, &venue, &guests] {
            n.remove(k);
        }
        n.get_mut(&root).unwrap()["children"] = json!([]);
    });
    assert_eq!(last_summary(&c), "修改导图「发布会」：删除 3 个节点");
}

#[test]
fn rename_one() {
    let (mut c, m, _) = setup();
    let venue = node(&c, &m, "场地");
    edit(&mut c, &m, |n| n.get_mut(&venue).unwrap()["text"] = json!("场地与时间"));
    assert_eq!(last_summary(&c), "修改导图「发布会」：「场地」改为「场地与时间」");
}

#[test]
fn rename_several() {
    let (mut c, m, _) = setup();
    let venue = node(&c, &m, "场地");
    let guests = node(&c, &m, "嘉宾");
    edit(&mut c, &m, |n| {
        n.get_mut(&venue).unwrap()["text"] = json!("场地与时间");
        n.get_mut(&guests).unwrap()["text"] = json!("嘉宾名单");
    });
    assert_eq!(last_summary(&c), "修改导图「发布会」：改了 2 个节点的文字");
}

#[test]
fn move_one() {
    let (mut c, m, root) = setup();
    let budget = node(&c, &m, "预算");
    edit(&mut c, &m, |n| reparent(n, &budget, &root));
    assert_eq!(last_summary(&c), "修改导图「发布会」：移动节点「预算」");
    // both parents' children arrays were updated
    let nodes = get(&c, &m)["nodes"].clone();
    let venue = node(&c, &m, "场地");
    assert_eq!(nodes[&venue]["children"], json!([]));
    assert!(nodes[&root]["children"].as_array().unwrap().contains(&json!(budget)));
}

#[test]
fn add_and_move_together() {
    let (mut c, m, root) = setup();
    let budget = node(&c, &m, "预算");
    edit(&mut c, &m, |n| {
        attach(n, &root, new_node("host", "主持人", &root));
        reparent(n, &budget, &root);
    });
    assert_eq!(last_summary(&c), "修改导图「发布会」：添加节点「主持人」、移动节点「预算」");
}

#[test]
fn collapse_only() {
    let (mut c, m, _) = setup();
    let venue = node(&c, &m, "场地");
    edit(&mut c, &m, |n| n.get_mut(&venue).unwrap()["collapsed"] = json!(true));
    assert_eq!(last_summary(&c), "修改导图「发布会」：折叠或展开节点");
}

#[test]
fn color_only() {
    let (mut c, m, _) = setup();
    let guests = node(&c, &m, "嘉宾");
    edit(&mut c, &m, |n| n.get_mut(&guests).unwrap()["color"] = json!("#ff6b6b"));
    assert_eq!(last_summary(&c), "修改导图「发布会」：调整节点");
}

#[test]
fn link_only() {
    let (mut c, m, _) = setup();
    let guests = node(&c, &m, "嘉宾");
    edit(&mut c, &m, |n| n.get_mut(&guests).unwrap()["link"] = json!({"type": "task", "id": "task_x"}));
    assert_eq!(last_summary(&c), "修改导图「发布会」：关联到任务或灵感");
}
