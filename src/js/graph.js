/* 拾忆 knowledge graph: explicit links + edges derived from fields,
   [[wikilinks]] and shared tags (same rules as `shiyi graph` in the core). */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, list, clamp, now } = SY;
const REL = SY.REL;
const RELS = [['relates_to', '相关'], ['inspired_by', '受启发于'], ['derived_from', '来源于'], ['part_of', '属于'], ['supports', '支持'], ['contradicts', '矛盾'], ['depends_on', '依赖'], ['similar_to', '相似']];
const SYMMETRIC = ['relates_to', 'similar_to', 'contradicts'];
const REV = { part_of: '包含', derived_from: '衍生出', depends_on: '被依赖', converted_to: '来自', expands: '展开为', supports: '被支持', inspired_by: '启发了', mentions: '被提到', relates_to: '相关', similar_to: '相似', contradicts: '矛盾' };
const relLabel = (e, out) => (e.kind === 'mention' ? (out ? '提到' : '被提到') : out ? REL[e.relation] || e.relation : REV[e.relation] || REL[e.relation] || e.relation);
const TYPE_CN = { idea: '灵感', project: '项目', task: '任务', map: '导图', tag: '标签' };
const TYPE_C = { idea: 'peach', project: 'lav', task: 'sky', map: 'butter', tag: 'mint' };
const ENT = ['ideas', 'projects', 'tasks', 'maps'];
S.view.graph = S.view.graph || { pan: null, sel: null, edge: null, linkMode: false };
const G = S.view.graph;
const gpos = new Map();
S.ui.graph = Object.assign({ types: { idea: true, project: true, task: true, map: true, tag: true }, done: false, suggested: true, center: null, depth: 2 }, S.ui.graph || {});

/* ---------- derivation ---------- */
const clip80 = s => { s = String(s || '').trim(); return [...s].length > 80 ? [...s].slice(0, 79).join('') + '…' : s; };
function fullTitle(col, d) {
  if (col === 'ideas') return clip80(String(d.text || '').split('\n').map(l => l.trim()).find(Boolean) || '');
  if (col === 'projects') return clip80(d.name);
  return clip80(d.title);
}
function wikilinks(text) {
  const out = [], re = /\[\[([^\[\]\n]+?)\]\]/g; let m;
  while ((m = re.exec(String(text || '')))) { const t = m[1].split('|')[0].trim(); if (t && !out.includes(t)) out.push(t); }
  return out;
}
function index() {
  const byId = new Map(), byTitle = new Map();
  for (const col of ENT) for (const d of list(col)) {
    const e = { col, type: SY.TYPE[col], d };
    byId.set(d.id, e);
    const t = fullTitle(col, d).toLowerCase();
    if (t) { if (!byTitle.has(t)) byTitle.set(t, []); byTitle.get(t).push(e); }
  }
  return { byId, byTitle };
}
const RANK = { project: 0, idea: 1, task: 2, map: 3 };
function resolve(ix, target) {
  if (ix.byId.has(target)) return target;
  const hits = ix.byTitle.get(String(target).trim().toLowerCase());
  if (!hits) return null;
  return [...hits].sort((a, b) => RANK[a.type] - RANK[b.type])[0].d.id;
}
function texts(e) {
  const d = e.d;
  if (e.type === 'idea') return [d.text, d.note];
  if (e.type === 'task') return [d.title, d.note];
  if (e.type === 'project') return [d.goal, d.note];
  if (e.type === 'map') return Object.values(d.nodes || {}).map(n => n && n.text);
  return [];
}
let memo = null;
function build() {
  if (memo && memo.rev === S.rev) return memo;
  const ix = index(), nodes = new Map(), edges = [], pairs = new Set(), unresolved = [];
  for (const [id, e] of ix.byId) nodes.set(id, { id, type: e.type, col: e.col, d: e.d, label: SY.titleOf(e.col, e.d), deg: 0 });
  const alive = id => ix.byId.has(id);
  const field = (a, b, rel) => {
    if (!a || !b || a === b || !alive(b)) return;
    const k = a < b ? a + '|' + b : b + '|' + a;
    if (rel !== 'depends_on') { if (pairs.has(k)) return; pairs.add(k); }
    edges.push({ source: a, target: b, relation: rel, kind: 'field' });
  };
  for (const [id, e] of ix.byId) {
    const d = e.d;
    if (e.type === 'idea') { field(id, d.taskId, 'converted_to'); field(id, d.projectId, d.status === 'converted' ? 'converted_to' : 'part_of'); }
    else if (e.type === 'task') { field(id, d.projectId, 'part_of'); (d.deps || []).forEach(x => field(id, x, 'depends_on')); field(id, d.fromIdea, 'derived_from'); }
    else if (e.type === 'project') { field(id, d.fromIdea, 'derived_from'); field(id, d.fromMap, 'derived_from'); }
    else if (e.type === 'map') { field(id, d.ideaId, 'expands'); field(id, d.projectId, 'part_of'); Object.values(d.nodes || {}).forEach(n => { if (n && n.link && n.link.id) field(id, n.link.id, 'mentions'); }); }
  }
  const seen = new Set();
  for (const [id, e] of ix.byId) for (const t of texts(e)) for (const w of wikilinks(t)) {
    const target = resolve(ix, w);
    if (!target) { unresolved.push({ in: id, text: w }); continue; }
    if (target === id || seen.has(id + '>' + target)) continue;
    seen.add(id + '>' + target);
    edges.push({ source: id, target, relation: 'mentions', kind: 'mention' });
  }
  for (const l of list('links')) {
    if (!alive(l.source) || !alive(l.target)) continue;
    edges.push({ source: l.source, target: l.target, relation: l.relation, kind: 'link', link: l, status: l.status || 'confirmed' });
  }
  for (const [id, e] of ix.byId) if (e.type === 'idea') (e.d.tags || []).forEach(t => {
    const tid = 'tag:' + t;
    if (!nodes.has(tid)) nodes.set(tid, { id: tid, type: 'tag', label: '#' + t, deg: 0 });
    edges.push({ source: id, target: tid, relation: 'tagged', kind: 'tag' });
  });
  edges.forEach(e => { nodes.get(e.source).deg++; nodes.get(e.target).deg++; });
  memo = { rev: S.rev, ix, nodes, edges, unresolved };
  return memo;
}
SY.graph = { build, wikilinks, resolveTitle: t => resolve(build().ix, t) };

/* ---------- possibly related (local text similarity, no AI) ---------- */
S.ui.relNo = Array.isArray(S.ui.relNo) ? S.ui.relNo : [];
const pairKey = (a, b) => (a < b ? a + '|' + b : b + '|' + a);
let relMemo = { rev: -1, items: null };
function relItems() {
  if (relMemo.rev !== S.rev) relMemo = { rev: S.rev, items: ENT.flatMap(col => list(col).map(d => ({ id: d.id, type: SY.TYPE[col], doc: d }))) };
  return relMemo.items;
}
SY.related = (id, { limit = 3, text = null } = {}) => {
  const lib = window.ShiyiRelated; if (!lib) return [];
  let exclude;
  if (id) {
    exclude = new Set();
    build().edges.forEach(e => { if (e.source === id) exclude.add(e.target); if (e.target === id) exclude.add(e.source); });
    list('links').forEach(l => { if (l.source === id) exclude.add(l.target); if (l.target === id) exclude.add(l.source); });
    S.ui.relNo.forEach(k => { const [a, b] = k.split('|'); if (a === id) exclude.add(b); if (b === id) exclude.add(a); });
  }
  return lib.rank(relItems(), id ? { targetId: id, limit, exclude } : { text, limit });
};

/* ---------- opening things ---------- */
function openEntity(id) {
  if (String(id).startsWith('tag:')) { S.ui.view = 'ideas'; S.ui.ideaFilter = 'all'; S.ui.tag = id.slice(4); SY.saveUI(); SY.renderAll(); return; }
  for (const col of ENT) if (S.data[col].has(id)) {
    if (col === 'maps') { S.ui.mapId = id; SY.setView('mindmap'); return; }
    if (col === 'projects') { SY.goProject(id); return; }
    SY.select(col, id); return;
  }
  SY.toast('找不到这一条，可能已经删除了');
}
SY.openEntity = openEntity;
SY.richText = text => esc(text).replace(/\[\[([^\[\]\n]+?)\]\]/g, (m, inner) => {
  const [t, label] = inner.split('|');
  return `<span class="wl" role="button" tabindex="0" data-act="wl" data-wl="${t.trim()}">${(label || t).trim()}</span>`;
});

/* ---------- link helpers ---------- */
function findLink(a, b, rel) {
  return list('links').find(l => l.relation === rel && ((l.source === a && l.target === b) || (SYMMETRIC.includes(rel) && l.source === b && l.target === a)));
}
function createLink(source, target, relation, note, status = 'confirmed') {
  if (!source || !target || source === target) return null;
  const ex = findLink(source, target, relation);
  if (ex) { if (ex.status === 'suggested' && status === 'confirmed') SY.patch('links', ex.id, { status: 'confirmed' }); return ex; }
  return SY.put('links', { id: SY.newId('links'), source, target, relation, note: note || '', status, confidence: null, createdAt: now() });
}
SY.createLink = createLink;
const nameOf = id => SY.titleOfId(id);

function entityOptions(exclude) {
  const rows = [];
  for (const col of ENT) for (const d of list(col)) {
    if (d.id === exclude) continue;
    if (col === 'tasks' && d.status === 'done') continue;
    rows.push({ id: d.id, type: SY.TYPE[col], title: fullTitle(col, d), at: d.updatedAt || 0 });
  }
  return rows.sort((a, b) => b.at - a.at);
}
function openLinkModal(source, target) {
  const cands = entityOptions(source);
  const row = r => `<button type="button" class="pick-row" data-act="lm-pick" data-id="${esc(r.id)}"><span class="tchip t-${r.type}">${TYPE_CN[r.type]}</span><span>${esc(r.title)}</span></button>`;
  SY.openModal('关联到…', `
    <p class="te" style="margin:0 0 10px">从「<b>${esc(nameOf(source))}</b>」连到另一条灵感、项目、任务或导图。</p>
    <input class="in" id="lm-q" placeholder="搜索标题…" autocomplete="off" ${target ? 'hidden' : ''}>
    <div class="pick-list" id="lm-list" ${target ? 'hidden' : ''}>${cands.slice(0, 40).map(row).join('') || '<div class="empty">还没有别的条目可以关联。</div>'}</div>
    <div id="lm-target" class="lm-target" ${target ? '' : 'hidden'}>${target ? `→ <b>${esc(nameOf(target))}</b>` : ''}</div>
    <div class="ai-grid" style="margin-top:12px">
      <label>关系<select class="in" id="lm-rel">${RELS.map(([k, l]) => `<option value="${k}">${l}</option>`).join('')}</select></label>
      <label>说明（可选）<input class="in" id="lm-note" placeholder="为什么相关？"></label>
    </div>`,
    `<button class="btn" type="button" data-act="close-modal">取消</button><button class="btn btn-primary" type="button" data-act="lm-save" ${target ? '' : 'disabled'}>建立关联</button>`, { iconName: 'link' });
  const st = { source, target };
  SY.linkModal = st;
  const q = document.getElementById('lm-q');
  if (q) {
    q.focus();
    q.addEventListener('input', () => {
      const v = q.value.trim().toLowerCase();
      document.getElementById('lm-list').innerHTML = cands.filter(r => !v || r.title.toLowerCase().includes(v)).slice(0, 40).map(row).join('') || '<div class="empty">没有匹配的条目。</div>';
    });
  }
}
SY.openLinkModal = openLinkModal;

/* ---------- inspector section ---------- */
SY.relSection = (col, id) => {
  const g = build();
  const rows = [];
  for (const e of g.edges) {
    if (e.kind !== 'link' && e.kind !== 'mention') continue;
    const out = e.source === id, other = out ? e.target : e.source;
    if (!out && e.target !== id) continue;
    const label = relLabel(e, out);
    const sug = e.kind === 'link' && e.status === 'suggested';
    rows.push(`<div class="rel ${sug ? 'is-sug' : ''}" ${sug && e.link.note ? `title="${esc(e.link.note)}"` : ''}>
      <span class="rel-k">${esc(label)}</span>
      <button type="button" class="linkish rel-t" data-act="g-open" data-id="${esc(other)}">${esc(nameOf(other))}</button>
      ${sug ? `<button type="button" class="mini" data-act="link-ok" data-id="${esc(e.link.id)}" title="接受这条 AI 建议">${icon('check', 12)}</button>` : ''}
      ${e.kind === 'link' ? `<button type="button" class="mini" data-act="link-del" data-id="${esc(e.link.id)}" title="${sug ? '忽略' : '删除关联'}">${icon('x', 12)}</button>` : ''}
    </div>${sug && e.link.note ? `<div class="rel-note">AI：${esc(e.link.note)}</div>` : ''}`);
  }
  return `<div class="field col1"><span class="lbl" style="display:flex;gap:10px;align-items:center">关联与反向链接
      <button type="button" class="linkish" data-act="g-link-new" data-id="${esc(id)}">＋ 关联到…</button>
      <button type="button" class="linkish" data-act="g-focus" data-id="${esc(id)}">在图谱中看</button></span>
    <div class="rels">${rows.join('') || '<span class="te">还没有关联。在备注里写 [[标题]] 也能连上。</span>'}</div>${maybeRows(id)}</div>`;
};
function maybeRows(id) {
  const hits = SY.related(id, { limit: 3 });
  if (!hits.length) return '';
  return `<div class="maybe"><div class="maybe-h">可能相关 <span class="te">· 按文字相似度，不调用 AI</span></div>
    ${hits.map(h => `<div class="rel is-maybe" title="${esc(h.shared.concat(h.tags.map(t => '#' + t)).join('、'))}">
      <span class="tchip t-${h.type}">${TYPE_CN[h.type]}</span>
      <button type="button" class="linkish rel-t" data-act="g-open" data-id="${esc(h.id)}">${esc(nameOf(h.id))}</button>
      <span class="maybe-why">${esc(h.shared.concat(h.tags.map(t => '#' + t)).slice(0, 2).join('、'))}</span>
      <button type="button" class="mini" data-act="rel-yes" data-a="${esc(id)}" data-b="${esc(h.id)}" title="连上（相关）">${icon('plus', 12)}连上</button>
      <button type="button" class="mini" data-act="rel-no" data-a="${esc(id)}" data-b="${esc(h.id)}" title="不相关，不再推荐">${icon('x', 12)}</button>
    </div>`).join('')}</div>`;
}

/* ---------- graph view ---------- */
function visible() {
  const g = build(), o = S.ui.graph;
  const ok = n => {
    if (!o.types[n.type]) return false;
    if (n.type === 'task' && !o.done && n.d.status === 'done') return false;
    if (n.type === 'idea' && n.d.status === 'archived' && !o.done) return false;
    return true;
  };
  let ids = new Set([...g.nodes.values()].filter(ok).map(n => n.id));
  let edges = g.edges.filter(e => ids.has(e.source) && ids.has(e.target) && (o.suggested || e.status !== 'suggested'));
  let center = o.center && g.nodes.has(o.center) ? o.center : null;
  if (center) {
    ids.add(center);
    const adj = new Map();
    for (const e of g.edges) { if (!adj.has(e.source)) adj.set(e.source, []); if (!adj.has(e.target)) adj.set(e.target, []); adj.get(e.source).push(e.target); adj.get(e.target).push(e.source); }
    const dist = new Map([[center, 0]]), q = [center];
    while (q.length) {
      const x = q.shift(), dx = dist.get(x);
      if (dx >= o.depth || (x.startsWith('tag:') && x !== center)) continue;
      for (const y of adj.get(x) || []) if (!dist.has(y) && ids.has(y)) { dist.set(y, dx + 1); q.push(y); }
    }
    ids = new Set([...dist.keys()]);
    edges = g.edges.filter(e => ids.has(e.source) && ids.has(e.target) && (o.suggested || e.status !== 'suggested'));
  }
  const nodes = new Map([...ids].map(id => [id, g.nodes.get(id)]).filter(x => x[1]));
  const deg = new Map(); edges.forEach(e => { deg.set(e.source, (deg.get(e.source) || 0) + 1); deg.set(e.target, (deg.get(e.target) || 0) + 1); });
  return { g, nodes, edges, deg, center };
}

function layout(nodes, edges, fresh) {
  const arr = [...nodes.values()], n = arr.length; if (!n) return;
  const nb = new Map(); edges.forEach(e => { (nb.get(e.source) || nb.set(e.source, []).get(e.source)).push(e.target); (nb.get(e.target) || nb.set(e.target, []).get(e.target)).push(e.source); });
  let added = 0;
  arr.forEach((v, i) => {
    if (gpos.has(v.id)) return;
    added++;
    const p = (nb.get(v.id) || []).map(x => gpos.get(x)).find(Boolean), a = (i * 2.399963) % 6.2832, r = p ? 50 : 40 + 16 * Math.sqrt(i);
    gpos.set(v.id, { x: (p ? p.x : 0) + Math.cos(a) * r, y: (p ? p.y : 0) + Math.sin(a) * r * 0.8 });
  });
  if (!added && !fresh) return;
  const P = arr.map(v => gpos.get(v.id)), idx = new Map(arr.map((v, i) => [v.id, i]));
  const E = edges.map(e => [idx.get(e.source), idx.get(e.target), e.kind]).filter(x => x[0] != null && x[1] != null);
  const K = 72, iters = fresh ? (n > 500 ? 90 : 300) : 70;
  let temp = fresh ? 70 : 18;
  const fx = new Float64Array(n), fy = new Float64Array(n);
  for (let it = 0; it < iters; it++) {
    fx.fill(0); fy.fill(0);
    for (let i = 0; i < n; i++) for (let j = i + 1; j < n; j++) {
      let dx = P[i].x - P[j].x, dy = P[i].y - P[j].y, d2 = dx * dx + dy * dy;
      if (d2 > 90000) continue;
      if (d2 < 0.01) { dx = (i - j) * 0.1 + 0.05; dy = 0.05; d2 = dx * dx + dy * dy; }
      const f = (K * K) / d2;
      fx[i] += dx * f; fy[i] += dy * f; fx[j] -= dx * f; fy[j] -= dy * f;
    }
    for (const [a, b, kind] of E) {
      const dx = P[b].x - P[a].x, dy = P[b].y - P[a].y, d = Math.sqrt(dx * dx + dy * dy) || 0.01;
      const w = kind === 'tag' ? 0.35 : kind === 'field' ? 1.1 : 0.8;
      const f = (d * d / K) * w / d;
      fx[a] += dx * f; fy[a] += dy * f; fx[b] -= dx * f; fy[b] -= dy * f;
    }
    for (let i = 0; i < n; i++) {
      if (P[i].pin) continue;
      fx[i] -= P[i].x * 0.13; fy[i] -= P[i].y * 0.17;
      const m = Math.sqrt(fx[i] * fx[i] + fy[i] * fy[i]) || 1, s = Math.min(m, temp) / m;
      P[i].x += fx[i] * s; P[i].y += fy[i] * s;
    }
    temp = Math.max(1, temp * 0.975);
  }
}

const radius = (n, deg) => (n.type === 'tag' ? 5 : n.type === 'project' ? 11 + Math.min(8, Math.sqrt(deg) * 1.6) : 7 + Math.min(9, Math.sqrt(deg) * 1.7));
function short(s, n = 16) { s = String(s || ''); return [...s].length > n ? [...s].slice(0, n - 1).join('') + '…' : s; }

SY.views.graph = {
  canvas: true,
  render() {
    const o = S.ui.graph, v = visible(), g = v.g;
    const total = [...g.nodes.values()].filter(n => n.type !== 'tag').length;
    const sug = list('links').filter(l => l.status === 'suggested' && g.nodes.has(l.source) && g.nodes.has(l.target));
    const isolated = [...g.nodes.values()].filter(n => n.type === 'idea' && n.deg === 0).length;
    const since = S.ui.linkSweepAt || 0;
    const freshN = [...list('ideas').filter(i => i.status !== 'archived'), ...list('projects').filter(p => p.status !== 'done'), ...list('maps')].filter(d => (d.updatedAt || d.createdAt || 0) > since).length;
    layout(v.nodes, v.edges, !G.laidOut);
    G.laidOut = true;
    const ns = [...v.nodes.values()];
    let minX = 0, minY = 0, maxX = 0, maxY = 0;
    ns.forEach((n, i) => { const p = gpos.get(n.id); if (i === 0) { minX = maxX = p.x; minY = maxY = p.y; } minX = Math.min(minX, p.x); maxX = Math.max(maxX, p.x); minY = Math.min(minY, p.y); maxY = Math.max(maxY, p.y); });
    const pad = 70, W = maxX - minX + pad * 2, H = maxY - minY + pad * 2, ox = pad - minX, oy = pad - minY;
    G.off = { ox, oy, W, H };
    const sel = G.sel && v.nodes.has(G.sel) ? G.sel : null;
    const nbr = new Set(); if (sel) { nbr.add(sel); v.edges.forEach(e => { if (e.source === sel) nbr.add(e.target); if (e.target === sel) nbr.add(e.source); }); }
    const edgeSvg = v.edges.map((e, i) => {
      const a = gpos.get(e.source), b = gpos.get(e.target);
      const on = sel && (e.source === sel || e.target === sel), lid = e.link ? e.link.id : '';
      const cls = `ge k-${e.kind} ${e.status === 'suggested' ? 'is-sug' : ''} ${on ? 'is-on' : ''} ${G.edge && G.edge === lid ? 'is-sel' : ''}`;
      const line = `<line class="${cls}" x1="${(a.x + ox).toFixed(1)}" y1="${(a.y + oy).toFixed(1)}" x2="${(b.x + ox).toFixed(1)}" y2="${(b.y + oy).toFixed(1)}" data-s="${esc(e.source)}" data-t="${esc(e.target)}"/>`;
      const hit = lid ? `<line class="ge-hit" data-link="${esc(lid)}" x1="${(a.x + ox).toFixed(1)}" y1="${(a.y + oy).toFixed(1)}" x2="${(b.x + ox).toFixed(1)}" y2="${(b.y + oy).toFixed(1)}" data-s="${esc(e.source)}" data-t="${esc(e.target)}"><title>${esc(nameOf(e.source))} ${esc(REL[e.relation] || e.relation)} ${esc(nameOf(e.target))}${e.link.note ? '：' + esc(e.link.note) : ''}</title></line>` : '';
      return line + hit;
    }).join('');
    const q = (G.q || '').trim().toLowerCase();
    const nodeSvg = ns.map(n => {
      const p = gpos.get(n.id), d = v.deg.get(n.id) || 0, r = radius(n, d);
      const done = n.type === 'task' && n.d.status === 'done';
      const hit = q && n.label.toLowerCase().includes(q);
      const cls = `gnode t-${n.type} ${done ? 'is-done' : ''} ${sel && !nbr.has(n.id) ? 'is-dim' : ''} ${sel && nbr.has(n.id) ? 'is-nb' : ''} ${sel === n.id ? 'is-sel' : ''} ${hit ? 'is-hit' : ''} ${v.center === n.id ? 'is-center' : ''} ${d >= 3 || n.type === 'project' ? 'is-hub' : ''}`;
      return `<g class="${cls}" data-id="${esc(n.id)}" transform="translate(${(p.x + ox).toFixed(1)},${(p.y + oy).toFixed(1)})"><circle r="${r.toFixed(1)}" style="--c:var(--${TYPE_C[n.type]})"/><text y="${(r + 13).toFixed(1)}">${esc(short(n.label))}</text><title>${TYPE_CN[n.type]} · ${esc(n.label)}</title></g>`;
    }).join('');
    const typeChip = (k, l) => `<button type="button" class="chip ${o.types[k] ? 'is-on' : ''}" data-act="g-type" data-v="${k}"><i class="gdot t-${k}"></i>${l}</button>`;
    const centerChip = v.center ? `<span class="chip is-on gcenter">以「${esc(short(nameOf(v.center), 14))}」为中心<span class="seg seg-sm" style="margin-left:6px">${[1, 2, 3].map(dp => `<button type="button" class="${o.depth === dp ? 'is-on' : ''}" data-act="g-depth" data-v="${dp}">${dp} 层</button>`).join('')}</span><button type="button" class="mini" data-act="g-uncenter" aria-label="取消">${icon('x', 12)}</button></span>` : '';
    const card = graphCard(v);
    const empty = !total;
    return `<div class="vh"><div><h1>知识图谱</h1><div class="sub">灵感、项目、任务和导图之间的关系：手动关联、AI 建议、备注里的 [[双链]]、相同的标签，以及项目归属和依赖。</div></div>
        <div class="vh-acts"><button type="button" class="btn btn-ai ai" data-act="ai-links" title="手动触发，发送前会先告诉你范围和大约的 token">${icon('spark', 15)}AI 找关联${freshN ? `<span class="n" style="margin-left:4px;opacity:.7">${freshN} 条新内容</span>` : ''}</button></div></div>
      <div class="toolbar">${typeChip('idea', '灵感')}${typeChip('project', '项目')}${typeChip('task', '任务')}${typeChip('map', '导图')}${typeChip('tag', '标签')}
        <button type="button" class="chip ${o.done ? 'is-on' : ''}" data-act="g-done">含已完成 / 归档</button>
        <button type="button" class="chip ${o.suggested ? 'is-on' : ''}" data-act="g-sug">AI 建议${sug.length ? `<span class="n">${sug.length}</span>` : ''}</button>
        ${centerChip}<span class="sp"></span>
        <input class="ctl" id="g-q" data-keep="g-q" placeholder="找节点…" value="${esc(G.q || '')}" style="width:150px">
        <button type="button" class="chip ${G.linkMode ? 'is-on' : ''}" data-act="g-linkmode" title="打开后，从一个节点拖到另一个节点建立关联">${icon('link', 14)}连线</button></div>
      <div class="gstats">${v.nodes.size} 个节点 · ${v.edges.length} 条连线${isolated ? ` · <b>${isolated}</b> 条灵感还没有任何关联` : ''}${g.unresolved.length ? ` · <span title="${esc(g.unresolved.slice(0, 12).map(u => u.text).join('、'))}">${g.unresolved.length} 个 [[双链]] 还没有对应条目</span>` : ''}</div>
      ${empty ? `<div class="empty-big clay"><h3>图谱还是空的</h3><p>记下几条灵感、建个项目，它们之间的关系就会出现在这里。也可以让 AI 帮你找关联。</p></div>` : `
      <div class="canvas-wrap dotted ${G.linkMode ? 'is-linking' : ''}" id="g-wrap">
        <div class="canvas" id="g-canvas" style="width:${W.toFixed(0)}px;height:${H.toFixed(0)}px">
          <svg id="g-svg" width="${W.toFixed(0)}" height="${H.toFixed(0)}" class="${sel ? 'has-sel' : ''}"><g class="g-edges">${edgeSvg}</g><line id="g-tmp" class="ge-tmp" x1="0" y1="0" x2="0" y2="0"/><g class="g-nodes">${nodeSvg}</g></svg>
        </div>
        ${card}
        ${sug.length && o.suggested ? suggestPanel(sug) : ''}
        <div class="canvas-tools"><button type="button" class="icon-btn clay-btn sm" data-act="g-relayout" aria-label="重新排布" title="重新排布">${icon('reopen')}</button><button type="button" class="icon-btn clay-btn sm" data-act="gz-out" aria-label="缩小">${icon('minus')}</button><button type="button" class="icon-btn clay-btn sm" data-act="gz-in" aria-label="放大">${icon('plus')}</button><button type="button" class="icon-btn clay-btn sm" data-act="gz-fit" aria-label="适应窗口">${icon('fit')}</button></div>
        <div class="canvas-hint">${G.linkMode ? '<b>连线模式</b>：从一个节点拖到另一个节点' : '点节点看关系 · 双击打开 · 拖动调整位置 · ⌘ + 滚轮缩放'}</div>
      </div>
      <div class="legend"><span><i style="--c:var(--ink-3)"></i>归属 / 依赖 / 来源</span><span><i style="--c:var(--lav)"></i>手动关联</span><span><i class="dash" style="--c:var(--lav)"></i>AI 建议（待确认）</span><span><i class="dash" style="--c:var(--peach)"></i>[[双链]] 提到</span><span><i style="--c:var(--mint);opacity:.5"></i>标签</span></div>`}`;
  },
  after(root) {
    const wrap = root.querySelector('#g-wrap'), canvas = root.querySelector('#g-canvas'); if (!wrap) return;
    const fresh = !G.pan;
    if (!G.pan) G.pan = { x: 0, y: 0, k: 1 };
    const pz = SY.panZoom(wrap, canvas, G.pan, { onChange: st => wrap.classList.toggle('g-low', st.k < 0.85) });
    G.pz = pz;
    if (fresh || G.refit) { G.refit = false; requestAnimationFrame(() => pz.fit(G.off.W, G.off.H, 20)); }
    const svg = root.querySelector('#g-svg');
    svg.querySelectorAll('.gnode').forEach(el => el.addEventListener('pointerdown', e => nodeDown(e, el, wrap)));
    svg.querySelectorAll('.gnode').forEach(el => el.addEventListener('dblclick', e => { e.stopPropagation(); openEntity(el.dataset.id); }));
    svg.querySelectorAll('.ge-hit').forEach(el => el.addEventListener('click', e => { e.stopPropagation(); G.edge = el.dataset.link; G.sel = null; SY.renderAll(); }));
    wrap.addEventListener('click', e => {
      if (performance.now() < SY.suppressClickUntil) return;
      if (!e.target.closest('.gnode,.ge-hit,.gcard,.gsug,.canvas-tools') && (G.sel || G.edge)) { G.sel = null; G.edge = null; SY.renderAll(); }
    });
    const q = root.querySelector('#g-q');
    if (q) {
      q.addEventListener('input', () => { G.q = q.value; svg.querySelectorAll('.gnode').forEach(el => { const n = build().nodes.get(el.dataset.id); el.classList.toggle('is-hit', !!(G.q.trim() && n && n.label.toLowerCase().includes(G.q.trim().toLowerCase()))); }); });
      q.addEventListener('keydown', e => {
        if (e.key !== 'Enter' || e.isComposing) return;
        const hit = svg.querySelector('.gnode.is-hit'); if (!hit) return;
        G.sel = hit.dataset.id; centerOn(hit.dataset.id); SY.renderAll();
      });
    }
  },
  key(e) {
    if (e.key === 'Escape' && (G.sel || G.edge || G.linkMode)) { G.sel = null; G.edge = null; G.linkMode = false; SY.renderAll(); return true; }
    if ((e.key === 'Delete' || e.key === 'Backspace') && G.edge) { const id = G.edge; G.edge = null; delLink(id); return true; }
    return false;
  }
};

function centerOn(id) {
  const p = gpos.get(id), wrap = document.getElementById('g-wrap'); if (!p || !wrap || !G.pan) return;
  const r = wrap.getBoundingClientRect();
  G.pan.x = r.width / 2 - (p.x + G.off.ox) * G.pan.k; G.pan.y = r.height / 2 - (p.y + G.off.oy) * G.pan.k;
}

function graphCard(v) {
  if (G.edge) {
    const l = S.data.links.get(G.edge); if (!l) return '';
    return `<div class="gcard clay">
      <div class="gc-h"><span class="eyebrow">${l.status === 'suggested' ? 'AI 建议的关联' : '关联'}</span><button type="button" class="icon-btn sm" data-act="g-close" aria-label="关闭">${icon('x')}</button></div>
      <div class="gc-line"><button type="button" class="linkish" data-act="g-open" data-id="${esc(l.source)}">${esc(nameOf(l.source))}</button>
        <select class="in sm" id="g-rel" data-link="${esc(l.id)}">${RELS.map(([k, t]) => `<option value="${k}" ${l.relation === k ? 'selected' : ''}>${t}</option>`).join('')}</select>
        <button type="button" class="linkish" data-act="g-open" data-id="${esc(l.target)}">${esc(nameOf(l.target))}</button></div>
      ${l.note ? `<p class="gc-note">${esc(l.note)}</p>` : ''}
      ${l.createdBy && l.createdBy !== 'user' ? `<div class="te">由 ${esc(l.createdBy.replace(/^agent:/, ''))} 提出</div>` : ''}
      <div class="gc-acts">${l.status === 'suggested' ? `<button type="button" class="btn btn-sm btn-primary" data-act="link-ok" data-id="${esc(l.id)}">${icon('check', 14)}接受</button>` : ''}
        <button type="button" class="btn btn-sm btn-danger" data-act="link-del" data-id="${esc(l.id)}">${icon('trash', 14)}${l.status === 'suggested' ? '忽略' : '删除关联'}</button></div></div>`;
  }
  const id = G.sel; if (!id || !v.nodes.has(id)) return '';
  const n = v.nodes.get(id);
  const rels = v.edges.filter(e => e.source === id || e.target === id);
  const byKind = k => rels.filter(e => e.kind === k).length;
  const other = e => (e.source === id ? e.target : e.source);
  const lines = rels.filter(e => e.kind !== 'tag').slice(0, 8).map(e => `<div class="rel ${e.status === 'suggested' ? 'is-sug' : ''}"><span class="rel-k">${esc(relLabel(e, e.source === id))}</span><button type="button" class="linkish rel-t" data-act="g-select" data-id="${esc(other(e))}">${esc(nameOf(other(e)))}</button></div>`).join('');
  return `<div class="gcard clay">
    <div class="gc-h"><span class="eyebrow"><i class="gdot t-${n.type}"></i>${TYPE_CN[n.type]}</span><button type="button" class="icon-btn sm" data-act="g-close" aria-label="关闭">${icon('x')}</button></div>
    <div class="gc-title">${esc(n.label)}</div>
    <div class="te">${rels.length} 条连线${byKind('link') ? ` · 关联 ${byKind('link')}` : ''}${byKind('mention') ? ` · 双链 ${byKind('mention')}` : ''}${byKind('tag') ? ` · 标签 ${byKind('tag')}` : ''}</div>
    ${lines ? `<div class="rels">${lines}</div>` : ''}
    <div class="gc-acts">${n.type !== 'tag' ? `<button type="button" class="btn btn-sm" data-act="g-open" data-id="${esc(id)}">打开</button><button type="button" class="btn btn-sm" data-act="g-link-new" data-id="${esc(id)}">${icon('link', 14)}关联到…</button>` : `<button type="button" class="btn btn-sm" data-act="g-open" data-id="${esc(id)}">看这个标签的灵感</button>`}
      <button type="button" class="btn btn-sm" data-act="g-center" data-id="${esc(id)}">以它为中心</button></div></div>`;
}
function suggestPanel(sug) {
  return `<div class="gsug clay"><div class="gc-h"><span class="eyebrow">${icon('spark', 13)} 待确认的关联 · ${sug.length}</span>${sug.length > 1 ? `<button type="button" class="linkish" data-act="link-ok-all">全部接受</button>` : ''}</div>
    <div class="gsug-list">${sug.slice(0, 12).map(l => `<div class="gsug-row" title="${esc(l.note || '')}">
      <button type="button" class="linkish" data-act="g-edge" data-id="${esc(l.id)}">${esc(short(nameOf(l.source), 12))} <span class="te">${esc(REL[l.relation] || l.relation)}</span> ${esc(short(nameOf(l.target), 12))}</button>
      <button type="button" class="mini" data-act="link-ok" data-id="${esc(l.id)}" title="接受">${icon('check', 12)}</button><button type="button" class="mini" data-act="link-del" data-id="${esc(l.id)}" title="忽略">${icon('x', 12)}</button></div>`).join('')}</div></div>`;
}

function toGraph(ev, wrap) {
  const r = wrap.getBoundingClientRect(), st = G.pan;
  return { x: (ev.clientX - r.left - st.x) / st.k - G.off.ox, y: (ev.clientY - r.top - st.y) / st.k - G.off.oy };
}
function nodeDown(e, el, wrap) {
  if (e.button !== 0) return;
  e.stopPropagation();
  const id = el.dataset.id, p = gpos.get(id); if (!p) return;
  const svg = wrap.querySelector('#g-svg');
  if (G.linkMode) {
    const tmp = svg.querySelector('#g-tmp'); let over = null;
    SY.drag(e, {
      threshold: 2,
      onMove(ev) {
        const q = toGraph(ev, wrap);
        tmp.setAttribute('x1', p.x + G.off.ox); tmp.setAttribute('y1', p.y + G.off.oy); tmp.setAttribute('x2', q.x + G.off.ox); tmp.setAttribute('y2', q.y + G.off.oy);
        const t = document.elementFromPoint(ev.clientX, ev.clientY), n = t && t.closest && t.closest('.gnode');
        if (over && over !== n) over.classList.remove('is-target');
        over = n && n !== el && !n.dataset.id.startsWith('tag:') ? n : null; if (over) over.classList.add('is-target');
      },
      onEnd() {
        tmp.setAttribute('x2', tmp.getAttribute('x1')); tmp.setAttribute('y2', tmp.getAttribute('y1'));
        if (over && !id.startsWith('tag:')) { const to = over.dataset.id; over.classList.remove('is-target'); openLinkModal(id, to); }
      }
    });
    return;
  }
  const x0 = p.x, y0 = p.y; let moved = false;
  SY.drag(e, {
    threshold: 3,
    onMove(ev, dx, dy) {
      moved = true;
      p.x = x0 + dx / G.pan.k; p.y = y0 + dy / G.pan.k; p.pin = true;
      el.setAttribute('transform', `translate(${(p.x + G.off.ox).toFixed(1)},${(p.y + G.off.oy).toFixed(1)})`);
      svg.querySelectorAll(`line[data-s="${CSS.escape(id)}"]`).forEach(l => { l.setAttribute('x1', p.x + G.off.ox); l.setAttribute('y1', p.y + G.off.oy); });
      svg.querySelectorAll(`line[data-t="${CSS.escape(id)}"]`).forEach(l => { l.setAttribute('x2', p.x + G.off.ox); l.setAttribute('y2', p.y + G.off.oy); });
    },
    onEnd() {}
  });
  // a click without movement selects
  const up = () => { window.removeEventListener('pointerup', up); if (!moved) { G.sel = G.sel === id ? null : id; G.edge = null; SY.renderAll(); } };
  window.addEventListener('pointerup', up);
}

function delLink(id) {
  const l = S.data.links.get(id); if (!l) return;
  SY.remove('links', id);
  if (G.edge === id) G.edge = null;
  SY.toast(l.status === 'suggested' ? '已忽略这条建议' : '已删除关联', { undo: () => SY.put('links', l) });
}

Object.assign(SY.acts, {
  'wl': el => {
    const t = el.dataset.wl, id = SY.graph.resolveTitle(t);
    if (id) openEntity(id);
    else SY.toast(`还没有叫「${t}」的条目`, { action: { label: '记成灵感', fn: () => { const i = SY.put('ideas', { id: SY.newId('ideas'), text: t, note: '', tags: [], status: 'inbox', star: false, projectId: null, createdAt: now() }); SY.select('ideas', i.id); } } });
  },
  'g-type': el => { const k = el.dataset.v; S.ui.graph.types[k] = !S.ui.graph.types[k]; SY.saveUI(); SY.renderAll(); },
  'g-done': () => { S.ui.graph.done = !S.ui.graph.done; SY.saveUI(); SY.renderAll(); },
  'g-sug': () => { S.ui.graph.suggested = !S.ui.graph.suggested; SY.saveUI(); SY.renderAll(); },
  'g-depth': el => { S.ui.graph.depth = Number(el.dataset.v); SY.saveUI(); G.refit = true; SY.renderAll(); },
  'g-uncenter': () => { S.ui.graph.center = null; SY.saveUI(); G.refit = true; SY.renderAll(); },
  'g-center': el => { S.ui.graph.center = el.dataset.id; G.sel = el.dataset.id; SY.saveUI(); G.refit = true; SY.renderAll(); },
  'g-focus': el => { S.ui.graph.center = el.dataset.id; G.sel = el.dataset.id; G.refit = true; SY.saveUI(); SY.closeInsp(); SY.setView('graph'); },
  'g-select': el => { G.sel = el.dataset.id; G.edge = null; centerOn(el.dataset.id); SY.renderAll(); },
  'g-open': el => openEntity(el.dataset.id),
  'g-edge': el => { G.edge = el.dataset.id; G.sel = null; SY.renderAll(); },
  'g-close': () => { G.sel = null; G.edge = null; SY.renderAll(); },
  'g-linkmode': () => { G.linkMode = !G.linkMode; SY.renderAll(); },
  'g-relayout': () => { gpos.clear(); G.laidOut = false; G.refit = true; SY.renderAll(); },
  'gz-in': () => G.pz && G.pz.zoom(1.2),
  'gz-out': () => G.pz && G.pz.zoom(1 / 1.2),
  'gz-fit': () => G.pz && G.off && G.pz.fit(G.off.W, G.off.H, 20),
  'g-link-new': el => openLinkModal(el.dataset.id),
  'lm-pick': el => {
    const st = SY.linkModal; if (!st) return;
    st.target = el.dataset.id;
    document.querySelectorAll('#lm-list .pick-row').forEach(b => b.classList.toggle('is-on', b === el));
    const t = document.getElementById('lm-target'); t.hidden = false; t.innerHTML = `→ <b>${esc(nameOf(st.target))}</b>`;
    document.querySelector('[data-act="lm-save"]').disabled = false;
  },
  'lm-save': () => {
    const st = SY.linkModal; if (!st || !st.target) return;
    const rel = document.getElementById('lm-rel').value, note = document.getElementById('lm-note').value.trim();
    const ex = findLink(st.source, st.target, rel);
    const l = createLink(st.source, st.target, rel, note, 'confirmed');
    SY.closeModal(); SY.linkModal = null; G.linkMode = false;
    SY.toast(ex ? '这两条已经关联过了' : `已关联：${short(nameOf(st.source), 12)} ${REL[rel]} ${short(nameOf(st.target), 12)}`, ex ? {} : { undo: () => l && SY.remove('links', l.id) });
  },
  'link-ok': el => { const l = S.data.links.get(el.dataset.id); if (l) { SY.patch('links', l.id, { status: 'confirmed' }); SY.toast('已接受这条关联'); } },
  'link-ok-all': () => { const sug = list('links').filter(l => l.status === 'suggested'); SY.withNote('接受全部 AI 建议', () => sug.forEach(l => SY.patch('links', l.id, { status: 'confirmed' }))); SY.toast(`已接受 ${sug.length} 条关联`); },
  'link-del': el => delLink(el.dataset.id),
  'rel-yes': el => { const l = createLink(el.dataset.a, el.dataset.b, 'relates_to', '', 'confirmed'); SY.toast(`已关联「${short(nameOf(el.dataset.b), 14)}」`, { undo: () => l && SY.remove('links', l.id) }); },
  'rel-no': el => { S.ui.relNo = [...S.ui.relNo, pairKey(el.dataset.a, el.dataset.b)].slice(-500); SY.saveUI(); S.rev++; SY.renderAll(); }
});
SY.changeHandlers.push(e => {
  const el = e.target;
  if (el.id === 'g-rel' && el.dataset.link) SY.patch('links', el.dataset.link, { relation: el.value });
});

/* ---------- [[ autocomplete in notes and the capture box ---------- */
let pop = null, popState = null;
function closePop() { if (pop) pop.hidden = true; popState = null; }
function showPop(el, query, start) {
  const v = query.toLowerCase();
  const rows = entityOptions(null).filter(r => !v || r.title.toLowerCase().includes(v)).slice(0, 8);
  if (!rows.length) { closePop(); return; }
  if (!pop) { pop = document.createElement('div'); pop.className = 'wl-pop clay'; pop.setAttribute('role', 'listbox'); document.body.appendChild(pop); }
  popState = { el, start, rows, i: 0 };
  const r = el.getBoundingClientRect();
  pop.style.left = Math.min(r.left, window.innerWidth - 300) + 'px';
  pop.style.top = Math.min(r.bottom + 4, window.innerHeight - 260) + 'px';
  pop.hidden = false;
  renderPop();
}
function renderPop() {
  pop.innerHTML = `<div class="wl-h">插入双链 [[…]]</div>` + popState.rows.map((r, i) => `<div class="wl-row ${i === popState.i ? 'is-on' : ''}" data-i="${i}" role="option"><span class="tchip t-${r.type}">${TYPE_CN[r.type]}</span>${esc(short(r.title, 30))}</div>`).join('');
  pop.querySelectorAll('.wl-row').forEach(row => row.addEventListener('mousedown', e => { e.preventDefault(); popState.i = Number(row.dataset.i); insertPick(); }));
}
function insertPick() {
  const { el, start, rows, i } = popState, r = rows[i];
  const caret = el.selectionStart, text = el.value;
  const ins = '[[' + r.title + ']]';
  el.value = text.slice(0, start) + ins + text.slice(caret);
  const pos = start + ins.length; el.setSelectionRange(pos, pos);
  closePop();
  el.dispatchEvent(new Event('input', { bubbles: true }));
}
document.addEventListener('input', e => {
  const el = e.target;
  if (!(el.matches && el.matches('#insp textarea, #cap-text'))) { closePop(); return; }
  const before = el.value.slice(0, el.selectionStart), m = before.match(/\[\[([^\[\]\n]{0,40})$/);
  if (m) showPop(el, m[1], before.length - m[0].length); else closePop();
});
document.addEventListener('keydown', e => {
  if (!popState || e.target !== popState.el) return;
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); e.stopPropagation(); popState.i = (popState.i + (e.key === 'ArrowDown' ? 1 : -1) + popState.rows.length) % popState.rows.length; renderPop(); }
  else if ((e.key === 'Enter' || e.key === 'Tab') && !e.isComposing) { e.preventDefault(); e.stopPropagation(); insertPick(); }
  else if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); closePop(); }
}, true);
document.addEventListener('focusout', e => { if (popState && e.target === popState.el) setTimeout(closePop, 120); });
})();
