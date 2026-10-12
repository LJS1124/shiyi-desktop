/* 十一遗 mind map: balanced tree layout, keyboard editing, drag to reparent, undo, convert to tasks/ideas */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, uid, list, getP, cv, PALETTE, ago, now, AREA } = SY;
S.view.mm = S.view.mm || {};
let selId = null, pendingEdit = null, pendingNew = null;
const stacks = new Map();
const HG0 = 64, HG = 40, VG = 14;

const curMap = () => S.data.maps.get(S.ui.mapId) || null;
const nodesOf = m => m.nodes || {};
function stackOf(id) { if (!stacks.has(id)) stacks.set(id, { u: [], r: [] }); return stacks.get(id); }
function snapshot(m) { return JSON.stringify({ rootId: m.rootId, nodes: m.nodes, title: m.title }); }
function mutate(fn, { record = true } = {}) {
  const m = curMap(); if (!m) return;
  if (record) { const st = stackOf(m.id); st.u.push(snapshot(m)); if (st.u.length > 80) st.u.shift(); st.r = []; }
  const nodes = JSON.parse(JSON.stringify(m.nodes || {}));
  const extra = fn(nodes, m) || {};
  SY.patch('maps', m.id, { nodes, ...extra });
}
function undo() { const m = curMap(); if (!m) return; const st = stackOf(m.id); if (!st.u.length) return SY.toast('没有可以撤销的操作'); st.r.push(snapshot(m)); const s = JSON.parse(st.u.pop()); SY.patch('maps', m.id, s); if (selId && !s.nodes[selId]) selId = s.rootId; }
function redo() { const m = curMap(); if (!m) return; const st = stackOf(m.id); if (!st.r.length) return; st.u.push(snapshot(m)); const s = JSON.parse(st.r.pop()); SY.patch('maps', m.id, s); if (selId && !s.nodes[selId]) selId = s.rootId; }

function makeNode(text, parent) { return { id: uid(), text: text || '', parent: parent || null, children: [], collapsed: false, color: null, link: null }; }
function newMap({ title, rootText, ideaId = null, projectId = null, build } = {}) {
  const root = makeNode(rootText || title || '中心主题', null);
  const nodes = { [root.id]: root };
  if (build) build(nodes, root);
  const m = SY.put('maps', { id: SY.newId('maps'), title: title || rootText || '未命名导图', rootId: root.id, nodes, ideaId, projectId, createdAt: now() });
  S.ui.mapId = m.id; selId = root.id; SY.saveUI();
  return m;
}
SY.newMap = newMap;
function addChildTo(nodes, pid, text) { const n = makeNode(text, pid); nodes[n.id] = n; nodes[pid].children.push(n.id); nodes[pid].collapsed = false; return n.id; }
SY.mmAddChild = addChildTo;
function isDesc(nodes, id, anc) { let x = nodes[id]; while (x) { if (x.parent === anc) return true; x = nodes[x.parent]; } return false; }
function subtreeIds(nodes, id) { const out = [id]; (nodes[id]?.children || []).forEach(c => out.push(...subtreeIds(nodes, c))); return out; }

/* ---------- side assignment + colors (needed at render time) ---------- */
function analyze(m) {
  const N = nodesOf(m), root = N[m.rootId]; const side = {}, branch = {}, depth = {};
  if (!root) return { side, branch, depth };
  const leaves = id => { const n = N[id]; if (!n || n.collapsed || !n.children.length) return 1; return n.children.reduce((s, c) => s + leaves(c), 0); };
  const kids = root.children.filter(k => N[k]);
  const total = kids.reduce((s, k) => s + leaves(k), 0); let acc = 0;
  kids.forEach((k, i) => { const l = leaves(k); const s = kids.length > 1 && acc >= total / 2 ? 'l' : 'r'; acc += l; side[k] = s; branch[k] = N[k].color || PALETTE[i % PALETTE.length]; });
  const walk = (id, d, s, b) => { depth[id] = d; if (d > 0) { side[id] = s; branch[id] = N[id].color || b; } (N[id].collapsed ? [] : N[id].children).forEach(c => N[c] && walk(c, d + 1, d === 0 ? side[c] : s, d === 0 ? branch[c] : (N[id].color || b))); };
  walk(root.id, 0, 'r', 'lav');
  return { side, branch, depth };
}

/* ---------- view ---------- */
SY.views.mindmap = {
  canvas: true,
  render() {
    const maps = list('maps').sort((a, b) => (b.updatedAt || 0) - (a.updatedAt || 0));
    let m = curMap(); if (!m && maps.length) { m = maps[0]; S.ui.mapId = m.id; }
    const projs = list('projects').filter(p => p.status !== 'done');
    const listHtml = `<div class="mm-list clay">
      <button class="btn btn-primary btn-sm" type="button" data-act="mm-new">${icon('plus', 14)}新建导图</button>
      ${projs.length ? `<select class="ctl" id="mm-fromproj" aria-label="从项目生成"><option value="">从项目生成…</option>${projs.map(p => `<option value="${esc(p.id)}">${esc(p.name)}</option>`).join('')}</select>` : ''}
      <div class="nav-h" style="padding:10px 4px 2px">我的导图 · ${maps.length}</div>
      ${maps.map(x => `<button type="button" class="it ${m && x.id === m.id ? 'is-on' : ''}" data-act="mm-open" data-id="${esc(x.id)}"><b>${esc(x.title || '未命名导图')}</b><span>${Object.keys(x.nodes || {}).length} 个节点 · ${esc(ago(x.updatedAt))}</span></button>`).join('')}
    </div>`;
    if (!m) return `<div class="mm-layout">${listHtml}<div class="mm-main"><div class="empty-big clay"><h3>用导图把想法摊开</h3><p>从一个中心主题出发，Tab 加子节点、Enter 加同级，几分钟就能把一件事想清楚。想清楚的分支可以一键变成项目和任务。</p>
      <div style="display:flex;gap:10px;flex-wrap:wrap"><button class="btn btn-primary" type="button" data-act="mm-new">${icon('plus')}新建导图</button></div><p style="font-size:12.5px;color:var(--ink-3)">也可以在灵感详情里点「用导图展开」，或在左侧「从项目生成」。</p></div></div></div>`;
    const N = nodesOf(m); if (!N[m.rootId]) return `<div class="warn">这张导图的数据不完整。</div>`;
    if (!selId || !N[selId]) selId = m.rootId;
    const a = analyze(m);
    const sel = N[selId], isRoot = selId === m.rootId;
    const selColor = a.branch[selId] || 'lav';
    const tb = `<div class="toolbar">
      <input class="mm-title" id="mm-title" data-keep="mm-title" value="${esc(m.title || '')}" aria-label="导图标题" placeholder="导图标题">
      <span style="flex:1"></span>
      <div class="mm-tb">
        <button type="button" class="mini" data-act="mm-child" title="Tab">${icon('plus', 13)}子节点</button>
        <button type="button" class="mini" data-act="mm-sib" title="Enter" ${isRoot ? 'disabled style="opacity:.45"' : ''}>${icon('plus', 13)}同级</button>
        <button type="button" class="mini" data-act="mm-edit" title="空格 / 双击">${icon('edit', 13)}编辑</button>
        <button type="button" class="mini" data-act="mm-del" title="Delete" ${isRoot ? 'disabled style="opacity:.45"' : ''}>${icon('trash', 13)}删除</button>
        <button type="button" class="mini is-ai ai" data-act="ai-mm">${icon('spark', 13)}AI 扩展</button>
        <button type="button" class="mini" data-act="mm-task" ${isRoot ? 'disabled style="opacity:.45"' : ''}>${icon('task', 13)}转任务</button>
        <button type="button" class="mini" data-act="mm-branch">${icon('folder', 13)}${isRoot ? '整张转项目' : '分支转项目'}</button>
        <button type="button" class="mini" data-act="mm-idea">${icon('bulb', 13)}存为灵感</button>
        ${!isRoot && a.depth[selId] === 1 ? `<span style="display:inline-flex;gap:5px;margin-left:4px">${PALETTE.map(c => `<button type="button" class="swatch ${selColor === c ? 'is-on' : ''}" style="--c:${cv(c)};width:18px;height:18px" data-act="mm-color" data-v="${c}" aria-label="分支颜色 ${c}"></button>`).join('')}</span>` : ''}
        <button type="button" class="icon-btn sm" data-act="mm-undo" title="撤销 ⌘Z" aria-label="撤销">${icon('undo')}</button>
        <button type="button" class="icon-btn sm" data-act="mm-redo" title="重做 ⇧⌘Z" aria-label="重做">${icon('redo')}</button>
        <button type="button" class="icon-btn sm" data-act="mm-delmap" title="删除这张导图" aria-label="删除导图">${icon('trash')}</button>
      </div></div>`;
    const nodes = [];
    const visit = id => {
      const n = N[id]; if (!n) return; const d = a.depth[id] ?? 0;
      const b = a.branch[id] || 'lav', s = a.side[id] || 'r';
      let lk = '';
      if (n.link && n.link.type === 'task') { const t = S.data.tasks.get(n.link.id); lk = t ? `<span class="lk ${t.status === 'done' ? 'is-done' : ''}" data-open="tasks:${esc(t.id)}" title="打开任务">${t.status === 'done' ? '✓ 已完成' : '任务'}</span>` : ''; }
      else if (n.link && n.link.type === 'idea') { const i = S.data.ideas.get(n.link.id); lk = i ? `<span class="lk" data-open="ideas:${esc(i.id)}" title="打开灵感">灵感</span>` : ''; }
      else if (n.link && n.link.type === 'project') { const p = getP(n.link.id); lk = p ? `<span class="lk" data-act="open-project" role="button" data-id="${esc(p.id)}" title="打开项目">项目</span>` : ''; }
      const kids = n.children.filter(c => N[c]);
      const fold = d > 0 && kids.length ? `<span class="fold ${s}" data-fold="${esc(id)}" title="${n.collapsed ? '展开' : '折叠'}">${n.collapsed ? '+' + (subtreeIds(N, id).length - 1) : '−'}</span>` : '';
      nodes.push(`<div class="mnode lv${Math.min(d, 2)} ${id === selId ? 'is-sel' : ''}" data-mid="${esc(id)}" style="--b:${cv(b)};--b-soft:${cv(b, '-soft')};--b-ink:${cv(b, '-ink')};visibility:hidden">${n.text ? esc(n.text) : '<span style="opacity:.45">未命名</span>'}${lk}${fold}</div>`);
      if (!n.collapsed) kids.forEach(visit);
    };
    visit(m.rootId);
    const st = S.view.mm[m.id];
    return `<div class="vh"><div><h1>思维导图</h1><div class="sub">从一个主题发散，想清楚的分支一键转成项目和任务。</div></div></div>
      <div class="mm-layout">${listHtml}<div class="mm-main">${tb}
        <div class="canvas-wrap dotted" id="mm-wrap"><div class="canvas" id="mm-canvas"><svg id="mm-svg"></svg>${nodes.join('')}</div>
          <div class="canvas-tools"><button type="button" class="icon-btn clay-btn sm" data-act="mm-zoom-out" aria-label="缩小">${icon('minus')}</button><button type="button" class="icon-btn clay-btn sm" data-act="mm-zoom-in" aria-label="放大">${icon('plus')}</button><button type="button" class="icon-btn clay-btn sm" data-act="mm-fit" aria-label="适应窗口">${icon('fit')}</button></div>
          <div class="canvas-hint">Tab 子节点 · Enter 同级 · 空格或双击编辑 · Delete 删除 · 方向键移动 · ⌘Z 撤销 · 拖动节点到另一个节点上可换父级</div>
        </div></div></div>`;
  },
  after(root) {
    const m = curMap(); const wrap = root.querySelector('#mm-wrap'), canvas = root.querySelector('#mm-canvas');
    if (!m || !wrap) return;
    const N = nodesOf(m), a = analyze(m);
    canvas.style.width = '6000px'; canvas.style.height = '6000px';
    const els = new Map(); canvas.querySelectorAll('.mnode').forEach(el => els.set(el.dataset.mid, el));
    const size = new Map(); els.forEach((el, id) => size.set(id, { w: el.offsetWidth, h: el.offsetHeight }));
    const kidsOf = id => (N[id].collapsed ? [] : N[id].children.filter(c => N[c] && els.has(c)));
    const memo = new Map();
    const subH = id => { if (memo.has(id)) return memo.get(id); const k = kidsOf(id); const own = size.get(id).h; const v = k.length ? Math.max(own, k.reduce((s, c) => s + subH(c), 0) + VG * (k.length - 1)) : own; memo.set(id, v); return v; };
    const pos = new Map();
    const place = (id, cx, cy, dir, depth) => {
      pos.set(id, { cx, cy });
      const k = kidsOf(id); if (!k.length) return;
      const tot = k.reduce((s, c) => s + subH(c), 0) + VG * (k.length - 1); let y = cy - tot / 2;
      k.forEach(c => { const h = subH(c); place(c, cx + dir * (size.get(id).w / 2 + (depth === 0 ? HG0 : HG) + size.get(c).w / 2), y + h / 2, dir, depth + 1); y += h + VG; });
    };
    const rootId = m.rootId; pos.set(rootId, { cx: 0, cy: 0 });
    for (const dir of [1, -1]) {
      const k = kidsOf(rootId).filter(c => (a.side[c] || 'r') === (dir === 1 ? 'r' : 'l')); if (!k.length) continue;
      const tot = k.reduce((s, c) => s + subH(c), 0) + (VG + 8) * (k.length - 1); let y = -tot / 2;
      k.forEach(c => { const h = subH(c); place(c, dir * (size.get(rootId).w / 2 + HG0 + size.get(c).w / 2), y + h / 2, dir, 1); y += h + VG + 8; });
    }
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    pos.forEach((p, id) => { const s = size.get(id); x0 = Math.min(x0, p.cx - s.w / 2); x1 = Math.max(x1, p.cx + s.w / 2); y0 = Math.min(y0, p.cy - s.h / 2); y1 = Math.max(y1, p.cy + s.h / 2); });
    const pad = 80, ox = -x0 + pad, oy = -y0 + pad, W = x1 - x0 + pad * 2, H = y1 - y0 + pad * 2;
    canvas.style.width = W + 'px'; canvas.style.height = H + 'px';
    let svg = '';
    pos.forEach((p, id) => {
      const el = els.get(id), s = size.get(id);
      el.style.left = (p.cx - s.w / 2 + ox) + 'px'; el.style.top = (p.cy - s.h / 2 + oy) + 'px'; el.style.visibility = 'visible';
      const n = N[id]; if (!n.parent || !pos.has(n.parent)) return;
      const pp = pos.get(n.parent), ps = size.get(n.parent), dir = (a.side[id] || 'r') === 'r' ? 1 : -1;
      const sx = pp.cx + dir * ps.w / 2 + ox, sy = pp.cy + oy, ex = p.cx - dir * s.w / 2 + ox, ey = p.cy + oy, dx = Math.abs(ex - sx) * 0.55;
      const d = a.depth[id] || 1;
      svg += `<path class="me" d="M${sx},${sy} C${sx + dir * dx},${sy} ${ex - dir * dx},${ey} ${ex},${ey}" style="stroke:${cv(a.branch[id] || 'lav')};stroke-width:${d === 1 ? 4.5 : d === 2 ? 3 : 2.2};opacity:${d === 1 ? .85 : .7}"/>`;
    });
    const sv = canvas.querySelector('#mm-svg'); sv.setAttribute('width', W); sv.setAttribute('height', H); sv.innerHTML = svg;
    const rootEl = els.get(rootId), rp = { x: parseFloat(rootEl.style.left) + size.get(rootId).w / 2, y: parseFloat(rootEl.style.top) + size.get(rootId).h / 2 };
    let st = S.view.mm[m.id];
    const fresh = !st;
    if (!st) st = S.view.mm[m.id] = { x: 0, y: 0, k: 1 };
    const pz = SY.panZoom(wrap, canvas, st);
    SY.mmPZ = { pz, W, H };
    if (fresh) { const r = wrap.getBoundingClientRect(); st.x = r.width / 2 - rp.x; st.y = r.height / 2 - rp.y; pz.apply(); }
    // keep selected node in view
    const se = els.get(selId);
    if (se && !fresh) {
      const r = wrap.getBoundingClientRect(), nx = parseFloat(se.style.left) * st.k + st.x, ny = parseFloat(se.style.top) * st.k + st.y, nw = se.offsetWidth * st.k, nh = se.offsetHeight * st.k;
      let moved = false;
      if (nx < 20) { st.x += 20 - nx; moved = true; } else if (nx + nw > r.width - 20) { st.x -= nx + nw - r.width + 20; moved = true; }
      if (ny < 20) { st.y += 20 - ny; moved = true; } else if (ny + nh > r.height - 70) { st.y -= ny + nh - r.height + 70; moved = true; }
      if (moved) pz.apply();
    }
    canvas.querySelectorAll('.mnode').forEach(el => {
      el.addEventListener('click', e => {
        const f = e.target.closest('[data-fold]');
        if (f) { const id = f.dataset.fold; mutate(nodes => { nodes[id].collapsed = !nodes[id].collapsed; }); return; }
        if (e.target.closest('.lk')) return;
        if (selId !== el.dataset.mid) { selId = el.dataset.mid; SY.renderAll(); }
      });
      el.addEventListener('dblclick', e => { e.preventDefault(); selId = el.dataset.mid; startEdit(selId); });
      el.addEventListener('pointerdown', e => nodeDown(e, el, wrap, st));
    });
    if (pendingEdit && els.has(pendingEdit)) { const id = pendingEdit; pendingEdit = null; startEdit(id); }
  },
  key(e) {
    if (S.editing) return false;
    const m = curMap(); if (!m) return false;
    const N = nodesOf(m); const id = selId && N[selId] ? selId : m.rootId; const isRoot = id === m.rootId;
    if ((e.metaKey || e.ctrlKey) && (e.key === 'z' || e.key === 'Z')) { e.preventDefault(); e.shiftKey ? redo() : undo(); return true; }
    if (e.metaKey || e.ctrlKey || e.altKey) return false;
    const a = analyze(m);
    const sibs = () => { const p = N[N[id].parent]; if (!p) return [id]; return p.children.filter(c => N[c] && (N[id].parent !== m.rootId || (a.side[c] || 'r') === (a.side[id] || 'r'))); };
    const go = x => { if (x && N[x]) { selId = x; SY.renderAll(); } };
    switch (e.key) {
      case 'Tab': e.preventDefault(); addChild(id); return true;
      case 'Enter': e.preventDefault(); isRoot ? addChild(id) : addSibling(id); return true;
      case 'Delete': case 'Backspace': if (!isRoot) { e.preventDefault(); delNode(id); return true; } return false;
      case ' ': case 'F2': e.preventDefault(); startEdit(id); return true;
      case 'Escape': if (selId && selId !== m.rootId) { selId = m.rootId; SY.renderAll(); return true; } return false;
      case 'ArrowRight': case 'ArrowLeft': {
        e.preventDefault(); const right = e.key === 'ArrowRight';
        if (isRoot) { const k = N[id].children.filter(c => N[c] && (a.side[c] || 'r') === (right ? 'r' : 'l')); go(k[0]); return true; }
        const side = a.side[id] || 'r';
        if ((side === 'r') === right) { if (!N[id].collapsed) go(N[id].children.find(c => N[c])); } else go(N[id].parent);
        return true;
      }
      case 'ArrowUp': case 'ArrowDown': {
        e.preventDefault(); if (isRoot) return true; const s = sibs(), i = s.indexOf(id);
        go(s[i + (e.key === 'ArrowDown' ? 1 : -1)]); return true;
      }
    }
    return false;
  }
};

function addChild(pid) {
  let nid = null;
  mutate(nodes => { nid = addChildTo(nodes, pid, ''); });
  selId = nid; pendingEdit = nid; pendingNew = nid;
}
function addSibling(id) {
  const m = curMap(); const N = nodesOf(m); const p = N[id] && N[id].parent; if (!p) return addChild(id);
  let nid = null;
  mutate(nodes => { const n = makeNode('', p); nodes[n.id] = n; const arr = nodes[p].children; arr.splice(arr.indexOf(id) + 1, 0, n.id); nid = n.id; });
  selId = nid; pendingEdit = nid; pendingNew = nid;
}
function delNode(id) {
  const m = curMap(); const N = nodesOf(m); const n = N[id]; if (!n || !n.parent) return;
  const parent = n.parent, sib = N[parent].children, i = sib.indexOf(id);
  const count = subtreeIds(N, id).length;
  mutate(nodes => { subtreeIds(nodes, id).forEach(x => delete nodes[x]); nodes[parent].children = nodes[parent].children.filter(c => c !== id); });
  selId = sib[i + 1] && sib[i + 1] !== id ? sib[i + 1] : sib[i - 1] || parent;
  SY.toast(`已删除 ${count} 个节点`, { undo });
}
function setText(id, text, isNew) {
  const m = curMap(); if (!m) return; const N = nodesOf(m);
  if (!N[id]) return;
  if (isNew && !text.trim()) { const st = stackOf(m.id); const prev = st.u.pop(); if (prev) SY.patch('maps', m.id, JSON.parse(prev)); selId = N[id].parent || m.rootId; return; }
  if (N[id].text === text) { SY.scheduleRender(); return; }
  const wasRoot = id === m.rootId;
  mutate((nodes, map) => { nodes[id].text = text; if (wasRoot && (!map.title || map.title === N[id].text)) return { title: text.slice(0, 40) }; }, { record: !isNew });
}

function startEdit(id) {
  const wrap = document.querySelector('#mm-wrap'); const el = wrap && wrap.querySelector(`.mnode[data-mid="${CSS.escape(id)}"]`);
  const m = curMap(); if (!el || !m) return;
  const isNew = pendingNew === id; pendingNew = null;
  const st = S.view.mm[m.id] || { k: 1 }, k = st.k || 1;
  const wr = wrap.getBoundingClientRect(), r = el.getBoundingClientRect(), cs = getComputedStyle(el);
  // the editor takes the node's exact place and look, and grows away from the parent
  const N = nodesOf(m), root = wrap.querySelector(`.mnode[data-mid="${CSS.escape(m.rootId)}"]`);
  const rr = root ? root.getBoundingClientRect() : r;
  const leftSide = id !== m.rootId && r.left + r.width / 2 < rr.left + rr.width / 2;
  const isRoot = id === m.rootId;
  const px = v => parseFloat(v) || 0;
  const padX = px(cs.paddingLeft), padY = px(cs.paddingTop);
  const ta = document.createElement('textarea'); ta.className = 'mm-edit'; ta.rows = 1; ta.spellcheck = false;
  ta.value = N[id].text || '';
  ta.placeholder = '输入内容';
  Object.assign(ta.style, {
    top: (r.top - wr.top) + 'px', fontSize: (px(cs.fontSize) * k) + 'px', fontWeight: cs.fontWeight, lineHeight: cs.lineHeight === 'normal' ? '1.45' : (px(cs.lineHeight) * k) + 'px',
    padding: `${padY * k}px ${padX * k}px`, borderRadius: (px(cs.borderTopLeftRadius) * k) + 'px',
    background: cs.backgroundColor, color: cs.color, minHeight: r.height + 'px'
  });
  const meas = document.createElement('span');
  Object.assign(meas.style, { position: 'absolute', visibility: 'hidden', whiteSpace: 'pre', font: `${cs.fontWeight} ${px(cs.fontSize) * k}px ${cs.fontFamily}`, left: '-9999px', top: '0' });
  wrap.appendChild(meas);
  const maxW = (px(cs.maxWidth) || 260) * k + (isRoot ? 0 : 40 * k), minW = Math.max(r.width, 56 * k);
  const fit = () => {
    const longest = (ta.value || ta.placeholder).split('\n').reduce((a, l) => (l.length > a.length ? l : a), '');
    meas.textContent = longest || ' ';
    const w = Math.min(maxW, Math.max(minW, meas.getBoundingClientRect().width + padX * k * 2 + 6));
    ta.style.width = w + 'px';
    let left = leftSide ? r.right - wr.left - w : isRoot ? r.left - wr.left + r.width / 2 - w / 2 : r.left - wr.left;
    // keep the editor inside the canvas: pan the map instead of letting the text run off the edge
    const over = left + ox < 10 ? 10 - (left + ox) : left + ox + w > wr.width - 10 ? wr.width - 10 - (left + ox + w) : 0;
    if (over && SY.mmPZ && st.x != null) { ox += over; st.x += over; SY.mmPZ.pz.apply(); }
    ta.style.left = (left + ox) + 'px';
    ta.style.height = 'auto'; ta.style.height = Math.max(ta.scrollHeight, r.height) + 'px';
  };
  let ox = 0;
  el.style.visibility = 'hidden';
  wrap.appendChild(ta); S.editing = true;
  fit(); ta.focus(); ta.select();
  let done = false;
  const finish = (save, then) => {
    if (done) return; done = true; S.editing = false; const v = ta.value.replace(/\s+$/, ''); ta.remove(); meas.remove(); el.style.visibility = '';
    if (save) setText(id, v, isNew); else if (isNew) setText(id, '', true); else SY.scheduleRender();
    if (then) then();
  };
  ta.addEventListener('input', fit);
  ta.addEventListener('keydown', e => {
    if (e.isComposing || e.keyCode === 229) return;
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); e.stopPropagation(); finish(true); }
    else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); finish(false); }
    else if (e.key === 'Tab') { e.preventDefault(); e.stopPropagation(); finish(true, () => { if (curMap() && nodesOf(curMap())[id]) addChild(id); }); }
  });
  ta.addEventListener('blur', () => finish(true));
}

function nodeDown(e, el, wrap, st) {
  if (e.button !== 0 || e.target.closest('[data-fold],.lk')) return;
  const m = curMap(); const id = el.dataset.mid; if (!m || id === m.rootId) return;
  let over = null;
  SY.drag(e, {
    threshold: 6,
    onStart() { el.classList.add('is-dragging'); },
    onMove(ev, dx, dy) {
      el.style.transform = `translate(${dx / st.k}px,${dy / st.k}px)`;
      const t = document.elementFromPoint(ev.clientX, ev.clientY), n = t && t.closest('.mnode');
      if (over && over !== n) over.classList.remove('is-drop');
      const N = nodesOf(m);
      over = n && n !== el && !isDesc(N, n.dataset.mid, id) && N[id].parent !== n.dataset.mid ? n : null;
      if (over) over.classList.add('is-drop');
    },
    onEnd() {
      el.classList.remove('is-dragging'); el.style.transform = '';
      if (!over) return; over.classList.remove('is-drop');
      const to = over.dataset.mid;
      mutate(nodes => { const old = nodes[id].parent; nodes[old].children = nodes[old].children.filter(c => c !== id); nodes[id].parent = to; nodes[to].children.push(id); nodes[to].collapsed = false; });
      selId = id; SY.toast('已移动到新的父节点', { undo });
    }
  });
}

/* ---------- conversions ---------- */
function pickProject(title, onPick) {
  const ps = list('projects').filter(p => p.status !== 'done');
  SY.openModal(title, `<label class="f" style="display:flex;flex-direction:column;gap:6px;font-size:12px;color:var(--ink-3);font-weight:600">放到哪个项目<select class="in" id="pick-proj"><option value="">零散任务（无项目）</option>${ps.map(p => `<option value="${esc(p.id)}" ${curMap() && curMap().projectId === p.id ? 'selected' : ''}>${esc(AREA[p.area] || '')} · ${esc(p.name)}</option>`).join('')}</select></label>
    <label style="display:flex;gap:8px;align-items:center;font-size:13px"><input type="checkbox" id="pick-kids"> 同时把子节点也转成任务</label>`,
    `<button class="btn" type="button" data-act="close-modal">取消</button><button class="btn btn-primary" type="button" data-act="mm-pick-ok">转成任务</button>`, { iconName: 'task' });
  SY.mmPick = () => { const pid = document.querySelector('#pick-proj').value || null, kids = document.querySelector('#pick-kids').checked; SY.closeModal(); onPick(pid, kids); };
}
function toTasks(id, pid, withKids) {
  const m = curMap(); const N = nodesOf(m); const made = [];
  const ids = withKids ? subtreeIds(N, id) : [id];
  const links = {};
  ids.forEach((x, i) => { const n = N[x]; if (!n.text.trim() || (n.link && n.link.type === 'task' && S.data.tasks.has(n.link.id))) return; const t = SY.newTask({ title: n.text.trim().slice(0, 140), projectId: pid, createdAt: now() + i, note: `来自导图「${m.title}」` }); links[x] = t.id; made.push(t); });
  mutate(nodes => { Object.entries(links).forEach(([nid, tid]) => { nodes[nid].link = { type: 'task', id: tid }; }); });
  SY.toast(made.length ? `已生成 ${made.length} 个任务` : '这些节点已经转过了');
}
function branchToProject(id) {
  const m = curMap(); const N = nodesOf(m); const n = N[id];
  const p = SY.newProject({ name: n.text.trim().slice(0, 40) || m.title || '新项目', note: `来自导图「${m.title}」`, fromMap: m.id });
  const links = {}; let i = 0;
  n.children.filter(c => N[c] && N[c].text.trim()).forEach(c => {
    const sub = subtreeIds(N, c).slice(1).map(x => N[x].text.trim()).filter(Boolean);
    const t = SY.newTask({ title: N[c].text.trim().slice(0, 140), projectId: p.id, note: sub.length ? sub.map(s => '· ' + s).join('\n') : '', createdAt: now() + (i++) });
    links[c] = t.id;
  });
  mutate((nodes, map) => { nodes[id].link = { type: 'project', id: p.id }; Object.entries(links).forEach(([nid, tid]) => { nodes[nid].link = { type: 'task', id: tid }; }); if (!map.projectId) return { projectId: p.id }; });
  SY.toast(`已建项目「${p.name}」和 ${i} 个任务`);
}
function toIdea(id) {
  const m = curMap(); const N = nodesOf(m); const n = N[id];
  const path = []; let x = N[n.parent]; while (x) { path.unshift(x.text); x = N[x.parent]; }
  const kids = n.children.map(c => N[c] && N[c].text).filter(Boolean);
  const i = SY.put('ideas', { id: SY.newId('ideas'), text: n.text.trim() || m.title, note: [path.length ? '导图路径：' + path.join(' › ') : '', kids.length ? kids.map(k => '· ' + k).join('\n') : ''].filter(Boolean).join('\n'), tags: ['导图'], status: 'inbox', star: false, projectId: null, createdAt: now() });
  mutate(nodes => { nodes[id].link = { type: 'idea', id: i.id }; });
  SY.toast('已存到灵感收件箱');
}

Object.assign(SY.acts, {
  'mm-new': () => { newMap({ title: '新导图', rootText: '中心主题' }); pendingEdit = selId; pendingNew = null; S.ui.view = 'mindmap'; SY.renderAll(); },
  'mm-open': el => { S.ui.mapId = el.dataset.id; selId = null; SY.saveUI(); SY.renderAll(); },
  'mm-child': () => { const m = curMap(); if (m) addChild(selId || m.rootId); },
  'mm-sib': () => { const m = curMap(); if (m && selId && selId !== m.rootId) addSibling(selId); },
  'mm-edit': () => { const m = curMap(); if (m) startEdit(selId || m.rootId); },
  'mm-del': () => { const m = curMap(); if (m && selId && selId !== m.rootId) delNode(selId); },
  'mm-color': el => { const id = selId; mutate(nodes => { if (nodes[id]) nodes[id].color = el.dataset.v; }); },
  'mm-undo': () => undo(), 'mm-redo': () => redo(),
  'mm-zoom-in': () => SY.mmPZ && SY.mmPZ.pz.zoom(1.2),
  'mm-zoom-out': () => SY.mmPZ && SY.mmPZ.pz.zoom(1 / 1.2),
  'mm-fit': () => SY.mmPZ && SY.mmPZ.pz.fit(SY.mmPZ.W, SY.mmPZ.H, 30),
  'mm-delmap': () => { const m = curMap(); if (!m) return; SY.del('maps', m.id); S.ui.mapId = null; selId = null; },
  'mm-task': () => { const m = curMap(); if (!m || !selId || selId === m.rootId) return; const id = selId; pickProject('节点转为任务', (pid, kids) => toTasks(id, pid, kids)); },
  'mm-pick-ok': () => SY.mmPick && SY.mmPick(),
  'mm-branch': () => { const m = curMap(); if (m) branchToProject(selId || m.rootId); },
  'mm-idea': () => { const m = curMap(); if (m) toIdea(selId || m.rootId); },
  'idea-to-map': el => {
    const i = S.data.ideas.get(el.dataset.id); if (!i) return;
    const existing = list('maps').find(x => x.ideaId === i.id);
    if (existing) { S.ui.mapId = existing.id; selId = null; SY.setView('mindmap'); return; }
    const lines = (i.note || '').split('\n').map(s => s.replace(/^[\s·\-*•]+/, '').trim()).filter(Boolean).slice(0, 8);
    newMap({ title: i.text.split('\n')[0].slice(0, 40), rootText: i.text.split('\n')[0].slice(0, 60), ideaId: i.id, build: (nodes, root) => lines.forEach(l => addChildTo(nodes, root.id, l.slice(0, 60))) });
    if (i.status === 'inbox') SY.patch('ideas', i.id, { status: 'incubating' });
    S.sel = null; SY.setView('mindmap');
    SY.toast('已用导图展开这条灵感，按 Tab 加分支');
  }
});
SY.changeHandlers.push(e => {
  const el = e.target;
  if (el.id === 'mm-title') { const m = curMap(); const v = el.value.trim(); if (m && v && v !== m.title) SY.patch('maps', m.id, { title: v }); return; }
  if (el.id === 'mm-fromproj' && el.value) {
    const p = getP(el.value); if (!p) return;
    const ts = SY.scopeTasks(p.id);
    newMap({ title: p.name, rootText: p.name, projectId: p.id, build: (nodes, root) => {
      nodes[root.id].link = { type: 'project', id: p.id };
      [['doing', '进行中'], ['todo', '待办'], ['done', '已完成']].forEach(([st, l]) => {
        const g = ts.filter(t => t.status === st).sort(SY.taskSort); if (!g.length) return;
        const gid = addChildTo(nodes, root.id, l);
        g.forEach(t => { const nid = addChildTo(nodes, gid, t.title.slice(0, 80)); nodes[nid].link = { type: 'task', id: t.id }; });
        if (st === 'done') nodes[gid].collapsed = true;
      });
    } });
    SY.renderAll(); SY.toast('已从项目生成导图');
  }
});
SY.mmSel = () => selId;
SY.mmMutate = mutate;
SY.mmSetPending = id => { selId = id; };
})();
