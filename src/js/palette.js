/* 拾忆 ⌘K: one box to find anything (ideas, tasks, projects, mind maps and their
   nodes, tags) and to run commands. Everything is already in memory, so the
   search runs right here; the index is rebuilt only after data changes. */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, list, lsGet, lsSet, getP, dueLabel, TODAY, IDEA_ST, TASK_ST, PROJ_ST, AREA } = SY;
const K_RECENT = 'shiyi.recent';
const MAX = 40;
const P = SY.PAL = { open: false, q: '', sel: 0, rows: [], idx: null, idxRev: -1, lastMs: 0 };

/* ---------- recently opened ---------- */
SY.noteOpened = (col, id) => {
  if (!col || !id || id === '_loose') return;
  const r = (lsGet(K_RECENT) || []).filter(x => !(x.col === col && x.id === id && !x.nid));
  r.unshift({ col, id, t: Date.now() });
  lsSet(K_RECENT, r.slice(0, 20));
};

/* ---------- index ---------- */
const low = s => String(s || '').toLowerCase();
function entry(o) { o.tl = low(o.title); o.hl = o.tl + '\n' + low(o.more); return o; }
function buildIndex() {
  if (P.idx && P.idxRev === S.rev) return P.idx;
  const out = [], tags = new Map();
  for (const i of list('ideas')) {
    (i.tags || []).forEach(t => tags.set(t, (tags.get(t) || 0) + 1));
    out.push(entry({ k: 'idea', id: i.id, title: i.text, more: (i.note || '') + ' ' + (i.tags || []).join(' '), note: i.note, closed: i.status === 'archived' || i.status === 'converted',
      sub: ['灵感 · ' + (IDEA_ST[i.status] || ''), ...(i.tags || []).slice(0, 3).map(t => '#' + t)].join(' '), ts: i.updatedAt || i.createdAt || 0 }));
  }
  for (const t of list('tasks')) {
    const p = getP(t.projectId);
    out.push(entry({ k: 'task', id: t.id, title: t.title, more: (t.note || '') + ' ' + (p ? p.name : ''), note: t.note, closed: t.status === 'done',
      sub: ['任务 · ' + (TASK_ST[t.status] || ''), p ? p.name : '零散', t.due && t.status !== 'done' ? dueLabel(t.due) : ''].filter(Boolean).join(' · '), ts: t.updatedAt || t.createdAt || 0, due: t.due }));
  }
  for (const p of list('projects')) {
    out.push(entry({ k: 'project', id: p.id, title: p.name, more: (p.goal || '') + ' ' + (p.note || ''), note: [p.goal, p.note].filter(Boolean).join(' · '), closed: p.status === 'done',
      sub: '项目 · ' + (AREA[p.area] || '') + ' · ' + (PROJ_ST[p.status] || ''), ts: p.updatedAt || 0 }));
  }
  for (const m of list('maps')) {
    out.push(entry({ k: 'map', id: m.id, title: m.title || '未命名导图', more: '', sub: '思维导图 · ' + Object.keys(m.nodes || {}).length + ' 个节点', ts: m.updatedAt || 0 }));
    for (const n of Object.values(m.nodes || {})) {
      if (!n || n.id === m.rootId || !String(n.text || '').trim()) continue;
      out.push(entry({ k: 'node', id: m.id, nid: n.id, title: n.text, more: '', sub: '导图节点 · ' + (m.title || '未命名导图'), ts: m.updatedAt || 0 }));
    }
  }
  for (const [t, n] of tags) out.push(entry({ k: 'tag', id: t, title: '#' + t, more: '', sub: `标签 · ${n} 条灵感`, ts: 0 }));
  P.idx = out; P.idxRev = S.rev;
  return out;
}

/* ---------- commands ---------- */
function themeNow() {
  const t = S.ui.theme;
  if (t === 'light' || t === 'dark') return t;
  return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}
function commands() {
  const views = SY.NAV.flatMap(g => g[1]).map(([k, ic, label]) => ({ k: 'cmd', id: 'view:' + k, ic, title: label, sub: '打开视图', kw: k + ' 视图 打开', run: () => SY.setView(k) }));
  const act = name => () => { const f = SY.acts[name]; if (f) f(document.body); };
  const dark = themeNow() === 'dark';
  const cmds = [
    { id: 'capture', ic: 'edit', title: '快速记录', sub: SY.keys('⌘N'), kw: 'capture new 记录 输入 新建灵感', run: () => SY.focusCapture() },
    { id: 'new-project', ic: 'folder', title: '新建项目', kw: 'new project 创建', run: act('new-project') },
    { id: 'new-map', ic: 'mind', title: '新建思维导图', kw: 'new mind map 导图 创建', run: act('mm-new') },
    { id: 'ai-today', ic: 'spark', title: 'AI 排今日', sub: '让 AI 挑出今天最该做的事', kw: 'ai today plan 规划 今天', ai: true, run: () => { SY.setView('today'); act('ai-today')(); } },
    { id: 'ai-links', ic: 'graph', title: 'AI 找关联', sub: '在知识图谱里找可能相关的内容', kw: 'ai links graph 关联 图谱', ai: true, run: () => { SY.setView('graph'); act('ai-links')(); } },
    { id: 'theme', ic: dark ? 'sun' : 'moon', title: dark ? '切换到浅色' : '切换到深色', kw: 'theme dark light 主题 深色 浅色 夜间 外观', run: () => setTheme(dark ? 'light' : 'dark') },
    { id: 'theme-sys', ic: 'fit', title: '外观跟随系统', kw: 'theme system 主题 跟随 系统', run: () => setTheme('system') },
    { id: 'settings', ic: 'gear', title: '打开设置', kw: 'settings preferences 设置 偏好 模型 api key', run: act('settings') },
    { id: 'update', ic: 'down', title: '检查更新', kw: 'update version 更新 版本 升级', run: () => SY.checkUpdate && SY.checkUpdate(true) },
    { id: 'export', ic: 'down', title: '导出备份', kw: 'export backup 导出 备份', run: act('export') },
    { id: 'import', ic: 'up', title: '导入备份', kw: 'import 导入 恢复', run: act('import') }
  ];
  if (SY.diagCommands) cmds.push(...SY.diagCommands());
  return [...cmds.map(c => ({ k: 'cmd', ...c })), ...views].filter(c => !c.ai || !document.body.classList.contains('no-ai')).map(c => entry({ ...c, more: c.kw || '' }));
}
function setTheme(t) { S.ui.theme = t; SY.saveUI(); SY.applyTheme(); SY.scheduleRender(); }

/* ---------- matching ---------- */
const KIND_W = { project: 6, task: 5, idea: 5, map: 4, tag: 3, node: 2, cmd: 0 };
function score(e, terms) {
  let s = 0;
  for (const t of terms) {
    if (!e.hl.includes(t)) return -1;
    s += e.tl === t ? 60 : e.tl.startsWith(t) ? 34 : e.tl.includes(t) ? 24 : 6;
  }
  if (e.tl === terms.join(' ')) s += 40;
  s += KIND_W[e.k] || 0;
  if (!e.closed) s += 10;
  if (e.ts && Date.now() - e.ts < 7 * 864e5) s += 3;
  return s;
}
function search(q) {
  const text = q.trim(), whole = low(text);
  const terms = whole.split(/\s+/).filter(Boolean);
  const t0 = performance.now();
  const hits = [];
  for (const e of buildIndex()) { const s = score(e, terms); if (s >= 0) hits.push([s, e]); }
  // a command whose name contains what was typed beats items that merely mention it
  let nc = 0;
  for (const e of commands()) { const s = score(e, terms); if (s >= 0) { hits.push([s + (e.tl.includes(whole) ? 20 : 0), e]); nc++; } }
  hits.sort((a, b) => b[0] - a[0] || (b[1].ts || 0) - (a[1].ts || 0));
  P.lastMs = performance.now() - t0;
  const capture = [
    { k: 'new', id: 'idea', ic: 'bulb', title: `记成灵感：${text}`, run: () => captureAs('idea', text) },
    { k: 'new', id: 'task', ic: 'task', title: `记成任务：${text}`, run: () => captureAs('task', text) }
  ];
  P.total = hits.length - nc;
  return [...hits.slice(0, MAX).map(x => x[1]), ...capture];
}
function recentRows() {
  const out = [];
  for (const r of lsGet(K_RECENT) || []) {
    const col = r.col, doc = S.data[col] && S.data[col].get(r.id);
    if (!doc) continue;
    const kind = { ideas: 'idea', tasks: 'task', projects: 'project', maps: 'map' }[col];
    const e = buildIndex().find(x => x.k === kind && x.id === r.id);
    if (e) out.push(e);
    if (out.length >= 7) break;
  }
  return out;
}
function emptyRows() {
  const cs = commands();
  const pick = ['capture', 'new-project', 'ai-today', 'settings'].map(id => cs.find(c => c.id === id)).filter(Boolean);
  return [...recentRows(), ...pick];
}

/* ---------- open / jump ---------- */
function captureAs(mode, text) {
  if (!text) return;
  if (mode === 'idea') {
    const tags = [...text.matchAll(/#([^\s#,，。；;、]+)/g)].map(m => m[1]);
    const body = text.replace(/#[^\s#,，。；;、]+/g, ' ').replace(/[ \t]{2,}/g, ' ').trim() || text;
    SY.put('ideas', { id: SY.newId('ideas'), text: body, note: '', tags: [...new Set(tags)], status: 'inbox', star: false, createdAt: SY.now(), projectId: null });
    SY.toast('灵感已记下');
  } else {
    SY.newTask({ title: text, todayDate: S.ui.view === 'today' ? TODAY() : null });
    SY.toast(S.ui.view === 'today' ? '已加入今日' : '任务已添加');
  }
}
function go(e) {
  close();
  if (!e) return;
  if (e.run) { e.run(); return; }
  if (e.k === 'idea') { SY.noteOpened('ideas', e.id); SY.select('ideas', e.id); }
  else if (e.k === 'task') { SY.noteOpened('tasks', e.id); SY.select('tasks', e.id); }
  else if (e.k === 'project') { SY.goProject(e.id); }
  else if (e.k === 'map') { SY.noteOpened('maps', e.id); S.ui.mapId = e.id; if (SY.mmFocus) SY.mmFocus(e.id, null); SY.setView('mindmap'); }
  else if (e.k === 'node') { SY.noteOpened('maps', e.id); if (SY.mmFocus) SY.mmFocus(e.id, e.nid); SY.setView('mindmap'); }
  else if (e.k === 'tag') { S.ui.ideaFilter = 'all'; S.ui.tag = e.id; SY.setView('ideas'); }
}

/* ---------- rendering ---------- */
const KIND_IC = { idea: 'bulb', task: 'task', project: 'folder', map: 'mind', node: 'mind', tag: 'hash' };
const reEsc = s => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
function hi(text, terms) {
  const s = String(text || '');
  if (!terms.length) return esc(s);
  const re = new RegExp(terms.map(reEsc).join('|'), 'gi');
  let out = '', last = 0, m;
  while ((m = re.exec(s))) { if (!m[0]) { re.lastIndex++; continue; } out += esc(s.slice(last, m.index)) + '<mark>' + esc(m[0]) + '</mark>'; last = m.index + m[0].length; }
  return out + esc(s.slice(last));
}
/* when only the note matched, show where */
function snippet(e, terms) {
  if (!e.note) return '';
  const t = terms.find(x => !e.tl.includes(x)); if (!t) return '';
  const n = String(e.note).replace(/\s+/g, ' '), i = n.toLowerCase().indexOf(t); if (i < 0) return '';
  const a = Math.max(0, i - 18), b = Math.min(n.length, i + t.length + 36);
  return (a > 0 ? '…' : '') + hi(n.slice(a, b), terms) + (b < n.length ? '…' : '');
}
function rowHtml(e, i, terms) {
  const ic = e.ic || KIND_IC[e.k] || 'arrow';
  const sn = e.k !== 'cmd' && e.k !== 'new' ? snippet(e, terms) : '';
  const isView = e.k === 'cmd' && e.id.startsWith('view:');
  const tail = e.k === 'cmd' ? `<span class="pal-k">${isView ? '视图' : '命令'}</span>` : '';
  const sub = isView ? '' : e.sub;
  return `<div class="pal-row ${i === P.sel ? 'is-sel' : ''} ${e.closed ? 'is-closed' : ''}" role="option" id="pal-r${i}" aria-selected="${i === P.sel}" data-i="${i}" data-k="${e.k}">
    <span class="pal-ic k-${e.k}">${icon(ic, 15)}</span>
    <span class="pal-main"><span class="pal-t">${hi(e.title, e.k === 'new' ? [] : terms)}</span>${sn ? `<span class="pal-s">${sn}</span>` : sub ? `<span class="pal-s">${esc(sub)}</span>` : ''}</span>${tail}
  </div>`;
}
function paint() {
  const box = document.getElementById('pal-list'); if (!box) return;
  const terms = low(P.q).split(/\s+/).filter(Boolean);
  const q = P.q.trim();
  P.rows = q ? search(q) : emptyRows();
  if (P.sel >= P.rows.length) P.sel = Math.max(0, P.rows.length - 1);
  let html = '', head = '';
  const noHits = P.rows.length && P.rows[0].k === 'new';
  P.rows.forEach((e, i) => {
    const h = !q ? (e.k === 'cmd' ? '常用' : '最近打开') : e.k === 'new' ? (noHits ? '没有找到，直接记下' : '记下来') : '';
    if (h !== head) { if (h) html += `<div class="pal-h">${h}</div>`; head = h; }
    html += rowHtml(e, i, terms);
  });
  box.innerHTML = html || '<div class="pal-empty">输入关键词，搜索灵感、任务、项目、导图和标签</div>';
  const foot = document.getElementById('pal-foot');
  if (foot) foot.innerHTML = `<span><b>↑↓</b> 选择</span><span><b>↵</b> 打开</span><span><b>esc</b> 关闭</span><span class="pal-n">${q ? (P.total ? `${P.total} 条内容${P.total > MAX ? `，显示前 ${MAX} 条` : ''}` : noHits ? '没有匹配的内容' : '') : ''}</span>`;
  const el = document.getElementById('pal-r' + P.sel); if (el) el.scrollIntoView({ block: 'nearest' });
  const inp = document.getElementById('pal-q'); if (inp) inp.setAttribute('aria-activedescendant', 'pal-r' + P.sel);
}
function move(d) {
  if (!P.rows.length) return;
  P.sel = (P.sel + d + P.rows.length) % P.rows.length;
  document.querySelectorAll('#pal-list .pal-row').forEach(r => { const on = Number(r.dataset.i) === P.sel; r.classList.toggle('is-sel', on); r.setAttribute('aria-selected', on); });
  const el = document.getElementById('pal-r' + P.sel); if (el) el.scrollIntoView({ block: 'nearest' });
  const inp = document.getElementById('pal-q'); if (inp) inp.setAttribute('aria-activedescendant', 'pal-r' + P.sel);
}

function open(initial = '') {
  if (P.open) { const i = document.getElementById('pal-q'); if (i) { i.focus(); i.select(); } return; }
  if (!document.getElementById('modal').hidden) SY.closeModal();
  P.open = true; P.q = initial; P.sel = 0;
  const wrap = document.createElement('div');
  wrap.className = 'modal pal'; wrap.id = 'pal';
  wrap.innerHTML = `<div class="modal-card clay pal-card" role="dialog" aria-modal="true" aria-label="搜索和命令">
    <div class="pal-in">${icon('search', 18)}<input id="pal-q" role="combobox" aria-expanded="true" aria-controls="pal-list" aria-autocomplete="list" autocomplete="off" spellcheck="false" placeholder="搜索灵感、任务、项目、导图，或输入命令…" value="${esc(initial)}"><kbd>esc</kbd></div>
    <div class="pal-list" id="pal-list" role="listbox"></div>
    <div class="pal-foot" id="pal-foot"></div></div>`;
  document.body.appendChild(wrap);
  const inp = wrap.querySelector('#pal-q');
  inp.addEventListener('input', e => { if (e.isComposing) return; P.q = inp.value; P.sel = 0; paint(); });
  inp.addEventListener('compositionend', () => { P.q = inp.value; P.sel = 0; paint(); });
  wrap.addEventListener('mousedown', e => { if (e.target === wrap) { e.preventDefault(); close(); } });
  const list = wrap.querySelector('#pal-list');
  list.addEventListener('mousemove', e => { const r = e.target.closest('.pal-row'); if (r && Number(r.dataset.i) !== P.sel) { P.sel = Number(r.dataset.i); move(0); } });
  list.addEventListener('click', e => { const r = e.target.closest('.pal-row'); if (r) go(P.rows[Number(r.dataset.i)]); });
  paint();
  inp.focus(); inp.select();
}
function close() {
  const w = document.getElementById('pal'); if (w) w.remove();
  P.open = false;
}
SY.openPalette = open;
SY.closePalette = close;

/* keys inside the palette (capture phase, before the app's own shortcuts) */
document.addEventListener('keydown', e => {
  if (!P.open) return;
  const k = e.key;
  if ((e.metaKey || e.ctrlKey) && (k === 'k' || k === 'K')) { e.preventDefault(); e.stopPropagation(); close(); return; }
  if (e.isComposing || e.keyCode === 229) return;
  if (k === 'Escape') { e.preventDefault(); e.stopPropagation(); close(); return; }
  if (k === 'ArrowDown' || (e.ctrlKey && k === 'n' && SY.isMac)) { e.preventDefault(); e.stopPropagation(); move(1); return; }
  if (k === 'ArrowUp' || (e.ctrlKey && k === 'p' && SY.isMac)) { e.preventDefault(); e.stopPropagation(); move(-1); return; }
  if (k === 'Enter') { e.preventDefault(); e.stopPropagation(); go(P.rows[P.sel]); return; }
  if (k === 'Tab') { e.preventDefault(); e.stopPropagation(); move(e.shiftKey ? -1 : 1); return; }
  e.stopPropagation(); // typing in the box: no app shortcuts
}, true);

SY.acts['palette'] = () => open();
const btn = document.getElementById('pal-btn'); if (btn) btn.title = '搜索和命令（' + SY.keys('⌘K') + '）';
})();
