use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

fn db() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("shiyi-cli-test-{}-{}", std::process::id(), shiyi_rand()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("t.db")
}

fn shiyi_rand() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

fn run(db: &std::path::Path, args: &[&str], stdin: Option<&str>) -> (i32, Value) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_shiyi"));
    cmd.args(args).arg("--compact").env("SHIYI_DB", db).env_remove("SHIYI_AGENT").stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    if let Some(s) = stdin {
        child.stdin.take().unwrap().write_all(s.as_bytes()).unwrap();
    } else {
        drop(child.stdin.take());
    }
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    let v: Value = serde_json::from_str(text.trim()).unwrap_or(Value::String(text));
    (out.status.code().unwrap_or(-1), v)
}

#[test]
fn cli_round_trip() {
    let db = db();
    let (code, v) = run(&db, &["project", "add", "搬家", "--area", "life", "--due", "+30d", "--as", "tester"], None);
    assert_eq!(code, 0, "{}", v);
    let pid = v["results"][0]["id"].as_str().unwrap().to_string();
    let (code, v) = run(&db, &["task", "add", "打包", "书", "--project", "搬家", "--estimate", "1,2,3", "--today"], None);
    assert_eq!(code, 0, "{}", v);
    let tid = v["results"][0]["id"].as_str().unwrap().to_string();
    let (_, v) = run(&db, &["get", &tid], None);
    assert_eq!(v["item"]["title"], "打包 书");
    assert_eq!(v["item"]["project_id"], pid.as_str());
    assert_eq!(v["item"]["created_by"], "agent:cli");
    let (_, v) = run(&db, &["today"], None);
    assert_eq!(v["today"][0]["id"], tid.as_str());

    // errors: exit 1 with code and hint
    let (code, v) = run(&db, &["task", "set", &tid, "--colour", "red"], None);
    assert_eq!(code, 1);
    assert_eq!(v["error"]["code"], "unknown_field");
    assert!(v["error"]["hint"].as_str().unwrap().contains("title"));
    // usage errors: exit 2
    let (code, _) = run(&db, &["frobnicate"], None);
    assert_eq!(code, 2);

    // apply from stdin, dry run
    let req = r#"{"ops":[{"op":"idea.create","text":"纸箱去哪买"}]}"#;
    let (code, v) = run(&db, &["apply", "-", "--dry-run"], Some(req));
    assert_eq!(code, 0);
    assert_eq!(v["dry_run"], true);
    let (_, v) = run(&db, &["list", "ideas"], None);
    assert_eq!(v["total"], 0);
    let (_, v) = run(&db, &["apply", "-", "--as", "tester"], Some(req));
    let batch = v["batch"].as_str().unwrap().to_string();
    let (_, v) = run(&db, &["log", "--actor", "tester", "--limit", "1"], None);
    assert_eq!(v["batches"][0]["batch"], batch.as_str());
    let (code, v) = run(&db, &["undo", &batch, "--as", "tester"], None);
    assert_eq!(code, 0, "{}", v);
    let (_, v) = run(&db, &["list", "ideas"], None);
    assert_eq!(v["total"], 0);

    // task shortcuts and the skill
    let (code, _) = run(&db, &["task", "done", &tid], None);
    assert_eq!(code, 0);
    let (_, v) = run(&db, &["get", &tid], None);
    assert_eq!(v["item"]["status"], "done");
    let (code, v) = run(&db, &["skill"], None);
    assert_eq!(code, 0);
    assert!(v.as_str().unwrap().contains("shiyi"));
    let _ = std::fs::remove_dir_all(db.parent().unwrap());
}

#[test]
fn cli_related_and_suggested_links() {
    let db = db();
    let (code, v) = run(&db, &["idea", "add", "书房的路由器信号差，考虑加一个有线回程"], None);
    assert_eq!(code, 0, "{}", v);
    let a = v["results"][0]["id"].as_str().unwrap().to_string();
    let (code, v) = run(&db, &["idea", "add", "家里路由器换成 Mesh，书房信号太差"], None);
    assert_eq!(code, 0, "{}", v);
    let b = v["results"][0]["id"].as_str().unwrap().to_string();

    // the other idea is possibly related
    let (code, v) = run(&db, &["related", &a], None);
    assert_eq!(code, 0, "{}", v);
    let ids: Vec<&str> = v["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&b.as_str()), "{}", v);
    // no id and no --text: usage error
    let (code, _) = run(&db, &["related"], None);
    assert_eq!(code, 2);

    // the CLI acts as an agent: its link is a suggestion that needs review
    let (code, v) = run(&db, &["link", "add", &a, &b, "--rel", "supports"], None);
    assert_eq!(code, 0, "{}", v);
    assert_eq!(v["results"][0]["status"], "suggested");
    assert_eq!(v["results"][0]["needs_review"], true);
    let lid = v["results"][0]["id"].as_str().unwrap().to_string();
    // and it cannot accept its own suggestion
    let (code, v) = run(&db, &["link", "confirm", &lid], None);
    assert_eq!(code, 1);
    assert_eq!(v["error"]["code"], "permission_denied");
    let _ = std::fs::remove_dir_all(db.parent().unwrap());
}
