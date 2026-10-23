/* 拾忆 settings → AI 模型: pick a provider from a grid, paste a key, choose the
   main and quick model (from the provider's own list), test, use.
   Keys never come to this page: only "saved" and a masked hint. */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, $ } = SY;
const P = SY.MP = { st: null, sel: null, dirty: false, models: {}, busy: '' };

const cat = () => (P.st && P.st.catalog) || [];
const prov = id => cat().find(p => p.id === id);
const saved = id => (P.st && P.st.providers && P.st.providers[id]) || null;
const ready = id => { const c = prov(id), s = saved(id); return !!(c && s && (s.hasKey || !c.needsKey) && (s.apiBase || c.base) && s.model); };

/* logo from assets/providers (bundled at build time from the open-source LobeHub
   icon set); a letter badge in the brand colour when the file is missing */
function logo(p, size = 28) {
  const letter = (p.name.match(/[A-Za-z一-龥]/) || ['?'])[0].toUpperCase();
  return `<span class="mp-ico" style="--sz:${size}px;--bc:${esc(p.color)}"><b>${esc(letter)}</b><img src="${esc((window.__SHIYI_PROVIDER_ICONS__ || {})[p.id] || `assets/providers/${p.id}.svg`)}" alt="" onerror="this.parentNode.classList.add('no-img');this.remove()"></span>`;
}

const REGION = { global: '海外', cn: '国内', local: '本地与自定义' };
function grid() {
  const groups = ['cn', 'global', 'local'];
  return groups.map(g => `<div class="mp-gh">${REGION[g]}</div><div class="mp-grid">${cat().filter(p => p.region === g).map(p => {
    const on = P.sel === p.id, use = P.st.provider === p.id, ok = ready(p.id);
    return `<button type="button" class="mp-card ${on ? 'is-on' : ''}" data-act="mp-pick" data-v="${esc(p.id)}" title="${esc(p.name)}">${logo(p)}<span class="nm">${esc(p.name)}</span>${use ? '<em class="mp-use">使用中</em>' : ok ? '<i class="mp-dot" title="已配置"></i>' : ''}</button>`;
  }).join('')}</div>`).join('');
}

function detail() {
  const p = prov(P.sel); if (!p) return '';
  const s = saved(p.id) || {}, use = P.st.provider === p.id;
  const fmt = s.apiFormat || p.format, base = s.apiBase ?? '', model = s.model || p.models[0] || '', quick = s.quickModel || p.models[1] || p.models[0] || '';
  const list = [...new Set([...(p.models || []), ...(P.models[p.id] || [])])];
  const status = use ? '<span class="mp-st is-use">使用中</span>' : ready(p.id) ? '<span class="mp-st is-ok">已配置</span>' : '<span class="mp-st">未配置</span>';
  const keyRow = p.needsKey
    ? `<label class="mp-f">API Key<div class="mp-row"><input class="in mono" id="mp-key" type="password" autocomplete="off" spellcheck="false" placeholder="${s.hasKey ? `已保存 ${esc(s.keyHint || '')}（留空则不修改）` : '粘贴 API Key'}">${p.keyUrl ? `<button type="button" class="btn btn-sm btn-ghost" data-act="mp-keyurl" data-v="${esc(p.keyUrl)}">去获取 ↗</button>` : ''}</div></label>`
    : `<p class="mp-note">不需要 API Key。先在本机安装并运行 Ollama，再用 <code>ollama pull 模型名</code> 下载模型。${p.keyUrl ? ` <button type="button" class="linkish" data-act="mp-keyurl" data-v="${esc(p.keyUrl)}">下载 Ollama ↗</button>` : ''}</p>`;
  return `<div class="mp-head">${logo(p, 36)}<div><div class="mp-name">${esc(p.name)} ${status}</div><div class="te">${esc(fmt === 'anthropic' ? 'Anthropic Messages 接口' : 'OpenAI 兼容接口')}${base || p.base ? ' · ' + esc(base || p.base) : ''}</div></div></div>
    ${keyRow}
    <div class="mp-models">
      <label class="mp-f">主模型 <span class="te">拆解、规划、找关联</span><input class="in mono" id="mp-model" list="mp-list" value="${esc(model)}" placeholder="模型名"></label>
      <label class="mp-f">快速模型 <span class="te">排今日、扩展导图</span><input class="in mono" id="mp-quick" list="mp-list" value="${esc(quick)}" placeholder="模型名，可以和主模型相同"></label>
      <datalist id="mp-list">${list.map(m => `<option value="${esc(m)}">`).join('')}</datalist>
    </div>
    <div class="mp-row mp-fetch"><button type="button" class="btn btn-sm btn-ghost" data-act="mp-fetch" ${P.busy ? 'disabled' : ''}>${icon('reopen', 13)}获取模型列表</button><span class="te" id="mp-fetch-out">${P.models[p.id] ? `可选 ${P.models[p.id].length} 个模型，点输入框选择` : '从服务商拉取可用的模型，也可以直接填写模型名'}</span></div>
    <details class="mp-adv" ${p.id === 'custom' ? 'open' : ''}><summary>高级：接口地址与格式</summary>
      <label class="mp-f">接口地址<input class="in mono" id="mp-base" value="${esc(base)}" placeholder="${esc(p.base || 'https://你的网关/v1')}"></label>
      <label class="mp-f">接口格式<select class="in" id="mp-format"><option value="openai" ${fmt !== 'anthropic' ? 'selected' : ''}>OpenAI 兼容（/chat/completions）</option><option value="anthropic" ${fmt === 'anthropic' ? 'selected' : ''}>Anthropic Messages（/v1/messages）</option></select></label>
    </details>
    <div class="mp-row mp-acts"><button type="button" class="btn btn-sm" data-act="mp-test" ${P.busy ? 'disabled' : ''}>测试连接</button>${use ? '' : `<button type="button" class="btn btn-sm btn-primary" data-act="mp-use" ${P.busy ? 'disabled' : ''}>保存并使用</button>`}<span class="te" id="mp-out"></span></div>`;
}

SY.modelSection = st => {
  P.st = st; P.dirty = false;
  if (!P.sel || !prov(P.sel)) P.sel = st.provider || 'anthropic';
  return `<div class="set-sec" id="s-models"><h3>AI 模型</h3>
    <p>选一家服务商，填上 API Key 就能用。Key 只保存在本机，界面上只显示首尾几位。</p>
    <div class="mp"><div class="mp-side">${grid()}</div><div class="mp-detail" id="mp-detail">${detail()}</div></div></div>`;
};
function paint() {
  const sec = document.getElementById('s-models'); if (!sec) return;
  sec.querySelector('.mp-side').innerHTML = grid();
  sec.querySelector('#mp-detail').innerHTML = detail();
}
async function reload() {
  try { P.st = await SY.invoke('get_settings'); } catch (e) {}
  paint();
}

function form() {
  const p = prov(P.sel), v = id => { const el = document.getElementById(id); return el ? el.value.trim() : ''; };
  const key = document.getElementById('mp-key');
  return {
    apiFormat: document.getElementById('mp-format') ? document.getElementById('mp-format').value : p.format,
    apiBase: v('mp-base') || p.base,
    model: v('mp-model'),
    quickModel: v('mp-quick') || v('mp-model'),
    apiKey: key && key.value.trim() ? key.value.trim() : null
  };
}
function check(f, needModel = true) {
  const p = prov(P.sel), s = saved(P.sel) || {};
  if (p.needsKey && !f.apiKey && !s.hasKey) return '先填 API Key';
  if (!f.apiBase && !p.base) return '先在「高级」里填接口地址';
  if (needModel && !f.model) return '先填主模型';
  return '';
}
async function save(activate, needModel = true) {
  const f = form(), bad = check(f, needModel), out = document.getElementById('mp-out');
  if (bad) { if (out) out.textContent = bad; return false; }
  try { await SY.invoke('save_provider', { provider: P.sel, cfg: f, activate }); }
  catch (e) { if (out) out.textContent = '保存失败：' + SY.errText(e); return false; }
  P.dirty = false; await reload(); return true;
}
/* the dialog's 保存 button: keep what was typed for the selected provider; switch to it
   only when it is the one in use or the user changed something there */
SY.saveModelPanel = async () => {
  if (!P.st || !document.getElementById('s-models')) return true;
  if (!P.dirty) return true;
  const f = form(); if (check(f)) return true; // incomplete: leave it, nothing to lose
  return save(P.st.provider === P.sel || !ready(P.st.provider));
};

Object.assign(SY.acts, {
  'mp-pick': el => { P.sel = el.dataset.v; P.dirty = false; paint(); },
  'mp-keyurl': el => SY.invoke('open_external', { url: el.dataset.v }).catch(e => SY.toast('打不开：' + SY.errText(e))),
  'mp-use': async () => {
    P.busy = 'use'; const ok = await save(true); P.busy = ''; paint();
    if (ok) SY.toast(`已切换到 ${prov(P.st.provider).name}`);
  },
  'mp-fetch': async () => {
    const out = () => document.getElementById('mp-fetch-out');
    const f = form();
    if (P.dirty || f.apiKey || !saved(P.sel)) { if (!(await save(false, false))) { const o = out(); const m = document.getElementById('mp-out'); if (o && m) o.textContent = m.textContent; return; } }
    P.busy = 'fetch'; if (out()) out().textContent = '正在获取…';
    try {
      const ms = await SY.invoke('list_models', { provider: P.sel });
      P.models[P.sel] = ms; P.busy = ''; paint();
      SY.toast(`${prov(P.sel).name} 有 ${ms.length} 个可用模型`);
    } catch (e) { P.busy = ''; paint(); const o = out(); if (o) o.textContent = '获取失败：' + SY.errText(e); }
  },
  'mp-test': async () => {
    const id = P.sel;
    if (!(await save(false))) return;
    P.busy = 'test'; paint();
    const out = () => document.getElementById('mp-out');
    if (out()) out().textContent = '正在连接…';
    const t0 = performance.now();
    try {
      const r = await SY.invoke('ai_complete', { prompt: '只回复两个字：你好', quick: true, provider: id });
      P.busy = ''; paint(); const o = out(); if (o) o.innerHTML = `<span class="ok">✓ 连接成功</span> ${((performance.now() - t0) / 1000).toFixed(1)} 秒 · 回复「${esc(String(r).trim().slice(0, 30))}」`;
    } catch (e) { P.busy = ''; paint(); const o = out(); if (o) o.innerHTML = `<span class="bad">连接失败</span>：${esc(SY.errText(e))}`; }
  }
});
document.addEventListener('input', e => { if (e.target.closest && e.target.closest('#mp-detail')) P.dirty = true; });
document.addEventListener('change', e => { if (e.target.closest && e.target.closest('#mp-detail')) P.dirty = true; });
})();
