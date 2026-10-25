/* 拾忆 system: reminders and background running (设置 → 提醒与后台), and
   diagnostics + feedback through GitHub Issues (设置 → 关于, ⌘K). */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, lsGet, fmtTs } = SY;
const ISSUES = 'https://github.com/LJS1124/shiyi-desktop/issues';

/* ---------- 设置 → 提醒与后台 ---------- */
let prefs = null;
/* half-hour slots from 05:00 to 23:30 (plus the saved time if it is in between) */
function timeOptions(cur) {
  const ts = []; for (let m = 5 * 60; m < 24 * 60; m += 30) ts.push(String(Math.floor(m / 60)).padStart(2, '0') + ':' + String(m % 60).padStart(2, '0'));
  if (cur && !ts.includes(cur)) { ts.push(cur); ts.sort(); }
  return ts.map(t => `<option value="${t}" ${t === cur ? 'selected' : ''}>${t}</option>`).join('');
}
SY.prefsSection = st => {
  const p = prefs = st && st.prefs; if (!p) return '';
  const where = p.tray ? '关闭窗口后拾忆会缩到任务栏右下角的托盘里' : SY.isMac ? `关闭窗口后拾忆仍在后台运行（${SY.keys('⌘Q')} 才是退出）` : '拾忆运行时';
  return `<div class="set-sec" id="s-prefs"><h3>提醒与后台</h3>
    <p>每天到点发一条系统通知：今天要做几件、有没有逾期。${where}，提醒和全局快捷键照常工作。</p>
    <div class="pref-row">
      <label class="opt-chk"><input type="checkbox" id="pf-daily" ${p.dailySummary ? 'checked' : ''}> 每天发一条今日摘要，时间</label>
      <select class="ctl mono" id="pf-time" ${p.dailySummary ? '' : 'disabled'} aria-label="每日摘要的时间">${timeOptions(p.dailyTime)}</select>
      <button class="btn btn-sm btn-ghost" type="button" data-act="pf-test">${icon('spark', 13)}发一条试试</button>
      <span class="te" id="pf-out"></span>
    </div>
    ${p.tray ? `<label class="opt-chk"><input type="checkbox" id="pf-tray" ${p.closeToTray ? 'checked' : ''}> 关闭窗口时缩到托盘，在后台继续运行（不勾选则关闭即退出）</label>` : ''}
    ${p.canAutostart ? `<label class="opt-chk"><input type="checkbox" id="pf-auto" ${p.autostart ? 'checked' : ''}> 开机自动启动（不弹出窗口，在后台等着提醒你）</label>` : ''}
  </div>`;
};
const pfOut = (t, cls = '') => { const o = document.getElementById('pf-out'); if (o) { o.className = 'te ' + cls; o.textContent = t; } };
async function savePrefs(part, el) {
  try {
    prefs = await SY.invoke('save_prefs', { prefs: part });
    const time = document.getElementById('pf-time'); if (time) { time.disabled = !prefs.dailySummary; if (time.value !== prefs.dailyTime) time.innerHTML = timeOptions(prefs.dailyTime); }
    pfOut('已保存', 'ok');
  } catch (e) {
    if (el && el.type === 'checkbox') el.checked = !el.checked;
    pfOut(SY.errText(e), 'bad');
  }
}
SY.changeHandlers.push(e => {
  const el = e.target;
  if (el.id === 'pf-daily') savePrefs({ dailySummary: el.checked }, el);
  else if (el.id === 'pf-time') { if (el.value) savePrefs({ dailyTime: el.value }, el); }
  else if (el.id === 'pf-tray') savePrefs({ closeToTray: el.checked }, el);
  else if (el.id === 'pf-auto') savePrefs({ autostart: el.checked }, el);
});
SY.acts['pf-test'] = async () => {
  pfOut('正在发送…');
  try {
    await SY.invoke('notify_test');
    pfOut(SY.isMac ? '已发送。没看到的话，到「系统设置 → 通知」里允许 Shiyi 发通知' : '已发送。没看到的话，到「设置 → 系统 → 通知」里允许 Shiyi 发通知', 'ok');
  } catch (e) { pfOut(SY.errText(e), 'bad'); }
};

/* ---------- diagnostics ---------- */
const mb = n => (n >= 1048576 ? (n / 1048576).toFixed(1) + ' MB' : Math.max(1, Math.round((n || 0) / 1024)) + ' KB');
async function osLabel(d) {
  const os = d.os || (SY.isMac ? 'macos' : SY.isWin ? 'windows' : 'linux');
  if (os === 'macos') return 'macOS ' + (d.osVersion || '');
  if (os === 'windows') {
    let v = 'Windows';
    try { const h = await navigator.userAgentData.getHighEntropyValues(['platformVersion']); v = Number(String(h.platformVersion).split('.')[0]) >= 13 ? 'Windows 11' : 'Windows 10'; } catch (e) {}
    return v;
  }
  return os;
}
SY.diagText = async () => {
  let d = {};
  if (SY.isApp) { try { d = await SY.invoke('diagnostics'); } catch (e) { d = { error: SY.errText(e) }; } }
  const c = d.counts || {}, p = d.prefs || {};
  const prov = (((SY.MP && SY.MP.st && SY.MP.st.catalog) || []).find(x => x.id === d.provider) || {}).name || d.provider || '';
  const errs = lsGet('shiyi.errors') || [];
  const ver = d.version || (SY.UPD && SY.UPD.app && SY.UPD.app.version) || '';
  const lines = [
    '### 诊断信息',
    '（拾忆自动生成，不含笔记内容和 API Key）',
    '',
    `- 版本：${ver ? 'v' + ver : '未知'}${SY.isApp ? '' : '（网页预览）'}`,
    `- 系统：${(await osLabel(d)).trim()}${d.arch ? ' · ' + d.arch : ''}`,
    SY.isApp ? `- 数据：灵感 ${c.ideas ?? 0} · 项目 ${c.projects ?? 0} · 任务 ${c.tasks ?? 0} · 导图 ${c.maps ?? 0} · 关联 ${c.links ?? 0}；数据库 ${mb(d.dbBytes)}${d.dbError ? '；打不开：' + d.dbError : ''}；最近备份 ${d.lastBackup || '无'}` : `- 数据：灵感 ${S.data.ideas.size} · 项目 ${S.data.projects.size} · 任务 ${S.data.tasks.size} · 导图 ${S.data.maps.size} · 关联 ${S.data.links.size}`,
    SY.isApp ? `- 内置 AI：${prov || '未设置'} · ${d.hasKey ? '已填 Key' : '未填 Key'} · ${d.format === 'anthropic' ? 'Anthropic 格式' : 'OpenAI 兼容'} · ${d.model || '-'} / ${d.quickModel || '-'}` : '',
    SY.isApp ? `- AI 助手：命令行${d.cli ? '已安装' : '未安装'} · 技能${d.skill ? (d.skillCurrent ? '已安装（最新）' : '已安装（旧版）') : '未安装'} · 权限 ${d.permission || '-'}` : '',
    SY.isApp ? `- 提醒：${p.dailySummary ? '每天 ' + p.dailyTime : '关闭'}${p.tray ? ' · 关窗缩到托盘 ' + (p.closeToTray ? '开' : '关') : ''}${p.canAutostart ? ' · 开机自启 ' + (p.autostart ? '开' : '关') : ''} · 自动更新${d.autoUpdate ? '可用' : '不可用'}` : '',
    `- 界面：${(SY.SKINS.find(x => x[0] === S.ui.skin) || ['', S.ui.skin])[1]} · ${{ system: '跟随系统', light: '浅色', dark: '深色' }[S.ui.theme] || S.ui.theme} · 窗口 ${innerWidth}×${innerHeight}`,
    d.error ? `- 读取诊断信息失败：${d.error}` : '',
    '',
    '**最近的报错**',
    ...(errs.length ? errs.slice(-20).reverse().map(e => `- ${fmtTs(e.t)} [${e.w}] ${e.m}`) : ['- 没有']),
  ];
  if (d.log && d.log.length) lines.push('', '**后台日志（最后 20 行）**', '```', ...d.log, '```');
  return lines.filter(l => l !== '').join('\n').replace(/\n(\*\*|###)/g, '\n\n$1');
};

async function openDiag() {
  const text = await SY.diagText();
  SY.openModal('诊断信息', `<p style="margin:0">版本、系统、数据条数和最近的报错，用来排查问题。<b>不含任何笔记内容和 API Key。</b>提问题时可以把它附上。</p>
    <pre class="diag" id="diag-text">${esc(text)}</pre>`,
    `<button class="btn" type="button" data-act="diag-copy">${icon('copy', 14)}复制</button><span style="flex:1"></span><button class="btn" type="button" data-act="close-modal">关闭</button><button class="btn btn-primary" type="button" data-act="feedback">${icon('arrow', 14)}反馈问题</button>`,
    { iconName: 'shield' });
}

/* ---------- feedback: a GitHub issue with the diagnostics filled in ---------- */
async function openFeedback() {
  SY.openModal('反馈问题', `<p style="margin:0">会在浏览器里打开拾忆在 GitHub 上的「新建 Issue」页面，并把下面的内容填好，<b>登录 GitHub 后点「Submit new issue」</b>就提交了。不方便的话，也可以直接到 <button type="button" class="link-btn" data-act="open-issues">GitHub Issues</button> 看看有没有人提过。</p>
    <label class="mp-f">标题<input class="in" id="fb-title" maxlength="100" placeholder="一句话说清楚，比如：导图节点拖不动"></label>
    <label class="mp-f">发生了什么<textarea class="in" id="fb-body" rows="6" placeholder="做了什么、期望看到什么、实际看到什么。有截图的话，打开页面后可以直接粘贴进去。"></textarea></label>
    <div class="pref-row"><label class="opt-chk"><input type="checkbox" id="fb-diag" checked> 附上诊断信息（版本、系统和报错，不含笔记内容和 Key）</label><button type="button" class="link-btn" data-act="fb-preview">看看内容</button></div>
    <pre class="diag" id="fb-pre" hidden></pre>
    <span class="te" id="fb-out"></span>`,
    `<button class="btn" type="button" data-act="close-modal">取消</button><button class="btn btn-primary" type="button" data-act="feedback-go">${icon('arrow', 14)}去 GitHub 提交</button>`,
    { iconName: 'edit' });
  setTimeout(() => { const t = document.getElementById('fb-title'); if (t) t.focus(); }, 30);
}
async function sendFeedback() {
  const title = (document.getElementById('fb-title').value || '').trim();
  const desc = (document.getElementById('fb-body').value || '').trim();
  const out = document.getElementById('fb-out');
  if (!title && !desc) { if (out) out.textContent = '先写一句发生了什么'; return; }
  const withDiag = document.getElementById('fb-diag').checked;
  const body = (desc || '（没有描述）') + (withDiag ? '\n\n' + await SY.diagText() : '');
  const t = title || desc.split('\n')[0].slice(0, 60);
  // a copy on the clipboard too, in case the page cuts a long text
  if (SY.copyText) await SY.copyText(body);
  try {
    if (SY.isApp) await SY.invoke('open_feedback', { title: t, body });
    else window.open(`${ISSUES}/new?title=${encodeURIComponent(t)}&body=${encodeURIComponent(body).slice(0, 6000)}`, '_blank', 'noopener');
    SY.closeModal();
    SY.toast('已在浏览器打开 GitHub，内容也复制好了。登录后点「Submit new issue」提交', { ms: 9000 });
  } catch (e) { if (out) out.textContent = '打不开浏览器：' + SY.errText(e); }
}

const openIssues = () => (SY.isApp ? SY.invoke('open_project_url', { url: ISSUES }).catch(e => SY.toast('打不开：' + SY.errText(e))) : window.open(ISSUES, '_blank', 'noopener'));
Object.assign(SY.acts, {
  'diag-open': () => openDiag(),
  'diag-copy': async () => { const el = document.getElementById('diag-text'); const ok = el && SY.copyText ? await SY.copyText(el.textContent) : false; SY.toast(ok ? '诊断信息已复制' : '复制失败，可以手动选中复制'); },
  'feedback': () => openFeedback(),
  'feedback-go': () => sendFeedback(),
  'fb-preview': async () => { const pre = document.getElementById('fb-pre'); if (!pre) return; if (pre.hidden) pre.textContent = await SY.diagText(); pre.hidden = !pre.hidden; },
  'open-issues': () => openIssues()
});
document.addEventListener('keydown', e => {
  if (e.target && e.target.id === 'fb-title' && e.key === 'Enter' && !e.isComposing) { e.preventDefault(); const b = document.getElementById('fb-body'); if (b) b.focus(); }
});

/* 设置 → 关于 and ⌘K */
SY.diagButtons = () => `<button class="btn btn-sm" type="button" data-act="diag-open">${icon('shield', 14)}诊断信息</button><button class="btn btn-sm" type="button" data-act="feedback">${icon('edit', 14)}反馈问题</button>`;
SY.diagCommands = () => [
  { id: 'feedback', ic: 'edit', title: '反馈问题', sub: '在 GitHub 上提一个 Issue', kw: 'feedback bug issue github 反馈 问题 建议 报错', run: openFeedback },
  { id: 'diag', ic: 'shield', title: '诊断信息', sub: '版本、系统和最近的报错', kw: 'diagnostics debug log 诊断 日志 报错', run: openDiag }
];
})();
