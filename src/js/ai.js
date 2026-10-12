/* 拾忆 AI flows: every result is previewed and edited before it is written */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, list, getP, TODAY, WD, AREA, TASK_ST, scopeTasks, taskSort, prioTag, dueTag, now, $ } = SY;
let runId = 0, timer = 0;
const todayCtx = () => `${TODAY()}（周${WD[new Date().getDay()]}）`;
const prioSel = (name, v) => `<select class="ctl" name="${name}" aria-label="优先级" style="height:30px">${[[1, 'P1'], [2, 'P2'], [3, 'P3'], [0, '—']].map(([k, l]) => `<option value="${k}" ${Number(v) === k ? 'selected' : ''}>${l}</option>`).join('')}</select>`;
const clampP = v => ([1, 2, 3].includes(Number(v)) ? Number(v) : 2);
const numOr = (v, d) => { const n = Number(v); return isFinite(n) && n >= 0 ? Math.round(n * 2) / 2 : d; };

SY.runAI = async cfg => {
  const my = ++runId; clearInterval(timer);
  SY.openModal(cfg.title, `<div class="ai-wait"><span class="spin"></span><div><b>AI 正在思考…</b><p>${esc(cfg.wait || '通常需要 10–40 秒。')}</p><p class="mono" id="ai-el" style="color:var(--ink-3)">0 秒</p></div></div>`,
    `<button class="btn" type="button" data-act="close-modal">停止</button>`, { wide: cfg.wide });
  const t0 = Date.now(); timer = setInterval(() => { const el = $('#ai-el'); if (el) el.textContent = Math.round((Date.now() - t0) / 1000) + ' 秒'; }, 1000);
  SY.onModalClose = () => { runId++; clearInterval(timer); };
  SY.aiRetry = () => SY.runAI(cfg); SY.aiApply = null; SY.aiTitle = cfg.title.split(' · ')[0].replace(/^AI\s*/, '');
  try {
    const text = await SY.aiCall(cfg.prompt, cfg.quick);
    if (my !== runId) return; clearInterval(timer);
    const res = SY.parseJSON(text);
    const html = cfg.render(res);
    if (!html) { $('#m-body').innerHTML = '<p class="ai-err">没有得到可用的建议。补充一些描述后再试。</p>'; $('#m-foot').innerHTML = `<button class="btn" type="button" data-act="close-modal">关闭</button><button class="btn btn-primary" type="button" data-act="ai-retry">重试</button>`; return; }
    SY.aiApply = cfg.apply;
    $('#m-body').innerHTML = html;
    $('#m-foot').innerHTML = `<button class="btn btn-ghost" type="button" data-act="ai-retry">${icon('reopen', 14)}换一版</button><span style="flex:1"></span><button class="btn" type="button" data-act="close-modal">取消</button><button class="btn btn-primary" type="button" data-act="ai-apply">${esc(cfg.applyLabel || '应用')}</button>`;
  } catch (e) {
    if (my !== runId) return; clearInterval(timer);
    $('#m-body').innerHTML = `<p class="ai-err">${esc(SY.aiErrText(e))}</p>${e && e.code === 'invalid_json' && e.text ? `<details><summary style="font-size:12px;color:var(--ink-3)">查看原始回复</summary><pre style="white-space:pre-wrap;font-size:12px">${esc(String(e.text).slice(0, 2000))}</pre></details>` : ''}`;
    $('#m-foot').innerHTML = e && (e.code === 'no_key' || e.code === 'api')
      ? `<button class="btn" type="button" data-act="close-modal">关闭</button><button class="btn" type="button" data-act="settings">${icon('gear', 14)}打开设置</button>${e.code === 'api' ? '<button class="btn btn-primary" type="button" data-act="ai-retry">重试</button>' : ''}`
      : `<button class="btn" type="button" data-act="close-modal">关闭</button>${e && e.code !== 'no_backend' ? '<button class="btn btn-primary" type="button" data-act="ai-retry">重试</button>' : ''}`;
  }
};
SY.acts['ai-apply'] = () => { if (SY.aiApply) SY.withNote('AI · ' + (SY.aiTitle || '建议'), SY.aiApply); };
SY.acts['ai-retry'] = () => { if (SY.aiRetry) SY.aiRetry(); };

/* -------- idea → project + tasks -------- */
SY.acts['ai-idea'] = el => {
  const i = S.data.ideas.get(el.dataset.id); if (!i) return;
  const actives = list('projects').filter(p => p.status === 'active');
  const prompt = `你是一位务实的项目规划助手。用户在个人「灵感 + 项目管理」桌面应用里记了一条灵感，请把它拆成一个可以马上开始的小项目。
今天是 ${todayCtx()}。

灵感：${i.text}
${i.note ? '用户的补充：' + i.note + '\n' : ''}${(i.tags || []).length ? '标签：' + i.tags.join('、') + '\n' : ''}用户正在进行的项目：${actives.map(p => p.name).join('；') || '（无）'}

要求：
- name：项目名，不超过 16 个汉字，具体。
- area：只能是 "work" 或 "life"。
- goal：一句话完成标准，必须可验证。
- tasks：3 到 8 条，按执行顺序；每条以动词开头，单条 0.5 到 5 天能完成；第一条要今天就能动手。
  - priority：1=高 2=中 3=低。
  - days：最可能需要的天数（可以是 0.5 的倍数）。
  - after：这条任务要等哪些任务先完成，填它们在数组里的序号（从 0 开始），没有就填 []。
- existingProject：如果这条灵感明显应该并入上面某个正在进行的项目，填那个项目的完整名字，否则填 null。
- 用简体中文。

只回复 JSON：{"name":"...","area":"work","goal":"...","existingProject":null,"tasks":[{"title":"...","priority":2,"days":1,"after":[]}]}`;
  SY.runAI({
    title: 'AI 拆解灵感', prompt, applyLabel: '创建', wide: true,
    render(r) {
      const ts = (r && Array.isArray(r.tasks) ? r.tasks : []).filter(x => x && x.title).slice(0, 10);
      if (!ts.length) return '';
      const match = actives.find(p => r.existingProject && p.name === r.existingProject);
      return `<div class="ai-note">${esc(i.text.length > 120 ? i.text.slice(0, 120) + '…' : i.text)}</div>
      <form id="ai-form" class="ai-f">
        <label class="f">放到<select class="in" name="target"><option value="_new">新建项目</option>${actives.map(p => `<option value="${esc(p.id)}" ${match && match.id === p.id ? 'selected' : ''}>并入：${esc(p.name)}</option>`).join('')}</select></label>
        <div class="ai-grid new-only" ${match ? 'hidden' : ''}><label>项目名<input class="in" name="name" value="${esc(String(r.name || '').slice(0, 40))}"></label>
          <label>领域<select class="in" name="area"><option value="work" ${r.area !== 'life' ? 'selected' : ''}>工作</option><option value="life" ${r.area === 'life' ? 'selected' : ''}>生活</option></select></label></div>
        <label class="f new-only" ${match ? 'hidden' : ''}>完成标准<input class="in" name="goal" value="${esc(String(r.goal || ''))}"></label>
        <div><div style="font-size:12px;color:var(--ink-3);margin-bottom:8px;font-weight:600">任务（取消勾选的不会创建；标题、优先级、工期都可以直接改）</div>
        <div class="ai-list">${ts.map((t, k) => `<div class="ai-item" style="grid-template-columns:22px minmax(0,1fr) auto 70px"><input type="checkbox" name="pick" value="${k}" checked aria-label="选用"><input class="in" name="t${k}" value="${esc(String(t.title).slice(0, 140))}" aria-label="任务标题">${prioSel('p' + k, clampP(t.priority))}<input class="in mono" name="d${k}" value="${numOr(t.days, 1)}" aria-label="工期（天）" title="工期（天）" style="text-align:center">${Array.isArray(t.after) && t.after.length ? `<div class="why">等待：${t.after.map(n => ts[n] ? esc(String(ts[n].title).slice(0, 24)) : '').filter(Boolean).join('、')}</div>` : ''}<input type="hidden" name="a${k}" value="${esc((Array.isArray(t.after) ? t.after : []).filter(n => Number.isInteger(n) && n >= 0 && n < k).join(','))}"></div>`).join('')}</div></div>
        <label style="display:flex;gap:8px;align-items:center;font-size:13px"><input type="checkbox" name="sched" checked> 按依赖和工期自动排出日期（之后在甘特图里可以拖动调整）</label>
      </form>`;
    },
    apply() {
      const fd = new FormData($('#ai-form')), picks = fd.getAll('pick').map(Number);
      let pid = fd.get('target'); const isNew = pid === '_new';
      if (isNew) pid = SY.newProject({ name: String(fd.get('name') || '').trim() || i.text.slice(0, 40), area: fd.get('area') === 'life' ? 'life' : 'work', goal: String(fd.get('goal') || '').trim(), note: [i.text, i.note || ''].filter(Boolean).join('\n\n'), fromIdea: i.id }).id;
      const idOf = {}, base = now();
      picks.forEach((k, n) => { const title = String(fd.get('t' + k) || '').trim(); if (!title) return; const t = SY.newTask({ title, projectId: pid, priority: Number(fd.get('p' + k)) || 0, estM: numOr(fd.get('d' + k), 1), createdAt: base + n }); idOf[k] = t.id; });
      picks.forEach(k => { if (!idOf[k]) return; String(fd.get('a' + k) || '').split(',').filter(Boolean).map(Number).forEach(a => { if (idOf[a]) SY.addDep(idOf[k], idOf[a]); }); });
      SY.patch('ideas', i.id, { status: 'converted', projectId: pid });
      if (fd.get('sched')) SY.autoSchedule(pid);
      SY.closeModal(); S.sel = null; SY.goProject(pid, 'board');
      SY.toast(isNew ? `已创建项目和 ${Object.keys(idOf).length} 个任务` : `已并入项目，新增 ${Object.keys(idOf).length} 个任务`);
    }
  });
};
SY.changeHandlers.push(e => {
  if (e.target.name === 'target' && e.target.closest('#ai-form')) { const isNew = e.target.value === '_new'; document.querySelectorAll('#ai-form .new-only').forEach(n => { n.hidden = !isNew; }); }
});

/* -------- project: fill missing tasks -------- */
SY.acts['ai-project'] = el => {
  const p = getP(el.dataset.id); if (!p) return;
  const ts = scopeTasks(p.id).sort(taskSort);
  const prompt = `你是一位务实的项目推进教练。下面是用户的一个项目和已有任务，请补充推进目标最关键、但还没被覆盖的下一步任务。
今天是 ${todayCtx()}。

项目：${p.name}（${AREA[p.area] || '工作'}）
完成标准：${p.goal || '（未填写）'}
${p.note ? '项目笔记：' + p.note.slice(0, 1500) + '\n' : ''}已有任务：
${ts.map(t => `- [${TASK_ST[t.status]}] ${t.title}`).join('\n') || '（暂无）'}

要求：3 到 6 条新任务，不要和已有任务重复；每条以动词开头、半天到 5 天能完成；按建议的执行顺序排列；priority 1=高 2=中 3=低；days 为最可能的天数；why 用不超过 20 个字说明为什么需要这一步。用简体中文。
只回复 JSON：{"tasks":[{"title":"...","priority":2,"days":1,"why":"..."}]}`;
  SY.runAI({
    title: 'AI 补全任务 · ' + p.name, prompt, applyLabel: '添加到待办',
    render(r) {
      const arr = (r && Array.isArray(r.tasks) ? r.tasks : []).filter(x => x && x.title).slice(0, 8);
      if (!arr.length) return '';
      return `<form id="ai-form" class="ai-f"><div class="ai-list">${arr.map((t, k) => `<div class="ai-item" style="grid-template-columns:22px minmax(0,1fr) auto 64px"><input type="checkbox" name="pick" value="${k}" checked aria-label="选用"><input class="in" name="t${k}" value="${esc(String(t.title).slice(0, 140))}" aria-label="任务标题">${prioSel('p' + k, clampP(t.priority))}<input class="in mono" name="d${k}" value="${numOr(t.days, 1)}" title="工期（天）" aria-label="工期（天）" style="text-align:center">${t.why ? `<div class="why">${esc(String(t.why).slice(0, 60))}</div>` : ''}</div>`).join('')}</div></form>`;
    },
    apply() {
      const fd = new FormData($('#ai-form')), picks = fd.getAll('pick').map(Number), base = now();
      picks.forEach((k, n) => { const title = String(fd.get('t' + k) || '').trim(); if (title) SY.newTask({ title, projectId: p.id, priority: Number(fd.get('p' + k)) || 0, estM: numOr(fd.get('d' + k), 1), createdAt: base + n }); });
      SY.closeModal(); SY.toast(`已添加 ${picks.length} 个任务`);
    }
  });
};

/* -------- today plan -------- */
SY.acts['ai-today'] = () => {
  const today = TODAY();
  const open = list('tasks').filter(t => t.status !== 'done').sort(taskSort).slice(0, 80);
  if (!open.length) { SY.toast('没有未完成的任务可以安排。'); return; }
  const lines = open.map(t => { const p = getP(t.projectId); return `${t.id} | ${t.title} | ${p ? p.name + '/' + (AREA[p.area] || '') : '零散'} | ${TASK_ST[t.status]} | ${t.priority ? 'P' + t.priority : '无优先级'} | ${t.due ? '截止 ' + t.due : '无截止'}${t.start ? ' | 计划开始 ' + t.start : ''}${t.todayDate === today ? ' | 已在今日' : ''}`; }).join('\n');
  const prompt = `你是用户的每日计划助手。今天是 ${todayCtx()}。下面是用户所有未完成的任务，每行格式：id | 标题 | 项目/领域 | 状态 | 优先级 | 截止 | 计划开始。

${lines}

请选出今天最该做的 3 到 5 件：优先已逾期和今天到期的、计划今天开始的、高优先级的、已经在进行中的；兼顾工作与生活，总量控制在一个普通工作日能完成。
why 用不超过 18 个字说明理由。note 用一句话给出今天的节奏建议。用简体中文。
只回复 JSON：{"picks":[{"id":"任务id","why":"..."}],"note":"..."}`;
  SY.runAI({
    title: 'AI 排今日', prompt, quick: true, wait: '一般几秒钟。', applyLabel: '放进今日',
    render(r) {
      const picks = (r && Array.isArray(r.picks) ? r.picks : []).filter(x => x && S.data.tasks.has(String(x.id))).slice(0, 7);
      if (!picks.length) return '';
      return `${r.note ? `<div class="ai-note">${esc(String(r.note).slice(0, 200))}</div>` : ''}<form id="ai-form" class="ai-f"><div class="ai-list">${picks.map(x => { const t = S.data.tasks.get(String(x.id)), p = getP(t.projectId);
        return `<label class="ai-item"><input type="checkbox" name="pick" value="${esc(t.id)}" checked><span style="font-weight:600">${esc(t.title)}</span><span class="meta" style="margin:0">${prioTag(t.priority)}${dueTag(t)}</span><span class="why">${p ? esc(p.name) + ' · ' : ''}${esc(String(x.why || '').slice(0, 50))}</span></label>`; }).join('')}</div></form>`;
    },
    apply() {
      const ids = new FormData($('#ai-form')).getAll('pick').map(String);
      ids.forEach(id => { const t = S.data.tasks.get(id); if (t && t.todayDate !== today) SY.patch('tasks', id, { todayDate: today }); });
      SY.closeModal(); if (S.ui.view !== 'today') SY.setView('today'); SY.toast(`已把 ${ids.length} 件事放进今日`);
    }
  });
};

/* -------- estimates + dependencies for PERT / Gantt -------- */
SY.acts['ai-schedule'] = el => {
  const p = getP(el.dataset.id); if (!p) return;
  const ts = scopeTasks(p.id).filter(t => t.status !== 'done').sort(taskSort);
  if (ts.length < 2) { SY.toast('至少要有两件未完成的任务才值得排依赖。'); return; }
  const done = scopeTasks(p.id).filter(t => t.status === 'done');
  const lines = ts.map(t => `${t.id} | ${t.title} | ${TASK_ST[t.status]} | 现有估算 ${t.estM != null ? `${t.estO ?? '?'}/${t.estM}/${t.estP ?? '?'} 天` : '无'} | 现有前置 ${(t.deps || []).map(d => S.data.tasks.get(d)?.title).filter(Boolean).join('、') || '无'}`).join('\n');
  const prompt = `你是一位经验丰富的项目经理，熟悉 PERT 三点估算。请为下面这个项目的未完成任务给出工期估算和依赖关系。
今天是 ${todayCtx()}。

项目：${p.name}
完成标准：${p.goal || '（未填写）'}
${done.length ? '已完成：' + done.map(t => t.title).join('；') + '\n' : ''}未完成任务（id | 标题 | 状态 | 现有估算 乐观/最可能/悲观 | 现有前置）：
${lines}

要求：
- 对每个任务给出 o（乐观）、m（最可能）、p（悲观）天数，满足 o ≤ m ≤ p，可以用 0.5 的倍数；一个人做、按自然日估算。
- deps：这个任务开始前必须完成的其他任务 id（只能用上面列出的 id），只写真正的硬依赖，不要形成循环。可以参考或修正现有前置。
- note：一句话指出最大的进度风险。
- 用简体中文。

只回复 JSON：{"tasks":[{"id":"...","o":1,"m":2,"p":4,"deps":["..."]}],"note":"..."}`;
  SY.runAI({
    title: 'AI 估算工期与依赖 · ' + p.name, prompt, applyLabel: '应用', wide: true,
    render(r) {
      const valid = new Set(ts.map(t => t.id));
      const arr = (r && Array.isArray(r.tasks) ? r.tasks : []).filter(x => x && valid.has(String(x.id)));
      if (!arr.length) return '';
      return `${r.note ? `<div class="ai-note"><b>风险：</b>${esc(String(r.note).slice(0, 200))}</div>` : ''}
      <form id="ai-form" class="ai-f"><table class="sched"><thead><tr><th></th><th>任务</th><th>乐观</th><th>最可能</th><th>悲观</th><th>前置任务</th></tr></thead><tbody>
      ${arr.map((x, k) => { const t = S.data.tasks.get(String(x.id)); const o = numOr(x.o, 1), m = numOr(x.m, Math.max(o, 1)), pp = numOr(x.p, Math.max(m, 1)); const deps = (Array.isArray(x.deps) ? x.deps : []).map(String).filter(d => valid.has(d) && d !== t.id);
        return `<tr><td><input type="checkbox" name="pick" value="${k}" checked aria-label="应用"></td><td style="font-weight:600">${esc(t.title)}<input type="hidden" name="id${k}" value="${esc(t.id)}"><input type="hidden" name="dp${k}" value="${esc(deps.join(','))}"></td>
          <td><input class="in mono" name="o${k}" value="${o}"></td><td><input class="in mono" name="m${k}" value="${m}"></td><td><input class="in mono" name="p${k}" value="${pp}"></td>
          <td class="dl">${deps.map(d => esc(S.data.tasks.get(d).title.slice(0, 18))).join('、') || '—'}</td></tr>`; }).join('')}
      </tbody></table>
      <label style="display:flex;gap:8px;align-items:center;font-size:13px"><input type="checkbox" name="sched" checked> 应用后按依赖从今天起重新排出开始和截止日期</label></form>`;
    },
    apply() {
      const fd = new FormData($('#ai-form')), picks = fd.getAll('pick').map(Number);
      const proposed = new Map();
      picks.forEach(k => {
        const id = String(fd.get('id' + k));
        let o = numOr(fd.get('o' + k), 1), m = numOr(fd.get('m' + k), 1), pp = numOr(fd.get('p' + k), 1);
        m = Math.max(o, m); pp = Math.max(m, pp);
        SY.patch('tasks', id, { estO: o, estM: m, estP: pp, deps: [] });
        proposed.set(id, String(fd.get('dp' + k) || '').split(',').filter(Boolean));
      });
      let dropped = 0;
      proposed.forEach((deps, id) => deps.forEach(d => { if (!SY.canAddDep(id, d)) { dropped++; return; } const t = S.data.tasks.get(id); SY.patch('tasks', id, { deps: [...new Set([...(t.deps || []), d])] }); }));
      if (fd.get('sched')) SY.autoSchedule(p.id, { force: true });
      SY.closeModal();
      SY.toast(`已更新 ${picks.length} 个任务的估算和依赖${dropped ? `，跳过 ${dropped} 条会形成循环的依赖` : ''}`);
    }
  });
};

/* -------- mind map: expand selected node -------- */
SY.acts['ai-mm'] = () => {
  const m = S.data.maps.get(S.ui.mapId); if (!m) return;
  const N = m.nodes || {}, id = SY.mmSel() && N[SY.mmSel()] ? SY.mmSel() : m.rootId, n = N[id];
  const path = []; let x = n; while (x) { path.unshift(x.text || '（未命名）'); x = N[x.parent]; }
  const kids = n.children.map(c => N[c] && N[c].text).filter(Boolean);
  const prompt = `你在帮用户用思维导图发散想法。导图主题是「${m.title}」。
当前节点的路径：${path.join(' › ')}
当前节点已有的子节点：${kids.join('；') || '（无）'}

请为当前节点再补充 4 到 6 个有价值、彼此不重复、也不和已有子节点重复的分支，每个分支可以再带 0 到 3 个更具体的子点。分支文字不超过 16 个汉字，具体、可执行或可思考，不要空话。用简体中文。
只回复 JSON：{"children":[{"text":"...","children":["...","..."]}]}`;
  SY.runAI({
    title: 'AI 扩展节点 · ' + (n.text || '').slice(0, 20), prompt, quick: true, wait: '一般几秒钟。', applyLabel: '加到导图',
    render(r) {
      const arr = (r && Array.isArray(r.children) ? r.children : []).filter(c => c && c.text).slice(0, 8);
      if (!arr.length) return '';
      return `<div class="ai-note">${esc(path.join(' › '))}</div><form id="ai-form" class="ai-f"><div class="ai-list">${arr.map((c, k) => `<div class="ai-item" style="grid-template-columns:22px minmax(0,1fr)"><input type="checkbox" name="pick" value="${k}" checked aria-label="选用"><input class="in" name="t${k}" value="${esc(String(c.text).slice(0, 60))}">
        ${(Array.isArray(c.children) ? c.children : []).slice(0, 4).map((s, j) => `<label class="why" style="display:flex;gap:6px;align-items:center"><input type="checkbox" name="s${k}" value="${esc(String(s).slice(0, 60))}" checked>${esc(String(s).slice(0, 60))}</label>`).join('')}</div>`).join('')}</div></form>`;
    },
    apply() {
      const fd = new FormData($('#ai-form')), picks = fd.getAll('pick').map(Number);
      SY.mmMutate(nodes => { picks.forEach(k => { const t = String(fd.get('t' + k) || '').trim(); if (!t || !nodes[id]) return; const cid = SY.mmAddChild(nodes, id, t); fd.getAll('s' + k).forEach(s => SY.mmAddChild(nodes, cid, String(s))); }); });
      SY.closeModal(); SY.toast(`已添加 ${picks.length} 个分支`);
    }
  });
};

/* -------- knowledge graph: find links between ideas and projects -------- */
const linkPool = () => [
  ...list('projects').filter(p => p.status !== 'done').map(d => ['projects', d]),
  ...list('ideas').filter(i => i.status !== 'archived').map(d => ['ideas', d]),
  ...list('maps').map(d => ['maps', d])
];
/* user-triggered only: pick the scope, see the size, then send */
SY.acts['ai-links'] = () => {
  const pool = linkPool(), since = S.ui.linkSweepAt || 0;
  if (pool.length < 3) { SY.toast('至少要有三条灵感或项目，才找得出关联。'); return; }
  const fresh = pool.filter(([, d]) => (d.updatedAt || d.createdAt || 0) > since);
  const scopeOf = mode => {
    if (mode === 'all') return { items: pool.slice(0, 150), fresh: new Set() };
    const ids = new Set(fresh.map(([, d]) => d.id)), keep = new Map(fresh.map(x => [x[1].id, x]));
    fresh.forEach(([, d]) => (SY.related ? SY.related(d.id, { limit: 6 }) : []).forEach(h => { const x = pool.find(p => p[1].id === h.id); if (x) keep.set(h.id, x); }));
    pool.filter(([c, d]) => c === 'projects' && d.status === 'active').slice(0, 15).forEach(x => keep.set(x[1].id, x));
    return { items: [...keep.values()].slice(0, 150), fresh: ids };
  };
  const est = sc => Math.round(sc.items.reduce((n, [, d]) => n + String(d.text || d.name || d.title || '').length + String(d.note || d.goal || '').slice(0, 50).length + 12, 900) * 0.9);
  const inc = scopeOf('new'), all = scopeOf('all');
  const k = n => (n >= 1000 ? (n / 1000).toFixed(1) + 'k' : String(n));
  SY.openModal('AI 找关联', `<p style="margin:0 0 10px;color:var(--ink-2);font-size:13px">只有你点「开始」才会调用 AI。发送前先选范围：</p>
    <label class="ai-item" style="grid-template-columns:22px minmax(0,1fr)"><input type="radio" name="lk-scope" value="new" ${fresh.length ? 'checked' : 'disabled'}><span><b>只看${since ? '上次之后' : ''}新增或改过的 ${fresh.length} 条</b>（推荐）<br><span class="te">连同本地算法为它们挑出的候选，共 ${inc.items.length} 条 · 约 ${k(est(inc))} token</span></span></label>
    <label class="ai-item" style="grid-template-columns:22px minmax(0,1fr)"><input type="radio" name="lk-scope" value="all" ${fresh.length ? '' : 'checked'}><span><b>全部 ${all.items.length} 条</b><br><span class="te">约 ${k(est(all))} token${pool.length > 150 ? '（超过 150 条只取最近的）' : ''}</span></span></label>
    ${since ? `<p class="te" style="margin:8px 0 0">上次找关联：${SY.fmtTs(since)}</p>` : ''}`,
    `<button class="btn" type="button" data-act="close-modal">取消</button><button class="btn btn-primary" type="button" data-act="ai-links-go">${SY.icon('spark', 14)}开始</button>`, { iconName: 'spark' });
  SY.acts['ai-links-go'] = () => { const m = (document.querySelector('input[name=lk-scope]:checked') || {}).value || 'all'; runLinks(m === 'new' ? inc : all); };
};
function runLinks(scope) {
  const ideas = scope.items.filter(x => x[0] === 'ideas').map(x => x[1]);
  const projects = scope.items.filter(x => x[0] === 'projects').map(x => x[1]);
  const maps = scope.items.filter(x => x[0] === 'maps').map(x => x[1]);
  const isNew = d => scope.fresh.has(d.id);
  const key = new Map(), back = new Map();
  const k = (prefix, d, i) => { const s = prefix + (i + 1); key.set(d.id, s); back.set(s, d.id); return s; };
  const one = s => String(s || '').replace(/\s+/g, ' ').trim();
  const lines = [
    ...projects.map((p, i) => `${k('p', p, i)}${isNew(p) ? '*' : ''} | 项目 | ${one(p.name)}${p.goal ? ' — ' + one(p.goal).slice(0, 60) : ''}`),
    ...ideas.map((x, i) => `${k('i', x, i)}${isNew(x) ? '*' : ''} | 灵感 | ${one(x.text).slice(0, 90)}${(x.tags || []).length ? ' #' + x.tags.join(' #') : ''}${x.note ? ' — ' + one(x.note).slice(0, 50) : ''}`),
    ...maps.map((m, i) => `${k('m', m, i)}${isNew(m) ? '*' : ''} | 导图 | ${one(m.title)}`)
  ].join('\n');
  const have = list('links').filter(l => key.has(l.source) && key.has(l.target)).map(l => `${key.get(l.source)}-${key.get(l.target)}`);
  const g = SY.graph.build();
  g.edges.filter(e => e.kind !== 'tag' && key.has(e.source) && key.has(e.target)).forEach(e => have.push(`${key.get(e.source)}-${key.get(e.target)}`));
  const RELS = { relates_to: '相关', inspired_by: '受启发于', derived_from: '来源于', part_of: '属于', supports: '支持', contradicts: '矛盾', depends_on: '依赖', similar_to: '相似' };
  const prompt = `你在帮用户整理个人知识图谱。下面是用户的项目、灵感和导图，每行：编号 | 类型 | 内容。

${lines}

已经有关系的对（不要重复）：${[...new Set(have)].join('，') || '无'}

${scope.fresh.size ? '编号后带 * 的是新增或刚改过的条目，每一对关联里至少要有一条带 *（输出编号时去掉 *）。\n' : ''}请找出 4 到 12 对真正有意义、用户自己可能没意识到的关联：同一个问题的不同解法、一个灵感能推进某个项目、两条灵感可以合并、观点相互矛盾等。不要因为都属于“效率”“生活”这种宽泛主题就连起来。
relation 只能是：${Object.keys(RELS).join('、')}。a→b 的方向要读得通，比如「a supports b」「a inspired_by b」。why 用一句话（不超过 30 个字）说明理由。confidence 是 0 到 1 的把握程度。用简体中文。
只回复 JSON：{"links":[{"a":"i3","b":"p1","relation":"supports","why":"...","confidence":0.8}]}`;
  SY.runAI({
    title: 'AI 找关联', prompt, wide: true, applyLabel: '建立关联',
    render(r) {
      const arr = (r && Array.isArray(r.links) ? r.links : []).filter(x => x && back.has(String(x.a)) && back.has(String(x.b)) && x.a !== x.b).slice(0, 16);
      if (!arr.length) return '';
      SY.aiLinkCands = arr.map(x => ({ a: back.get(String(x.a)), b: back.get(String(x.b)), relation: RELS[x.relation] ? x.relation : 'relates_to', why: String(x.why || '').slice(0, 80), confidence: Math.max(0, Math.min(1, Number(x.confidence) || 0.5)) }));
      return `<div class="ai-note">勾选要保留的关联，可以改关系类型。建立后会出现在知识图谱里，也能在「操作记录」里一键撤销。</div>
        <form id="ai-form" class="ai-f"><div class="ai-list">${SY.aiLinkCands.map((c, k) => `<div class="ai-item ai-link"><input type="checkbox" name="pick" value="${k}" ${c.confidence >= 0.45 ? 'checked' : ''} aria-label="选用">
          <span class="al-ends"><b>${esc(SY.titleOfId(c.a))}</b><select class="ctl" name="r${k}" style="height:28px">${Object.entries(RELS).map(([rk, rl]) => `<option value="${rk}" ${rk === c.relation ? 'selected' : ''}>${rl}</option>`).join('')}</select><b>${esc(SY.titleOfId(c.b))}</b></span>
          <span class="why">${esc(c.why)}<span class="mono" style="margin-left:6px;opacity:.6">${Math.round(c.confidence * 100)}%</span></span></div>`).join('')}</div></form>`;
    },
    apply() {
      const fd = new FormData($('#ai-form')), picks = fd.getAll('pick').map(Number);
      let n = 0;
      picks.forEach(k => { const c = SY.aiLinkCands[k]; if (!c) return; const l = SY.createLink(c.a, c.b, String(fd.get('r' + k) || c.relation), c.why, 'confirmed'); if (l) { if (l.confidence == null) SY.patch('links', l.id, { confidence: c.confidence }); n++; } });
      S.ui.linkSweepAt = Date.now(); SY.saveUI();
      SY.closeModal(); SY.toast(`已建立 ${n} 条关联`);
      if (S.ui.view !== 'graph') SY.setView('graph');
    }
  });
};
})();
