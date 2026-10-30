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


def SY_view(pg):
    return pg.evaluate("SY.S.ui.view")


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
        check(pg.evaluate("SY.graphOverlaps()") == 0, "no node or label overlaps another")
        lines = pg.evaluate("Math.max(...[...document.querySelectorAll('.gnode text')].map(t => t.querySelectorAll('tspan').length))")
        check(lines == 2, "long titles wrap onto two lines")
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
        check(pg.locator(".mp-card").count() == 12, "settings list the 12 model providers")
        check("使用中" in pg.inner_text("#mp-detail"), "the provider in use is selected")
        pg.screenshot(path=str(SHOTS / "e2e-settings.png"))

        print("reminders, diagnostics, feedback")
        pg.locator("#s-prefs").scroll_into_view_if_needed()
        check(pg.locator("#pf-daily").is_checked(), "daily summary is on by default")
        pg.select_option("#pf-time", "07:30"); pg.wait_for_timeout(300)
        check(pg.evaluate("SY.invoke('get_settings')")["prefs"]["dailyTime"] == "07:30", "summary time is saved")
        pg.uncheck("#pf-daily"); pg.wait_for_timeout(300)
        check(pg.evaluate("SY.invoke('get_settings')")["prefs"]["dailySummary"] is False and pg.locator("#pf-time").is_disabled(), "summary can be turned off")
        pg.check("#pf-daily"); pg.wait_for_timeout(300)
        pg.click('[data-act="pf-test"]'); pg.wait_for_timeout(300)
        check(pg.evaluate("SY.invoke('e2e_state')")["notified"] == 1 and "已发送" in pg.inner_text("#pf-out"), "test notification is sent")
        pg.evaluate("SY.noteError('e2e 测试报错', 'test')")
        pg.click('#s-about [data-act="diag-open"]'); pg.wait_for_selector("#diag-text")
        diag = pg.inner_text("#diag-text")
        check("v1.4.2" in diag and "任务 " in diag and "e2e 测试报错" in diag, "diagnostics list version, counts and recent errors")
        check("sk-test" not in diag and "失败轨迹回放面板" not in diag, "diagnostics hold no key and no note contents")
        pg.click('#m-foot [data-act="feedback"]'); pg.wait_for_selector("#fb-title")
        pg.fill("#fb-title", "导图节点拖不动"); pg.fill("#fb-body", "拖到另一个分支后弹回原位")
        pg.click('[data-act="feedback-go"]'); pg.wait_for_timeout(500)
        fb = pg.evaluate("SY.invoke('e2e_state')")["feedback"]
        check(fb and fb["title"] == "导图节点拖不动" and "弹回原位" in fb["body"] and "诊断信息" in fb["body"], "feedback opens a GitHub issue with the diagnostics")
        check(pg.locator("#modal").is_hidden(), "feedback dialog closes")

        print("⌘K search and commands")
        pg.keyboard.press("Control+k"); pg.wait_for_selector("#pal-q")
        pg.keyboard.type("打分"); pg.wait_for_timeout(150)
        check("写 trajectory 打分脚本" in pg.locator(".pal-row").first.inner_text(), "title matches come first")
        pg.keyboard.press("Enter"); pg.wait_for_timeout(300)
        check(pg.evaluate("SY.S.sel && SY.S.sel.id") == "task_s-a4" and pg.locator("#pal").count() == 0, "Enter opens the task and closes the box")
        pg.keyboard.press("Escape"); pg.wait_for_timeout(100)
        pg.keyboard.press("Control+k"); pg.wait_for_selector("#pal-q")
        check("写 trajectory 打分脚本" in pg.inner_text("#pal-list"), "recently opened shows with an empty box")
        pg.keyboard.type("标注口径"); pg.wait_for_timeout(150)
        row = pg.locator('.pal-row[data-k="task"]').first
        check("给样本标注期望的工具调用序列" in row.inner_text() and row.locator(".pal-s mark").count() > 0, "a note match shows where it matched")
        pg.fill("#pal-q", "MCP"); pg.wait_for_timeout(150); pg.keyboard.press("Enter"); pg.wait_for_timeout(600)
        node = pg.evaluate("(() => { const m = SY.S.data.maps.get(SY.S.ui.mapId); return m && m.nodes[SY.mmSel()] && m.nodes[SY.mmSel()].text; })()")
        check(SY_view(pg) == "mindmap" and node == "MCP 连接器", "a mind-map node opens selected in its map")
        pg.keyboard.press("Control+k"); pg.fill("#pal-q", "深色"); pg.wait_for_timeout(150); pg.keyboard.press("Enter"); pg.wait_for_timeout(200)
        check(pg.evaluate("SY.S.ui.theme") == "dark", "commands run from the box")
        pg.evaluate("SY.S.ui.theme = 'system'; SY.saveUI(); SY.applyTheme()")
        pg.keyboard.press("Control+k"); pg.fill("#pal-q", "下周去湖边爬山"); pg.wait_for_timeout(150)
        check(pg.locator(".pal-row").first.get_attribute("data-k") == "new", "no match offers to write it down")
        pg.keyboard.press("Enter"); pg.wait_for_timeout(800)
        check(cli("search", "下周去湖边爬山")["total"] == 1, "…and Enter saves it as an idea")
        pg.keyboard.press("Control+k"); pg.wait_for_selector("#pal-q"); pg.keyboard.press("Escape"); pg.wait_for_timeout(100)
        check(pg.locator("#pal").count() == 0, "Esc closes the box")
        pg.keyboard.press("Control+n"); pg.wait_for_timeout(100)
        check(pg.evaluate("document.activeElement.id") == "cap-text", "⌘N focuses the capture box")
        pg.evaluate("document.activeElement.blur()")
        # 3000 extra items, kept in memory only
        pg.evaluate("""(() => { for (let i = 0; i < 3000; i++) SY.S.data.tasks.set('perf' + i, { id: 'perf' + i, title: '压力测试任务 ' + i + ' 评测 导图', note: '第 ' + i + ' 条备注', status: i % 3 ? 'todo' : 'done', deps: [], createdAt: Date.now(), updatedAt: Date.now() }); SY.S.rev++; })()""")
        pg.keyboard.press("Control+k"); pg.wait_for_selector("#pal-q")
        ms = pg.evaluate("(() => { const i = document.getElementById('pal-q'); const t0 = performance.now(); for (const q of ['压', '压力', '压力 评测', '第 2999']) { i.value = q; i.dispatchEvent(new Event('input')); } return performance.now() - t0; })()")
        check(ms < 400 and pg.locator(".pal-row").count() > 0, f"search over 3000 more items stays fast (4 searches + redraws: {ms:.0f} ms)")
        pg.keyboard.press("Escape")
        pg.evaluate("(() => { for (let i = 0; i < 3000; i++) SY.S.data.tasks.delete('perf' + i); SY.S.rev++; SY.closeInsp(); })()")

        print("today: carried over, next 7 days, reschedule")
        subprocess.run([str(CLI), "apply", "-", "--as", "claude", "--compact"], env=env, capture_output=True, text=True,
                       input=json.dumps({"ops": [{"op": "task.create", "ref": "c", "title": "昨天计划没做完的事", "today": "-1d"},
                                                 {"op": "task.create", "title": "后天到期的事", "due": "+2d"}]}))
        pg.wait_for_timeout(1800)
        pg.evaluate("SY.closeModal(); SY.closeInsp(); SY.setView('today')"); pg.wait_for_timeout(400)
        check("昨天计划没做完的事" in pg.inner_text(".carried"), "a task planned yesterday and not done shows under 之前计划")
        check("后天到期的事" in pg.inner_text(".upcoming"), "a task due in two days shows under 接下来 7 天")
        pg.click('[data-act="carry-all"]'); pg.wait_for_timeout(800)
        check(cli("get", "昨天计划没做完的事")["item"].get("today_date") == pg.evaluate("SY.TODAY()") and pg.locator(".carried").count() == 0, "全部放进今天 plans them for today")
        over = pg.evaluate("[...SY.S.data.tasks.values()].find(t => t.status !== 'done' && t.due && t.due < SY.TODAY())")
        pg.hover(f'.trow[data-open="tasks:{over["id"]}"]')
        pg.click(f'.trow[data-open="tasks:{over["id"]}"] [data-act="resched"][data-v="tomorrow"]'); pg.wait_for_timeout(800)
        check(cli("get", over["id"])["item"]["due_date"] == pg.evaluate("SY.addDays(SY.TODAY(), 1)"), "an overdue task moves to tomorrow in one click")

        print("done tasks, global undo")
        pg.evaluate("SY.setView('kanban')"); pg.wait_for_timeout(400)
        done_text = pg.inner_text('.kb-col[data-col="done"]')
        check("逾期" not in done_text and "完成于" in done_text, "finished tasks say when they were done, not 逾期")
        pg.evaluate("SY.setView('today')"); pg.wait_for_timeout(300)
        pg.click('.trow[data-open="tasks:task_s-l2"] [data-act="toggle-done"]'); pg.wait_for_timeout(700)
        pg.evaluate("document.activeElement && document.activeElement.blur()")
        pg.keyboard.press("Control+z"); pg.wait_for_timeout(1000)
        check(cli("get", "task_s-l2")["item"]["status"] == "todo", "⌘Z undoes your last change")
        pg.keyboard.press("Control+Shift+z"); pg.wait_for_timeout(1000)
        check(cli("get", "task_s-l2")["item"]["status"] == "done", "⇧⌘Z redoes it")

        print("recently deleted, restore from backup")
        pg.evaluate("SY.invoke('e2e_backup_daily')")
        pg.evaluate("SY.del('projects', 'proj_s-read')"); pg.wait_for_timeout(800)
        check(cli("get", "proj_s-read").get("ok") is False or cli("get", "proj_s-read")["item"].get("deleted_at"), "deleting a project")
        pg.evaluate("SY.setView('log')"); pg.wait_for_timeout(300)
        pg.click('[data-act="op-filter"][data-v="trash"]'); pg.wait_for_timeout(900)
        row = pg.locator('.trash-row:has-text("Q4 读书计划")')
        check(row.count() == 1 and "及其 1 个任务" in row.inner_text(), "最近删除 lists the project with its task")
        row.locator('[data-act="trash-restore"]').click(); pg.wait_for_timeout(1000)
        check(pg.evaluate("SY.S.data.projects.has('proj_s-read') && SY.S.data.tasks.has('task_s-c1')"), "restoring brings back the project and its task")
        pg.click('[data-act="op-filter"][data-v="all"]'); pg.wait_for_timeout(300)
        n_before = pg.evaluate("SY.S.data.ideas.size")
        pg.fill("#cap-text", "备份之后才记下的灵感"); pg.press("#cap-text", "Enter"); pg.wait_for_timeout(900)
        pg.evaluate("SY.acts['settings']()"); pg.wait_for_timeout(500)
        pg.click('[data-act="backups"]'); pg.wait_for_selector(".bk-row"); pg.wait_for_timeout(200)
        pg.locator('.bk-row:has(.pill:text("每日备份")) [data-act="backup-restore"]').first.click(); pg.wait_for_timeout(300)
        pg.click('[data-act="ask-yes"]'); pg.wait_for_timeout(1500)
        check(cli("search", "备份之后才记下的灵感")["total"] == 0, "restoring the backup puts the data back to that day")
        check(any(b["kind"] == "restore" for b in pg.evaluate("SY.invoke('list_backups')")), "the data from before the restore is kept as a backup")
        pg.click('#toast [data-act="toast-action"]'); pg.wait_for_timeout(1500)
        check(cli("search", "备份之后才记下的灵感")["total"] == 1 and pg.evaluate("SY.S.data.ideas.size") == n_before + 1, "撤销恢复 brings the newer data back")

        print("ideas as rows")
        pg.evaluate("SY.closeModal(); SY.S.ui.ideaFilter = 'all'; SY.setView('ideas')"); pg.wait_for_timeout(400)
        check(pg.locator(".ilist .irow").count() > 5 and pg.locator(".ideas .idea").count() == 0, "ideas are listed one per row")

        print("calendar")
        pg.evaluate("SY.closeModal(); SY.S.ui.calMonth = SY.TODAY().slice(0, 7); SY.setView('calendar')")
        pg.wait_for_timeout(500)
        check(pg.locator(".cal-bar").count() > 0, "calendar shows tasks")
        today_cell = pg.locator(f'.cal-cell[data-date="{pg.evaluate("SY.TODAY()")}"]')
        today_cell.scroll_into_view_if_needed(); pg.wait_for_timeout(200)  # late in the month today is in the last row, below the fold
        cell = today_cell.bounding_box()
        pg.mouse.dblclick(cell["x"] + 30, cell["y"] + cell["height"] - 12)
        pg.wait_for_timeout(200)
        pg.keyboard.type("日历测试任务"); pg.keyboard.press("Enter"); pg.wait_for_timeout(1500)
        got = cli("get", "日历测试任务")
        doc = got.get("item") or got.get("doc") or got
        check(pg.evaluate("SY.TODAY()") in json.dumps(doc), "double-click on a day adds a task due that day (seen by the CLI)")
        pg.screenshot(path=str(SHOTS / "e2e-calendar.png"))

        print("smallest window (1080×680)")
        sm = b.new_page(viewport={"width": 1080, "height": 680})
        sm.on("pageerror", lambda e: errors.append(str(e)))
        sm.goto(f"http://127.0.0.1:{PORT}/"); sm.wait_for_function("SY.S.ready && SY.S.data.tasks.size > 0"); sm.wait_for_timeout(500)
        foot = sm.locator('#side-foot [data-act="settings"]').bounding_box()
        check(foot and foot["y"] + foot["height"] <= 680, "the sidebar's 设置 button is on screen")
        sm.evaluate("SY.S.ui.calMonth = SY.TODAY().slice(0, 7); SY.setView('calendar')"); sm.wait_for_timeout(800)
        gb = sm.evaluate("document.querySelector('.cal-grid').getBoundingClientRect().bottom")
        check(gb <= 680, f"the whole month fits ({gb:.0f}px of 680)")
        sm.evaluate("SY.setView('pert')"); sm.wait_for_timeout(800)
        k = sm.evaluate("Math.min(...Object.values(SY.S.view.pert).map(st => st.k))")
        check(k >= 0.69, f"the network view opens at a readable zoom ({k:.2f})")
        sm.close()
        b.close()
finally:
    server.terminate()

errs = [e for e in errors if "favicon" not in e]
check(not errs, "no page errors" + (": " + " | ".join(errs[:5]) if errs else ""))
print("all good; screenshots in", SHOTS)
