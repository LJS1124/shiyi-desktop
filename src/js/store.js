/* 拾忆 store: the one write path of the GUI.
   App: every change becomes an operation on the shiyi-core SQLite database,
   the same one AI agents use through the `shiyi` CLI. The database keeps an
   operation log with before/after snapshots, so anything can be undone.
   Web preview: data and a local operation log live in this browser. */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, now, lsGet, lsSet } = SY;
const invoke = SY.invoke;

const TYPE = { ideas: 'idea', projects: 'project', tasks: 'task', maps: 'map', links: 'link' };
const COL_OF = { idea: 'ideas', project: 'projects', task: 'tasks', map: 'maps', link: 'links' };
const PREFIX = { ideas: 'idea', projects: 'proj', tasks: 'task', maps: 'map', links: 'link' };
const CN = { ideas: '灵感', projects: '项目', tasks: '任务', maps: '导图', links: '关联' };
const REL = { relates_to: '相关', inspired_by: '受启发于', derived_from: '来源于', part_of: '属于', supports: '支持', contradicts: '矛盾', depends_on: '依赖', similar_to: '相似', mentions: '提到', converted_to: '转化为', expands: '展开', tagged: '标签' };
const LS_DATA = 'shiyi.desk.data.v1', LS_OPS = 'shiyi.desk.ops.v1';
Object.assign(SY, { TYPE, COL_OF, REL });

SY.newId = col => `${PREFIX[col] || 'x'}_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
SY.errText = e => (e && typeof e === 'object' ? (e.message || e.code || JSON.stringify(e)) + (e.hint ? '（' + e.hint + '）' : '') : String(e));

/* ---------- GUI shape <-> database shape ---------- */
const MAP = {
  ideas: { text: 'text', note: 'note', tags: 'tags', status: 'status', star: 'starred', projectId: 'project_id', taskId: 'task_id', sample: 'sample' },
  projects: { name: 'name', area: 'area', color: 'color', status: 'status', goal: 'goal', note: 'note', start: 'start_date', due: 'due_date', fromIdea: 'from_idea', fromMap: 'from_map', sample: 'sample' },
  tasks: { title: 'title', projectId: 'project_id', status: 'status', priority: 'priority', start: 'start_date', due: 'due_date', todayDate: 'today_date', note: 'note', deps: 'depends_on', rank: 'rank', doneAt: 'done_at', fromIdea: 'from_idea', sample: 'sample' },
  maps: { title: 'title', rootId: 'root_id', nodes: 'nodes', ideaId: 'idea_id', projectId: 'project_id', sample: 'sample' },
  links: { source: 'source', target: 'target', relation: 'relation', note: 'note', status: 'status', confidence: 'confidence', sample: 'sample' }
};
const SYS = { created_at: 'createdAt', updated_at: 'updatedAt', version: 'version', created_by: 'createdBy', updated_by: 'updatedBy' };
const est3 = d => (d.estO == null && d.estM == null && d.estP == null ? null : { optimistic: d.estO ?? null, likely: d.estM ?? null, pessimistic: d.estP ?? null });

function toCore(col, doc, keys) {
  const m = MAP[col], out = {};
  for (const k of keys || Object.keys(doc)) {
    if (col === 'tasks' && (k === 'estO' || k === 'estM' || k === 'estP')) { out.estimate = est3(doc); continue; }
    if (k === 'createdAt' && !keys) { if (doc.createdAt) out.created_at = doc.createdAt; continue; }
    const ck = m[k]; if (!ck) continue;
    out[ck] = doc[k] === undefined ? null : doc[k];
  }
  return out;
}
function fromCore(doc) {
  const col = COL_OF[doc.type]; if (!col) return [null, null];
  const out = { id: doc.id };
  for (const [fk, ck] of Object.entries(MAP[col])) if (ck in doc) out[fk] = doc[ck];
  for (const [ck, fk] of Object.entries(SYS)) if (ck in doc) out[fk] = doc[ck];
  if (col === 'tasks') { const e = doc.estimate || {}; out.estO = e.optimistic ?? null; out.estM = e.likely ?? null; out.estP = e.pessimistic ?? null; out.deps = out.deps || []; }
  for (const k of ['createdAt', 'updatedAt', 'doneAt']) if (typeof out[k] === 'string') { const t = Date.parse(out[k]); out[k] = isNaN(t) ? null : t; }
  return [col, out];
}
const isoOf = ms => (ms ? new Date(ms).toISOString() : null);
function toSnapshotDoc(col, d) {
  return { id: d.id, type: TYPE[col], ...toCore(col, d), created_at: isoOf(d.createdAt), updated_at: isoOf(d.updatedAt), version: d.version || 1, created_by: d.createdBy || 'user' };
}

/* ---------- titles & summaries (preview log, toasts) ---------- */
const clip = (s, n) => { s = String(s || '').trim(); return s.length > n ? s.slice(0, n - 1) + '…' : s; };
function titleOf(col, d) {
  if (!d) return '';
  if (col === 'ideas') return clip(String(d.text || '').split('\n').find(l => l.trim()) || '', 40);
  if (col === 'projects') return clip(d.name, 40);
  if (col === 'links') return REL[d.relation] || d.relation || '';
  return clip(d.title, 40);
}
function titleOfId(id) {
  for (const c of ['ideas', 'projects', 'tasks', 'maps']) { const d = S.data[c].get(id); if (d) return titleOf(c, d); }
  return id;
}
SY.titleOf = titleOf; SY.titleOfId = titleOfId;
const FIELD_CN = { status: '状态', due: '截止', start: '开始', todayDate: '今日计划', priority: '优先级', title: '标题', text: '内容', name: '名称', note: '备注', tags: '标签', deps: '前置任务', projectId: '项目', estO: '估时', estM: '估时', estP: '估时', star: '星标', goal: '目标', area: '领域', color: '颜色', nodes: '节点', rank: '排序', relation: '关系' };
const ST_CN = { ...SY.TASK_ST, ...SY.IDEA_ST, active: '进行中', paused: '暂停', confirmed: '已确认', suggested: '待确认' };
/* what changed inside a mind map (same wording as the core's describe_node_changes) */
function nodeChanges(b, a) {
  b = (b && typeof b === 'object') ? b : {}; a = (a && typeof a === 'object') ? a : {};
  const t = n => { const x = String((n && n.text) || '').trim(); return x ? clip(x, 16) : '未命名'; };
  const ids = o => Object.keys(o).sort();
  const added = ids(a).filter(k => !(k in b)), removed = ids(b).filter(k => !(k in a)), both = ids(a).filter(k => k in b);
  const renamed = both.filter(k => (b[k].text || '') !== (a[k].text || '')), moved = both.filter(k => (b[k].parent ?? null) !== (a[k].parent ?? null));
  const parts = [];
  if (added.length) parts.push(added.length === 1 ? `添加节点「${t(a[added[0]])}」` : `添加 ${added.length} 个节点`);
  if (removed.length) parts.push(removed.length === 1 ? `删除节点「${t(b[removed[0]])}」` : `删除 ${removed.length} 个节点`);
  if (renamed.length) parts.push(renamed.length === 1 ? `「${t(b[renamed[0]])}」改为「${t(a[renamed[0]])}」` : `改了 ${renamed.length} 个节点的文字`);
  if (moved.length) parts.push(moved.length === 1 ? `移动节点「${t(a[moved[0]])}」` : `移动 ${moved.length} 个节点`);
  if (parts.length) return parts.join('、');
  const diff = both.filter(k => JSON.stringify(b[k]) !== JSON.stringify(a[k]));
  if (diff.some(k => JSON.stringify({ ...b[k], collapsed: null }) === JSON.stringify({ ...a[k], collapsed: null }))) return '折叠或展开节点';
  if (diff.every(k => JSON.stringify({ ...b[k], w: null }) === JSON.stringify({ ...a[k], w: null }))) return '调整节点宽度';
  if (diff.some(k => JSON.stringify(b[k].link ?? null) !== JSON.stringify(a[k].link ?? null))) return '关联到任务或灵感';
  return '调整节点';
}
function summarize(verb, col, before, after) {
  const d = after || before, t = titleOf(col, d), cn = CN[col];
  if (col === 'links') {
    const line = `「${clip(titleOfId(d.source), 24)}」${REL[d.relation] || d.relation}「${clip(titleOfId(d.target), 24)}」`;
    return { create: (d.status === 'suggested' ? '建议' : '新建') + '关联 ' + line, delete: '删除关联 ' + line, undo: '撤销关联变更 ' + line }[verb] || '修改关联 ' + line;
  }
  if (verb === 'create') return `新建${cn}「${t}」`;
  if (verb === 'delete') return `删除${cn}「${t}」`;
  if (verb === 'undo') return !after ? `撤销：移除${cn}「${t}」` : !before ? `撤销：找回${cn}「${t}」` : `撤销：${cn}「${t}」恢复原状`;
  const parts = [];
  for (const k of new Set([...Object.keys(before || {}), ...Object.keys(after || {})])) {
    const l = FIELD_CN[k]; if (!l) continue;
    const x = before[k] ?? null, y = after[k] ?? null; if (JSON.stringify(x) === JSON.stringify(y)) continue;
    const sh = v => (v == null || v === '' ? '无' : ST_CN[v] || clip(String(v), 16));
    const p = k === 'nodes' ? nodeChanges(x, y) : ['status', 'due', 'start', 'todayDate', 'priority', 'area'].includes(k) ? `${l} ${sh(x)}→${sh(y)}` : l;
    if (!parts.includes(p)) parts.push(p);
  }
  return `修改${cn}「${t}」` + (parts.length ? '：' + parts.join('、') : '');
}

/* ---------- write queue ---------- */
let queue = [];
const notes = [];
SY.withNote = (note, fn) => { notes.push(note); try { return fn(); } finally { notes.pop(); } };
function record(col, before, after, keys) {
  if (!TYPE[col]) return;
  const id = (after || before).id, note = notes[notes.length - 1] || null, last = queue[queue.length - 1];
  if (last && last.col === col && last.id === id && last.note === note) {
    last.after = after;
    if (keys && last.keys) keys.forEach(k => last.keys.add(k)); else last.keys = null;
    if (!last.before && !after) queue.pop();
    return;
  }
  queue.push({ col, id, before, after, keys: keys ? new Set(keys) : null, note });
}
function toOp(it) {
  const type = TYPE[it.col];
  if (!it.after) return { op: 'entity.delete', type, id: it.id };
  if (!it.before || !it.keys) return { op: 'entity.put', type, id: it.id, doc: toCore(it.col, it.after) };
  const set = toCore(it.col, it.after, [...it.keys]);
  return Object.keys(set).length ? { op: 'entity.patch', type, id: it.id, set } : null;
}
function groups(items) {
  const out = [];
  for (const it of items) { const g = out[out.length - 1]; if (g && g.note === it.note) g.items.push(it); else out.push({ note: it.note, items: [it] }); }
  return out;
}

let timer = 0, running = null;
function schedule() { clearTimeout(timer); timer = setTimeout(flush, 160); }
async function flush() {
  clearTimeout(timer);
  while (running) await running;
  if (!queue.length) return;
  const items = queue; queue = [];
  running = (async () => {
    try {
      if (invoke) {
        for (const g of groups(items)) {
          const ops = g.items.map(toOp).filter(Boolean);
          if (!ops.length) continue;
          const res = await invoke('core_apply', { req: { ops, note: g.note } });
          if (res && res.batch) S.lastBatch = res.batch;
        }
      } else {
        saveLocal(items);
      }
      S.saveErr = null;
    } catch (e) {
      S.saveErr = SY.errText(e);
      console.error('save failed', e, items);
      SY.toast('保存失败：' + S.saveErr);
      if (invoke) { try { await load(); } catch (_) {} SY.renderAll(); }
    }
    if (SY.renderStatus) SY.renderStatus();
  })();
  try { await running; } finally { running = null; }
  if (queue.length) schedule();
}

/* ---------- load ---------- */
async function load() {
  const data = {}; SY.COLS.forEach(c => { data[c] = new Map(); });
  if (invoke) {
    const snap = await invoke('core_query', { q: { q: 'snapshot' } });
    for (const t of ['ideas', 'projects', 'tasks', 'maps', 'links']) for (const d of snap[t] || []) { const [col, doc] = fromCore(d); if (col) data[col].set(doc.id, doc); }
    S.seq = snap.seq || 0; S.permission = snap.agent_permission || 'write';
  } else {
    const d = lsGet(LS_DATA) || {};
    for (const c of SY.COLS) (Array.isArray(d[c]) ? d[c] : []).forEach(x => { if (x && x.id) data[c].set(String(x.id), { ...x, id: String(x.id) }); });
    S.permission = (lsGet('shiyi.desk.perm.v1') || 'write');
  }
  SY.COLS.forEach(c => { S.data[c] = data[c]; });
  if (S.sel && !(S.data[S.sel.col] && S.data[S.sel.col].has(S.sel.id))) S.sel = null;
  S.ready = true; S.rev++;
}
async function reload() { await flush(); await load(); SY.renderAll(); }

/* ---------- web preview: local data + operation log ---------- */
function persistLocal() {
  const payload = { app: 'shiyi', version: 3, savedAt: new Date().toISOString() };
  SY.COLS.forEach(c => { payload[c] = SY.list(c); });
  lsSet(LS_DATA, payload);
}
function localLog() { return lsGet(LS_OPS) || []; }
function saveLog(log) {
  while (log.length > 200) log.shift();
  lsSet(LS_OPS, log);
  if (lsGet(LS_OPS) == null && log.length) { // too big for storage: keep summaries only
    lsSet(LS_OPS, log.slice(-60).map(b => ({ ...b, ops: b.ops.map(o => ({ ...o, before: undefined, after: undefined, nodetail: true })) })));
  }
}
function saveLocal(items) {
  persistLocal();
  const log = localLog();
  for (const g of groups(items)) {
    const batch = SY.newId('links').replace('link_', 'bat_');
    log.push({
      batch, ts: now(), actor: 'user', note: g.note,
      ops: g.items.map(it => {
        const verb = !it.before ? 'create' : !it.after ? 'delete' : 'update';
        return { op: `${TYPE[it.col]}.${verb}`, entity_type: TYPE[it.col], entity_id: it.id, summary: summarize(verb, it.col, it.before, it.after), before: it.before, after: it.after };
      })
    });
    S.lastBatch = batch;
  }
  saveLog(log);
}
const strip = d => { if (!d) return null; const { updatedAt, version, updatedBy, ...rest } = d; return JSON.stringify(rest); };
function localUndo(batchId, force) {
  const log = localLog(), b = log.find(x => x.batch === batchId);
  if (!b) throw { code: 'not_found', message: '找不到这条记录' };
  if (b.undone_by) throw { code: 'already_undone', message: '已经撤销过了' };
  if (b.ops.some(o => o.nodetail)) throw { code: 'no_detail', message: '这条记录太旧，已经没有可恢复的内容' };
  const first = new Map(), last = new Map();
  b.ops.forEach(o => { const k = o.entity_type + ':' + o.entity_id; if (!first.has(k)) first.set(k, o); last.set(k, o); });
  const conflicts = [];
  for (const [k, o] of last) { const col = COL_OF[o.entity_type]; const cur = S.data[col].get(o.entity_id) || null; if (strip(cur) !== strip(o.after || null)) conflicts.push(k); }
  if (conflicts.length && !force) throw { code: 'conflict', message: `有 ${conflicts.length} 项之后又被改过`, conflicts };
  const nb = SY.newId('links').replace('link_', 'bat_'), ops = [];
  for (const [k, o] of [...first].reverse()) {
    const col = COL_OF[o.entity_type], cur = S.data[col].get(o.entity_id) || null;
    const restored = o.before ? { ...o.before, updatedAt: now() } : null;
    if (restored) S.data[col].set(o.entity_id, restored); else S.data[col].delete(o.entity_id);
    ops.push({ op: `${o.entity_type}.undo`, entity_type: o.entity_type, entity_id: o.entity_id, summary: summarize('undo', col, cur, restored), before: cur, after: restored });
  }
  b.undone_by = nb;
  log.push({ batch: nb, ts: now(), actor: 'user', note: '撤销', undo_of: batchId, ops });
  saveLog(log); persistLocal();
  return { undone: batchId, batch: nb };
}

/* ---------- public API ---------- */
async function undo(batch, { force = false } = {}) {
  await flush();
  try {
    let r;
    if (invoke) r = await invoke('core_apply', { req: { op: 'undo', batch, force } });
    else r = localUndo(batch, force);
    await load(); S.rev++; SY.renderAll();
    return r;
  } catch (e) {
    if (e && e.code === 'conflict' && !force) {
      const ok = await SY.ask('之后又改过', `这次操作涉及的内容后来又被修改过。仍然撤销会把这些内容恢复到这次操作之前，<b>之后的修改也会丢失</b>。`, '仍然撤销', { danger: true });
      return ok ? undo(batch, { force: true }) : null;
    }
    throw e;
  }
}
async function getLog({ actor = null, limit = 80 } = {}) {
  if (invoke) {
    const q = { q: 'log', limit }; if (actor) q.actor = actor;
    return (await invoke('core_query', { q })).batches || [];
  }
  return localLog().filter(b => !actor || (actor === 'agents' ? b.actor !== 'user' : b.actor === actor)).slice(-limit).reverse()
    .map(b => ({ ...b, undone: !!b.undone_by, ops: b.ops.map(o => ({ ...o, undone: !!b.undone_by })) }));
}
async function setPermission(level) {
  if (invoke) { await invoke('core_apply', { req: { op: 'settings.set', key: 'agent_permission', value: level } }); }
  else lsSet('shiyi.desk.perm.v1', level);
  S.permission = level;
}
function snapshot() {
  const out = { format: 'shiyi.snapshot', version: 1, exported_at: new Date().toISOString(), agent_permission: S.permission };
  SY.COLS.forEach(c => { out[c] = SY.list(c).map(d => toSnapshotDoc(c, d)); });
  return out;
}
/* web preview import: a snapshot (from the app or this page) or an old GUI backup */
function importLocal(obj) {
  let n = 0;
  if (obj.format === 'shiyi.snapshot' || obj.links) {
    for (const c of SY.COLS) for (const d of Array.isArray(obj[c]) ? obj[c] : []) {
      if (!d || !d.id || d.deleted_at) continue;
      const [col, doc] = d.type ? fromCore(d) : [c, { ...d }];
      if (col) { SY.put(col, doc); n++; }
    }
  } else {
    for (const c of ['ideas', 'projects', 'tasks', 'maps']) for (const d of Array.isArray(obj[c]) ? obj[c] : []) {
      if (!d || typeof d !== 'object' || !d.id) continue;
      const doc = { ...d, id: String(d.id) };
      if (c === 'tasks') doc.deps = Array.isArray(doc.deps) ? doc.deps.map(String) : [];
      SY.put(c, doc); n++;
    }
  }
  return n;
}

/* ---------- AI agents changed the database ---------- */
let watching = false;
async function onDbChanged() {
  const prev = S.seq;
  await reload();
  try {
    const res = await invoke('core_query', { q: { q: 'log', since_seq: prev, limit: 20 } });
    const agent = (res.batches || []).filter(b => b.actor !== 'user');
    if (!agent.length) return;
    const n = agent.reduce((a, b) => a + b.ops.length, 0);
    S.unseenAgent = (S.unseenAgent || 0) + n;
    const who = agent[0].actor.replace(/^agent:/, '');
    const first = agent[agent.length - 1].ops[0];
    const msg = `${who} 改了 ${n} 处${first && first.summary ? '：' + first.summary : ''}`;
    const one = agent.length === 1 ? agent[0].batch : null;
    SY.toast(msg, { action: { label: '查看', fn: () => SY.setView('log') }, undo: one ? () => undo(one).then(r => r && SY.toast('已撤销')).catch(e => SY.toast('撤销失败：' + SY.errText(e))) : null, ms: 9000 });
    SY.renderAll();
  } catch (e) { console.error(e); }
}
function watch() {
  if (watching || !invoke) return; watching = true;
  const T = window.__TAURI__;
  if (T && T.event && T.event.listen) T.event.listen('db-changed', () => { onDbChanged(); }).catch(() => {});
  // the window may miss events while hidden: refresh when it comes back
  document.addEventListener('visibilitychange', async () => {
    if (document.visibilityState !== 'visible') return;
    try { const st = await invoke('core_query', { q: { q: 'status' } }); if (st.last_seq !== S.seq) onDbChanged(); } catch (e) {}
  });
}

SY.store = { record, schedule, flush, load, reload, undo, getLog, setPermission, snapshot, importLocal, watch, toCore, fromCore, summarize };
})();
