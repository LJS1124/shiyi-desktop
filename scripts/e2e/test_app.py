#!/usr/bin/env python3
"""End-to-end test: the GUI and an "AI agent" (the shiyi CLI) share one database.

Needs: cd core && cargo build --bins --examples; python3 scripts/build-preview.py;
       pip install playwright (Chromium available).
Run:   python3 scripts/e2e/test_app.py [screenshot dir]
"""
import json, os, pathlib, subprocess, sys, tempfile, time
from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[2]
SHOTS = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path(tempfile.mkdtemp())
TMP = pathlib.Path(tempfile.mkdtemp())
DB = TMP / "e2e.db"
PORT = 8790
CLI = ROOT / "core/target/debug/shiyi"
env = {**os.environ, "SHIYI_DB": str(DB)}


def cli(*args, agent="claude"):
    out = subprocess.run([str(CLI), *args, "--as", agent, "--compact"], env=env, capture_output=True, text=True)
    return json.loads(out.stdout)


def check(cond, msg):
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        raise SystemExit(1)


server = subprocess.Popen([sys.executable, str(ROOT / "scripts/e2e/serve.py"), "--db", str(DB), "--port", str(PORT)])
time.sleep(1.0)
errors = []
try:
    with sync_playwright() as p:
        b = p.chromium.launch()
        pg = b.new_page(viewport={"width": 1440, "height": 900})
        pg.on("pageerror", lambda e: errors.append(str(e)))
        pg.on("console", lambda m: errors.append(m.text) if m.type == "error" else None)
        pg.goto(f"http://127.0.0.1:{PORT}/")
        pg.wait_for_function("window.SY && SY.S.ready && SY.S.data.tasks.size > 0", timeout=8000)
        pg.wait_for_timeout(600)

        print("first run")
        st = cli("status")
        check(st["counts"]["tasks"] == 14 and st["counts"]["links"] == 2, f"examples are in the database ({st['counts']})")
        log = cli("log", "--limit", "1")
        check(log["batches"][0]["actor"] == "user" and log["batches"][0].get("note") == "示例数据", "seeding is one user batch")

        print("GUI writes")
        pg.fill("#cap-text", "让 AI 每周五自动整理一次灵感收件箱 #效率")
        pg.press("#cap-text", "Enter")
        pg.wait_for_timeout(700)
        found = cli("search", "整理一次灵感收件箱")
        check(found["total"] == 1, "captured idea reached the database")
        iid = found["items"][0]["id"]
        check(iid.startswith("idea_"), f"GUI ids are prefixed ({iid})")
        check(cli("get", iid)["item"]["tags"] == ["效率"], "tags parsed from #hashtags")

        pg.evaluate("SY.setView('kanban')")
        pg.wait_for_timeout(300)
        pg.click('.kcard[data-kid="task_s-a4"] [data-act="advance"]')
        pg.wait_for_timeout(700)
        check(cli("get", "task_s-a4")["item"]["status"] == "doing", "kanban advance → database")

        pg.evaluate("SY.select('tasks','task_s-a4')")
        pg.wait_for_timeout(300)
        pg.fill("#i-note", "参考 [[整理一份常用 Prompt 模板库，按场景分类]] 的评分口径")
        pg.click(".insp-title")
        pg.wait_for_timeout(700)
        g = cli("get", "task_s-a4")
        check(any(e["kind"] == "mention" and e["target"] == "idea_s-prompt" for e in g["links_out"]), "[[wikilink]] in a note becomes a graph edge")

        print("an agent changes things through the CLI")
        r = cli("task", "add", "复盘本周评测结果", "--project", "Agent 评测集搭建", "--due", "fri", "--priority", "1")
        check(r["ok"], "agent created a task")
        pg.wait_for_timeout(1800)
        titles = pg.evaluate("[...SY.S.data.tasks.values()].map(t => t.title)")
        check("复盘本周评测结果" in titles, "GUI picked up the agent's task without reload")
        toast = pg.inner_text("#toast")
        check("claude" in toast, f"toast names the agent ({toast.strip()[:40]})")
        pg.screenshot(path=str(SHOTS / "e2e-toast.png"))

        print("operation log + undo")
        pg.evaluate("SY.closeInsp(); SY.setView('log')")
        pg.wait_for_selector(".opb", timeout=4000)
        pg.wait_for_timeout(500)
        pg.screenshot(path=str(SHOTS / "e2e-log.png"))
        first = pg.locator(".opb").first
        check("claude" in first.inner_text(), "newest batch is the agent's")
        first.locator('[data-act="op-undo"]').click()
        pg.wait_for_timeout(900)
        check(cli("search", "复盘本周评测结果")["total"] == 0, "undo from the GUI removed the agent's task")
        titles = pg.evaluate("[...SY.S.data.tasks.values()].map(t => t.title)")
        check("复盘本周评测结果" not in titles, "GUI state matches after undo")

        print("permissions")
        pg.evaluate("SY.store.setPermission('read')")
        pg.wait_for_timeout(300)
        r = cli("idea", "add", "这条不该写进去")
        check(not r["ok"] and r["error"]["code"] == "permission_denied", "read-only permission blocks agents")
        pg.evaluate("SY.store.setPermission('write')")
        pg.wait_for_timeout(300)
        r = cli("idea", "delete", iid)
        check(r["error"]["code"] == "permission_denied", "agents cannot delete the user's ideas with write permission")

        print("knowledge graph")
        r = cli("link", "add", iid, "idea_s-weekly", "--rel", "relates_to", "--note", "都是让 AI 定期整理", "--suggest")
        check(r["ok"], "agent suggested a link")
        pg.evaluate("SY.setView('graph')")
        pg.wait_for_timeout(1800)
        n = pg.locator(".gnode").count()
        check(n > 20, f"graph renders nodes ({n})")
        check(pg.locator(".gsug-row").count() == 2, "two suggestions are waiting")
        pg.screenshot(path=str(SHOTS / "e2e-graph.png"))
        pg.locator(".gsug-row").first.locator('[data-act="link-ok"]').click()
        pg.wait_for_timeout(700)
        sug = cli("list", "links", "--link-status", "suggested")
        check(sug["total"] == 1, "accepting a suggestion confirms it in the database")
        pg.locator('.gnode[data-id="proj_s-eval"]').click()
        pg.wait_for_timeout(400)
        check(pg.locator(".gcard").count() == 1, "clicking a node shows its card")
        pg.screenshot(path=str(SHOTS / "e2e-graph-sel.png"))

        print("inspector relations")
        pg.evaluate("SY.select('ideas','idea_s-replay')")
        pg.wait_for_timeout(400)
        rels = pg.inner_text("#insp .rels")
        check("Agent 评测集搭建" in rels, "inspector lists links and backlinks")
        pg.screenshot(path=str(SHOTS / "e2e-insp.png"))

        print("settings")
        pg.evaluate("SY.closeInsp(); SY.acts['settings']()")
        pg.wait_for_timeout(500)
        check(pg.locator("#s-perm").count() == 1, "settings show AI access")
        pg.screenshot(path=str(SHOTS / "e2e-settings.png"))
        b.close()
finally:
    server.terminate()

errs = [e for e in errors if "favicon" not in e]
check(not errs, "no page errors" + (": " + " | ".join(errs[:5]) if errs else ""))
print("all good; screenshots in", SHOTS)
