#!/usr/bin/env python3
"""Regenerate the README screenshots in docs/screenshots/.

Runs the real GUI in Chromium against a fresh database (the same setup as the
end-to-end test), adds some content the way an AI agent would through the
`shiyi` CLI, captures each view, then frames the images like a macOS window.

Needs: cd core && cargo build --bins --examples; pip install playwright.
Run:   python3 scripts/screenshots.py
"""
import json, os, pathlib, subprocess, sys, tempfile, time
from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[1]
TMP = pathlib.Path(tempfile.mkdtemp())
RAW = TMP / "raw"
OUT = ROOT / "docs" / "screenshots"
DB = TMP / "shots.db"
PORT = 8798
CLI = ROOT / "core/target/debug/shiyi"
env = {**os.environ, "SHIYI_DB": str(DB), "SHIYI_FAKE_UPDATE": "none"}


def cli(*args, agent="claude"):
    r = subprocess.run([str(CLI), *args, "--as", agent, "--compact"], env=env, capture_output=True, text=True)
    return json.loads(r.stdout)


def apply(ops, agent="claude"):
    r = subprocess.run([str(CLI), "apply", "-", "--as", agent, "--compact"], env=env, input=json.dumps({"ops": ops}, ensure_ascii=False), capture_output=True, text=True)
    out = json.loads(r.stdout)
    assert out.get("ok"), out
    return out


subprocess.run([sys.executable, str(ROOT / "scripts/build-preview.py")], check=True)
RAW.mkdir(parents=True)
server = subprocess.Popen([sys.executable, str(ROOT / "scripts/e2e/serve.py"), "--db", str(DB), "--port", str(PORT)], env=env)
time.sleep(1.2)
try:
    with sync_playwright() as p:
        b = p.chromium.launch()
        ctx = b.new_context(viewport={"width": 1440, "height": 900}, device_scale_factor=2)
        # the screenshots show the Mac app (⌘ / ⌥⇧Space), whatever machine renders them
        ctx.add_init_script("Object.defineProperty(navigator, 'platform', { get: () => 'MacIntel' })")
        pg = ctx.new_page()
        pg.goto(f"http://127.0.0.1:{PORT}/")
        pg.wait_for_function("window.SY && SY.S.ready && SY.S.data.tasks.size > 0", timeout=10000)
        pg.wait_for_timeout(500)
        # the in-app look without the desktop window's translucent sidebar
        pg.evaluate("document.documentElement.classList.remove('is-app'); document.body.classList.remove('no-ai')")

        def shot(name, wait=700, toast=False):
            pg.wait_for_timeout(wait)
            if not toast:
                pg.evaluate("document.getElementById('toast').classList.remove('is-on')")
                pg.wait_for_timeout(350)  # let the fade-out finish
            pg.screenshot(path=str(RAW / f"{name}.png"))
            print("captured", name)

        def view(v, **ui):
            js = ";".join(f"SY.S.ui.{k} = {json.dumps(val)}" for k, val in ui.items())
            pg.evaluate(f"SY.closeInsp(); SY.closeModal(); {js}; SY.saveUI(); SY.setView('{v}')")

        # content an agent adds through the CLI
        apply([
            {"op": "idea.create", "ref": "regress", "text": "把 Agent 的失败案例沉淀成回归测试集，每次改 prompt 都跑一遍", "tags": ["agent", "评测"]},
            {"op": "idea.create", "ref": "chart", "text": "评测报告自动生成趋势图，一眼看出哪类工单在退步", "tags": ["agent"]},
            {"op": "idea.create", "ref": "ddia", "text": "读 DDIA 第 5 章「复制」，写一篇笔记", "tags": ["读书"]},
            {"op": "idea.create", "text": "给家里 NAS 做一份异地备份", "tags": ["生活"]},
            {"op": "link.create", "source": "$regress", "target": "proj_s-eval", "relation": "supports", "note": "回归集就是评测集的一部分"},
            {"op": "link.create", "source": "$chart", "target": "task_s-a7", "relation": "supports", "note": "报告模板里正好需要趋势图"},
            {"op": "link.create", "source": "$ddia", "target": "proj_s-read", "relation": "part_of", "note": "可以作为 Q4 的第一本"},
        ])
        pg.wait_for_timeout(1800)  # the GUI picks the agent's changes up by itself

        view("today")
        shot("today")

        # a few hand edits on the mind map, so the log shows a grouped batch
        view("mindmap", mapId="map_s-m1")
        pg.wait_for_timeout(700)
        for t in ["数据飞轮", "标注规范"]:
            pg.locator(".mnode", has_text="评测").first.click()
            pg.keyboard.press("Tab"); pg.wait_for_timeout(250)
            pg.keyboard.type(t); pg.keyboard.press("Enter"); pg.wait_for_timeout(400)
        pg.locator(".mnode.lv0").click()
        shot("mindmap")

        view("graph")
        pg.wait_for_timeout(1500)
        pg.locator('.gnode[data-id="proj_s-eval"]').click()
        shot("graph")

        view("project", projectId="proj_s-eval", ptab="gantt", ganttZoom="week")
        pg.wait_for_timeout(500)
        # show the whole project: from a day before it started
        pg.evaluate("(() => { const g = document.querySelector('#g-scroll'); if (!g) return; const from = SY.addDays(SY.TODAY(), -13); g.scrollLeft = Math.max(0, SY.dayDiff(g.dataset.start, from) * Number(g.dataset.px)); })()")
        shot("gantt")
        view("pert", scope="proj_s-eval")
        shot("pert", 1000)
        view("burndown", scope="proj_s-eval")
        shot("burndown")
        view("kanban", scope="all", kbSwim=False)
        shot("kanban")

        view("calendar", scope="all", calMonth=None)
        pg.evaluate("SY.S.ui.calMonth = SY.TODAY().slice(0, 7); SY.scheduleRender()")
        shot("calendar")

        # ⌘K: one box for everything
        view("today")
        pg.wait_for_timeout(400)
        pg.keyboard.press("Meta+k")
        pg.wait_for_selector("#pal-q")
        pg.keyboard.type("评测")
        shot("palette", 400)
        pg.keyboard.press("Escape")

        # capture an idea: related items are suggested without AI
        view("ideas", ideaFilter="inbox")
        pg.fill("#cap-text", "失败对话可以按工具调用逐步回放，方便定位评测里出错的步骤 #agent")
        pg.press("#cap-text", "Enter")
        pg.wait_for_timeout(500)
        pg.click('#toast [data-act="toast-action"]')
        shot("ideas", 900)

        # an agent change arrives while you look at the log
        view("log")
        pg.wait_for_timeout(900)
        cli("task", "add", "复盘本周评测结果", "--project", "Agent 评测集搭建", "--due", "fri", "--priority", "1")
        pg.wait_for_timeout(2000)
        shot("log", 300, toast=True)

        pg.evaluate("SY.acts['settings']()")
        pg.wait_for_timeout(500)
        pg.evaluate("(() => { const b = document.querySelector('#m-body'), s = [...b.querySelectorAll('.set-sec')].find(x => x.textContent.includes('AI 接入')); if (s) s.scrollIntoView({ block: 'start' }); })()")
        shot("settings-ai", 400)
        pg.evaluate("SY.closeModal()")

        pg.evaluate("SY.S.ui.theme = 'dark'; SY.saveUI(); SY.applyTheme()")
        view("graph")
        pg.wait_for_timeout(1200)
        shot("graph-dark")
        pg.evaluate("SY.S.ui.theme = 'system'; SY.saveUI(); SY.applyTheme()")
        b.close()
finally:
    server.terminate()

OUT.mkdir(parents=True, exist_ok=True)
subprocess.run([sys.executable, str(ROOT / "scripts/frame_screenshots.py"), str(RAW), str(OUT)], check=True)
print("done:", OUT)
