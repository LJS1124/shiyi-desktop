/* 拾忆 kanban: status columns, optional project swimlanes, pointer drag with ordering */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, list, getP, pColor, cv, prioTag, dueTag, scopeTasks, TODAY, isSel } = SY;
const KCOLS = [['todo', '待办', 'lav'], ['doing', '进行中', 'butter'], ['done', '已完成', 'mint']];
const rk = t => (typeof t.rank === 'number' ? t.rank : (t.createdAt || 0));
const expanded = new Set();

function card(t, showProject) {
  const p = getP(t.projectId), inToday = t.todayDate === TODAY() && t.status !== 'done';
  const nx = t.status === 'todo' ? ['开始', 'arrow'] : t.status === 'doing' ? ['完成', 'check'] : ['重开', 'reopen'];
  return `<div class="kcard ${t.status === 'done' ? 'is-done' : ''} ${isSel('tasks', t.id) ? 'is-sel' : ''}" data-kid="${esc(t.id)}" data-open="tasks:${esc(t.id)}" tabindex="0" style="--c:${p ? cv(pColor(p)) : 'transparent'}">
    <div class="kc-title">${esc(t.title)}</div>
    <div class="kc-foot"><div class="meta">${prioTag(t.priority)}${showProject ? SY.projChip(p) : ''}${dueTag(t)}${inToday ? `<span style="color:var(--butter-ink);display:inline-flex">${icon('sun', 12)}</span>` : ''}${(t.deps || []).length ? `<span>${icon('link', 12)}${t.deps.length}</span>` : ''}</div>
      <button type="button" class="mini" data-act="advance" data-id="${esc(t.id)}">${icon(nx[1], 12)}${nx[0]}</button></div></div>`;
}
function column([k, l, c], tasks, lane, showProject) {
  let ts = tasks.filter(t => t.status === k);
  ts = k === 'done' ? ts.sort((a, b) => (b.doneAt || 0) - (a.doneAt || 0)) : ts.sort((a, b) => rk(a) - rk(b));
  const key = lane + ':' + k, lim = k === 'done' && !expanded.has(key) ? 12 : Infinity, shown = ts.slice(0, lim);
  return `<section class="kb-col" data-col="${k}" data-lane="${esc(lane)}" aria-label="${l}">
    <div class="kb-h"><span class="sw" style="--c:${cv(c)}"></span>${l}<span class="n">${ts.length}</span></div>
    ${shown.map(t => card(t, showProject)).join('')}
    ${ts.length > shown.length ? `<button type="button" class="kb-more" data-act="kb-more" data-key="${esc(key)}">还有 ${ts.length - shown.length} 件 · 展开</button>` : ''}
    ${k !== 'done' ? `<input class="add-task" data-add="${esc(lane)}" data-status="${k}" data-keep="add-${esc(lane)}-${k}" placeholder="＋ 添加任务，回车保存" aria-label="添加任务">` : ''}
  </section>`;
}

SY.mods.kanban = {
  tools(scope, inProject) {
    if (inProject) return '';
    return `<button type="button" class="chip ${S.ui.kbSwim ? 'is-on' : ''}" data-act="kb-swim" ${scope !== 'all' ? 'disabled style="opacity:.45"' : ''}>按项目分泳道</button>`;
  },
  body(scope, inProject) {
    const tasks = scopeTasks(scope);
    if (scope === 'all' && S.ui.kbSwim && !inProject) {
      const groups = [];
      list('projects').filter(p => p.status !== 'done').sort((a, b) => (a.area || '').localeCompare(b.area || '') || (a.createdAt || 0) - (b.createdAt || 0))
        .forEach(p => { const ts = tasks.filter(t => t.projectId === p.id); if (ts.length || p.status === 'active') groups.push([p.id, p, ts]); });
      const loose = scopeTasks('_loose'); groups.push(['_loose', null, loose]);
      return `<div class="kb-swim">${groups.map(([pid, p, ts]) => `<div class="lane"><div class="lane-h"><span class="dot" style="--c:${p ? cv(pColor(p)) : 'var(--ink-3)'}"></span>${p ? esc(p.name) : '零散任务'}<span class="mono">${ts.filter(t => t.status !== 'done').length} 件未完成</span></div>
        <div class="kb" data-swim="1">${KCOLS.map(c => column(c, ts, pid, false)).join('')}</div></div>`).join('')}</div>`;
    }
    const lane = scope === 'all' ? '_loose' : scope;
    return `<div class="kb">${KCOLS.map(c => column(c, tasks, lane, scope === 'all')).join('')}</div>`;
  },
  after(root) { root.querySelectorAll('.kb').forEach(kb => kb.addEventListener('pointerdown', onDown)); }
};

function onDown(e) {
  if (e.button !== 0) return;
  const c = e.target.closest('.kcard'); if (!c || e.target.closest('button,input')) return;
  const id = c.dataset.kid, rect = c.getBoundingClientRect(), ox = e.clientX - rect.left, oy = e.clientY - rect.top;
  let ghost = null, line = null, target = null;
  SY.drag(e, {
    onStart() {
      ghost = c.cloneNode(true); ghost.classList.add('ghost'); ghost.style.width = rect.width + 'px';
      ghost.style.left = rect.left + 'px'; ghost.style.top = rect.top + 'px'; document.body.appendChild(ghost);
      c.classList.add('is-placeholder'); line = document.createElement('div'); line.className = 'drop-line';
    },
    onMove(ev) {
      ghost.style.left = (ev.clientX - ox) + 'px'; ghost.style.top = (ev.clientY - oy) + 'px';
      const el = document.elementFromPoint(ev.clientX, ev.clientY), col = el && el.closest('.kb-col');
      document.querySelectorAll('.kb-col.is-over').forEach(x => { if (x !== col) x.classList.remove('is-over'); });
      if (!col) { target = null; line.remove(); return; }
      col.classList.add('is-over');
      const cards = [...col.querySelectorAll('.kcard')].filter(x => x !== c);
      let before = null;
      for (const k of cards) { const r = k.getBoundingClientRect(); if (ev.clientY < r.top + r.height / 2) { before = k; break; } }
      const anchor = before || col.querySelector('.kb-more,.add-task');
      if (anchor) col.insertBefore(line, anchor); else col.appendChild(line);
      target = { col, before, cards };
    },
    onEnd() {
      if (ghost) ghost.remove(); if (line) line.remove(); c.classList.remove('is-placeholder');
      document.querySelectorAll('.kb-col.is-over').forEach(x => x.classList.remove('is-over'));
      if (!target) return;
      const t = S.data.tasks.get(id); if (!t) return;
      const st = target.col.dataset.col, cards = target.cards;
      const idx = target.before ? cards.indexOf(target.before) : cards.length;
      const prev = cards[idx - 1] ? S.data.tasks.get(cards[idx - 1].dataset.kid) : null;
      const next = cards[idx] ? S.data.tasks.get(cards[idx].dataset.kid) : null;
      const part = { rank: prev && next ? (rk(prev) + rk(next)) / 2 : prev ? rk(prev) + 1000 : next ? rk(next) - 1000 : Date.now() };
      if (st !== t.status) { part.status = st; part.doneAt = st === 'done' ? Date.now() : null; }
      const kb = target.col.closest('.kb');
      if (kb && kb.dataset.swim) {
        const lane = target.col.dataset.lane, np = lane === '_loose' ? null : lane;
        if ((t.projectId || null) !== np) { part.projectId = np; part.deps = []; }
      }
      SY.patch('tasks', id, part);
    }
  });
}

Object.assign(SY.acts, {
  'kb-swim': () => { S.ui.kbSwim = !S.ui.kbSwim; SY.saveUI(); SY.renderAll(); },
  'kb-more': el => { expanded.add(el.dataset.key); SY.renderAll(); }
});
SY.views.kanban = SY.chartView('kanban', '看板', '拖动卡片改变状态和顺序；打开泳道后，拖到别的项目泳道即可换项目。');
})();
