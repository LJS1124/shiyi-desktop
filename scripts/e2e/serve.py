#!/usr/bin/env python3
"""Serve the bundled GUI for end-to-end tests.

  python3 scripts/e2e/serve.py --db /tmp/t.db      # "app" mode: Tauri commands answered by the real core
  python3 scripts/e2e/serve.py --preview           # web preview mode (data in localStorage)

App mode runs core/examples/bridge (cargo build --examples in core/) and fakes
window.__TAURI__ so the page talks to a real SQLite database, while the shiyi
CLI can change the same file from outside like an AI agent would.
"""
import argparse, http.server, json, pathlib, subprocess, threading, time

ROOT = pathlib.Path(__file__).resolve().parents[2]
ap = argparse.ArgumentParser()
ap.add_argument("--db")
ap.add_argument("--port", type=int, default=8765)
ap.add_argument("--preview", action="store_true")
ap.add_argument("--bridge", default=str(ROOT / "core/target/debug/examples/bridge"))
args = ap.parse_args()

page = (ROOT / "preview/index.html").read_text(encoding="utf-8")
SHIM = """<script>
window.__TAURI__ = {
  core: { invoke: async (cmd, a) => {
    const r = await fetch('/invoke', { method: 'POST', body: JSON.stringify({ cmd, args: a || {} }) });
    const j = await r.json(); if ('err' in j) throw j.err; return j.ok; } },
  event: { listen: async (name, cb) => {
    if (name !== 'db-changed') return () => {};
    let n = null;
    setInterval(async () => { try { const j = await (await fetch('/events')).json(); if (n !== null && j.n !== n) cb({ payload: {} }); n = j.n; } catch (e) {} }, 400);
    return () => {}; } }
};
</script>"""
html = "<!doctype html><html lang=zh-CN><head><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'></head><body>" + ("" if args.preview else SHIM) + page + "</body></html>"

lock = threading.Lock()
events = {"n": 0}
proc = None
if not args.preview:
    proc = subprocess.Popen([args.bridge, args.db], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)

def call(cmd, a):
    if cmd == "skill_text":
        return {"ok": (ROOT / "skills/shiyi/SKILL.md").read_text(encoding="utf-8")}
    if cmd == "get_settings":
        return {"ok": {"apiFormat": "anthropic", "apiBase": "", "model": "claude-sonnet-5-5", "quickModel": "claude-haiku-5-5", "hasKey": True, "dataPath": args.db}}
    if cmd == "save_settings":
        return {"ok": None}
    if cmd == "app_info":
        import os
        return {"ok": {"version": "1.2.2", "repo": "https://github.com/LJS1124/shiyi-desktop", "data_path": args.db, "backups": str(pathlib.Path(args.db).parent / "backups"), "os": "macos", "auto_update": True}}
    if cmd == "check_update":
        # pretend a newer release exists unless SHIYI_FAKE_UPDATE=none
        import os
        if os.environ.get("SHIYI_FAKE_UPDATE") == "none":
            return {"ok": {"available": False, "current": "1.2.2", "version": "1.2.2", "notes": "", "page": "https://github.com/LJS1124/shiyi-desktop/releases/tag/v1.1.1"}}
        notes = (ROOT / "docs/releases/v1.1.1.md").read_text(encoding="utf-8")
        return {"ok": {"available": True, "current": "1.2.2", "version": "1.3.0", "tag": "v1.3.0", "notes": notes, "published_at": "2026-10-20T08:00:00Z",
                       "page": "https://github.com/LJS1124/shiyi-desktop/releases/tag/v1.3.0", "asset_name": "Shiyi_1.3.0_universal.dmg",
                       "asset_url": "https://github.com/LJS1124/shiyi-desktop/releases/download/v1.3.0/Shiyi_1.3.0_universal.dmg"}}
    if cmd == "update_install":
        return {"err": "检查更新失败：模拟环境不能安装"}
    if cmd == "open_project_url":
        return {"ok": None}
    if cmd == "cli_status":
        return {"ok": {"installed": [], "skill_installed": False, "skill_current": False, "db": args.db}}
    with lock:
        proc.stdin.write(json.dumps({"cmd": cmd, "args": a}, ensure_ascii=False) + "\n")
        proc.stdin.flush()
        return json.loads(proc.stdout.readline())

def watcher():
    last = None
    while True:
        time.sleep(0.3)
        r = call("watch", {})["ok"]
        if last is not None and r["dv"] != last:
            events["n"] += 1
        last = r["dv"]

if proc:
    threading.Thread(target=watcher, daemon=True).start()

class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def send(self, code, body, ctype):
        b = body.encode("utf-8")
        self.send_response(code); self.send_header("Content-Type", ctype); self.send_header("Content-Length", str(len(b))); self.end_headers(); self.wfile.write(b)
    def do_GET(self):
        if self.path.startswith("/events"):
            return self.send(200, json.dumps(events), "application/json")
        self.send(200, html, "text/html; charset=utf-8")
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.send(200, json.dumps(call(body["cmd"], body.get("args", {})), ensure_ascii=False), "application/json")

http.server.ThreadingHTTPServer(("127.0.0.1", args.port), H).serve_forever()
