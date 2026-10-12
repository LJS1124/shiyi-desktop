/* 拾遗 gantt: timeline with drag-to-move / resize, dependency arrows, critical tasks, zoom */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, list, getP, pColor, cv, scopeTasks, span, dur, TODAY, addDays, dayDiff, parseYmd, ymd, isYmd, isSel, taskSort, WD, md } = SY;
const ROW = 40, TW = 340;
const ZOOM = { day: 34, week: 15, month: 5 };
S.view.gantt = S.view.gantt || {};

function monday(s) { const d = parseYmd(s); d.setDate(d.getDate() - (d.getDay() + 6) % 7); return ymd(d); }
const minS = a => a.reduce((x, y) => (x < y ? x : y));
const maxS = a => a.reduce((x, y) => (x > y ? x : y));

function model(scope) {
  let tasks = scopeTasks(scope);
  if (S.ui.ganttHideDone) tasks = tasks.filter(t => t.status !== 'done');
  const rows = [], today = TODAY();
  const split = ts => {
    const sch = [], un = [];
    ts.forEach(t => { const sp = span(t); (sp.s ? sch : un).push({ t, ...sp }); });
    sch.sort((a, b) => (a.s < b.s ? -1 : a.s > b.s ? 1 : taskSort(a.t, b.t))); un.sort((a, b) => taskSort(a.t, b.t));
    return [sch, un];
  };
  if (scope === 'all') {
    const ps = list('projects').filter(p => tasks.some(t => t.projectId === p.id)).sort((a, b) => (a.start || '9').localeCompare(b.start || '9') || (a.createdAt || 0) - (b.createdAt || 0));
    const groups = ps.map(p => [p, tasks.filter(t => t.projectId === p.id)]);
    const looseIds = new Set(scopeTasks('_loose').map(t => t.id));
    const loose = tasks.filter(t => looseIds.has(t.id)); if (loose.length) groups.push([null, loose]);
    for (const [p, ts] of groups) {
      const [sch, un] = split(ts);
      rows.push({ type: 'group', p, s: sch.length ? minS(sch.map(x => x.s)) : null, e: sch.length ? maxS(sch.map(x => x.e)) : null, n: ts.length });
      sch.forEach(x => rows.push({ type: 'task', ...x })); un.forEach(x => rows.push({ type: 'task', ...x, un: true }));
    }
  } else {
    const [sch, un] = split(tasks);
    sch.forEach(x => rows.push({ type: 'task', ...x }));
    if (un.length) { rows.push({ type: 'sub', label: `未排期 · ${un.length} 件` }); un.forEach(x => rows.push({ type: 'task', ...x, un: true })); }
  }
  const ds = rows.filter(r => r.s).map(r => r.s), de = rows.filter(r => r.e).map(r => r.e);
  let min = minS([today, ...ds]), max = maxS([addDays(today, 21), ...de]);
  const p = getP(scope);
  if (p && isYmd(p.start) && p.start < min) min = p.start;
  if (p && isYmd(p.due) && p.due > max) max = p.due;
  const start = monday(addDays(min, -7)), end = addDays(max, 14);
  return { rows, start, days: dayDiff(start, end) + 1, today, p };
}

SY.mods.gantt = {
  tools(scope) {
    const z = S.ui.ganttZoom;
    const isProj = scope !== 'all';
    return `<div class="seg seg-sm">${[['day', '日'], ['week', '周'], ['month', '月']].map(([k, l]) => `<button type="button" class="${z === k ? 'is-on' : ''}" data-act="g-zoom" data-v="${k}">${l}</button>`).join('')}</div>
      <button type="button" class="chip ${S.ui.ganttHideDone ? 'is-on' : ''}" data-act="g-hide">隐藏已完成</button>
      <button type="button" class="btn btn-sm" data-act="g-today">${icon('today', 14)}回到今天</button>
      <span class="sp" style="flex:1"></span>
      ${isProj ? `<button type="button" class="btn btn-sm" data-act="g-auto" data-scope-id="${esc(scope)}" title="按前置依赖和工期估算，把未排期或全部任务从今天起排开">${icon('wand', 14)}自动排期</button>` : ''}
      ${isProj && scope !== '_loose' ? `<button type="button" class="btn btn-sm btn-ai ai" data-act="ai-schedule" data-id="${esc(scope)}">${icon('spark', 14)}AI 估算工期与依赖</button>` : ''}`;
  },
  body(scope) {
    const m = model(scope);
    if (!m.rows.length) return `<div class="empty-big clay"><h3>这里还没有任务</h3><p>先在看板里加几件任务。给任务填上开始、截止日期，或者填好工期估算和前置任务后点「自动排期」，甘特图会自动画出来。</p></div>`;
    const px = ZOOM[S.ui.ganttZoom] || 34, W = m.days * px, x = ds => dayDiff(m.start, ds) * px;
    const crit = new Set();
    if (scope !== 'all' && SY.cpm) { const c = SY.cpm(scopeTasks(scope)); if (c.path.length > 1) c.crit.forEach(id => { const t = S.data.tasks.get(id); if (t && t.status !== 'done') crit.add(id); }); }
    let months = '', days = '';
    for (let i = 0; i < m.days; i++) {
      const ds = addDays(m.start, i), d = parseYmd(ds);
      if (d.getDate() === 1 || i === 0) months += `<div class="g-month" style="left:${i * px}px">${d.getFullYear()} 年 ${d.getMonth() + 1} 月</div>`;
      if (S.ui.ganttZoom === 'day') days += `<div class="g-day ${d.getDay() % 6 === 0 ? 'is-we' : ''} ${ds === m.today ? 'is-today' : ''}" style="left:${i * px + 2}px;width:${px - 4}px">${d.getDate()}<br>${WD[d.getDay()]}</div>`;
      else if (d.getDay() === 1) days += `<div class="g-day" style="left:${i * px}px;width:${px * 7}px;text-align:left;padding-left:5px;border-left:1px solid var(--line)">${S.ui.ganttZoom === 'week' ? md(ds) : d.getDate()}</div>`;
    }
    const idx = new Map(); m.rows.forEach((r, i) => { if (r.type === 'task') idx.set(r.t.id, i); });
    const rowsHtml = m.rows.map(r => {
      if (r.type === 'group') {
        const c = r.p ? pColor(r.p) : 'ink-3';
        return `<div class="g-row"><div class="g-cell is-group" style="width:${TW}px" ${r.p ? `role="button" data-act="open-project" data-id="${esc(r.p.id)}"` : ''}><span class="dot" style="--c:${cv(c)}"></span><span class="nm">${r.p ? esc(r.p.name) : '零散任务'}</span><span class="mono">${r.n} 项</span></div>
          <div class="g-track we" style="width:${W}px">${r.s ? `<div class="gsum" style="--c:${cv(c)};left:${x(r.s)}px;width:${(dayDiff(r.s, r.e) + 1) * px}px"></div>` : ''}</div></div>`;
      }
      if (r.type === 'sub') return `<div class="g-row"><div class="g-cell is-group" style="width:${TW}px"><span class="nm" style="color:var(--ink-3)">${r.label}</span></div><div class="g-track we" style="width:${W}px"></div></div>`;
      const t = r.t, sel = isSel('tasks', t.id);
      const cell = `<div class="g-cell" style="width:${TW}px" data-open="tasks:${esc(t.id)}"><button type="button" class="chk ${t.status === 'doing' ? 'is-doing' : ''} ${t.status === 'done' ? 'is-done' : ''}" style="${t.status === 'done' ? 'background:var(--mint)' : ''}" data-act="toggle-done" data-id="${esc(t.id)}" aria-label="切换完成">${t.status === 'done' ? icon('check', 11) : ''}</button><span class="nm" style="${t.status === 'done' ? 'color:var(--ink-3)' : ''}">${esc(t.title)}</span><span class="mono">${r.s ? md(r.s) : '—'}</span><span class="mono">${r.s ? (dayDiff(r.s, r.e) + 1) + ' 天' : ''}</span></div>`;
      const bar = r.s
        ? `<div class="gbar st-${t.status} ${crit.has(t.id) ? 'is-crit' : ''} ${sel ? 'is-sel' : ''} ${r.derived ? 'is-derived' : ''}" data-gid="${esc(t.id)}" data-open="tasks:${esc(t.id)}" style="left:${x(r.s)}px;width:${(dayDiff(r.s, r.e) + 1) * px - 2}px" title="${esc(t.title)} · ${r.s} → ${r.e}${r.derived ? '（按工期推算，拖动后固定）' : ''}"><span class="gh gh-l"></span><span class="gl">${esc(t.title)}</span><span class="gh gh-r"></span></div>`
        : `<span class="g-hint" style="left:${Math.max(8, x(m.today) - 60)}px">点这一行的任意一天，把任务排到那天</span>`;
      return `<div class="g-row ${r.un ? 'is-unsched' : ''} ${sel ? 'is-sel' : ''}">${cell}<div class="g-track we" style="width:${W}px" ${r.un ? `data-unsched="${esc(t.id)}"` : ''}>${bar}</div></div>`;
    }).join('');
    let paths = '';
    m.rows.forEach((r, i) => {
      if (r.type !== 'task' || !r.s) return;
      for (const d of (r.t.deps || [])) {
        const j = idx.get(d); if (j == null) continue; const pr = m.rows[j]; if (!pr.s) continue;
        const x1 = x(pr.e) + px - 2, y1 = j * ROW + ROW / 2, x2 = x(r.s), y2 = i * ROW + ROW / 2, bad = r.s <= pr.e;
        let dp;
        if (x2 - x1 >= 16) dp = `M${x1},${y1} H${x1 + 8} V${y2} H${x2 - 2}`;
        else { const ym = y2 > y1 ? y2 - ROW / 2 : y2 + ROW / 2; dp = `M${x1},${y1} h8 V${ym} H${x2 - 12} V${y2} H${x2 - 2}`; }
        paths += `<path class="${bad ? 'bad' : ''}" d="${dp}" marker-end="url(#g-arrow${bad ? '-bad' : ''})"/>`;
      }
    });
    const svg = `<svg class="g-deps" style="left:${TW}px" width="${W}" height="${m.rows.length * ROW}"><defs>
      <marker id="g-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L8,4 L0,8 z" style="fill:var(--ink-3)"/></marker>
      <marker id="g-arrow-bad" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L8,4 L0,8 z" style="fill:var(--rose)"/></marker></defs>${paths}</svg>`;
    const legend = `<div class="legend"><span><i style="--c:var(--lav)"></i>待办</span><span><i style="--c:var(--butter)"></i>进行中</span><span><i style="--c:var(--mint)"></i>已完成</span>${crit.size ? '<span><i style="--c:var(--rose)"></i>关键路径（红框）</span>' : ''}<span><i class="dash" style="--c:var(--ink-3)"></i>依赖箭头，红色虚线表示时间冲突</span><span>斜纹条＝按工期推算的日期</span></div>`;
    return `${legend}<div class="gantt clay" style="--px:${px}px;--row:${ROW}px">
      <div class="g-scroll" id="g-scroll" data-start="${m.start}" data-px="${px}" data-today="${x(m.today)}">
        <div class="g-inner" style="width:${TW + W}px">
          <div class="g-head"><div class="g-corner" style="width:${TW}px"><span>任务</span><span style="width:46px;text-align:right">开始</span><span style="width:46px;text-align:right">工期</span></div><div class="g-scale" style="width:${W}px">${months}${days}</div></div>
          <div class="g-body">${rowsHtml}${svg}<div class="g-today" style="left:${TW + x(m.today) + px / 2 - 1}px"></div></div>
        </div></div></div>`;
  },
  before(root, scope) {
    const sc = root.querySelector('#g-scroll'); if (!sc) return;
    const px = Number(sc.dataset.px), start = sc.dataset.start;
    S.view.gantt[scope] = { date: addDays(start, Math.round(sc.scrollLeft / px)), top: sc.scrollTop };
  },
  after(root, scope) {
    const sc = root.querySelector('#g-scroll'); if (!sc) return;
    const px = Number(sc.dataset.px), start = sc.dataset.start;
    const st = S.view.gantt[scope];
    if (st) { sc.scrollLeft = dayDiff(start, st.date) * px; sc.scrollTop = st.top; }
    else sc.scrollLeft = Math.max(0, Number(sc.dataset.today) - 5 * px);
    sc.querySelectorAll('.gbar').forEach(b => b.addEventListener('pointerdown', e => barDown(e, b, px)));
    sc.querySelectorAll('[data-unsched]').forEach(tr => tr.addEventListener('click', e => {
      if (e.target.closest('.gbar')) return;
      const t = S.data.tasks.get(tr.dataset.unsched); if (!t) return;
      const r = tr.getBoundingClientRect(), i = Math.floor((e.clientX - r.left) / px);
      const s = addDays(start, i); SY.patch('tasks', t.id, { start: s, due: addDays(s, dur(t) - 1) });
      SY.toast(`已排到 ${md(s)}，拖动条形可以调整`);
    }));
  }
};

let tip = null;
function showTip(ev, text) {
  if (!tip) { tip = document.createElement('div'); tip.className = 'g-tip'; document.body.appendChild(tip); }
  tip.textContent = text; tip.style.left = (ev.clientX + 14) + 'px'; tip.style.top = (ev.clientY + 14) + 'px';
}
function hideTip() { if (tip) { tip.remove(); tip = null; } }
function barDown(e, b, px) {
  if (e.button !== 0) return;
  const id = b.dataset.gid, t = S.data.tasks.get(id); if (!t) return;
  const mode = e.target.classList.contains('gh-l') ? 'l' : e.target.classList.contains('gh-r') ? 'r' : 'm';
  const sp = span(t), len = dayDiff(sp.s, sp.e);
  const l0 = parseFloat(b.style.left), w0 = parseFloat(b.style.width);
  let dd = 0;
  const calc = () => {
    let s = sp.s, en = sp.e;
    if (mode === 'm') { s = addDays(s, dd); en = addDays(en, dd); }
    else if (mode === 'l') s = addDays(s, Math.min(dd, len));
    else en = addDays(en, Math.max(dd, -len));
    return [s, en];
  };
  SY.drag(e, {
    threshold: 3,
    onStart() { b.style.cursor = mode === 'm' ? 'grabbing' : 'ew-resize'; },
    onMove(ev, dx) {
      dd = Math.round(dx / px);
      if (mode === 'm') b.style.left = (l0 + dd * px) + 'px';
      else if (mode === 'l') { const n = Math.min(dd, len); b.style.left = (l0 + n * px) + 'px'; b.style.width = (w0 - n * px) + 'px'; }
      else { const n = Math.max(dd, -len); b.style.width = (w0 + n * px) + 'px'; }
      const [s, en] = calc(); showTip(ev, `${s} → ${en} · ${dayDiff(s, en) + 1} 天`);
    },
    onEnd() { hideTip(); if (!dd) return; const [s, en] = calc(); SY.patch('tasks', id, { start: s, due: en }); }
  });
}

Object.assign(SY.acts, {
  'g-zoom': el => { S.ui.ganttZoom = el.dataset.v; SY.saveUI(); SY.renderAll(); },
  'g-hide': () => { S.ui.ganttHideDone = !S.ui.ganttHideDone; SY.saveUI(); SY.renderAll(); },
  'g-today': () => { const sc = document.querySelector('#g-scroll'); if (sc) sc.scrollTo({ left: Math.max(0, Number(sc.dataset.today) - 5 * Number(sc.dataset.px)), behavior: 'smooth' }); },
  'g-auto': el => {
    const r = SY.autoSchedule(el.dataset.scopeId);
    SY.toast(r.cyclic ? '依赖里有循环，部分任务没法排期' : r.n ? `已排好 ${r.n} 个任务的日期` : '没有需要调整的任务；已有开始日期的任务保持不变');
  }
});
SY.views.gantt = SY.chartView('gantt', '甘特图', '拖动条形改日期，拖两端改工期；箭头是前置依赖，红框是关键路径上的任务。');
})();
