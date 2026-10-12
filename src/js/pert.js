/* 拾遗 PERT network: CPM forward/backward pass, three-point estimates, on-time probability, drag-to-link */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, getP, scopeTasks, TODAY, addDays, dayDiff, isYmd, taskSort, md } = SY;
const NW = 216, NH = 108, GX = 100, GY = 28, EPS = 1e-6;
S.view.pert = S.view.pert || {};
let selEdge = null;

SY.cpm = tasks => {
  const ids = new Set(tasks.map(t => t.id));
  const preds = new Map(), succs = new Map();
  tasks.forEach(t => { preds.set(t.id, (t.deps || []).filter(d => ids.has(d))); succs.set(t.id, []); });
  tasks.forEach(t => preds.get(t.id).forEach(d => succs.get(d).push(t.id)));
  const indeg = new Map(tasks.map(t => [t.id, preds.get(t.id).length]));
  const q = tasks.filter(t => indeg.get(t.id) === 0).sort(taskSort).map(t => t.id), order = [];
  while (q.length) { const id = q.shift(); order.push(id); succs.get(id).forEach(s => { indeg.set(s, indeg.get(s) - 1); if (indeg.get(s) === 0) q.push(s); }); }
  const inOrder = new Set(order);
  const cycleIds = tasks.filter(t => !inOrder.has(t.id)).map(t => t.id);
  const D = new Map(tasks.map(t => { const e = SY.est(t); return [t.id, t.status === 'done' ? { te: 0, v: 0, set: e.set } : e]; }));
  const ES = new Map(), EF = new Map(), LS = new Map(), LF = new Map();
  order.forEach(id => { const es = Math.max(0, ...preds.get(id).filter(p => EF.has(p)).map(p => EF.get(p))); ES.set(id, es); EF.set(id, es + D.get(id).te); });
  const T = Math.max(0, ...order.map(id => EF.get(id)));
  [...order].reverse().forEach(id => { const ss = succs.get(id).filter(s => LS.has(s)); const lf = ss.length ? Math.min(...ss.map(s => LS.get(s))) : T; LF.set(id, lf); LS.set(id, lf - D.get(id).te); });
  const slack = new Map(order.map(id => [id, LS.get(id) - ES.get(id)]));
  const crit = new Set(order.filter(id => Math.abs(slack.get(id)) < EPS));
  const path = [];
  let cur = order.filter(id => crit.has(id) && Math.abs(EF.get(id) - T) < EPS).sort((a, b) => D.get(b).v - D.get(a).v)[0];
  while (cur) { path.unshift(cur); cur = preds.get(cur).filter(p => crit.has(p) && Math.abs(EF.get(p) - ES.get(cur)) < EPS).sort((a, b) => D.get(b).v - D.get(a).v)[0]; }
  const sigma = Math.sqrt(path.reduce((s, id) => s + D.get(id).v, 0));
  return { order, preds, succs, ES, EF, LS, LF, slack, crit, path, T, sigma, D, cycleIds };
};
function erf(x) { const s = Math.sign(x); x = Math.abs(x); const t = 1 / (1 + 0.3275911 * x); const y = 1 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * Math.exp(-x * x); return s * y; }
const phi = z => 0.5 * (1 + erf(z / Math.SQRT2));
const f1 = v => (Math.abs(v - Math.round(v)) < 0.05 ? String(Math.round(v)) : v.toFixed(1));

function layout(c, tasks) {
  const byId = new Map(tasks.map(t => [t.id, t]));
  const layer = new Map();
  c.order.forEach(id => { const ps = c.preds.get(id); layer.set(id, ps.length ? Math.max(...ps.map(p => (layer.get(p) ?? 0) + 1)) : 0); });
  const maxL = Math.max(-1, ...layer.values());
  c.cycleIds.forEach(id => layer.set(id, maxL + 1));
  const layers = [];
  for (const [id, l] of layer) (layers[l] = layers[l] || []).push(id);
  for (let i = 0; i < layers.length; i++) layers[i] = layers[i] || [];
  layers.forEach(L => L.sort((a, b) => (c.ES.get(a) ?? 0) - (c.ES.get(b) ?? 0) || taskSort(byId.get(a), byId.get(b))));
  for (let sweep = 0; sweep < 3; sweep++) {
    const pos = new Map(); layers.forEach(L => L.forEach((id, i) => pos.set(id, i)));
    for (let l = 1; l < layers.length; l++) {
      const bary = id => { const ps = (c.preds.get(id) || []).filter(p => pos.has(p)); return ps.length ? ps.reduce((s, p) => s + pos.get(p), 0) / ps.length : pos.get(id); };
      const b = new Map(layers[l].map(id => [id, bary(id)]));
      layers[l].sort((x, y) => b.get(x) - b.get(y));
      layers[l].forEach((id, i) => pos.set(id, i));
    }
  }
  const maxN = Math.max(1, ...layers.map(L => L.length)), H = maxN * (NH + GY) - GY;
  const pos = new Map();
  layers.forEach((L, l) => { const off = (H - (L.length * (NH + GY) - GY)) / 2; L.forEach((id, i) => pos.set(id, { x: l * (NW + GX), y: off + i * (NH + GY) })); });
  return { pos, W: Math.max(NW, layers.length * (NW + GX) - GX), H };
}

SY.mods.pert = {
  tools(scope) {
    const isProj = scope !== 'all' && scope !== '_loose';
    return `<span style="flex:1"></span>${scope !== 'all' ? `<button type="button" class="btn btn-sm" data-act="g-auto" data-scope-id="${esc(scope)}">${icon('wand', 14)}按网络图排进甘特图</button>` : ''}
      ${isProj ? `<button type="button" class="btn btn-sm btn-ai ai" data-act="ai-schedule" data-id="${esc(scope)}">${icon('spark', 14)}AI 估算工期与依赖</button>` : ''}`;
  },
  body(scope) {
    const tasks = scopeTasks(scope);
    if (!tasks.length) return `<div class="empty-big clay"><h3>还没有任务</h3><p>网络图把任务按前置依赖连成一张图，算出关键路径和按期完成的概率。先给项目加几件任务。</p></div>`;
    const c = SY.cpm(tasks), lay = layout(c, tasks), p = getP(scope);
    const base = p && isYmd(p.start) && p.start > TODAY() ? p.start : TODAY();
    const hasDeps = tasks.some(t => (t.deps || []).some(d => tasks.some(x => x.id === d)));
    const estCount = tasks.filter(t => SY.est(t).set).length;
    let prob = '', probCls = 't-lav', probSub = '项目没有截止日期';
    if (p && isYmd(p.due)) {
      const Tdue = dayDiff(base, p.due) + 1;
      const pr = c.sigma > EPS ? phi((Tdue - c.T) / c.sigma) : (c.T <= Tdue ? 1 : 0);
      prob = Math.round(pr * 100) + '%'; probCls = pr >= 0.8 ? 't-mint' : pr >= 0.5 ? 't-butter' : 't-rose';
      probSub = `距截止 ${Tdue} 天`;
    }
    const finish = c.T > 0 ? addDays(base, Math.ceil(c.T) - 1) : base;
    const tiles = `<div class="tiles">
      <div class="tile clay t-lav"><span class="k">期望工期 Te</span><span class="v">${f1(c.T)}<small style="font-size:14px;margin-left:4px">天</small></span><span class="d">从 ${md(base)} 起算，约 ${md(finish)} 完成</span></div>
      <div class="tile clay t-sky"><span class="k">标准差 σ</span><span class="v">${c.sigma.toFixed(2)}</span><span class="d">关键路径上的不确定性</span></div>
      <div class="tile clay ${probCls}"><span class="k">按期完成概率</span><span class="v">${prob || '—'}</span><span class="d">${esc(probSub)}</span></div>
      <div class="tile clay t-peach"><span class="k">已估算</span><span class="v">${estCount}/${tasks.length}</span><span class="d">未估算的按 1 天计</span></div></div>`;
    const cpath = c.path.length ? `<div class="cpath">关键路径：${c.path.map(id => `<b>${esc(S.data.tasks.get(id).title)}</b>`).join(' → ')}</div>` : '';
    const warn = (c.cycleIds.length ? `<div class="warn">有任务形成了循环依赖，没法计算：${c.cycleIds.map(id => esc(S.data.tasks.get(id).title)).join('、')}。在详情里移除其中一条前置任务即可。</div>` : '')
      + (!hasDeps ? `<div class="ai-note">还没有依赖关系。把鼠标移到任务卡片上，从右侧圆点拖一条线到后续任务，就建立了「先做完 A 才能做 B」。也可以用「AI 估算工期与依赖」一次补齐。</div>` : '');
    let edges = '';
    for (const t of tasks) for (const d of (t.deps || [])) {
      const a = lay.pos.get(d), b = lay.pos.get(t.id); if (!a || !b) continue;
      const x1 = a.x + NW, y1 = a.y + NH / 2, x2 = b.x, y2 = b.y + NH / 2, dx = Math.max(40, (x2 - x1) * 0.5);
      const crit = c.crit.has(d) && c.crit.has(t.id) && Math.abs(c.EF.get(d) - c.ES.get(t.id)) < EPS;
      const key = d + '>' + t.id, path = `M${x1},${y1} C${x1 + dx},${y1} ${x2 - dx},${y2} ${x2 - 3},${y2}`;
      edges += `<path class="pe ${crit ? 'crit' : ''} ${selEdge === key ? 'sel' : ''}" d="${path}" marker-end="url(#pa-${crit ? 'c' : 'n'})"/><path class="pe-hit" data-edge="${esc(key)}" d="${path}"/>`;
    }
    const nodes = tasks.map(t => {
      const q = lay.pos.get(t.id); if (!q) return '';
      const es = c.ES.get(t.id), ef = c.EF.get(t.id), ls = c.LS.get(t.id), lf = c.LF.get(t.id), sl = c.slack.get(t.id), d = c.D.get(t.id);
      const ok = es != null;
      return `<div class="pnode ${c.crit.has(t.id) && t.status !== 'done' ? 'is-crit' : ''} ${t.status === 'done' ? 'is-done' : ''} ${SY.isSel('tasks', t.id) ? 'is-sel' : ''}" data-pid="${esc(t.id)}" data-open="tasks:${esc(t.id)}" style="--nw:${NW}px;left:${q.x}px;top:${q.y}px;height:${NH}px">
        <div class="pn-row top"><span>ES<b>${ok ? f1(es) : '—'}</b></span><span>te<b>${f1(d.te)}${d.set ? '' : '*'}</b></span><span>EF<b>${ok ? f1(ef) : '—'}</b></span></div>
        <div class="pn-title">${t.status === 'done' ? '✓ ' : ''}${esc(t.title)}</div>
        <div class="pn-row bot"><span>LS<b>${ok ? f1(ls) : '—'}</b></span><span>松弛<b class="sl">${ok ? f1(sl) : '—'}</b></span><span>LF<b>${ok ? f1(lf) : '—'}</b></span></div>
        <span class="pn-handle" data-from="${esc(t.id)}" title="拖到另一个任务，表示那个任务要等这个完成"></span></div>`;
    }).join('');
    const pad = 40;
    return `${tiles}${cpath}${warn}
      <div class="canvas-wrap dotted" id="pert-wrap" data-w="${lay.W + pad * 2}" data-h="${lay.H + pad * 2}">
        <div class="canvas" id="pert-canvas" style="width:${lay.W + pad * 2}px;height:${lay.H + pad * 2}px">
          <svg id="pert-svg" width="${lay.W + pad * 2}" height="${lay.H + pad * 2}"><defs>
            <marker id="pa-n" viewBox="0 0 8 8" refX="6" refY="4" markerWidth="8" markerHeight="8" orient="auto"><path d="M0,0 L8,4 L0,8 z" style="fill:var(--ink-3)"/></marker>
            <marker id="pa-c" viewBox="0 0 8 8" refX="6" refY="4" markerWidth="7" markerHeight="7" orient="auto"><path d="M0,0 L8,4 L0,8 z" style="fill:var(--rose)"/></marker></defs>
            <g transform="translate(${pad},${pad})">${edges}<path class="pe-tmp" id="pe-tmp" d="" /></g></svg>
          <div style="position:absolute;left:${pad}px;top:${pad}px">${nodes}</div>
        </div>
        <div class="canvas-tools"><button type="button" class="icon-btn clay-btn sm" data-act="pz-out" aria-label="缩小">${icon('minus')}</button><button type="button" class="icon-btn clay-btn sm" data-act="pz-in" aria-label="放大">${icon('plus')}</button><button type="button" class="icon-btn clay-btn sm" data-act="pz-fit" aria-label="适应窗口">${icon('fit')}</button></div>
        <div class="canvas-hint">从卡片右侧圆点拖线建立依赖 · 点连线后按 Delete 删除 · 双指滚动平移，⌘ + 滚轮缩放 · ES/EF 最早开始/完成，LS/LF 最迟开始/完成（单位：天）${selEdge ? ' · <b>已选中一条依赖，按 Delete 删除</b>' : ''}</div>
      </div>`;
  },
  before(root, scope) { /* pan state lives in S.view.pert, updated live */ },
  after(root, scope) {
    const wrap = root.querySelector('#pert-wrap'), canvas = root.querySelector('#pert-canvas'); if (!wrap) return;
    const key = scope + ':' + S.ui.view;
    let st = S.view.pert[key]; const fresh = !st;
    if (!st) st = S.view.pert[key] = { x: 0, y: 0, k: 1 };
    const pz = SY.panZoom(wrap, canvas, st);
    SY.pertPZ = pz;
    if (fresh) requestAnimationFrame(() => pz.fit(Number(wrap.dataset.w), Number(wrap.dataset.h), 30));
    wrap.querySelectorAll('.pe-hit').forEach(h => h.addEventListener('click', e => { e.stopPropagation(); selEdge = h.dataset.edge; SY.renderAll(); }));
    wrap.addEventListener('click', e => { if (!e.target.closest('.pe-hit,.pnode') && selEdge) { selEdge = null; SY.renderAll(); } });
    wrap.querySelectorAll('.pn-handle').forEach(hd => hd.addEventListener('pointerdown', e => linkDown(e, hd, wrap, st)));
  },
  key(e) {
    if (selEdge && (e.key === 'Delete' || e.key === 'Backspace')) {
      e.preventDefault(); const [from, to] = selEdge.split('>'); selEdge = null;
      const t = S.data.tasks.get(to); const f = S.data.tasks.get(from);
      SY.removeDep(to, from); SY.toast(`已删除依赖：${f ? f.title : ''} → ${t ? t.title : ''}`, { undo: () => SY.addDep(to, from) });
      return true;
    }
    if (e.key === 'Escape' && selEdge) { selEdge = null; SY.renderAll(); return true; }
    return false;
  }
};
function linkDown(e, hd, wrap, st) {
  if (e.button !== 0) return;
  e.stopPropagation();
  const from = hd.dataset.from, node = hd.closest('.pnode');
  const tmp = wrap.querySelector('#pe-tmp');
  const x1 = parseFloat(node.style.left) + NW, y1 = parseFloat(node.style.top) + NH / 2;
  let over = null;
  const toLocal = ev => { const r = wrap.getBoundingClientRect(); return [(ev.clientX - r.left - st.x) / st.k - 40, (ev.clientY - r.top - st.y) / st.k - 40]; };
  SY.drag(e, {
    threshold: 2,
    onMove(ev) {
      const [x2, y2] = toLocal(ev), dx = Math.max(30, Math.abs(x2 - x1) * 0.5);
      tmp.setAttribute('d', `M${x1},${y1} C${x1 + dx},${y1} ${x2 - dx},${y2} ${x2},${y2}`);
      const el = document.elementFromPoint(ev.clientX, ev.clientY), n = el && el.closest('.pnode');
      if (over && over !== n) over.classList.remove('is-target');
      over = n && n !== node ? n : null; if (over) over.classList.add('is-target');
    },
    onEnd() {
      tmp.setAttribute('d', '');
      if (over) { over.classList.remove('is-target'); const to = over.dataset.pid; if (SY.addDep(to, from)) SY.toast('已建立依赖'); }
    }
  });
}
Object.assign(SY.acts, {
  'pz-in': () => SY.pertPZ && SY.pertPZ.zoom(1.2),
  'pz-out': () => SY.pertPZ && SY.pertPZ.zoom(1 / 1.2),
  'pz-fit': () => { const w = document.querySelector('#pert-wrap'); if (w && SY.pertPZ) SY.pertPZ.fit(Number(w.dataset.w), Number(w.dataset.h), 30); }
});
SY.views.pert = SY.chartView('pert', '网络图 / PERT', '按前置依赖把任务连成网络，用三点估算算出关键路径、期望工期和按期完成的概率。', { canvas: true, allowAll: false });
})();
