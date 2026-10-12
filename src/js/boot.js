/* 十一遗 boot: load data, seed marked examples on first run, wire app events */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { TODAY, addDays, now } = SY;

function seedExamples() {
  const D = 864e5, t0 = now(), td = TODAY(), d = n => addDays(td, n), ago = n => t0 - n * D;
  const P = (id, o) => ({ id, sample: true, goal: '', note: '', color: null, createdAt: ago(14), ...o });
  const projects = [
    P('proj_s-eval', { name: 'Agent 评测集搭建', area: 'work', color: 'lav', status: 'active', start: d(-12), due: d(16), goal: '覆盖 50 条真实工单的离线评测，每次发版前自动跑一遍' }),
    P('proj_s-mesh', { name: '书房网络改造', area: 'life', color: 'mint', status: 'active', start: d(-6), due: d(9), goal: '书房和卧室 Wi-Fi 稳定在 300Mbps 以上', fromIdea: 'idea_s-mesh' }),
    P('proj_s-read', { name: 'Q4 读书计划', area: 'life', color: 'peach', status: 'paused', start: d(-20), due: null, goal: '读完 3 本书，每本写一篇笔记', createdAt: ago(30) })
  ];
  const T = (id, title, projectId, status, o) => ({ id, title, projectId, status, priority: 0, start: null, due: null, todayDate: null, note: '', estO: null, estM: null, estP: null, deps: [], doneAt: null, sample: true, createdAt: ago(12), ...o });
  const tasks = [
    T('task_s-a1', '梳理评测目标和指标口径', 'proj_s-eval', 'done', { start: d(-12), due: d(-11), estO: 1, estM: 2, estP: 3, doneAt: ago(11), priority: 2 }),
    T('task_s-a2', '从工单系统导出近 3 个月失败对话', 'proj_s-eval', 'done', { start: d(-10), due: d(-8), estO: 2, estM: 3, estP: 4, deps: ['task_s-a1'], doneAt: ago(7) }),
    T('task_s-a3', '给样本标注期望的工具调用序列', 'proj_s-eval', 'doing', { start: d(-6), due: d(2), estO: 5, estM: 7, estP: 11, deps: ['task_s-a2'], priority: 1, todayDate: td, note: '先标 20 条，看标注口径是否一致' }),
    T('task_s-a4', '写 trajectory 打分脚本', 'proj_s-eval', 'todo', { start: d(-1), due: d(3), estO: 2, estM: 4, estP: 6, deps: ['task_s-a1'], priority: 2 }),
    T('task_s-a5', '跑第一轮基线评测', 'proj_s-eval', 'todo', { start: d(3), due: d(4), estO: 1, estM: 2, estP: 3, deps: ['task_s-a3', 'task_s-a4'], priority: 1 }),
    T('task_s-a6', '接入 CI，发版前自动评测', 'proj_s-eval', 'todo', { start: d(5), due: d(7), estO: 2, estM: 3, estP: 5, deps: ['task_s-a5'], createdAt: ago(9) }),
    T('task_s-a7', '整理评测报告模板', 'proj_s-eval', 'todo', { estO: 1, estM: 1, estP: 2, deps: ['task_s-a5'], createdAt: ago(4), priority: 3 }),
    T('task_s-b1', '测一下各房间现在的网速', 'proj_s-mesh', 'done', { start: d(-6), due: d(-6), estO: 0.5, estM: 1, estP: 1, doneAt: ago(5), createdAt: ago(6) }),
    T('task_s-b2', '对比三款 Mesh 路由器的价格和口碑', 'proj_s-mesh', 'todo', { start: d(-3), due: d(-1), estO: 1, estM: 2, estP: 3, deps: ['task_s-b1'], priority: 2, createdAt: ago(6) }),
    T('task_s-b3', '下单并约周末安装', 'proj_s-mesh', 'todo', { estO: 1, estM: 1, estP: 2, deps: ['task_s-b2'], createdAt: ago(6) }),
    T('task_s-b4', '重新规划书房插座和走线', 'proj_s-mesh', 'todo', { estO: 1, estM: 2, estP: 4, createdAt: ago(3) }),
    T('task_s-c1', '挑选第一本要读的书', 'proj_s-read', 'todo', { createdAt: ago(30) }),
    T('task_s-l1', '交物业费', null, 'todo', { due: td, priority: 1, createdAt: ago(2) }),
    T('task_s-l2', '买打印纸和墨盒', null, 'todo', { todayDate: td, createdAt: ago(1) })
  ];
  const I = (id, text, tags, status, a, o) => ({ id, text, note: '', tags, status, star: false, projectId: null, sample: true, createdAt: t0 - a, ...o });
  const H = 36e5;
  const ideas = [
    I('idea_s-replay', '给 Agent 加一个失败轨迹回放面板，按每次工具调用逐步重放，复盘时不用翻日志', ['agent', '工具'], 'inbox', 2 * H, { star: true, note: '数据可以直接用 [[从工单系统导出近 3 个月失败对话]] 的结果' }),
    I('idea_s-weekly', '周报能不能让 LLM 先从本周完成的任务里自动起草？', ['效率'], 'inbox', D + 3 * H, { note: '写作模板放进 [[整理一份常用 Prompt 模板库，按场景分类]]' }),
    I('idea_s-ride', '周末找条湖边绿道骑一圈，顺便试试新镜头', ['生活'], 'inbox', 3 * D),
    I('idea_s-prompt', '整理一份常用 Prompt 模板库，按场景分类', ['效率', 'prompt'], 'incubating', 9 * D, { note: '写代码、写文档、做评审各一套，附上好坏例子' }),
    I('idea_s-mesh', '家里路由器换成 Mesh，书房信号太差', ['生活'], 'converted', 12 * D, { projectId: 'proj_s-mesh' }),
    I('idea_s-digest', '把读过的技术文章自动摘要进知识库', ['知识管理'], 'inbox', 40 * D),
    I('idea_s-pomo', '试试番茄钟和任务清单结合的工作方式', ['效率'], 'archived', 20 * D)
  ];
  const nodes = {}; let seq = 0;
  const N = (text, parent, extra = {}) => { const id = 'n-s' + (seq++); nodes[id] = { id, text, parent, children: [], collapsed: false, color: null, link: null, ...extra }; if (parent) nodes[parent].children.push(id); return id; };
  const root = N('Agent 平台能力规划', null);
  const b1 = N('评测', root); N('离线评测集', b1); N('轨迹打分', b1, { link: { type: 'task', id: 'task_s-a4' } }); N('发版门禁', b1);
  const b2 = N('可观测性', root); N('Trace 回放', b2, { link: { type: 'idea', id: 'idea_s-replay' } }); N('成本看板', b2);
  const b3 = N('工具接入', root); N('MCP 连接器', b3); N('权限与审批', b3);
  const b4 = N('个人效率', root); N('全局快捷记录', b4); N('周报自动起草', b4, { link: { type: 'idea', id: 'idea_s-weekly' } });
  const maps = [{ id: 'map_s-m1', title: 'Agent 平台能力规划', rootId: root, nodes, projectId: 'proj_s-eval', sample: true, createdAt: ago(5) }];
  const L = (id, source, target, relation, note, status, o) => ({ id, source, target, relation, note, status, confidence: null, sample: true, createdAt: ago(1), ...o });
  const links = [
    L('link_s-1', 'idea_s-replay', 'proj_s-eval', 'supports', '回放面板正好用来复盘评测里失败的样本', 'confirmed'),
    L('link_s-2', 'idea_s-digest', 'idea_s-prompt', 'relates_to', '都在沉淀可复用的知识，摘要也可以按模板生成', 'suggested', { confidence: 0.7 })
  ];
  SY.withNote('示例数据', () => {
    projects.forEach(x => SY.put('projects', x)); ideas.forEach(x => SY.put('ideas', x)); tasks.forEach(x => SY.put('tasks', x));
    maps.forEach(x => SY.put('maps', x)); links.forEach(x => SY.put('links', x));
  });
}

async function boot() {
  SY.applyTheme();
  document.body.classList.toggle('no-ai', !SY.isApp);
  // web preview inside Claude: AI runs on the viewer's Claude account, export goes through the save dialog
  const C = window.claude;
  if (!SY.isApp && C && typeof C.use === 'function') {
    C.use('sample').then(fn => { if (fn) { SY.sampleFn = fn; document.body.classList.remove('no-ai'); } }).catch(() => {});
    C.use('downloads').then(d => { if (d) SY.dl = d; }).catch(() => {});
  }
  SY.renderAll();
  try { await SY.loadAll(); }
  catch (e) { S.saveErr = SY.errText(e); S.ready = true; SY.toast('读取数据失败：' + S.saveErr, { ms: 12000 }); }
  const empty = SY.COLS.every(c => S.data[c].size === 0);
  let fresh = false;
  try { fresh = SY.isApp ? !!(await SY.invoke('is_first_run')) : !SY.lsGet('shiyi.desk.data.v1'); } catch (e) { fresh = false; }
  if (empty && fresh) { seedExamples(); await SY.store.flush(); }
  SY.store.watch();
  if (S.ui.view === 'project' && !S.data.projects.has(S.ui.projectId) && S.ui.projectId !== '_loose') S.ui.view = 'projects';
  SY.renderAll();
  const T = window.__TAURI__;
  if (T && T.event && typeof T.event.listen === 'function') {
    T.event.listen('quick-capture', () => { SY.focusCapture(); }).catch(() => {});
  }
  setInterval(() => { if (!S.editing && !S.dragging && document.visibilityState === 'visible' && document.activeElement === document.body) SY.scheduleRender(); }, 60000);
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => SY.scheduleRender());
}
boot();
})();
