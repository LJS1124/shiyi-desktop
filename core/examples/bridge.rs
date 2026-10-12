//! Test bridge for the GUI end-to-end tests: JSON lines on stdin are the
//! app's Tauri commands, answered from a real database on stdout.
//! `cargo run --example bridge -- /tmp/test.db`
use serde_json::{json, Value};
use shiyi_core::{Core, Error};
use std::io::{BufRead, Write};

fn main() {
    let path = std::env::args().nth(1).expect("usage: bridge <db path>");
    let mut core = Core::open(&path).expect("open db");
    let mut out = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let args = req.get("args").cloned().unwrap_or(Value::Null);
        let res: Result<Value, Error> = match req["cmd"].as_str().unwrap_or("") {
            "core_query" => core.query(args["q"].clone()),
            "core_apply" => core.apply(args["req"].clone(), "user"),
            "is_first_run" => (|| {
                let seen = core.meta_get("first_run_at")?.is_some();
                if !seen {
                    core.set_meta("first_run_at", "test")?;
                }
                Ok(json!(!seen && core.is_empty()?))
            })(),
            "watch" => Ok(json!({ "dv": core.data_version(), "seq": core.max_seq() })),
            other => Err(Error::new("unknown_cmd", other)),
        };
        let reply = match res {
            Ok(v) => json!({ "ok": v }),
            Err(e) => json!({ "err": e.to_json()["error"] }),
        };
        let _ = writeln!(out, "{}", reply);
        let _ = out.flush();
    }
}
