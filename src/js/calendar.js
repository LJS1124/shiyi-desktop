/* 拾忆 calendar: month grid of tasks (multi-day tasks as bars across the week),
   project deadlines, unscheduled tasks on the side. Drag a bar to move it,
   drag from the side to schedule, double-click a day to add a task there. */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, list, getP, pColor, cv, scopeTasks, TODAY, addDays, dayDiff, parseYmd, ymd, isYmd, dur, taskSort, md } = SY;
const WEEK = ['一', '二', '三', '四', '五', '六', '日'];
const LANES = 4; // bars shown per week row before "+N"

const monthOf = d => d.slice(0, 7);
const addMonths = (m, n) => { const [y, mo] = m.split('-').map(Number); const d = new Date(y, mo - 1 + n, 1); return ymd(d).slice(0, 7); };
const mondayOf = d => { const w = (parseYmd(d).getDay() + 6) % 7; return addDays(d, -w); };
const curMonth = () => (/^\d{4}-\d{2}$/.test(S.ui.calMonth || '') ? S.ui.calMonth : monthOf(TODAY()));

/* what goes on the calendar */
function items(scope) {
  const showDone = !!S.ui.calDone, out = [];
  for (const t of scopeTasks(scope)) {
    if (t.status === 'done' && !showDone) continue;
    const a = isYmd(t.start) ? t.start : isYmd(t.due) ? t.due : null;
    const b = isYmd(t.due) ? t.due : a;
    if (!a) continue;
    const [s, e] = a <= b ? [a, b] : [b, a];
    out.push({ kind: 'task', id: t.id, s, e, t, p: getP(t.projectId) });
  }
  const ps = scope === 'all' ? list('projects').filter(p => p.status !== 'done') : scope !== '_loose' && getP(scope) ? [getP(scope)] : [];
  for (const p of ps) if (isYmd(p.due)) out.push({ kind: 'deadline', id: p.id, s: p.due, e: p.due, p });
  // deadlines first, then earlier start, then longer, then task order
  return out.sort((x, y) => (x.kind === y.kind ? 0 : x.kind === 'deadline' ? -1 : 1) || x.s.localeCompare(y.s) || y.e.localeCompare(x.e) || (x.t && y.t ? taskSort(x.t, y.t) : 0));
}

/* lay one week's items into lanes; returns placed segments and per-day overflow */
function layWeek(all, ws) {
  const we = addDays(ws, 6), lanes = [], placed = [], hidden = new Array(7).fill(0);
  for (const it of all) {
    if (it.e < ws || it.s > we) continue;
    const c0 = Math.max(0, dayDiff(ws, it.s)), c1 = Math.min(6, dayDiff(ws, it.e));
    let lane = lanes.findIndex(end => end < c0);
    if (lane < 0) { lane = lanes.length; lanes.push(-1); }
    lanes[lane] = c1;
    if (lane < LANES) placed.push({ it, c0, c1, lane, cutL: it.s < ws, cutR: it.e > we });
    else for (let c = c0; c <= c1; c++) hidden[c]++;
  }
  return { placed, hidden };
}

function barHtml({ it, c0, c1, lane, cutL, cutR }) {
  const col = `grid-column:${c0 + 1} / ${c1 + 2};grid-row:${lane + 2}`;
  if (it.kind === 'deadline') {
    const c = pColor(it.p);
    return `<button type="button" class="cal-bar is-deadline" style="${col};--c:${cv(c)};--c-soft:${cv(c, '-soft')};--c-ink:${cv(c, '-ink')}" data-act="cal-proj" data-v="${esc(it.id)}" title="项目截止：${esc(it.p.name)}">◆ ${esc(it.p.name)} 截止</button>`;
  }
  const t = it.t, c = it.p ? pColor(it.p) : 'ink-3', today = TODAY();
  const over = t.status !== 'done' && it.e < today;
  const cls = ['cal-bar', t.status === 'done' ? 'is-done' : '', t.status === 'doing' ? 'is-doing' : '', over ? 'is-over' : '', cutL ? 'cut-l' : '', cutR ? 'cut-r' : ''].join(' ');
  const when = it.s === it.e ? md(it.s) : `${md(it.s)} – ${md(it.e)}`;
  const tip = `${t.title}\n${when}${it.p ? ' · ' + it.p.name : ''}${over ? ' · 已逾期' : ''}`;
  const style = it.p ? `--c:${cv(c)};--c-soft:${cv(c, '-soft')};--c-ink:${cv(c, '-ink')}` : '--c:var(--ink-3);--c-soft:var(--clay-2);--c-ink:var(--ink-2)';
  return `<div class="${cls}" style="${col};${style}" data-open="tasks:${esc(t.id)}" data-cal-task="${esc(t.id)}" title="${esc(tip)}" tabindex="0">${t.priority === 1 ? '<b class="p1">!</b>' : ''}${esc(t.title)}</div>`;
}

function body(scope) {
  const month = curMonth(), first = month + '-01', today = TODAY();
  const ws0 = mondayOf(first);
  const last = addDays(addMonths(month, 1) + '-01', -1);
  const weeks = Math.ceil((dayDiff(ws0, last) + 1) / 7);
  const all = items(scope);
  let rows = '';
  for (let w = 0; w < weeks; w++) {
    const ws = addDays(ws0, w * 7), { placed, hidden } = layWeek(all, ws);
    let bg = '', nums = '';
    for (let c = 0; c < 7; c++) {
      const d = addDays(ws, c), other = monthOf(d) !== month, wk = c >= 5;
      bg += `<div class="cal-cell ${other ? 'is-other' : ''} ${wk ? 'is-wk' : ''} ${d === today ? 'is-today' : ''}" data-date="${d}"></div>`;
      const n = Number(d.slice(8));
      nums += `<div class="cal-num ${d === today ? 'is-today' : ''}" style="grid-column:${c + 1};grid-row:1"><span>${n === 1 ? `${Number(d.slice(5, 7))}月` : ''}${n}</span>${hidden[c] ? `<button type="button" class="cal-more" data-act="cal-day" data-v="${d}">+${hidden[c]}</button>` : ''}</div>`;
    }
    rows += `<div class="cal-week"><div class="cal-bg">${bg}</div><div class="cal-fg">${nums}${placed.map(barHtml).join('')}</div></div>`;
  }
  const [y, m] = month.split('-').map(Number);
  const unsched = scopeTasks(scope).filter(t => t.status !== 'done' && !isYmd(t.start) && !isYmd(t.due)).sort(taskSort);
  const side = `<aside class="cal-side clay"><div class="cal-side-h">未排期 <span class="n">${unsched.length}</span></div>
    <p class="te">拖到日历上的某一天就排好了。</p>
    <div class="cal-side-list">${unsched.slice(0, 60).map(t => { const p = getP(t.projectId), c = p ? pColor(p) : 'ink-3'; return `<div class="cal-un" data-open="tasks:${esc(t.id)}" data-cal-task="${esc(t.id)}" data-unsched="1" style="--c:${cv(c)}" tabindex="0"><i></i><span>${esc(t.title)}</span>${dur(t) > 1 ? `<em>${dur(t)}天</em>` : ''}</div>`; }).join('') || '<div class="empty">都排好了。</div>'}</div></aside>`;
  return `<div class="cal-wrap"><div class="cal clay">
      <div class="cal-head"><div class="cal-title">${y} 年 ${m} 月</div>
        <div class="cal-nav"><button type="button" class="icon-btn sm" data-act="cal-move" data-v="-1" aria-label="上个月">‹</button><button type="button" class="btn btn-sm btn-ghost" data-act="cal-today">今天</button><button type="button" class="icon-btn sm" data-act="cal-move" data-v="1" aria-label="下个月">›</button></div></div>
      <div class="cal-dow">${WEEK.map((d, i) => `<div class="${i >= 5 ? 'is-wk' : ''}">周${d}</div>`).join('')}</div>
      <div class="cal-grid" style="--weeks:${weeks}">${rows}</div>
    </div>${side}</div>`;
}

SY.mods.calendar = {
  tools() {
    return `<label class="opt-chk"><input type="checkbox" id="cal-done" ${S.ui.calDone ? 'checked' : ''}> 显示已完成</label>
      <span class="te">双击某一天新建任务 · 拖动条形改日期</span>`;
  },
  body,
  after(root, scope) {
    const grid = root.querySelector('.cal-wrap'); if (!grid) return;
    grid.addEventListener('pointerdown', e => onDown(e, scope));
    grid.addEventListener('dblclick', e => {
      if (e.target.closest('.cal-bar,.cal-more,.cal-side')) return;
      const cell = cellAt(e.clientX, e.clientY); if (cell) quickAdd(cell, scope);
    });
  },
  key(e) {
    if (e.target.closest && e.target.closest('input,textarea,select')) return false;
    if (e.key === 'ArrowLeft' && e.altKey) { move(-1); return true; }
    if (e.key === 'ArrowRight' && e.altKey) { move(1); return true; }
    return false;
  }
};

function cellAt(x, y) {
  for (const c of document.querySelectorAll('.cal-cell')) { const r = c.getBoundingClientRect(); if (x >= r.left && x < r.right && y >= r.top && y < r.bottom) return c; }
  return null;
}

/* drag a bar (moves start and due together) or an unscheduled task (schedules it) */
function onDown(e, scope) {
  if (e.button !== 0) return;
  const el = e.target.closest('[data-cal-task]'); if (!el) return;
  const t = S.data.tasks.get(el.dataset.calTask); if (!t) return;
  const from = el.dataset.unsched ? null : cellAt(e.clientX, e.clientY);
  let ghost = null, over = null;
  SY.drag(e, {
    onStart() {
      ghost = document.createElement('div'); ghost.className = 'cal-ghost'; ghost.textContent = t.title; document.body.appendChild(ghost);
      el.classList.add('is-lifting');
    },
    onMove(ev) {
      ghost.style.left = ev.clientX + 12 + 'px'; ghost.style.top = ev.clientY + 8 + 'px';
      const c = cellAt(ev.clientX, ev.clientY);
      if (c !== over) { if (over) over.classList.remove('is-drop'); over = c; if (c) c.classList.add('is-drop'); }
    },
    onEnd() {
      if (ghost) ghost.remove(); el.classList.remove('is-lifting'); if (over) over.classList.remove('is-drop');
      if (!over) return;
      const to = over.dataset.date;
      if (!from) { // schedule
        const d = dur(t);
        SY.patch('tasks', t.id, { start: to, due: addDays(to, d - 1) });
        SY.toast(`已排到 ${md(to)}${d > 1 ? `，工期 ${d} 天` : ''}`);
        return;
      }
      const delta = dayDiff(from.dataset.date, to); if (!delta) return;
      const ch = {};
      if (isYmd(t.start)) ch.start = addDays(t.start, delta);
      if (isYmd(t.due)) ch.due = addDays(t.due, delta);
      SY.patch('tasks', t.id, ch);
      SY.toast(`「${t.title.slice(0, 16)}」${delta > 0 ? '推后' : '提前'} ${Math.abs(delta)} 天`);
    }
  });
}

/* double-click a day: type a title, Enter adds a task due that day */
function quickAdd(cell, scope) {
  document.querySelectorAll('.cal-add').forEach(x => x.remove());
  const d = cell.dataset.date, r = cell.getBoundingClientRect();
  const box = document.createElement('div'); box.className = 'cal-add clay';
  box.style.left = Math.min(r.left, window.innerWidth - 300) + 'px'; box.style.top = r.top + 24 + 'px';
  const pid = scope !== 'all' && scope !== '_loose' ? scope : null;
  box.innerHTML = `<div class="te">${md(d)} 周${WEEK[(parseYmd(d).getDay() + 6) % 7]} · 新任务${pid ? ' · ' + esc(getP(pid).name) : ''}</div><input class="in" placeholder="要做的事，回车保存，Esc 取消">`;
  document.body.appendChild(box);
  const inp = box.querySelector('input'); inp.focus(); S.editing = true;
  const close = () => { S.editing = false; box.remove(); SY.scheduleRender(); };
  inp.addEventListener('keydown', ev => {
    if (ev.key === 'Escape') { ev.stopPropagation(); close(); }
    if (ev.key === 'Enter' && !ev.isComposing && ev.keyCode !== 229) {
      const title = inp.value.trim(); if (!title) return close();
      SY.newTask({ title, projectId: pid, start: d, due: d }); close(); SY.toast(`已添加到 ${md(d)}`);
    }
  });
  inp.addEventListener('blur', () => setTimeout(() => { if (document.body.contains(box)) close(); }, 120));
}

function move(n) { S.ui.calMonth = addMonths(curMonth(), n); SY.saveUI(); SY.scheduleRender(); }
Object.assign(SY.acts, {
  'cal-move': el => move(Number(el.dataset.v)),
  'cal-today': () => { S.ui.calMonth = monthOf(TODAY()); SY.saveUI(); SY.scheduleRender(); },
  'cal-proj': el => { SY.closeModal(); SY.goProject(el.dataset.v); },
  'cal-day': el => { // "+N": list that day's tasks
    const d = el.dataset.v, sc = SY.validScope(S.ui.scope, true);
    const its = items(sc).filter(it => it.s <= d && it.e >= d);
    SY.openModal(`${md(d)} 周${WEEK[(parseYmd(d).getDay() + 6) % 7]}`, `<div class="cal-daylist">${its.map(it => it.kind === 'deadline'
      ? `<button type="button" class="linkish" data-act="cal-proj" data-v="${esc(it.id)}">◆ ${esc(it.p.name)} 截止</button>`
      : `<div>${SY.taskRow ? SY.taskRow(it.t) : esc(it.t.title)}</div>`).join('')}</div>`, '', { iconName: 'cal' });
  }
});

document.addEventListener('change', e => { if (e.target.id === 'cal-done') { S.ui.calDone = !!e.target.checked; SY.saveUI(); SY.scheduleRender(); } });

SY.views.calendar = SY.chartView('calendar', '日历', '按日期看任务和项目截止；多天的任务横跨显示，颜色对应项目。');
})();
