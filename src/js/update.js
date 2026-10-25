/* 拾忆 updates: version + GitHub link in the sidebar, a check against GitHub
   Releases (at most twice a day, and from 设置 → 关于), and an update dialog
   that says plainly where the data lives and that it is backed up first. */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, lsGet, lsSet } = SY;
const REPO = 'https://github.com/LJS1124/shiyi-desktop';
const K_LAST = 'shiyi.update.last', K_AT = 'shiyi.update.checkedAt', K_SKIP = 'shiyi.update.skip';
const U = SY.UPD = { info: null, app: null, checking: false, err: '' };

const open = url => {
  if (SY.isApp) return SY.invoke('open_project_url', { url }).catch(e => SY.toast('打不开链接：' + SY.errText(e)));
  window.open(url, '_blank', 'noopener');
};
const autoOn = () => S.ui.autoUpdate !== false;
const pending = () => U.info && U.info.available && U.info.version !== lsGet(K_SKIP) ? U.info : null;

/* sidebar: "新版本" pill + version · GitHub */
SY.updateFootHtml = () => {
  const p = pending();
  const v = U.app ? 'v' + U.app.version : '';
  return `${p ? `<button class="upd-pill" type="button" data-act="update-open">${icon('spark', 13)}新版本 v${esc(p.version)} 可以更新</button>` : ''}
    <div class="foot-links">${v ? `<span class="mono">${esc(v)}</span><span>·</span>` : ''}<button type="button" class="link-btn" data-act="open-repo">GitHub</button></div>`;
};

/* tiny markdown for release notes: headings, bullets, bold, code */
function notesHtml(s) {
  const inline = t => esc(t).replace(/\*\*(.+?)\*\*/g, '<b>$1</b>').replace(/`([^`]+)`/g, '<code>$1</code>');
  const out = []; let list = false, code = false;
  for (const line of String(s || '').split(/\r?\n/)) {
    if (line.trim().startsWith('```')) { if (list) { out.push('</ul>'); list = false; } out.push(code ? '</code></pre>' : '<pre class="upd-code"><code>'); code = !code; continue; }
    if (code) { out.push(esc(line) + '\n'); continue; }
    const li = line.match(/^\s*[-*] (.*)/);
    if (li) { if (!list) { out.push('<ul>'); list = true; } out.push(`<li>${inline(li[1])}</li>`); continue; }
    if (list) { out.push('</ul>'); list = false; }
    const h = line.match(/^#{1,4} (.*)/);
    if (h) out.push(`<h4>${inline(h[1])}</h4>`);
    else if (line.trim()) out.push(`<p>${inline(line)}</p>`);
  }
  if (list) out.push('</ul>');
  if (code) out.push('</code></pre>');
  return out.join('');
}

function safetyHtml() {
  const a = U.app || {};
  return `<div class="upd-safe">${icon('shield', 16)}<div>
    <b>更新不会动你的数据。</b>数据存在应用之外的数据库文件里${a.data_path ? `（<span class="mono">${esc(a.data_path)}</span>）` : ''}，覆盖安装只替换应用本身。
    新版第一次打开时，还会先把整个数据库备份一份${a.backups ? `到 <span class="mono">${esc(a.backups)}</span>` : ''}，万一有问题可以退回。</div></div>`;
}

function stepsHtml() {
  if (U.app && U.app.auto_update) return `<p class="upd-auto">点「立即更新」：拾忆会在后台下载新版本，核对签名确认是官方发布的，然后自动安装并重启，前后大约一分钟。万一自动更新失败，也可以点「下载安装包」手动更新。</p>`;
  const win = (U.app && U.app.os === 'windows') || SY.isWin;
  if (win) return `<ol class="upd-steps"><li>点「下载安装包」，下载 <span class="mono">-setup.exe</span>。</li><li>关闭拾忆，双击安装包，直接覆盖安装。</li><li>只是升级的话不用先卸载；如果卸载，不要勾选「删除应用数据」。</li></ol>`;
  const cmd = 'xattr -dr com.apple.quarantine /Applications/Shiyi.app';
  return `<ol class="upd-steps"><li>点「下载安装包」，下载 <span class="mono">.dmg</span>。</li><li>退出拾忆（${SY.keys('⌘Q')}），打开 dmg，把 Shiyi 拖进「应用程序」，选「替换」。</li>
    <li>第一次打开前在「终端」执行一次：<div class="cmd"><code>${esc(cmd)}</code><button class="mini" type="button" data-act="copy-cmd" data-v="${esc(cmd)}">${icon('copy', 12)}</button></div></li></ol>`;
}

function openDialog() {
  const p = U.info; if (!p) return;
  const cur = U.app ? U.app.version : p.current;
  SY.openModal(p.available ? `新版本 v${p.version}` : '已经是最新版本', `
    <p class="upd-ver">当前 <b class="mono">v${esc(cur)}</b>${p.available ? ` → 最新 <b class="mono">v${esc(p.version)}</b>` : ''}${p.published_at ? ` · 发布于 ${esc(p.published_at.slice(0, 10))}` : ''}</p>
    ${p.available ? safetyHtml() + `<div class="upd-notes">${notesHtml(p.notes) || '<p>这个版本没有写更新说明。</p>'}</div><h4 class="upd-h">怎么更新</h4>${stepsHtml()}` : '<p>你用的就是最新版本。</p>'}`,
    p.available
      ? (U.app && U.app.auto_update
        ? `<button class="btn btn-ghost" type="button" data-act="update-skip">跳过这个版本</button><span style="flex:1"></span><button class="btn" type="button" data-act="update-download">下载安装包</button><button class="btn btn-primary" type="button" data-act="update-install" id="upd-go">${icon('down', 14)}立即更新</button>`
        : `<button class="btn btn-ghost" type="button" data-act="update-skip">跳过这个版本</button><span style="flex:1"></span><button class="btn" type="button" data-act="update-page">在 GitHub 查看</button><button class="btn btn-primary" type="button" data-act="update-download">${icon('down', 14)}下载安装包</button>`)
      : `<button class="btn" type="button" data-act="update-page">在 GitHub 查看</button><button class="btn btn-primary" type="button" data-act="close-modal">好</button>`,
    { iconName: 'spark' });
}

async function check(manual) {
  if (!SY.isApp) { if (manual) open(REPO + '/releases/latest'); return; }
  if (U.checking) return;
  U.checking = true; U.err = ''; paintAbout();
  try {
    const info = await SY.invoke('check_update');
    U.info = info; lsSet(K_LAST, info); lsSet(K_AT, Date.now());
    if (manual) openDialog();
    else if (pending()) SY.toast(`拾忆 v${info.version} 可以更新了`, { action: { label: '查看', fn: openDialog }, ms: 9000 });
  } catch (e) {
    U.err = SY.errText(e);
    if (manual) SY.toast('检查更新失败：' + U.err, { ms: 8000 });
  } finally { U.checking = false; paintAbout(); SY.scheduleRender(); }
}
SY.checkUpdate = check;

/* 设置 → 关于 */
SY.aboutSection = () => `<div class="set-sec" id="s-about"><h3>关于</h3>${aboutBody()}</div>`;
function aboutBody() {
  const a = U.app, p = pending();
  const state = U.checking ? '正在检查…' : U.err ? '上次检查失败：' + esc(U.err) : p ? `有新版本 v${esc(p.version)}` : U.info ? '已经是最新版本' : '';
  return `<p>拾忆 ${a ? `<b class="mono">v${esc(a.version)}</b>` : ''} · 开源在 <button type="button" class="link-btn" data-act="open-repo">GitHub</button>，遇到问题或有建议，点右边的「反馈问题」，会带上诊断信息在 GitHub 上提一个 Issue。</p>
    <div style="display:flex;gap:8px;flex-wrap:wrap;align-items:center">
      <button class="btn btn-sm" type="button" data-act="update-check" ${U.checking ? 'disabled' : ''}>${icon('down', 14)}检查更新</button>
      ${p ? `<button class="btn btn-sm btn-primary" type="button" data-act="update-open">查看 v${esc(p.version)}</button>` : ''}
      <span class="te">${state}</span>
      <span style="flex:1"></span>${SY.diagButtons ? SY.diagButtons() : ''}
    </div>
    ${SY.isApp ? `<label class="opt-chk"><input type="checkbox" id="s-auto-upd" ${autoOn() ? 'checked' : ''} data-act="update-auto"> 启动时自动检查新版本（每天最多两次，只访问 GitHub）</label>` : ''}`;
}
function paintAbout() { const el = document.getElementById('s-about'); if (el) el.innerHTML = '<h3>关于</h3>' + aboutBody(); }

/* in-app update: download, verify signature, install, restart (all in the backend) */
let installing = false;
async function install() {
  if (installing) return; installing = true;
  const foot = document.getElementById('m-foot'), body = document.getElementById('m-body');
  if (body) body.insertAdjacentHTML('afterbegin', `<div class="upd-prog" id="upd-prog"><div class="upd-bar"><i id="upd-bar" style="width:0%"></i></div><span id="upd-txt">正在下载…</span></div>`);
  if (foot) foot.querySelectorAll('button').forEach(b => { b.disabled = true; });
  const T = window.__TAURI__; let un = null;
  try {
    if (T && T.event && T.event.listen) un = await T.event.listen('update-progress', e => {
      const { done, total } = e.payload || {}, bar = document.getElementById('upd-bar'), txt = document.getElementById('upd-txt');
      const mb = n => (n / 1048576).toFixed(1) + ' MB';
      if (bar && total) bar.style.width = Math.min(100, done / total * 100).toFixed(0) + '%';
      if (txt) txt.textContent = total && done >= total ? '下载完成，正在核对签名并安装，完成后会自动重启…' : `正在下载… ${mb(done)}${total ? ' / ' + mb(total) : ''}`;
    });
    await SY.invoke('update_install');           // restarts the app when it succeeds
  } catch (e) {
    const prog = document.getElementById('upd-prog');
    if (prog) prog.outerHTML = `<p class="ai-err">自动更新没有完成：${esc(SY.errText(e))}。可以点「下载安装包」手动更新，数据不受影响。</p>`;
    if (foot) foot.querySelectorAll('button').forEach(b => { b.disabled = false; });
  } finally { installing = false; if (un) un(); }
}

Object.assign(SY.acts, {
  'update-install': () => install(),
  'open-repo': () => open(REPO),
  'update-open': () => openDialog(),
  'update-check': () => check(true),
  'update-page': () => open((U.info && U.info.page) || REPO + '/releases/latest'),
  'update-download': () => open((U.info && (U.info.asset_url || U.info.page)) || REPO + '/releases/latest'),
  'update-skip': () => { if (U.info) lsSet(K_SKIP, U.info.version); SY.closeModal(); SY.toast('已跳过这个版本，有更新的版本时会再提示'); SY.scheduleRender(); },
  'update-auto': el => { S.ui.autoUpdate = !!el.checked; SY.saveUI(); }
});

/* start: show what we knew last time right away, check again if it's been a while */
U.info = lsGet(K_LAST);
if (U.info && SY.isApp) U.info = { ...U.info, available: false }; // re-evaluated once the app version is known
(async () => {
  if (!SY.isApp) return;
  try { U.app = await SY.invoke('app_info'); } catch (e) { return; }
  const last = lsGet(K_LAST);
  if (last && last.version && U.app.version) {
    const newer = (a, b) => { const p = s => String(s).split('-')[0].split('.').map(n => +n || 0); const x = p(a), y = p(b); for (let i = 0; i < 3; i++) if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) > (y[i] || 0); return false; };
    U.info = { ...last, current: U.app.version, available: newer(last.version, U.app.version) };
  }
  SY.scheduleRender();
  const due = Date.now() - (lsGet(K_AT) || 0) > 12 * 36e5;
  if (autoOn() && due) setTimeout(() => check(false), 6000);
})();
})();
