/* 拾忆 core: utilities, state, storage, render loop, inspector, modal, settings */
(() => {
'use strict';
const SY = window.SY = { views: {}, mods: {}, acts: {}, changeHandlers: [] };

/* ================= utilities ================= */
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const uid = () => Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
const pad = n => String(n).padStart(2, '0');
const ymd = d => d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate());
const parseYmd = s => { const [y, m, d] = String(s).split('-').map(Number); return new Date(y, m - 1, d); };
const addDays = (s, n) => { const d = parseYmd(s); d.setDate(d.getDate() + n); return ymd(d); };
const dayDiff = (a, b) => { const [y1, m1, d1] = a.split('-').map(Number), [y2, m2, d2] = b.split('-').map(Number); return Math.round((Date.UTC(y2, m2 - 1, d2) - Date.UTC(y1, m1 - 1, d1)) / 864e5); };
const TODAY = () => ymd(new Date());
const tsDay = ts => ymd(new Date(ts));
const now = () => Date.now();
const WD = '日一二三四五六';
const clamp = (v, a, b) => Math.max(a, Math.min(b, v));
const num = v => (v === '' || v == null || isNaN(Number(v)) ? null : Number(v));
const isYmd = s => typeof s === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(s);
function lsGet(k) { try { return JSON.parse(localStorage.getItem(k) || 'null'); } catch (e) { return null; } }
function lsSet(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch (e) {} }
function isoWeek(d) {
  const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  const dn = (t.getUTCDay() + 6) % 7; t.setUTCDate(t.getUTCDate() - dn + 3);
  const fy = new Date(Date.UTC(t.getUTCFullYear(), 0, 4));
  return 1 + Math.round(((t - fy) / 864e5 - 3 + ((fy.getUTCDay() + 6) % 7)) / 7);
}
function weekStartTs() { const d = new Date(); d.setHours(0, 0, 0, 0); d.setDate(d.getDate() - (d.getDay() + 6) % 7); return d.getTime(); }
const ageDays = ts => Math.floor((now() - (ts || now())) / 864e5);
function ago(ts) {
  if (!ts) return '';
  const s = (now() - ts) / 1000;
  if (s < 60) return '刚刚';
  if (s < 3600) return Math.floor(s / 60) + ' 分钟前';
  if (s < 86400) return Math.floor(s / 3600) + ' 小时前';
  const d = Math.floor(s / 86400);
  if (d < 45) return d + ' 天前';
  const dt = new Date(ts); return dt.getFullYear() + '/' + (dt.getMonth() + 1) + '/' + dt.getDate();
}
function dueLabel(due) {
  const n = dayDiff(TODAY(), due);
  if (n === 0) return '今天';
  if (n === 1) return '明天';
  if (n === -1) return '昨天';
  if (n < 0) return '逾期 ' + (-n) + ' 天';
  if (n < 7) return '周' + WD[parseYmd(due).getDay()];
  const [, m, d] = due.split('-'); return Number(m) + '/' + Number(d);
}
const md = s => { const [, m, d] = s.split('-'); return Number(m) + '/' + Number(d); };
const fmtTs = ts => { if (!ts) return ''; const d = new Date(ts); return ymd(d) + ' ' + pad(d.getHours()) + ':' + pad(d.getMinutes()); };

const IC = {
  sun: '<circle cx="8" cy="8" r="3"/><path d="M8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M3.4 12.6l1.1-1.1M11.5 4.5l1.1-1.1"/>',
  bulb: '<path d="M6 12.2h4M6.6 14.3h2.8M8 1.8a4.2 4.2 0 0 0-2.6 7.5c.5.4.7.9.7 1.5v.4h3.8v-.4c0-.6.2-1.1.7-1.5A4.2 4.2 0 0 0 8 1.8z"/>',
  folder: '<path d="M1.8 4.2c0-.6.4-1 1-1h3.3l1.5 1.6h5.6c.6 0 1 .4 1 1v6.9c0 .6-.4 1-1 1H2.8c-.6 0-1-.4-1-1z"/>',
  cols: '<rect x="1.8" y="2.5" width="3.6" height="11" rx="1.2"/><rect x="6.2" y="2.5" width="3.6" height="7.5" rx="1.2"/><rect x="10.6" y="2.5" width="3.6" height="9.5" rx="1.2"/>',
  gantt: '<path d="M2 2.5v11h12"/><rect x="4" y="4" width="5" height="2" rx="1"/><rect x="6.5" y="7.3" width="6" height="2" rx="1"/><rect x="5" y="10.6" width="4" height="2" rx="1"/>',
  burn: '<path d="M2 2.5v11h12"/><path d="M3.8 4.5l2.8 2.2 2.4-.6 4 5.2"/>',
  net: '<circle cx="3.5" cy="8" r="1.8"/><circle cx="12.5" cy="3.8" r="1.8"/><circle cx="12.5" cy="12.2" r="1.8"/><path d="M5.2 7.2l5.6-2.7M5.2 8.8l5.6 2.7"/>',
  mind: '<rect x="5.6" y="6.4" width="4.8" height="3.2" rx="1.6"/><path d="M5.6 8H3.5M10.4 8h2.1M3.5 8V4.2h-1M3.5 8v3.8h-1M12.5 8V4.2h1M12.5 8v3.8h1"/>',
  spark: '<path d="M8 1.6l1.3 4.1 4.1 1.3-4.1 1.3L8 12.4 6.7 8.3 2.6 7l4.1-1.3z"/><path d="M12.6 11.2l.5 1.4 1.4.5-1.4.5-.5 1.4-.5-1.4-1.4-.5 1.4-.5z"/>',
  star: '<path d="M8 1.9l1.85 3.85 4.2.55-3.07 2.9.77 4.17L8 11.35l-3.75 2.02.77-4.17L1.95 6.3l4.2-.55z"/>',
  check: '<path d="M3.6 8.4l2.9 2.9 5.9-6.6"/>',
  plus: '<path d="M8 3v10M3 8h10"/>',
  minus: '<path d="M3 8h10"/>',
  x: '<path d="M4 4l8 8M12 4l-8 8"/>',
  arrow: '<path d="M3 8h10M9 4l4 4-4 4"/>',
  back: '<path d="M13 8H3M7 4L3 8l4 4"/>',
  cal: '<path d="M2.5 3.8h11v9.7h-11zM2.5 6.8h11M5.5 2.2v3M10.5 2.2v3"/>',
  archive: '<path d="M2.2 3.2h11.6v3H2.2zM3.2 6.2v6.6h9.6V6.2M6.3 8.8h3.4"/>',
  trash: '<path d="M2.8 4.3h10.4M6.2 4.3V2.8h3.6v1.5M4.3 4.3l.6 9.2h6.2l.6-9.2"/>',
  task: '<path d="M2.5 3.5h11v9h-11z"/><path d="M5 8l2 2 4-4"/>',
  down: '<path d="M8 2.5v8M4.6 7.2L8 10.5l3.4-3.3M3 13.5h10"/>',
  up: '<path d="M8 10.5v-8M4.6 5.8L8 2.5l3.4 3.3M3 13.5h10"/>',
  reopen: '<path d="M3 8a5 5 0 1 0 1.5-3.6M3 2.8v2.6h2.6"/>',
  edit: '<path d="M10.8 2.6l2.6 2.6-7.9 7.9H2.9v-2.6z"/>',
  fit: '<path d="M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10"/>',
  undo: '<path d="M5.5 4.5L2.5 7.5l3 3"/><path d="M2.5 7.5h7a4 4 0 0 1 0 8h-2"/>',
  redo: '<path d="M10.5 4.5l3 3-3 3"/><path d="M13.5 7.5h-7a4 4 0 0 0 0 8h2"/>',
  today: '<path d="M8 2v12"/><circle cx="8" cy="8" r="2.4"/>',
  link: '<path d="M6.8 9.2a2.6 2.6 0 0 0 3.7 0l2.2-2.2a2.6 2.6 0 0 0-3.7-3.7l-.9.9"/><path d="M9.2 6.8a2.6 2.6 0 0 0-3.7 0L3.3 9a2.6 2.6 0 0 0 3.7 3.7l.9-.9"/>',
  gear: '<circle cx="8" cy="8" r="2.2"/><path d="M8 1.6v1.8M8 12.6v1.8M1.6 8h1.8M12.6 8h1.8M3.5 3.5l1.3 1.3M11.2 11.2l1.3 1.3M3.5 12.5l1.3-1.3M11.2 4.8l1.3-1.3"/>',
  graph: '<circle cx="3.6" cy="4.2" r="1.7"/><circle cx="12.2" cy="3.6" r="1.5"/><circle cx="8.6" cy="12" r="2"/><circle cx="13" cy="10.4" r="1.1"/><path d="M5.2 4.9l5.5-1M4.6 5.6l3 4.7M10 3.6L9 10M10.5 11.5l1.6-.7"/>',
  clock: '<circle cx="8" cy="8" r="6"/><path d="M8 4.6V8l2.4 1.6"/>',
  shield: '<path d="M8 1.8l5 1.9v4.1c0 3-2.1 5.3-5 6.4-2.9-1.1-5-3.4-5-6.4V3.7z"/><path d="M5.8 8.1l1.6 1.6 2.9-3.1"/>',
  term: '<rect x="1.8" y="2.6" width="12.4" height="10.8" rx="1.6"/><path d="M4.4 6.2l2.2 1.8-2.2 1.8M8.4 10.2h3.2"/>',
  copy: '<rect x="5.2" y="5.2" width="8.4" height="8.4" rx="1.4"/><path d="M10.8 5.2V3.6c0-.6-.4-1.1-1-1.1H3.6c-.6 0-1.1.5-1.1 1.1v6.2c0 .6.5 1 1.1 1h1.6"/>',
  wand: '<path d="M3 13l7.5-7.5M9.2 3.2l.5 1.3 1.3.5-1.3.5-.5 1.3-.5-1.3-1.3-.5 1.3-.5zM12.8 7l.4.9.9.4-.9.4-.4.9-.4-.9-.9-.4.9-.4z"/>'
};
const icon = (n, size = 16) => `<svg width="${size}" height="${size}" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${IC[n] || ''}</svg>`;

Object.assign(SY, { $, $$, esc, uid, pad, ymd, parseYmd, addDays, dayDiff, TODAY, tsDay, now, WD, clamp, num, isYmd, lsGet, lsSet, isoWeek, weekStartTs, ageDays, ago, dueLabel, md, fmtTs, icon });

/* ================= constants & state ================= */
const COLS = ['ideas', 'projects', 'tasks', 'maps', 'links'];
const IDEA_ST = { inbox: '收件箱', incubating: '孵化中', converted: '已转化', archived: '归档' };
const TASK_ST = { todo: '待办', doing: '进行中', done: '已完成' };
const PROJ_ST = { active: '进行中', paused: '暂停', done: '已完成' };
const AREA = { work: '工作', life: '生活' };
const PALETTE = ['lav', 'peach', 'mint', 'butter', 'sky', 'rose'];
const LS_UI = 'shiyi.desk.ui.v1', LS_DATA = 'shiyi.desk.data.v1';
Object.assign(SY, { COLS, IDEA_ST, TASK_ST, PROJ_ST, AREA, PALETTE });

const S = SY.S = {
  data: { ideas: new Map(), projects: new Map(), tasks: new Map(), maps: new Map(), links: new Map() },
  rev: 0, seq: 0, permission: 'write', unseenAgent: 0,
  ready: false, saveErr: null, editing: false, dragging: false,
  ui: Object.assign({
    view: 'today', projectId: null, ptab: 'board', scope: 'all', ideaFilter: 'inbox', tag: '', q: '',
    projArea: 'all', projStatus: 'active', capMode: 'idea', capProject: '', ganttZoom: 'day', ganttHideDone: false,
    burnMetric: 'count', kbSwim: false, mapId: null, theme: 'system', skin: 'native'
  }, lsGet(LS_UI) || {}),
  view: {}, // transient per-view state (scroll, pan/zoom)
  sel: null
};
S.ui.q = '';
function saveUI() { const u = { ...S.ui }; delete u.q; lsSet(LS_UI, u); }
SY.saveUI = saveUI;

const T = window.__TAURI__;
const invoke = T && T.core && typeof T.core.invoke === 'function' ? T.core.invoke : null;
SY.invoke = invoke;
SY.isApp = !!invoke;

/* ================= storage =================
   All writes go through put / patch / remove, which record the change for
   SY.store (js/store.js): in the app it becomes an operation on the shared
   SQLite database (the same one AI agents reach through the `shiyi` CLI),
   in the web preview it is kept in this browser with a local operation log. */
const loadAll = () => SY.store.load();
const flush = () => SY.store.flush();
window.addEventListener('beforeunload', () => { SY.store.flush(); });
const list = col => [...S.data[col].values()];
function clean(o) { const r = {}; for (const k in o) r[k] = o[k] === undefined ? null : o[k]; return r; }
function put(col, doc) { const before = S.data[col].get(doc.id) || null; doc = clean({ ...doc, updatedAt: now() }); S.data[col].set(doc.id, doc); SY.store.record(col, before, doc, null); changed(); return doc; }
function patch(col, id, part) { const o = S.data[col].get(id); if (!o) return null; const n = clean({ ...o, ...part, updatedAt: now() }); S.data[col].set(id, n); SY.store.record(col, o, n, Object.keys(part)); changed(); return n; }
function remove(col, id) { const o = S.data[col].get(id); if (!o) return; S.data[col].delete(id); SY.store.record(col, o, null, null); changed(); }
function changed() { S.rev++; SY.store.schedule(); scheduleRender(); }
Object.assign(SY, { list, put, patch, remove, flush, loadAll, changed });

/* ================= domain helpers ================= */
const getP = id => (id ? S.data.projects.get(id) : null);
const pColor = p => (p && PALETTE.includes(p.color) ? p.color : p && p.area === 'life' ? 'mint' : 'lav');
const cv = (c, k = '') => `var(--${c}${k})`;
const prR = p => (p > 0 ? p : 9);
function taskSort(a, b) {
  const so = { doing: 0, todo: 1, done: 2 };
  return (so[a.status] - so[b.status]) || (prR(a.priority) - prR(b.priority)) ||
    ((a.due || '9999') < (b.due || '9999') ? -1 : (a.due || '9999') > (b.due || '9999') ? 1 : 0) ||
    ((a.createdAt || 0) - (b.createdAt || 0));
}
function scopeTasks(scope) {
  const all = list('tasks');
  if (!scope || scope === 'all') return all;
  if (scope === '_loose') return all.filter(t => !t.projectId || !S.data.projects.has(t.projectId));
  return all.filter(t => t.projectId === scope);
}
function est(t) {
  const o = num(t.estO), m = num(t.estM), p = num(t.estP);
  if (o == null && m == null && p == null) return { te: 1, v: 0, set: false };
  const known = [o, p].filter(x => x != null);
  const mm = m != null ? m : known.reduce((a, b) => a + b, 0) / known.length;
  const oo = o != null ? o : mm, pp = p != null ? p : mm;
  return { te: Math.max(0, (oo + 4 * mm + pp) / 6), v: Math.pow((pp - oo) / 6, 2), set: true };
}
const dur = t => Math.max(1, Math.round(est(t).te));
function span(t) {
  const d = dur(t);
  const s = isYmd(t.start) ? t.start : (isYmd(t.due) ? addDays(t.due, -(d - 1)) : null);
  let e = isYmd(t.due) ? t.due : (isYmd(t.start) ? addDays(t.start, d - 1) : null);
  if (s && e && e < s) e = s;
  return { s, e, derived: !(isYmd(t.start) && isYmd(t.due)) };
}
function pStats(pid) {
  const ts = scopeTasks(pid);
  const done = ts.filter(t => t.status === 'done').length, doing = ts.filter(t => t.status === 'doing').length;
  const open = ts.filter(t => t.status !== 'done').sort(taskSort);
  const p = getP(pid);
  const last = Math.max(p?.updatedAt || 0, ...ts.map(t => t.updatedAt || 0));
  return { total: ts.length, done, doing, todo: ts.length - done - doing, open: open.length, next: open[0] || null, last, tasks: ts };
}
function succMap() {
  const m = new Map();
  for (const t of list('tasks')) for (const d of (t.deps || [])) { if (!m.has(d)) m.set(d, []); m.get(d).push(t.id); }
  return m;
}
function reaches(src, dst, sm = succMap()) {
  const st = [src], seen = new Set();
  while (st.length) { const x = st.pop(); if (x === dst) return true; if (seen.has(x)) continue; seen.add(x); (sm.get(x) || []).forEach(y => st.push(y)); }
  return false;
}
const canAddDep = (taskId, depId) => taskId !== depId && !reaches(taskId, depId);
function addDep(taskId, depId) {
  const t = S.data.tasks.get(taskId); if (!t) return false;
  if ((t.deps || []).includes(depId)) return true;
  if (!canAddDep(taskId, depId)) { toast('这样会形成循环依赖，没有添加。'); return false; }
  patch('tasks', taskId, { deps: [...(t.deps || []), depId] }); return true;
}
function removeDep(taskId, depId) { const t = S.data.tasks.get(taskId); if (t) patch('tasks', taskId, { deps: (t.deps || []).filter(d => d !== depId) }); }
const hasSample = () => COLS.some(c => list(c).some(d => d.sample));

/* forward-pass scheduling from today using deps + estimates */
function autoSchedule(scope, { force = false } = {}) {
  const ts = scopeTasks(scope).filter(t => t.status !== 'done');
  const ids = new Set(ts.map(t => t.id));
  const p = getP(scope);
  const base = p && isYmd(p.start) && p.start > TODAY() ? p.start : TODAY();
  const indeg = new Map(ts.map(t => [t.id, 0]));
  const succ = new Map();
  ts.forEach(t => (t.deps || []).filter(d => ids.has(d)).forEach(d => { indeg.set(t.id, indeg.get(t.id) + 1); if (!succ.has(d)) succ.set(d, []); succ.get(d).push(t.id); }));
  const q = ts.filter(t => indeg.get(t.id) === 0).sort(taskSort).map(t => t.id);
  const ef = new Map(); let n = 0; const order = [];
  while (q.length) { const id = q.shift(); order.push(id); (succ.get(id) || []).forEach(s => { indeg.set(s, indeg.get(s) - 1); if (indeg.get(s) === 0) q.push(s); }); }
  for (const id of order) {
    const t = S.data.tasks.get(id); const d = dur(t);
    const es = Math.max(0, ...((t.deps || []).filter(x => ef.has(x)).map(x => ef.get(x))));
    if (!force && isYmd(t.start)) { const off = Math.max(0, dayDiff(base, t.start)); ef.set(id, off + (isYmd(t.due) ? Math.max(1, dayDiff(t.start, t.due) + 1) : d)); continue; }
    const start = addDays(base, es), due = addDays(start, d - 1);
    if (t.start !== start || t.due !== due) { patch('tasks', id, { start, due }); n++; }
    ef.set(id, es + d);
  }
  return { n, cyclic: order.length < ts.length };
}
Object.assign(SY, { getP, pColor, cv, taskSort, scopeTasks, est, dur, span, pStats, succMap, reaches, canAddDep, addDep, removeDep, autoSchedule, hasSample });

/* ================= render loop ================= */
let rq = 0;
function scheduleRender() { if (rq) return; rq = requestAnimationFrame(() => { rq = 0; renderAll(); }); }
SY.scheduleRender = scheduleRender;
function renderAll() { renderNav(); renderCapture(); renderView(); renderInspector(); }
SY.renderAll = renderAll;

const NAV = [
  ['工作区', [['today', 'sun', '今日', 'butter'], ['ideas', 'bulb', '灵感', 'peach'], ['projects', 'folder', '项目', 'lav']]],
  ['视图', [['kanban', 'cols', '看板', 'sky'], ['gantt', 'gantt', '甘特图', 'mint'], ['burndown', 'burn', '燃尽图', 'rose'], ['pert', 'net', '网络图 / PERT', 'lav'], ['mindmap', 'mind', '思维导图', 'peach']]],
  ['知识与 AI', [['graph', 'graph', '知识图谱', 'sky'], ['log', 'clock', '操作记录', 'butter']]]
];
SY.NAV = NAV;
function navCounts() {
  const today = TODAY(); const open = list('tasks').filter(t => t.status !== 'done');
  return {
    today: open.filter(t => t.todayDate === today || (t.due && t.due <= today)).length,
    ideas: list('ideas').filter(i => i.status === 'inbox').length,
    projects: list('projects').filter(p => p.status === 'active').length,
    kanban: open.length, mindmap: S.data.maps.size,
    graph: list('links').filter(l => l.status === 'suggested').length || '', log: S.unseenAgent || ''
  };
}
function renderNav() {
  const c = navCounts(), v = S.ui.view;
  const on = k => v === k || (k === 'projects' && v === 'project');
  $('#nav').innerHTML = NAV.map(([h, items]) => `<div class="nav-h">${h}</div>` + items.map(([k, ic, l, col]) =>
    `<button type="button" data-act="view" data-v="${k}" class="${on(k) ? 'is-on' : ''}" ${on(k) ? 'aria-current="page"' : ''}><span class="ic" style="--c:${cv(col)};--c-soft:${cv(col, '-soft')};--c-ink:${cv(col, '-ink')}">${icon(ic)}</span><span>${l}</span><span class="n">${c[k] || ''}</span></button>`).join('')).join('');
  renderStatus();
}
function renderStatus() {
  const el = $('#side-foot'); if (!el) return;
  const cls = S.saveErr ? 'is-err' : SY.isApp ? '' : 'is-local';
  const txt = S.saveErr ? '保存出错：' + S.saveErr : SY.isApp ? '已保存到本机' : '网页预览 · 数据存在浏览器里';
  el.innerHTML = `<span class="status ${cls}"><i></i>${txt}</span>
    <div style="display:flex;gap:6px;flex-wrap:wrap">
      <button class="btn btn-sm btn-ghost" type="button" data-act="export">${icon('down', 14)}导出</button>
      <button class="btn btn-sm btn-ghost" type="button" data-act="import">${icon('up', 14)}导入</button>
      <button class="btn btn-sm btn-ghost" type="button" data-act="settings">${icon('gear', 14)}设置</button>
    </div>
    <div style="line-height:1.7">⌘K 快速记录 · ⌘1–⌘8 切换<br>Esc 关闭详情</div>`;
}
function renderCapture() {
  const f = $('#capture'); const task = S.ui.capMode === 'task';
  f.classList.toggle('is-task', task);
  $('#cm-idea').classList.toggle('is-on', !task); $('#cm-task').classList.toggle('is-on', task);
  $('#cap-text').placeholder = task ? '要做的一件事，回车添加（无截止日期时在「今日」里会直接放进今天）' : '记下一个想法，#标签 自动归类，回车保存；以 /t 开头直接记成任务';
  const sel = $('#cap-project');
  if (document.activeElement !== sel) {
    const want = S.ui.view === 'project' && S.ui.projectId && S.ui.projectId !== '_loose' ? S.ui.projectId : S.ui.capProject;
    const ps = list('projects').filter(p => p.status !== 'done');
    sel.innerHTML = `<option value="">零散任务（无项目）</option>` + ps.map(p => `<option value="${esc(p.id)}">${esc(AREA[p.area] || '')} · ${esc(p.name)}</option>`).join('');
    sel.value = ps.some(p => p.id === want) ? want : '';
  }
}
let composing = false;
let curView = null;
function renderView() {
  const root = $('#view');
  if (S.editing || S.dragging) return;
  const ae = document.activeElement;
  if (composing && ae && root.contains(ae)) return;
  let keep = null;
  if (ae && ae.dataset && ae.dataset.keep && root.contains(ae)) keep = { k: ae.dataset.keep, v: ae.value, s: ae.selectionStart, e: ae.selectionEnd };
  const v = SY.views[S.ui.view] || SY.views.today;
  if (curView && curView.before) { try { curView.before(root); } catch (e) { console.error(e); } }
  root.classList.toggle('is-canvas', !!(v.canvas && (typeof v.canvas !== 'function' || v.canvas())));
  root.innerHTML = S.ready ? v.render() : `<div class="skel"><i style="width:40%"></i><i style="width:62%"></i><i style="width:55%"></i></div>`;
  curView = v;
  if (S.ready && v.after) { try { v.after(root); } catch (e) { console.error(e); } }
  if (keep) { const el = root.querySelector(`[data-keep="${keep.k}"]`); if (el) { el.value = keep.v; el.focus(); try { el.setSelectionRange(keep.s, keep.e); } catch (e) {} } }
}

/* ================= shared components ================= */
const isSel = (col, id) => S.sel && S.sel.col === col && S.sel.id === id;
const prioTag = p => (p > 0 ? `<span class="prio prio-${p}">P${p}</span>` : '');
const projChip = p => (p ? `<span class="pchip"><span class="dot" style="--c:${cv(pColor(p))}"></span>${esc(p.name)}</span>` : `<span class="pchip"><span class="dot"></span>零散</span>`);
function dueTag(t) {
  if (!t.due) return '';
  const today = TODAY(), done = t.status === 'done';
  const cls = !done && t.due < today ? 'is-over' : !done && t.due === today ? 'is-today' : '';
  return `<span class="due ${cls}">${icon('cal', 12)}${esc(dueLabel(t.due))}</span>`;
}
function taskRow(t, opt = {}) {
  const done = t.status === 'done', inToday = t.todayDate === TODAY();
  return `<div tabindex="0" class="trow ${done ? 'is-done' : ''} ${isSel('tasks', t.id) ? 'is-sel' : ''}" data-open="tasks:${esc(t.id)}">
    <button type="button" class="chk ${t.status === 'doing' ? 'is-doing' : ''}" data-act="toggle-done" data-id="${esc(t.id)}" aria-label="${done ? '标记为未完成' : '标记为完成'}">${done ? icon('check', 13) : ''}</button>
    <div class="trow-main"><div class="trow-title">${esc(t.title)}</div>
      <div class="meta">${prioTag(t.priority)}${t.status === 'doing' && !opt.hideDoing ? '<span class="pill pill-doing">进行中</span>' : ''}${opt.noProject ? '' : projChip(getP(t.projectId))}${dueTag(t)}${(t.deps || []).length ? `<span>${icon('link', 12)} ${t.deps.length}</span>` : ''}${t.note ? '<span>有备注</span>' : ''}</div></div>
    ${done ? '' : `<button type="button" class="today-btn ${inToday ? 'is-on' : ''}" data-act="toggle-today" data-id="${esc(t.id)}" title="${inToday ? '移出今日' : '加入今日'}" aria-label="${inToday ? '移出今日' : '加入今日'}">${icon('sun')}</button>`}
  </div>`;
}
function progBar(s) { const t = s.total || 1; return `<div class="bar"><i class="b-done" style="width:${(s.done / t) * 100}%"></i><i class="b-doing" style="width:${(s.doing / t) * 100}%"></i></div>`; }
function scopeSelect(scope, { allowAll = true, allowLoose = true } = {}) {
  const ps = list('projects').filter(p => p.status !== 'done' || p.id === scope).sort((a, b) => (a.area || '').localeCompare(b.area || '') || (a.createdAt || 0) - (b.createdAt || 0));
  return `<select class="ctl" id="scope-sel" data-scope aria-label="选择项目" style="min-width:200px">
    ${allowAll ? `<option value="all" ${scope === 'all' ? 'selected' : ''}>全部项目</option>` : ''}
    ${ps.map(p => `<option value="${esc(p.id)}" ${scope === p.id ? 'selected' : ''}>${esc(AREA[p.area] || '')} · ${esc(p.name)}</option>`).join('')}
    ${allowLoose ? `<option value="_loose" ${scope === '_loose' ? 'selected' : ''}>零散任务</option>` : ''}
  </select>`;
}
function validScope(sc, allowAll = true) {
  if (sc === '_loose') return sc;
  if (sc && sc !== 'all' && S.data.projects.has(sc)) return sc;
  if (allowAll) return 'all';
  const p = list('projects').find(x => x.status === 'active') || list('projects')[0];
  return p ? p.id : '_loose';
}
Object.assign(SY, { isSel, prioTag, projChip, dueTag, taskRow, progBar, scopeSelect, validScope });

/* generic chart view: header + scope toolbar + module body */
SY.chartView = (key, title, sub, opts = {}) => ({
  canvas: !!opts.canvas,
  render() {
    const m = SY.mods[key]; const sc = validScope(S.ui.scope, opts.allowAll !== false);
    return `<div class="vh"><div><h1>${title}</h1><div class="sub">${sub}</div></div></div>
      <div class="toolbar">${scopeSelect(sc, { allowAll: opts.allowAll !== false })}${m.tools ? m.tools(sc) : ''}</div>${m.body(sc)}`;
  },
  after(root) { const m = SY.mods[key]; if (m.after) m.after(root, validScope(S.ui.scope, opts.allowAll !== false)); },
  before(root) { const m = SY.mods[key]; if (m.before) m.before(root, validScope(S.ui.scope, opts.allowAll !== false)); },
  key: m => SY.mods[key].key && SY.mods[key].key(m)
});

/* ================= mutations ================= */
function newTask(fields) {
  const t = { id: SY.newId('tasks'), title: '', projectId: null, status: 'todo', priority: 0, start: null, due: null, todayDate: null, note: '', estO: null, estM: null, estP: null, deps: [], createdAt: now(), doneAt: null, ...fields };
  return put('tasks', t);
}
function setStatus(id, status) {
  const t = S.data.tasks.get(id); if (!t || t.status === status) return;
  patch('tasks', id, { status, doneAt: status === 'done' ? now() : null });
}
function newProject(fields = {}) {
  const p = put('projects', { id: SY.newId('projects'), name: '新项目', area: 'work', color: null, status: 'active', goal: '', note: '', start: TODAY(), due: null, createdAt: now(), ...fields });
  return p;
}
function del(col, id) {
  const doc = S.data[col].get(id); if (!doc) return;
  const extra = []; const depFix = [];
  if (col === 'projects') list('tasks').filter(t => t.projectId === id).forEach(t => { extra.push(t); remove('tasks', t.id); });
  if (col === 'tasks') list('tasks').filter(t => (t.deps || []).includes(id)).forEach(t => { depFix.push([t.id, t.deps]); patch('tasks', t.id, { deps: t.deps.filter(d => d !== id) }); });
  remove(col, id);
  if (S.sel && S.sel.id === id) S.sel = null;
  if (col === 'projects' && S.ui.view === 'project' && S.ui.projectId === id) setView('projects');
  const name = { tasks: '任务', ideas: '灵感', projects: '项目', maps: '导图', links: '关联' }[col];
  toast(`已删除${name}${extra.length ? '及 ' + extra.length + ' 个任务' : ''}`, { undo: () => { put(col, doc); extra.forEach(t => put('tasks', t)); depFix.forEach(([tid, deps]) => patch('tasks', tid, { deps })); } });
}
function setView(v) { S.ui.view = v; if (v !== 'project') S.ui.projectId = null; saveUI(); renderAll(); $('#view-wrap').scrollTop = 0; }
function goProject(id, tab) { S.ui.view = 'project'; S.ui.projectId = id; if (tab) S.ui.ptab = tab; saveUI(); renderAll(); $('#view-wrap').scrollTop = 0; }
function select(col, id) { S.sel = { col, id }; renderAll(); }
function closeInsp() { S.sel = null; renderAll(); }
Object.assign(SY, { newTask, setStatus, newProject, del, setView, goProject, select, closeInsp });

/* ================= inspector ================= */
function renderInspector() {
  const box = $('#insp');
  const sel = S.sel, item = sel && S.data[sel.col] && S.data[sel.col].get(sel.id);
  if (!item || sel.col === 'maps') {
    if (!box.hidden) { box.hidden = true; box.innerHTML = ''; box.dataset.key = ''; }
    if (S.sel && !item) S.sel = null;
    document.body.classList.remove('insp-open'); $('#scrim').hidden = true; return;
  }
  const forKey = sel.col + ':' + sel.id;
  if (box.contains(document.activeElement) && box.dataset.for === forKey && document.activeElement.matches('input,textarea,select')) return;
  const key = forKey + ':' + (item.updatedAt || 0) + ':' + S.rev;
  box.hidden = false; document.body.classList.add('insp-open'); $('#scrim').hidden = false;
  if (box.dataset.key === key) return;
  box.dataset.key = key; box.dataset.for = forKey;
  const head = l => `<div class="insp-head"><span class="eyebrow">${l}</span><button type="button" class="icon-btn sm" data-act="close-insp" aria-label="关闭详情">${icon('x')}</button></div>`;
  const seg = (f, cur, opts) => `<div class="seg seg-sm">${opts.map(([v, l]) => `<button type="button" class="${String(cur) === String(v) ? 'is-on' : ''}" data-act="set-field" data-f="${f}" data-v="${v}">${l}</button>`).join('')}</div>`;
  let h = '';
  if (sel.col === 'tasks') {
    const t = item, inToday = t.todayDate === TODAY(), e = est(t);
    const ps = list('projects').filter(p => p.status !== 'done' || p.id === t.projectId);
    const pool = scopeTasks(t.projectId && S.data.projects.has(t.projectId) ? t.projectId : '_loose').filter(x => x.id !== t.id && !(t.deps || []).includes(x.id));
    const sm = succMap();
    const cands = pool.filter(x => !reaches(t.id, x.id, sm));
    h = head('任务') + `<div class="insp-body">
      <textarea class="insp-title" data-f="title" rows="1" aria-label="任务标题">${esc(t.title)}</textarea>
      <div class="field"><span class="lbl">状态</span>${seg('status', t.status, [['todo', '待办'], ['doing', '进行中'], ['done', '已完成']])}</div>
      <div class="field"><span class="lbl">优先级</span>${seg('priority', t.priority || 0, [[1, 'P1'], [2, 'P2'], [3, 'P3'], [0, '无']])}</div>
      <div class="field"><label class="lbl" for="i-start">开始</label><input class="in mono" type="date" id="i-start" data-f="start" value="${esc(t.start || '')}"></div>
      <div class="field"><label class="lbl" for="i-due">截止</label><input class="in mono" type="date" id="i-due" data-f="due" value="${esc(t.due || '')}"></div>
      ${t.status !== 'done' ? `<div class="field"><span class="lbl">今日</span><button type="button" class="toggle ${inToday ? 'is-on' : ''}" data-act="toggle-today" data-id="${esc(t.id)}">${icon('sun', 14)}${inToday ? '已在今日清单' : '加入今日'}</button></div>` : ''}
      <div class="field"><label class="lbl" for="i-proj">项目</label><select class="in" id="i-proj" data-f="projectId"><option value="">零散任务（无项目）</option>${ps.map(p => `<option value="${esc(p.id)}" ${p.id === t.projectId ? 'selected' : ''}>${esc(AREA[p.area] || '')} · ${esc(p.name)}</option>`).join('')}</select></div>
      <div class="field col1"><span class="lbl">工期估算（天）· PERT 三点估算</span>
        <div class="est"><label>乐观<input class="in mono" type="number" min="0" step="0.5" data-f="estO" value="${t.estO ?? ''}"></label><label>最可能<input class="in mono" type="number" min="0" step="0.5" data-f="estM" value="${t.estM ?? ''}"></label><label>悲观<input class="in mono" type="number" min="0" step="0.5" data-f="estP" value="${t.estP ?? ''}"></label></div>
        <div class="te">${e.set ? `期望工期 te = <b>${e.te.toFixed(1)}</b> 天 · 标准差 σ = <b>${Math.sqrt(e.v).toFixed(2)}</b>` : '未估算时按 1 天计算'}</div></div>
      <div class="field col1"><span class="lbl">前置任务（完成后才能开始）</span>
        <div class="deps">${(t.deps || []).map(d => { const x = S.data.tasks.get(d); return x ? `<span class="dep"><span>${esc(x.title)}</span><button type="button" data-act="rm-dep" data-id="${esc(d)}" aria-label="移除">${icon('x', 11)}</button></span>` : ''; }).join('') || '<span class="te">无</span>'}</div>
        ${cands.length ? `<select class="in" id="i-adddep" aria-label="添加前置任务"><option value="">＋ 添加前置任务…</option>${cands.sort(taskSort).map(x => `<option value="${esc(x.id)}">${esc(x.title)}</option>`).join('')}</select>` : ''}</div>
      <div class="field col1"><label class="lbl" for="i-note">备注</label><textarea class="in" id="i-note" data-f="note" rows="5" placeholder="链接、细节、卡在哪里…">${esc(t.note || '')}</textarea></div>
      ${SY.relSection ? SY.relSection('tasks', t.id) : ''}
      <div class="insp-meta">创建 ${esc(fmtTs(t.createdAt))}${t.doneAt ? '<br>完成 ' + esc(fmtTs(t.doneAt)) : ''}<br>修改 ${esc(fmtTs(t.updatedAt))}${byLine(t)}</div>
      <div class="insp-acts"><button type="button" class="btn btn-sm btn-danger" data-act="del" data-col="tasks" data-id="${esc(t.id)}">${icon('trash', 14)}删除任务</button></div>
    </div>`;
  } else if (sel.col === 'ideas') {
    const i = item, p = getP(i.projectId);
    h = head('灵感') + `<div class="insp-body">
      <textarea class="insp-title" data-f="text" rows="2" aria-label="灵感内容">${esc(i.text)}</textarea>
      <div class="field"><span class="lbl">状态</span>${seg('status', i.status, [['inbox', '收件箱'], ['incubating', '孵化中'], ['converted', '已转化'], ['archived', '归档']])}</div>
      <div class="field"><label class="lbl" for="i-tags">标签</label><input class="in" id="i-tags" data-f="tags" value="${esc((i.tags || []).join(' '))}" placeholder="空格分隔，如：agent 效率"></div>
      <div class="field"><span class="lbl">星标</span><button type="button" class="toggle ${i.star ? 'is-on' : ''}" data-act="star" data-id="${esc(i.id)}">${icon('star', 14)}${i.star ? '已加星标' : '加星标'}</button></div>
      ${p ? `<div class="field"><span class="lbl">已转为</span><button type="button" class="linkish" style="justify-self:start" data-act="open-project" data-id="${esc(p.id)}">${esc(p.name)} →</button></div>` : ''}
      <div class="field col1"><label class="lbl" for="i-inote">展开想法</label><textarea class="in" id="i-inote" data-f="note" rows="6" placeholder="为什么想做？做成什么样？有什么顾虑？AI 拆解时会一起参考。">${esc(i.note || '')}</textarea></div>
      <div class="insp-acts">
        <button type="button" class="btn btn-sm btn-ai ai" data-act="ai-idea" data-id="${esc(i.id)}">${icon('spark', 14)}AI 拆解成项目</button>
        <button type="button" class="btn btn-sm" data-act="idea-to-map" data-id="${esc(i.id)}">${icon('mind', 14)}用导图展开</button>
        <button type="button" class="btn btn-sm" data-act="idea-to-task" data-id="${esc(i.id)}">${icon('task', 14)}转任务</button>
        <button type="button" class="btn btn-sm" data-act="idea-to-project" data-id="${esc(i.id)}">${icon('folder', 14)}转项目</button>
      </div>
      ${SY.relSection ? SY.relSection('ideas', i.id) : ''}
      <div class="insp-meta">记下于 ${esc(fmtTs(i.createdAt))}（${esc(ago(i.createdAt))}）<br>修改 ${esc(fmtTs(i.updatedAt))}${byLine(i)}</div>
      <div class="insp-acts"><button type="button" class="btn btn-sm btn-danger" data-act="del" data-col="ideas" data-id="${esc(i.id)}">${icon('trash', 14)}删除灵感</button></div>
    </div>`;
  } else {
    const p = item, s = pStats(p.id), col = pColor(p);
    h = head('项目') + `<div class="insp-body">
      <textarea class="insp-title" data-f="name" rows="1" aria-label="项目名称">${esc(p.name)}</textarea>
      <div class="field"><span class="lbl">领域</span>${seg('area', p.area || 'work', [['work', '工作'], ['life', '生活']])}</div>
      <div class="field"><span class="lbl">状态</span>${seg('status', p.status || 'active', [['active', '进行中'], ['paused', '暂停'], ['done', '已完成']])}</div>
      <div class="field"><span class="lbl">颜色</span><div style="display:flex;gap:8px">${PALETTE.map(c => `<button type="button" class="swatch ${c === col ? 'is-on' : ''}" style="--c:${cv(c)}" data-act="set-field" data-f="color" data-v="${c}" aria-label="${c}"></button>`).join('')}</div></div>
      <div class="field"><label class="lbl" for="i-pstart">开始</label><input class="in mono" type="date" id="i-pstart" data-f="start" value="${esc(p.start || '')}"></div>
      <div class="field"><label class="lbl" for="i-pdue">截止</label><input class="in mono" type="date" id="i-pdue" data-f="due" value="${esc(p.due || '')}"></div>
      <div class="field col1"><label class="lbl" for="i-goal">完成标准</label><textarea class="in" id="i-goal" data-f="goal" rows="3" placeholder="做到什么程度算完成？">${esc(p.goal || '')}</textarea></div>
      <div class="field col1"><label class="lbl" for="i-pnote">笔记</label><textarea class="in" id="i-pnote" data-f="note" rows="5" placeholder="背景、资料链接、决策记录…">${esc(p.note || '')}</textarea></div>
      ${SY.relSection ? SY.relSection('projects', p.id) : ''}
      <div class="insp-meta">${s.total} 个任务 · ${s.done} 已完成 · ${s.doing} 进行中<br>创建 ${esc(fmtTs(p.createdAt))}${byLine(p)}</div>
      <div class="insp-acts"><button type="button" class="btn btn-sm btn-danger" data-act="del" data-col="projects" data-id="${esc(p.id)}">${icon('trash', 14)}删除项目${s.total ? '及其 ' + s.total + ' 个任务' : ''}</button></div>
    </div>`;
  }
  box.innerHTML = h;
}
function byLine(d) {
  const who = a => (a && a !== 'user' ? esc(String(a).replace(/^agent:/, '')) + '（AI）' : '');
  const c = who(d.createdBy), u = who(d.updatedBy);
  return (c ? '<br>由 ' + c + ' 创建' : '') + (u && u !== c ? '<br>' + u + ' 最近修改' : '');
}
function commitField(el) {
  if (!S.sel) return;
  const { col, id } = S.sel; const old = S.data[col].get(id); if (!old) return;
  const f = el.dataset.f; let v = el.value;
  if (f === 'tags') v = parseTagList(v);
  else if (f === 'due' || f === 'start' || f === 'projectId') v = v || null;
  else if (f === 'estO' || f === 'estM' || f === 'estP') { v = num(v); if (v != null && v < 0) v = 0; }
  else if (f === 'title' || f === 'text' || f === 'name') { v = v.trim(); if (!v) { el.value = old[f] || ''; return; } }
  if (JSON.stringify(old[f] ?? null) === JSON.stringify(v ?? null)) return;
  const part = { [f]: v };
  if (col === 'tasks' && f === 'projectId' && (old.deps || []).length) part.deps = (old.deps || []).filter(d => { const x = S.data.tasks.get(d); return x && (x.projectId || null) === (v || null); });
  patch(col, id, part);
}
const parseTagList = s => [...new Set(String(s).split(/[\s,，、#]+/).map(x => x.trim()).filter(Boolean))].slice(0, 12);
SY.parseTagList = parseTagList;

/* ================= modal & toast ================= */
function openModal(title, body, foot, { iconName = 'spark', wide = false } = {}) {
  $('#m-title').innerHTML = (iconName ? icon(iconName, 18) : '') + esc(title);
  $('#m-body').innerHTML = body; $('#m-foot').innerHTML = foot || '';
  $('#m-close').innerHTML = icon('x');
  $('.modal-card').style.width = wide ? 'min(900px,100%)' : '';
  $('#modal').hidden = false;
}
function closeModal() { if (SY.onModalClose) { const f = SY.onModalClose; SY.onModalClose = null; f(); } $('#modal').hidden = true; }
let tt;
function toast(msg, opt = {}) {
  const el = $('#toast'); SY.undo = opt.undo || null; SY.toastAction = opt.action || null;
  el.innerHTML = `<span>${esc(msg)}</span>${SY.toastAction ? `<button type="button" data-act="toast-action">${esc(SY.toastAction.label)}</button>` : ''}${SY.undo ? '<button type="button" data-act="undo">撤销</button>' : ''}`;
  el.classList.add('is-on'); clearTimeout(tt);
  tt = setTimeout(() => { el.classList.remove('is-on'); SY.undo = null; SY.toastAction = null; }, opt.ms || (SY.undo || SY.toastAction ? 6500 : 2800));
}
/* small promise-based confirm built on the modal */
function ask(title, body, okLabel = '确定', { danger = false } = {}) {
  return new Promise(res => {
    let done = false; const fin = v => { if (done) return; done = true; res(v); };
    openModal(title, `<p style="margin:0;line-height:1.7">${body}</p>`,
      `<button class="btn" type="button" data-act="ask-no">取消</button><button class="btn ${danger ? 'btn-danger' : 'btn-primary'}" type="button" data-act="ask-yes">${esc(okLabel)}</button>`, { iconName: null });
    SY.acts['ask-yes'] = () => { fin(true); SY.onModalClose = null; closeModal(); };
    SY.acts['ask-no'] = () => { closeModal(); };
    SY.onModalClose = () => fin(false);
  });
}
Object.assign(SY, { openModal, closeModal, toast, ask });

/* ================= settings ================= */
const SKINS = [['native', 'macOS 原生'], ['mono', '素白高密度'], ['graphite', '暗夜专业'], ['ink', '宣纸水墨'], ['brutal', '新粗野'], ['clay', '黏土']];
SY.SKINS = SKINS;
let themeSetByApp = false;
function applyTheme() {
  document.documentElement.dataset.skin = SKINS.some(s => s[0] === S.ui.skin) ? S.ui.skin : 'native';
  document.documentElement.classList.toggle('is-app', SY.isApp);
  const t = S.ui.theme;
  try { const w = T && T.window && T.window.getCurrentWindow && T.window.getCurrentWindow(); if (w && w.setTheme) w.setTheme(t === 'light' || t === 'dark' ? t : null).catch(() => {}); } catch (e) {}
  // only touch data-theme when the user picked one here, so a host page's own theme choice is kept
  if (t === 'light' || t === 'dark') { document.documentElement.dataset.theme = t; themeSetByApp = true; }
  else if (themeSetByApp) { delete document.documentElement.dataset.theme; themeSetByApp = false; }
}
SY.applyTheme = applyTheme;
async function openSettings() {
  let st = { apiFormat: 'anthropic', apiBase: '', model: 'claude-sonnet-5-5', quickModel: 'claude-haiku-5-5', hasKey: false, dataPath: '' };
  if (invoke) { try { st = { ...st, ...(await invoke('get_settings')) }; } catch (e) { toast('读取设置失败：' + SY.errText(e)); } }
  const th = S.ui.theme;
  if (SY.loadAiAccess) SY.loadAiAccess();
  openModal('设置', `
    <div class="set-sec"><h3>AI 接口</h3>
      ${invoke ? '' : SY.sampleFn ? '<p><b>网页预览里，AI 通过你的 Claude 账号运行</b>，下面这些设置在预览里不起作用；装到 Mac 上以后才用这里填的接口和 Key。</p>' : '<p>浏览器预览模式下不能调用 AI，安装桌面版后可用。</p>'}
      <p>支持 Anthropic 官方接口，也支持兼容 OpenAI 格式的网关（如 LiteLLM、自建代理）。API Key 只保存在本机。</p>
      <div class="ai-grid">
        <label>接口格式<select class="in" id="s-format"><option value="anthropic" ${st.apiFormat !== 'openai' ? 'selected' : ''}>Anthropic Messages</option><option value="openai" ${st.apiFormat === 'openai' ? 'selected' : ''}>OpenAI 兼容（/chat/completions）</option></select></label>
        <label>接口地址（留空用官方默认）<input class="in mono" id="s-base" value="${esc(st.apiBase)}" placeholder="https://api.anthropic.com"></label>
        <label>主模型（拆解、规划）<input class="in mono" id="s-model" value="${esc(st.model)}"></label>
        <label>快速模型（排今日、扩展导图）<input class="in mono" id="s-quick" value="${esc(st.quickModel)}"></label>
      </div>
      <label class="f" style="display:flex;flex-direction:column;gap:6px;font-size:12px;color:var(--ink-3);font-weight:600">API Key<input class="in mono" id="s-key" type="password" placeholder="${st.hasKey ? '已保存（留空则不修改）' : 'sk-…'}" autocomplete="off"></label>
      <div style="display:flex;gap:10px;align-items:center"><button class="btn btn-sm" type="button" data-act="settings-test" ${invoke || SY.sampleFn ? '' : 'disabled'}>测试连接</button><span id="s-test" class="te"></span></div>
    </div>
    ${SY.aiAccessSection ? SY.aiAccessSection() : ''}
    <div class="set-sec"><h3>外观</h3>
      <div class="seg" id="s-skin">${SKINS.map(([k, l]) => `<button type="button" class="${(S.ui.skin || 'clay') === k ? 'is-on' : ''}" data-act="skin" data-v="${k}">${l}</button>`).join('')}</div>
      <div class="seg" id="s-theme">${[['system', '跟随系统'], ['light', '浅色'], ['dark', '深色']].map(([k, l]) => `<button type="button" class="${th === k ? 'is-on' : ''}" data-act="theme" data-v="${k}">${l}</button>`).join('')}</div>
    </div>
    <div class="set-sec"><h3>数据</h3>
      <p>所有数据保存在本机的一个数据库文件里，每天自动备份一份，保留最近 14 天。也可以随时导出成 JSON；网页版导出的备份可以直接导入。</p>
      ${st.dataPath ? `<div class="path">${esc(st.dataPath)}</div>` : ''}
      <div style="display:flex;gap:8px;flex-wrap:wrap">
        ${invoke ? `<button class="btn btn-sm" type="button" data-act="reveal-data">${icon('folder', 14)}在访达中显示</button>` : ''}
        <button class="btn btn-sm" type="button" data-act="export">${icon('down', 14)}导出备份</button>
        <button class="btn btn-sm" type="button" data-act="import">${icon('up', 14)}导入备份</button>
      </div>
    </div>
    <div class="set-sec"><h3>快捷键</h3>
      <p><b class="mono">⌥⇧Space</b> 在任何应用里唤出拾忆并聚焦记录框 · <b class="mono">⌘K</b> 聚焦记录框 · <b class="mono">⌘1–⌘8</b> 切换视图 · <b class="mono">Esc</b> 关闭详情</p>
    </div>`,
    `<button class="btn" type="button" data-act="close-modal">取消</button><button class="btn btn-primary" type="button" data-act="settings-save">保存</button>`, { iconName: 'gear' });
}
function readSettingsForm() {
  return { apiFormat: $('#s-format').value, apiBase: $('#s-base').value.trim(), model: $('#s-model').value.trim() || 'claude-sonnet-5-5', quickModel: $('#s-quick').value.trim() || 'claude-haiku-5-5', apiKey: $('#s-key').value.trim() };
}
async function saveSettings(close = true) {
  const f = readSettingsForm();
  if (invoke) { try { await invoke('save_settings', { settings: f, keepKey: !f.apiKey }); } catch (e) { toast('保存设置失败：' + e); return false; } }
  if (close) { closeModal(); toast('设置已保存'); }
  return true;
}
SY.acts['settings'] = () => openSettings();
SY.acts['settings-save'] = () => saveSettings(true);
SY.acts['settings-test'] = async () => {
  const out = $('#s-test'); if (!(await saveSettings(false))) return;
  out.textContent = '正在连接…';
  try { const r = await SY.aiCall('只回复两个字：你好', true); out.textContent = '连接成功：' + String(r).slice(0, 40); }
  catch (e) { out.textContent = SY.aiErrText(e); }
};
SY.acts['skin'] = el => { S.ui.skin = el.dataset.v; saveUI(); applyTheme(); $$('#s-skin button').forEach(b => b.classList.toggle('is-on', b.dataset.v === S.ui.skin)); scheduleRender(); };
SY.acts['theme'] = el => { S.ui.theme = el.dataset.v; saveUI(); applyTheme(); $$('#s-theme button').forEach(b => b.classList.toggle('is-on', b.dataset.v === S.ui.theme)); };
SY.acts['reveal-data'] = async () => { try { await invoke('reveal_data_dir'); } catch (e) { toast('打不开：' + e); } };

/* ================= AI transport ================= */
const SAMPLE_ERR = { not_granted: '你还没有允许这个页面调用 Claude。', sampling_disabled: '你的账号当前不能在页面里调用 Claude。', rate_limited: '调用太频繁或用量到了上限，过一会儿再试。', session_expired: '登录已过期，重新登录 Claude 后再试。', refused: 'Claude 没有处理这条内容，换个说法再试。' };
SY.aiCall = async (prompt, quick) => {
  if (!invoke && SY.sampleFn) {
    try { const r = await SY.sampleFn(prompt, { modelTier: quick ? 'quick' : 'default', cache: false }); return r.text; }
    catch (e) { throw { code: 'api', message: SAMPLE_ERR[e && e.code] || (e && e.message) || '连接中断了' }; }
  }
  if (!invoke) throw { code: 'no_backend' };
  try { return await invoke('ai_complete', { prompt, quick: !!quick }); }
  catch (e) { const m = String(e); if (m.includes('NO_KEY')) throw { code: 'no_key' }; throw { code: 'api', message: m }; }
};
SY.aiErrText = e => {
  if (!e) return '出错了';
  if (e.code === 'no_backend') return '浏览器预览模式下不能调用 AI。';
  if (e.code === 'no_key') return '还没有配置 API Key，请在设置里填写。';
  if (e.code === 'invalid_json') return 'AI 的回复格式不对，点「重试」再来一次。';
  return 'AI 调用失败：' + (e.message || e);
};
SY.parseJSON = text => {
  const t = String(text || '');
  try { return JSON.parse(t); } catch (e) {}
  const f = t.match(/```(?:json)?\s*([\s\S]*?)```/); if (f) { try { return JSON.parse(f[1]); } catch (e) {} }
  const a = t.search(/[\[{]/), b = Math.max(t.lastIndexOf('}'), t.lastIndexOf(']'));
  if (a >= 0 && b > a) { try { return JSON.parse(t.slice(a, b + 1)); } catch (e) {} }
  throw { code: 'invalid_json', text: t };
};

/* ================= export / import ================= */
async function doExport() {
  const fileName = `拾忆备份-${TODAY()}.json`;
  if (invoke) {
    await SY.store.flush();
    try { const ok = await invoke('core_export', { fileName }); if (ok) toast('备份已导出'); } catch (e) { toast('导出失败：' + SY.errText(e)); }
    return;
  }
  const data = JSON.stringify(SY.store.snapshot(), null, 2);
  if (SY.dl) {
    try { await SY.dl.save({ filename: fileName, data }); toast('备份已导出'); }
    catch (e) { if (!e || e.code !== 'declined') toast('导出没有成功，稍后再试。'); }
    return;
  }
  const blob = new Blob([data], { type: 'application/json' });
  const a = document.createElement('a'); a.href = URL.createObjectURL(blob); a.download = fileName; a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 2000);
}
function importObj(obj) {
  if (!obj || typeof obj !== 'object') { toast('这个文件不是拾忆的备份。'); return; }
  const n = SY.withNote('导入备份', () => SY.store.importLocal(obj));
  toast(n ? `导入完成，共 ${n} 条` : '文件里没有可导入的内容。');
}
async function doImport() {
  if (invoke) {
    await SY.store.flush();
    try { const r = await invoke('core_import'); if (r) { await SY.store.reload(); toast(r.imported ? `导入完成，共 ${r.imported} 条（可在「操作记录」里撤销）` : '文件里没有可导入的内容。'); } }
    catch (e) { toast('导入失败：' + SY.errText(e)); }
    return;
  }
  $('#import-file').click();
}
SY.acts['export'] = () => doExport();
SY.acts['import'] = () => doImport();
SY.importObj = importObj;

/* ================= events ================= */
SY.suppressClickUntil = 0;
document.addEventListener('click', e => {
  if (performance.now() < SY.suppressClickUntil) { e.preventDefault(); e.stopPropagation(); return; }
  const a = e.target.closest('[data-act]');
  if (a && !a.disabled) {
    if (a.tagName === 'BUTTON' || a.getAttribute('role') === 'button' || a.id === 'scrim') { const f = SY.acts[a.dataset.act]; if (f) f(a, e); }
    return;
  }
  const o = e.target.closest('[data-open]');
  if (o && !e.target.closest('input,textarea,select,label')) { const [col, id] = o.dataset.open.split(':'); select(col, id); }
}, true);
$('#modal').addEventListener('click', e => { if (e.target.id === 'modal') closeModal(); });
document.addEventListener('change', e => {
  const el = e.target;
  if (el.closest('#insp')) {
    if (el.id === 'i-adddep') { if (el.value && S.sel) addDep(S.sel.id, el.value); return; }
    if (el.dataset.f) { commitField(el); return; }
  }
  if (el.dataset && el.dataset.scope !== undefined) { S.ui.scope = el.value; saveUI(); renderAll(); return; }
  if (el.id === 'cap-project') { if (S.ui.view !== 'project') { S.ui.capProject = el.value; saveUI(); } return; }
  if (el.id === 'import-file') { const f = el.files && el.files[0]; el.value = ''; if (f) f.text().then(t => { try { importObj(JSON.parse(t)); } catch (err) { toast('这个文件不是有效的 JSON。'); } }); return; }
  (SY.changeHandlers || []).forEach(f => f(e));
});
document.addEventListener('compositionstart', () => { composing = true; });
document.addEventListener('compositionend', e => { composing = false; if (e.target.id === 'q') S.ui.q = e.target.value; scheduleRender(); });
document.addEventListener('input', e => { if (e.target.id === 'q') { S.ui.q = e.target.value; if (!e.isComposing) scheduleRender(); } });
$('#insp').addEventListener('focusout', () => setTimeout(scheduleRender, 0));

/* actions */
Object.assign(SY.acts, {
  'view': el => setView(el.dataset.v),
  'open-project': el => goProject(el.dataset.id),
  'edit-project': el => select('projects', el.dataset.id),
  'new-project': () => { const p = newProject({ area: S.ui.projArea === 'life' ? 'life' : 'work' }); goProject(p.id, 'board'); select('projects', p.id); setTimeout(() => { const t = $('#insp .insp-title'); if (t) { t.focus(); t.select(); } }, 60); },
  'close-insp': () => closeInsp(),
  'cap-mode': el => { S.ui.capMode = el.dataset.v; saveUI(); renderCapture(); $('#cap-text').focus(); },
  'toggle-done': el => { const t = S.data.tasks.get(el.dataset.id); if (t) setStatus(t.id, t.status === 'done' ? 'todo' : 'done'); },
  'toggle-today': el => { const t = S.data.tasks.get(el.dataset.id); if (t) patch('tasks', t.id, { todayDate: t.todayDate === TODAY() ? null : TODAY() }); },
  'advance': el => { const t = S.data.tasks.get(el.dataset.id); if (t) setStatus(t.id, t.status === 'todo' ? 'doing' : t.status === 'doing' ? 'done' : 'todo'); },
  'set-field': el => {
    if (!S.sel) return; const f = el.dataset.f; let v = el.dataset.v;
    if (f === 'priority') v = Number(v);
    if (S.sel.col === 'tasks' && f === 'status') setStatus(S.sel.id, v); else patch(S.sel.col, S.sel.id, { [f]: v });
  },
  'rm-dep': el => { if (S.sel) removeDep(S.sel.id, el.dataset.id); },
  'star': el => { const i = S.data.ideas.get(el.dataset.id); if (i) patch('ideas', i.id, { star: !i.star }); },
  'del': el => del(el.dataset.col, el.dataset.id),
  'undo': () => { if (SY.undo) { const u = SY.undo; SY.undo = null; u(); $('#toast').classList.remove('is-on'); } },
  'toast-action': () => { if (SY.toastAction) { const a = SY.toastAction; SY.toastAction = null; $('#toast').classList.remove('is-on'); a.fn(); } },
  'close-modal': () => closeModal(),
  'tab': el => { S.ui.ptab = el.dataset.v; saveUI(); renderAll(); }
});

/* capture */
$('#capture').addEventListener('submit', e => {
  e.preventDefault();
  const inp = $('#cap-text'); let text = inp.value.trim(); if (!text) return;
  let mode = S.ui.capMode;
  if (/^\/t\s+/i.test(text)) { mode = 'task'; text = text.replace(/^\/t\s+/i, '').trim(); if (!text) return; }
  if (mode === 'idea') {
    const tags = [...text.matchAll(/#([^\s#,，。；;、]+)/g)].map(m => m[1]);
    const body = text.replace(/#[^\s#,，。；;、]+/g, ' ').replace(/[ \t]{2,}/g, ' ').trim() || text;
    const idea = put('ideas', { id: SY.newId('ideas'), text: body, note: '', tags: [...new Set(tags)], status: 'inbox', star: false, createdAt: now(), projectId: null });
    const rel = SY.related ? SY.related(idea.id, { limit: 3 }) : [];
    if (rel.length) toast(`灵感已记下 · 可能和「${SY.titleOfId(rel[0].id).slice(0, 18)}」${rel.length > 1 ? `等 ${rel.length} 条` : ''}相关`, { action: { label: '看看', fn: () => select('ideas', idea.id) }, ms: 7000 });
    else toast('灵感已记下');
  } else {
    const due = $('#cap-due').value || null, projectId = $('#cap-project').value || null;
    newTask({ title: text, projectId, due, todayDate: S.ui.view === 'today' && !due ? TODAY() : null });
    $('#cap-due').value = '';
    const p = getP(projectId);
    toast(S.ui.view === 'today' && !due ? '已加入今日' : '任务已添加' + (p ? ' · ' + p.name : ''));
  }
  inp.value = '';
});
SY.focusCapture = () => { const i = $('#cap-text'); i.focus(); i.select(); };

/* keyboard */
document.addEventListener('keydown', e => {
  const t = e.target;
  const typing = t.matches && t.matches('input,textarea,select,[contenteditable]');
  if (t.id === 'cap-text' && e.key === 'Enter' && (e.isComposing || e.keyCode === 229)) { e.preventDefault(); return; }
  if (t.dataset && t.dataset.add !== undefined && e.key === 'Enter' && !e.isComposing) {
    e.preventDefault(); const title = t.value.trim(); if (!title) return;
    const pid = t.dataset.add, st = t.dataset.status || 'todo';
    newTask({ title, projectId: pid && pid !== '_loose' && pid !== 'all' ? pid : null, status: st }); t.value = ''; return;
  }
  if (t.classList && t.classList.contains('insp-title') && e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); t.blur(); return; }
  if ((e.metaKey || e.ctrlKey) && (e.key === 'k' || e.key === 'K')) { e.preventDefault(); SY.focusCapture(); return; }
  if ((e.metaKey || e.ctrlKey) && /^[1-8]$/.test(e.key)) {
    e.preventDefault(); const all = NAV.flatMap(g => g[1]); const it = all[Number(e.key) - 1]; if (it) setView(it[0]); return;
  }
  if (e.key === 'Escape') {
    if (!$('#modal').hidden) { closeModal(); return; }
    if (S.editing) return;
    const v = SY.views[S.ui.view]; if (v && v.key && v.key(e)) return;
    if (S.sel) { closeInsp(); return; }
    if (typing) t.blur();
    return;
  }
  if (!$('#modal').hidden) return;
  if (typing || e.isComposing) return;
  if ((e.key === 'Enter' || e.key === ' ') && t.matches && t.matches('[role="button"][data-act]')) { e.preventDefault(); const f = SY.acts[t.dataset.act]; if (f) f(t, e); return; }
  if (e.key === 'Enter' && t.matches && t.matches('[data-open]')) { e.preventDefault(); const [col, id] = t.dataset.open.split(':'); select(col, id); return; }
  const v = SY.views[S.ui.view]; if (v && v.key && v.key(e)) return;
  if (e.key === '/' && !e.metaKey && !e.ctrlKey) { e.preventDefault(); SY.focusCapture(); }
});

/* pointer-drag helper shared by kanban / gantt / canvases */
SY.drag = (e, { onStart, onMove, onEnd, threshold = 4 }) => {
  const sx = e.clientX, sy = e.clientY; let started = false;
  const move = ev => {
    const dx = ev.clientX - sx, dy = ev.clientY - sy;
    if (!started) { if (Math.hypot(dx, dy) < threshold) return; started = true; S.dragging = true; if (onStart) onStart(ev); }
    if (onMove) onMove(ev, dx, dy);
  };
  const up = ev => {
    window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up); window.removeEventListener('pointercancel', up);
    if (started) { S.dragging = false; SY.suppressClickUntil = performance.now() + 80; if (onEnd) onEnd(ev, ev.clientX - sx, ev.clientY - sy); scheduleRender(); }
  };
  window.addEventListener('pointermove', move); window.addEventListener('pointerup', up); window.addEventListener('pointercancel', up);
};

/* pan/zoom helper for canvas views */
SY.panZoom = (wrap, layer, state, { onChange } = {}) => {
  const apply = () => { layer.style.transform = `translate(${state.x}px,${state.y}px) scale(${state.k})`; if (onChange) onChange(state); };
  apply();
  wrap.addEventListener('wheel', e => {
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) {
      const r = wrap.getBoundingClientRect(), mx = e.clientX - r.left, my = e.clientY - r.top;
      const k2 = clamp(state.k * Math.exp(-e.deltaY * 0.01), 0.25, 2.5);
      state.x = mx - (mx - state.x) * (k2 / state.k); state.y = my - (my - state.y) * (k2 / state.k); state.k = k2;
    } else { state.x -= e.deltaX; state.y -= e.deltaY; }
    apply();
  }, { passive: false });
  wrap.addEventListener('pointerdown', e => {
    if (e.button !== 0 || e.target.closest('.pnode,.mnode,.gnode,.gcard,.gsug,.canvas-tools,.pe-hit,.ge-hit,button,input,textarea,select')) return;
    const x0 = state.x, y0 = state.y;
    SY.drag(e, { threshold: 2, onStart: () => wrap.classList.add('is-panning'), onMove: (ev, dx, dy) => { state.x = x0 + dx; state.y = y0 + dy; apply(); }, onEnd: () => wrap.classList.remove('is-panning') });
  });
  return {
    apply,
    zoom(f) { const r = wrap.getBoundingClientRect(), mx = r.width / 2, my = r.height / 2; const k2 = clamp(state.k * f, 0.25, 2.5); state.x = mx - (mx - state.x) * (k2 / state.k); state.y = my - (my - state.y) * (k2 / state.k); state.k = k2; apply(); },
    fit(bw, bh, pad = 60) { const r = wrap.getBoundingClientRect(); const k = clamp(Math.min((r.width - pad * 2) / Math.max(bw, 1), (r.height - pad * 2) / Math.max(bh, 1)), 0.3, 1.2); state.k = k; state.x = (r.width - bw * k) / 2; state.y = (r.height - bh * k) / 2; apply(); }
  };
};

SY.renderStatus = renderStatus;
SY.renderInspector = renderInspector;
})();
