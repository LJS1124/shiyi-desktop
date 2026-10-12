/* 十一遗 AI access: the operation log view (every change, by you or by an
   AI agent through the `shiyi` CLI, with undo) and the 设置 → AI 接入 section. */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, $, pad } = SY;
const invoke = SY.invoke;
const V = S.view.log = S.view.log || { rev: -1, items: null, loading: false, filter: 'all', open: {} };
const PERM = [['read', '只读', 'AI 只能查看，不能做任何修改。'], ['write', '可写', 'AI 可以记灵感、建项目和任务、改状态、建关联、撤销自己的操作；只能删除 AI 自己建的内容。推荐。'], ['full', '完全', 'AI 也可以删除你建的内容、撤销你的操作。删除都是软删除，仍能在这里恢复。']];

const whoOf = actor => (!actor || actor === 'user' ? null : String(actor).replace(/^agent:/, ''));
const tsMs = ts => (typeof ts === 'number' ? ts : Date.parse(ts));
function dayLabel(ms) {
  const d = new Date(ms), t = new Date(); t.setHours(0, 0, 0, 0);
  const diff = Math.round((t - new Date(d.getFullYear(), d.getMonth(), d.getDate())) / 864e5);
  return diff === 0 ? '今天' : diff === 1 ? '昨天' : `${d.getMonth() + 1} 月 ${d.getDate()} 日`;
}
const hm = ms => { const d = new Date(ms); return pad(d.getHours()) + ':' + pad(d.getMinutes()); };

async function fetchLog() {
  if (V.loading) return;
  V.loading = true;
  try { await SY.store.flush(); } catch (e) {}
  const rev = S.rev;
  try {
    V.items = await SY.store.getLog({ actor: V.filter === 'all' ? null : V.filter, limit: 120 });
    V.err = null;
  } catch (e) { V.err = SY.errText(e); V.items = V.items || []; }
  V.rev = rev; V.loading = false;
  if (S.ui.view === 'log') SY.renderAll();
}

function batchHtml(b) {
  const who = whoOf(b.actor), ms = tsMs(b.ts), isUndo = !!b.undo_of || (b.ops.length && b.ops.every(o => /\.undo$/.test(o.op)));
  const open = V.open[b.batch], ops = open ? b.ops : b.ops.slice(0, 5);
  const exists = id => ['ideas', 'projects', 'tasks', 'maps', 'links'].some(c => S.data[c].has(id));
  return `<div class="opb ${b.undone ? 'is-undone' : ''} ${who ? 'is-ai' : ''}">
    <div class="opb-h">
      <span class="who">${who ? `${icon('spark', 13)}${esc(who)}` : '我'}</span>
      <span class="mono te">${hm(ms)}</span>
      ${b.note ? `<span class="opb-note">${esc(b.note)}</span>` : ''}
      ${isUndo ? '<span class="pill">撤销</span>' : ''}${b.undone ? '<span class="pill">已撤销</span>' : ''}
      <span class="sp"></span>
      ${!b.undone ? `<button type="button" class="btn btn-sm btn-ghost" data-act="op-undo" data-batch="${esc(b.batch)}">${icon('undo', 14)}${isUndo ? '重做' : '撤销'}</button>` : ''}
    </div>
    <ul class="opb-ops">${ops.map(o => `<li class="${o.undone ? 'is-undone' : ''}">${o.entity_id && exists(o.entity_id) && !/^link/.test(o.entity_type || '') ? `<button type="button" class="linkish" data-act="g-open" data-id="${esc(o.entity_id)}">${esc(o.summary || o.op)}</button>` : esc(o.summary || o.op)}</li>`).join('')}
      ${b.ops.length > ops.length ? `<li><button type="button" class="linkish" data-act="op-more" data-batch="${esc(b.batch)}">还有 ${b.ops.length - ops.length} 项…</button></li>` : ''}</ul>
  </div>`;
}

SY.views.log = {
  render() {
    if (S.unseenAgent) { S.unseenAgent = 0; setTimeout(() => SY.renderAll(), 0); }
    if (V.rev !== S.rev && !V.loading) fetchLog();
    const perm = PERM.find(p => p[0] === S.permission) || PERM[1];
    const items = V.items;
    let body;
    if (!items) body = `<div class="skel"><i style="width:40%"></i><i style="width:62%"></i><i style="width:55%"></i></div>`;
    else if (!items.length) body = `<div class="empty-big clay"><h3>${V.filter === 'agents' ? 'AI 还没有动过你的数据' : '还没有记录'}</h3><p>${V.filter === 'agents' ? '在「设置 → AI 接入」里装好命令行工具和技能，让 Claude Code 之类的 AI 助手帮你记灵感、拆任务、找关联。它做的每一处修改都会出现在这里。' : '之后的每一次修改都会记在这里，可以随时撤销。'}</p>${V.filter === 'agents' ? `<button class="btn btn-primary" type="button" data-act="settings">${icon('gear', 15)}设置 AI 接入</button>` : ''}</div>`;
    else {
      let last = '', out = '';
      for (const b of items) {
        const day = dayLabel(tsMs(b.ts));
        if (day !== last) { out += `<div class="op-day">${day}</div>`; last = day; }
        out += batchHtml(b);
      }
      body = `<div class="oplog">${out}</div>`;
    }
    return `<div class="vh"><div><h1>操作记录</h1><div class="sub">每一次修改都记在这里——你在界面里做的，和 AI 通过命令行做的。任何一批都能撤销，撤销本身也能再撤销。</div></div>
        <div class="vh-acts"><div class="seg">${[['all', '全部'], ['agents', 'AI'], ['user', '我']].map(([k, l]) => `<button type="button" class="${V.filter === k ? 'is-on' : ''}" data-act="op-filter" data-v="${k}">${l}</button>`).join('')}</div></div></div>
      <div class="agent-banner clay">
        <span class="ab-ic">${icon('shield', 18)}</span>
        <div><b>AI 权限：${perm[1]}</b><div class="te">${perm[2]}</div></div>
        <span class="sp"></span>
        <button type="button" class="btn btn-sm" data-act="settings">${icon('term', 14)}AI 接入设置</button>
      </div>
      ${V.err ? `<div class="warn">读取记录失败：${esc(V.err)}</div>` : ''}
      ${body}`;
  }
};

/* ---------- settings section ---------- */
SY.aiAccessSection = () => {
  const cur = S.permission || 'write', p = PERM.find(x => x[0] === cur) || PERM[1];
  return `<div class="set-sec"><h3>AI 接入</h3>
    <p>让任何能运行命令的 AI 助手（Claude Code、Codex、Cursor、自己写的 agent…）通过 <code>shiyi</code> 命令行读写十一遗：记灵感、拆项目、排日程、找关联。它们的每一处修改都会出现在「操作记录」里，可以一键撤销。</p>
    <div class="field"><span class="lbl">AI 权限</span><div class="seg seg-sm" id="s-perm">${PERM.map(([k, l]) => `<button type="button" class="${cur === k ? 'is-on' : ''}" data-act="perm" data-v="${k}">${l}</button>`).join('')}</div></div>
    <p class="te" id="s-perm-desc" style="margin-top:-4px">${p[2]}</p>
    ${invoke ? `<div class="ai-steps">
      <div class="ai-step"><b>1</b><div><div class="ai-step-t">安装命令行工具 <span id="s-cli-state" class="te"></span></div>
        <div class="te">在终端里提供 <code>shiyi</code> 命令（装到 ~/.local/bin）。</div>
        <div class="ai-step-a"><button class="btn btn-sm" type="button" data-act="install-cli">${icon('term', 14)}安装命令行工具</button></div><div id="s-cli-out" class="te"></div></div></div>
      <div class="ai-step"><b>2</b><div><div class="ai-step-t">安装技能说明 <span id="s-skill-state" class="te"></span></div>
        <div class="te">告诉 AI 这个软件怎么用。Claude Code 会自动读取 ~/.claude/skills；别的 AI 可以把说明粘进它的提示词。</div>
        <div class="ai-step-a"><button class="btn btn-sm" type="button" data-act="install-skill">${icon('down', 14)}安装到 Claude Code</button><button class="btn btn-sm btn-ghost" type="button" data-act="copy-skill">${icon('copy', 14)}复制技能说明</button></div><div id="s-skill-out" class="te"></div></div></div>
      <div class="ai-step"><b>3</b><div><div class="ai-step-t">试一试</div>
        <div class="te">在终端运行下面的命令，或者直接对 AI 说「看看我十一遗里今天要做什么」。</div>
        <div class="cmd"><code>shiyi today --pretty</code><button class="mini" type="button" data-act="copy-cmd" data-v="shiyi today --pretty">${icon('copy', 12)}</button></div></div></div>
    </div>` : `<p class="te">网页预览里不能连接命令行；装上 Mac 版后在这里一键安装。你可以先看看给 AI 的技能说明：</p>
      <div class="ai-step-a"><button class="btn btn-sm" type="button" data-act="copy-skill">${icon('copy', 14)}复制技能说明</button><button class="btn btn-sm btn-ghost" type="button" data-act="view-skill">查看</button></div>`}
  </div>`;
};
SY.loadAiAccess = async () => {
  if (!invoke) return;
  try {
    const st = await invoke('cli_status');
    const set = (id, h) => { const el = document.getElementById(id); if (el) el.innerHTML = h; };
    set('s-cli-state', st.installed && st.installed.length ? `· 已安装 <span class="mono">${esc(st.installed[0])}</span>` : '· 未安装');
    set('s-skill-state', st.skill_installed ? (st.skill_current ? '· 已安装' : '· 有新版本，点安装更新') : '· 未安装');
  } catch (e) { console.error(e); }
};
async function skillText() {
  if (invoke) return invoke('skill_text');
  return window.__SHIYI_SKILL__ || '';
}
async function copy(text) {
  try { await navigator.clipboard.writeText(text); return true; }
  catch (e) {
    const ta = document.createElement('textarea'); ta.value = text; ta.style.position = 'fixed'; ta.style.opacity = '0'; document.body.appendChild(ta); ta.select();
    let ok = false; try { ok = document.execCommand('copy'); } catch (_) {} ta.remove(); return ok;
  }
}
Object.assign(SY.acts, {
  'op-filter': el => { V.filter = el.dataset.v; V.items = null; V.rev = -1; SY.renderAll(); },
  'op-more': el => { V.open[el.dataset.batch] = true; SY.renderAll(); },
  'op-undo': async el => {
    el.disabled = true;
    try { const r = await SY.store.undo(el.dataset.batch); if (r) SY.toast('已撤销，需要的话可以在这里再撤销回来'); }
    catch (e) { SY.toast('撤销失败：' + SY.errText(e)); }
    V.rev = -1; SY.renderAll();
  },
  'perm': async el => {
    const v = el.dataset.v;
    try { await SY.store.setPermission(v); } catch (e) { SY.toast('没能修改权限：' + SY.errText(e)); return; }
    document.querySelectorAll('#s-perm button').forEach(b => b.classList.toggle('is-on', b.dataset.v === v));
    const d = document.getElementById('s-perm-desc'); if (d) d.textContent = (PERM.find(p => p[0] === v) || PERM[1])[2];
    SY.toast('AI 权限已改为「' + (PERM.find(p => p[0] === v) || PERM[1])[1] + '」');
  },
  'install-cli': async () => {
    const out = document.getElementById('s-cli-out');
    try {
      const r = await invoke('install_cli');
      out.innerHTML = `已安装到 <span class="mono">${esc(r.paths.join('、'))}</span>。` + (r.on_default_path ? '' : `<br>如果终端提示找不到 shiyi，先运行一次：<div class="cmd"><code>${esc(r.path_hint)}</code><button class="mini" type="button" data-act="copy-cmd" data-v="${esc(r.path_hint)}">${icon('copy', 12)}</button></div>然后重新打开终端。`);
      SY.loadAiAccess();
    } catch (e) { out.textContent = '安装失败：' + SY.errText(e); }
  },
  'install-skill': async () => {
    const out = document.getElementById('s-skill-out');
    try { const p = await invoke('install_skill', { dir: null }); out.innerHTML = `已写入 <span class="mono">${esc(p)}</span>，重启 Claude Code 后生效。`; SY.loadAiAccess(); }
    catch (e) { out.textContent = '安装失败：' + SY.errText(e); }
  },
  'copy-skill': async () => { const ok = await copy(await skillText()); SY.toast(ok ? '技能说明已复制，可以粘贴给任何 AI' : '复制失败，点「查看」后手动复制'); },
  'view-skill': async () => { const t = await skillText(); SY.openModal('给 AI 的技能说明（SKILL.md）', `<pre class="skill-pre">${esc(t)}</pre>`, `<button class="btn" type="button" data-act="close-modal">关闭</button><button class="btn btn-primary" type="button" data-act="copy-skill">${icon('copy', 14)}复制</button>`, { iconName: 'term', wide: true }); },
  'copy-cmd': async el => { const ok = await copy(el.dataset.v); SY.toast(ok ? '已复制' : '复制失败'); }
});
})();
