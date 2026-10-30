/* 拾忆 main views: today, ideas, projects, project detail */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, icon, TODAY, WD, ymd, isoWeek, weekStartTs, ageDays, ago, dueLabel, list, getP, pColor, cv, taskSort, taskRow, progBar, pStats, AREA, PROJ_ST, isYmd, addDays, parseYmd, md } = SY;

/* ---------------- today ---------------- */
SY.views.today = {
  render() {
    const today = TODAY(), d = new Date();
    const all = list('tasks'), open = all.filter(t => t.status !== 'done');
    const overdue = open.filter(t => t.due && t.due < today).sort(taskSort);
    const todays = open.filter(t => !(t.due && t.due < today) && (t.todayDate === today || t.due === today)).sort(taskSort);
    // planned for an earlier day and never finished (otherwise they would just vanish)
    const carried = open.filter(t => isYmd(t.todayDate) && t.todayDate < today && !(t.due && t.due <= today)).sort((a, b) => (a.todayDate === b.todayDate ? taskSort(a, b) : a.todayDate < b.todayDate ? 1 : -1));
    const shown = new Set([...overdue, ...todays, ...carried].map(t => t.id));
    const doing = open.filter(t => t.status === 'doing' && !shown.has(t.id)).sort(taskSort);
    const doneToday = all.filter(t => t.status === 'done' && t.doneAt && ymd(new Date(t.doneAt)) === today).sort((a, b) => b.doneAt - a.doneAt);
    const ws = weekStartTs();
    const weekDone = all.filter(t => t.status === 'done' && (t.doneAt || 0) >= ws).length;
    const inbox = list('ideas').filter(i => i.status === 'inbox').sort((a, b) => (b.star ? 1 : 0) - (a.star ? 1 : 0) || (b.createdAt || 0) - (a.createdAt || 0));
    const actives = list('projects').filter(p => p.status === 'active');
    const empty = !all.length && !list('ideas').length && !list('projects').length;
    const head = `<div class="vh"><div><h1>${d.getMonth() + 1}月${d.getDate()}日<small>周${WD[d.getDay()]} · 第 ${isoWeek(d)} 周</small></h1>
      <div class="sub">${SY.hasSample() ? '现在显示的是示例内容，看明白后可以在右侧一键清空。' : '先做最要紧的几件事。'}</div></div>
      <div class="vh-acts">${SY.hasSample() ? '<button class="btn btn-sm" type="button" data-act="clear-samples">清空示例</button>' : ''}<button class="btn btn-ai ai" type="button" data-act="ai-today">${icon('spark')}AI 排今日</button></div></div>
      <div class="tiles">
        <div class="tile clay t-butter"><span class="k">今天要做</span><span class="v">${todays.length + overdue.length}</span><span class="d">含逾期 ${overdue.length} 件</span></div>
        <div class="tile clay t-lav"><span class="k">进行中</span><span class="v">${open.filter(t => t.status === 'doing').length}</span><span class="d">共 ${open.length} 件未完成</span></div>
        <div class="tile clay t-mint"><span class="k">今天完成</span><span class="v">${doneToday.length}</span><span class="d">本周已完成 ${weekDone} 件</span></div>
        <div class="tile clay t-peach"><span class="k">灵感待处理</span><span class="v">${inbox.length}</span><span class="d">收件箱里的想法</span></div>
      </div>`;
    if (empty) {
      return head + `<div class="onboard">
        <div class="clay"><div class="k">01 · 记下</div><h3>先把脑子里的东西倒出来</h3><p>顶部输入框随手记灵感，#标签 自动归类。在任何应用里按 ${SY.keys('⌥⇧Space')} 都能唤出。</p></div>
        <div class="clay"><div class="k">02 · 规划</div><h3>值得做的，变成项目</h3><p>灵感可以一键转项目、用导图展开，或让 AI 拆成任务，再用甘特图和网络图排期。</p></div>
        <div class="clay"><div class="k">03 · 推进</div><h3>每天挑几件放进今日</h3><p>看板拖动推进状态，燃尽图看进度是否赶得上截止日期。</p></div></div>`;
    }
    let main = '';
    if (carried.length) {
      const more = carried.length > 8 ? `<div class="te" style="padding:6px 4px 0">还有 ${carried.length - 8} 件</div>` : '';
      main += `<div class="carried"><h2 class="sh">之前计划、还没做完<span class="n">${carried.length}</span>
        <span class="sh-acts"><button type="button" class="mini" data-act="carry-all">${icon('sun', 13)}全部放进今天</button><button type="button" class="mini" data-act="carry-clear" title="只是不再出现在这里，任务本身不会删除">不再提醒</button></span></h2>
        <div class="tlist">${carried.slice(0, 8).map(t => taskRow(t, { planned: t.todayDate })).join('')}</div>${more}</div>`;
    }
    if (overdue.length) main += `<div><h2 class="sh is-warn">已逾期<span class="n">${overdue.length}</span></h2><div class="tlist">${overdue.map(t => taskRow(t, { resched: true })).join('')}</div></div>`;
    main += `<div><h2 class="sh">今天<span class="n">${todays.length}</span></h2>`;
    if (todays.length) main += `<div class="tlist">${todays.map(t => taskRow(t)).join('')}</div>`;
    else {
      const cand = open.filter(t => !shown.has(t.id)).sort(taskSort).slice(0, 6);
      main += cand.length ? `<div class="cand-lead">今天还没有安排。点任务右侧的 ${icon('sun', 13)} 把它放进今天：</div><div class="tlist">${cand.map(t => taskRow(t)).join('')}</div>`
        : `<div class="empty clay">今天没有安排，也没有积压的待办。切到「任务」记一件要做的事，它会直接出现在这里。</div>`;
    }
    main += `</div>`;
    // the next seven days: due then, or planned for then
    const week = addDays(today, 7), when = t => (isYmd(t.todayDate) && t.todayDate > today ? t.todayDate : isYmd(t.due) ? t.due : null);
    const soon = open.filter(t => !shown.has(t.id) && when(t) && when(t) > today && when(t) <= week).sort((a, b) => (when(a) < when(b) ? -1 : when(a) > when(b) ? 1 : taskSort(a, b)));
    if (soon.length) {
      const up = S.ui.upOpen !== false;
      const dayName = d => (d === addDays(today, 1) ? '明天' : d === addDays(today, 2) ? '后天' : '周' + WD[parseYmd(d).getDay()]) + ' · ' + md(d);
      const days = [];
      for (const t of soon) { const d = when(t); if (!days.length || days[days.length - 1][0] !== d) days.push([d, []]); days[days.length - 1][1].push(t); }
      const body = days.map(([d, ts]) => `<div class="up-day">${dayName(d)}</div><div class="tlist">${ts.map(t => taskRow(t)).join('')}</div>`).join('');
      main += `<div class="upcoming ${up ? '' : 'is-closed'}"><h2 class="sh"><button type="button" class="sh-toggle" data-act="up-toggle" aria-expanded="${up}">${icon('arrow', 12)}接下来 7 天</button><span class="n">${soon.length}</span></h2>${up ? body : ''}</div>`;
    }
    soon.forEach(t => shown.add(t.id));
    if (doing.length) main += `<div><h2 class="sh">其他进行中<span class="n">${doing.length}</span></h2><div class="tlist">${doing.map(t => taskRow(t, { hideDoing: true })).join('')}</div></div>`;
    if (doneToday.length) main += `<div><h2 class="sh">今天完成<span class="n">${doneToday.length}</span></h2><div class="tlist">${doneToday.map(t => taskRow(t)).join('')}</div></div>`;
    const side = `<div class="side-col">
      <div class="panel clay"><h2 class="sh">灵感收件箱<span class="n">${inbox.length}</span><button type="button" class="more" data-act="view" data-v="ideas">全部 →</button></h2>
        ${inbox.slice(0, 6).map(i => `<div class="iline" tabindex="0" data-open="ideas:${esc(i.id)}">${i.star ? `<span style="color:var(--butter)">${icon('star', 14)}</span>` : ''}<p>${SY.richText(i.text)}</p><span class="age">${esc(ago(i.createdAt))}</span></div>`).join('') || '<div class="empty">收件箱是空的。</div>'}</div>
      <div class="panel clay"><h2 class="sh">进行中的项目<span class="n">${actives.length}</span><button type="button" class="more" data-act="view" data-v="projects">全部 →</button></h2>
        ${actives.map(p => { const s = pStats(p.id); return `<div class="pmini" role="button" tabindex="0" data-act="open-project" data-id="${esc(p.id)}"><span class="top"><span class="dot" style="--c:${cv(pColor(p))}"></span><span class="nm">${esc(p.name)}</span><span class="mono">${s.done}/${s.total}</span></span>${progBar(s)}</div>`; }).join('') || '<div class="empty">还没有进行中的项目。</div>'}</div>
    </div>`;
    return head + `<div class="today-grid"><div class="stack">${main}</div>${side}</div>`;
  }
};
/* the one-click helpers on the Today page */
const carriedNow = () => { const today = TODAY(); return list('tasks').filter(t => t.status !== 'done' && isYmd(t.todayDate) && t.todayDate < today && !(t.due && t.due <= today)); };
function nextMonday(today) { const dow = parseYmd(today).getDay(); return addDays(today, ((8 - dow) % 7) || 7); }
Object.assign(SY.acts, {
  'carry-all': () => {
    const ts = carriedNow(); if (!ts.length) return;
    const before = ts.map(t => [t.id, t.todayDate]);
    SY.withNote('放进今天', () => ts.forEach(t => SY.patch('tasks', t.id, { todayDate: TODAY() })));
    SY.toast(`${ts.length} 件放进了今天`, { undo: () => SY.withNote('撤销放进今天', () => before.forEach(([id, d]) => SY.patch('tasks', id, { todayDate: d }))) });
  },
  'carry-clear': () => {
    const ts = carriedNow(); if (!ts.length) return;
    const before = ts.map(t => [t.id, t.todayDate]);
    SY.withNote('不再提醒', () => ts.forEach(t => SY.patch('tasks', t.id, { todayDate: null })));
    SY.toast(`这 ${ts.length} 件不会再出现在这里，任务都还在`, { undo: () => before.forEach(([id, d]) => SY.patch('tasks', id, { todayDate: d })) });
  },
  'resched': el => {
    const t = S.data.tasks.get(el.dataset.id); if (!t) return;
    const today = TODAY(), to = el.dataset.v === 'today' ? today : el.dataset.v === 'tomorrow' ? addDays(today, 1) : nextMonday(today);
    const old = { due: t.due, start: t.start };
    const part = { due: to };
    // keep the length: a task with a start date moves as a whole
    if (isYmd(t.start) && isYmd(t.due)) part.start = addDays(t.start, SY.dayDiff(t.due, to));
    SY.patch('tasks', t.id, part);
    SY.toast(`已改到${el.dataset.v === 'today' ? '今天' : el.dataset.v === 'tomorrow' ? '明天' : '下周一'}（${md(to)}）`, { undo: () => SY.patch('tasks', t.id, old) });
  },
  'up-toggle': () => { S.ui.upOpen = S.ui.upOpen === false; SY.saveUI(); SY.renderAll(); }
});
SY.acts['clear-samples'] = () => {
  const docs = SY.COLS.flatMap(c => list(c).filter(d => d.sample).map(d => [c, d.id]));
  if (S.sel && docs.some(([, id]) => id === S.sel.id)) S.sel = null;
  docs.forEach(([c, id]) => SY.remove(c, id));
  if (S.ui.view === 'project') S.ui.view = 'projects';
  S.ui.mapId = null;
  SY.toast('示例已清空，开始记你自己的吧');
};

/* ---------------- ideas ---------------- */
const IDEA_COL = { inbox: 'peach', incubating: 'lav', converted: 'mint', archived: 'ink-3' };
SY.views.ideas = {
  render() {
    const all = list('ideas');
    const f = S.ui.ideaFilter, tag = S.ui.tag, q = (S.ui.q || '').trim().toLowerCase();
    const cnt = k => (k === 'all' ? all.length : k === 'star' ? all.filter(i => i.star && i.status !== 'archived').length : all.filter(i => i.status === k).length);
    const filters = [['inbox', '收件箱'], ['incubating', '孵化中'], ['star', '星标'], ['converted', '已转化'], ['archived', '归档'], ['all', '全部']];
    let items = all.filter(i => f === 'all' ? true : f === 'star' ? i.star && i.status !== 'archived' : i.status === f);
    const tagCount = new Map();
    items.forEach(i => (i.tags || []).forEach(t => tagCount.set(t, (tagCount.get(t) || 0) + 1)));
    if (tag) items = items.filter(i => (i.tags || []).includes(tag));
    if (q) items = items.filter(i => (i.text + ' ' + (i.note || '') + ' ' + (i.tags || []).join(' ')).toLowerCase().includes(q));
    items.sort((a, b) => (b.createdAt || 0) - (a.createdAt || 0));
    const head = `<div class="vh"><div><h1>灵感</h1><div class="sub">先记下来，再决定值不值得做。在收件箱放了 30 天以上的会标黄提醒你。</div></div></div>
      <div class="toolbar"><div class="chips">${filters.map(([k, l]) => `<button type="button" class="chip ${f === k ? 'is-on' : ''}" data-act="idea-filter" data-v="${k}">${l}<span class="n">${cnt(k)}</span></button>`).join('')}</div>
      <input class="search" type="search" id="q" data-keep="q" placeholder="搜索灵感…" value="${esc(S.ui.q)}" aria-label="搜索灵感"></div>
      ${tagCount.size ? `<div class="tagbar"><span class="lbl">标签</span>${[...tagCount.entries()].sort((a, b) => b[1] - a[1]).map(([t, n]) => `<button type="button" class="tag ${tag === t ? 'is-on' : ''}" data-act="tag" data-v="${esc(t)}">${esc(t)}<span class="n">${n}</span></button>`).join('')}${tag ? '<button type="button" class="mini" data-act="tag" data-v="">清除筛选</button>' : ''}</div>` : ''}`;
    if (!items.length) {
      const msg = all.length ? (q || tag ? '没有符合条件的灵感。' : f === 'inbox' ? '<b>收件箱清空了。</b>新想法从顶部输入框记下就会出现在这里。' : '这里还没有内容。')
        : '<b>还没有灵感。</b>在顶部输入框写下脑子里闪过的任何念头，回车保存。用 #标签 可以顺手归类。';
      return head + `<div class="empty">${msg}</div>`;
    }
    const bucket = ts => { const td = new Date(); td.setHours(0, 0, 0, 0); if (ts >= td.getTime()) return '今天'; if (ts >= td.getTime() - 864e5) return '昨天'; if (ts >= weekStartTs()) return '本周'; if (ageDays(ts) < 31) return '本月'; return '更早'; };
    const groups = [];
    for (const i of items) { const b = bucket(i.createdAt || 0); if (!groups.length || groups[groups.length - 1][0] !== b) groups.push([b, []]); groups[groups.length - 1][1].push(i); }
    return head + groups.map(([b, arr]) => `<div class="group-h">${b}<span class="n">${arr.length}</span></div><div class="tlist ilist">${arr.map(ideaRow).join('')}</div>`).join('');
  }
};
/* one idea per row (fills a wide window; quicker to triage than cards) */
function ideaRow(i) {
  const d = ageDays(i.createdAt), stale = i.status === 'inbox' && d >= 30, p = getP(i.projectId);
  const st = i.status === 'inbox' ? '' : i.status === 'converted' ? '<span class="pill pill-ok">已转化</span>' : i.status === 'incubating' ? '<span class="pill pill-lav">孵化中</span>' : '<span class="pill">归档</span>';
  const acts = i.status === 'archived' ? `<button type="button" class="mini" data-act="idea-status" data-v="inbox" data-id="${esc(i.id)}">放回收件箱</button>`
    : i.status === 'converted' ? `<button type="button" class="mini" data-act="idea-to-map" data-id="${esc(i.id)}" title="用导图展开">${icon('mind', 13)}<span class="al">导图</span></button>`
    : `<button type="button" class="mini is-ai ai" data-act="ai-idea" data-id="${esc(i.id)}">${icon('spark', 13)}AI 拆解</button>
       <button type="button" class="mini" data-act="idea-to-map" data-id="${esc(i.id)}" title="用导图展开">${icon('mind', 13)}<span class="al">导图</span></button>
       <button type="button" class="mini" data-act="idea-to-task" data-id="${esc(i.id)}" title="转任务">${icon('task', 13)}<span class="al">转任务</span></button>
       <button type="button" class="mini" data-act="idea-to-project" data-id="${esc(i.id)}" title="转项目">${icon('folder', 13)}<span class="al">转项目</span></button>
       ${i.status === 'inbox' ? `<button type="button" class="mini" data-act="idea-status" data-v="incubating" data-id="${esc(i.id)}" title="先放着孵化">${icon('bulb', 13)}<span class="al">孵化</span></button>` : ''}
       <button type="button" class="mini" data-act="idea-status" data-v="archived" data-id="${esc(i.id)}" title="归档" aria-label="归档">${icon('archive', 13)}</button>`;
  return `<div tabindex="0" class="trow irow ${SY.isSel('ideas', i.id) ? 'is-sel' : ''} ${i.status === 'archived' ? 'is-faded' : ''}" data-open="ideas:${esc(i.id)}" style="--c:${cv(IDEA_COL[i.status] || 'lav')}">
    <button type="button" class="star ${i.star ? 'is-on' : ''}" data-act="star" data-id="${esc(i.id)}" aria-label="${i.star ? '取消星标' : '加星标'}">${icon('star')}</button>
    <div class="trow-main"><p class="idea-text">${SY.richText(i.text)}</p>
      <div class="meta">${(i.tags || []).map(t => `<span class="tag">${esc(t)}</span>`).join('')}<span class="age ${stale ? 'is-stale' : ''}">${stale ? '已放 ' + d + ' 天' : esc(ago(i.createdAt))}</span>${st}
        ${p ? `<button type="button" class="linkish" data-act="open-project" data-id="${esc(p.id)}">→ ${esc(p.name)}</button>` : ''}${i.note ? '<span>有补充</span>' : ''}</div></div>
    <div class="row-acts">${acts}</div></div>`;
}
Object.assign(SY.acts, {
  'idea-filter': el => { S.ui.ideaFilter = el.dataset.v; S.ui.tag = ''; SY.saveUI(); SY.renderAll(); },
  'tag': el => { S.ui.tag = S.ui.tag === el.dataset.v ? '' : el.dataset.v; SY.renderAll(); },
  'idea-status': el => { const id = el.dataset.id, v = el.dataset.v; const old = S.data.ideas.get(id); if (!old) return; SY.patch('ideas', id, { status: v }); if (v === 'archived') SY.toast('已归档', { undo: () => SY.patch('ideas', id, { status: old.status }) }); },
  'idea-to-task': el => {
    const i = S.data.ideas.get(el.dataset.id); if (!i) return;
    const title = i.text.split('\n')[0].slice(0, 120);
    const t = SY.newTask({ title, note: [i.text.length > 120 || i.text.includes('\n') ? i.text : '', i.note || ''].filter(Boolean).join('\n\n'), fromIdea: i.id });
    SY.patch('ideas', i.id, { status: 'converted', taskId: t.id });
    SY.toast('已转为零散任务，可在详情里改项目', { undo: () => { SY.remove('tasks', t.id); SY.patch('ideas', i.id, { status: i.status, taskId: null }); } });
    SY.select('tasks', t.id);
  },
  'idea-to-project': el => {
    const i = S.data.ideas.get(el.dataset.id); if (!i) return;
    const p = SY.newProject({ name: i.text.split('\n')[0].slice(0, 40), area: /生活|家|旅|骑|健身|读书|运动|购物|life/i.test((i.tags || []).join(' ') + i.text) ? 'life' : 'work', note: [i.text, i.note || ''].filter(Boolean).join('\n\n'), fromIdea: i.id });
    SY.patch('ideas', i.id, { status: 'converted', projectId: p.id });
    SY.goProject(p.id, 'board'); SY.select('projects', p.id); SY.toast('已建成项目，补一句完成标准吧');
  }
});

/* ---------------- projects ---------------- */
SY.views.projects = {
  render() {
    const area = S.ui.projArea, st = S.ui.projStatus, all = list('projects');
    const ps = all.filter(p => (area === 'all' || p.area === area) && (st === 'all' || p.status === st));
    const head = `<div class="vh"><div><h1>项目</h1><div class="sub">工作和生活里正在推进的事。一周没动静的项目会标黄。</div></div>
      <div class="vh-acts"><button class="btn btn-primary" type="button" data-act="new-project">${icon('plus')}新建项目</button></div></div>
      <div class="toolbar"><div class="seg">${[['all', '全部'], ['work', '工作'], ['life', '生活']].map(([k, l]) => `<button type="button" class="${area === k ? 'is-on' : ''}" data-act="proj-area" data-v="${k}">${l}</button>`).join('')}</div>
      <div class="chips">${[['active', '进行中'], ['paused', '暂停'], ['done', '已完成'], ['all', '全部']].map(([k, l]) => `<button type="button" class="chip ${st === k ? 'is-on' : ''}" data-act="proj-status" data-v="${k}">${l}<span class="n">${all.filter(p => (area === 'all' || p.area === area) && (k === 'all' || p.status === k)).length}</span></button>`).join('')}</div></div>`;
    const cards = ps.sort((a, b) => (a.area || '').localeCompare(b.area || '') || (a.due || '9999').localeCompare(b.due || '9999')).map(pcard).join('');
    const ls = pStats('_loose');
    const loose = (st === 'active' || st === 'all') ? `<div class="pcard clay" role="button" tabindex="0" data-act="open-project" data-id="_loose" style="--c:var(--ink-3)">
      <div class="pcard-h"><span class="dot"></span><span class="nm">零散任务</span></div><p class="goal">交费、买东西、回个消息这类不属于任何项目的日常小事</p>
      <div class="prog">${progBar(ls)}<span class="mono">${ls.done}/${ls.total}</span></div>
      <div class="next">${ls.next ? `<span class="k">下一步</span>${esc(ls.next.title)}` : '<span class="k">没有待办</span>'}</div><div class="foot"><span>${ls.open} 件待办</span></div></div>` : '';
    if (!all.length) return head + `<div class="empty-big clay"><h3>还没有项目</h3><p>点「新建项目」，或者在灵感里点「转项目」，把一个想法升级成项目。</p><button class="btn btn-primary" type="button" data-act="new-project">${icon('plus')}新建项目</button></div><div class="pgrid">${loose}</div>`;
    return head + `<div class="pgrid">${cards}${loose}</div>${!ps.length ? '<div class="empty">这个筛选下没有项目。</div>' : ''}`;
  }
};
function pcard(p) {
  const s = pStats(p.id), idle = ageDays(s.last), stale = p.status === 'active' && idle >= 7;
  return `<div class="pcard clay" role="button" tabindex="0" data-act="open-project" data-id="${esc(p.id)}" style="--c:${cv(pColor(p))}">
    <div class="pcard-h"><span class="dot" style="--c:${cv(pColor(p))}"></span><span class="nm">${esc(p.name)}</span><span class="pill">${AREA[p.area] || '工作'}</span>${p.status !== 'active' ? `<span class="pill ${p.status === 'done' ? 'pill-ok' : ''}">${PROJ_ST[p.status]}</span>` : ''}</div>
    <p class="goal">${p.goal ? esc(p.goal) : '<span style="color:var(--ink-3)">还没写完成标准</span>'}</p>
    <div class="prog">${progBar(s)}<span class="mono">${s.done}/${s.total}</span></div>
    <div class="next">${s.next ? `<span class="k">下一步</span>${esc(s.next.title)}` : `<span class="k">${s.total ? '任务都完成了' : '还没有任务'}</span>`}</div>
    <div class="foot"><span class="mono">${p.due ? '截止 ' + esc(p.status === 'done' ? md(p.due) : dueLabel(p.due)) : '无截止'}</span><span class="${stale ? 'is-stale' : ''}">${stale ? idle + ' 天没动' : '更新于 ' + esc(ago(s.last))}</span></div></div>`;
}
Object.assign(SY.acts, {
  'proj-area': el => { S.ui.projArea = el.dataset.v; SY.saveUI(); SY.renderAll(); },
  'proj-status': el => { S.ui.projStatus = el.dataset.v; SY.saveUI(); SY.renderAll(); }
});

/* ---------------- project detail ---------------- */
const PTABS = [['board', '看板', 'kanban'], ['gantt', '甘特图', 'gantt'], ['burndown', '燃尽图', 'burndown'], ['pert', '网络图', 'pert']];
const curTab = () => PTABS.find(x => x[0] === S.ui.ptab) || PTABS[0];
SY.views.project = {
  canvas: () => curTab()[0] === 'pert',
  render() {
    const pid = S.ui.projectId, loose = pid === '_loose', p = loose ? null : getP(pid);
    if (!loose && !p) { S.ui.view = 'projects'; return SY.views.projects.render(); }
    const s = pStats(pid), tab = curTab(), mod = SY.mods[tab[2]];
    const idea = p && p.fromIdea ? S.data.ideas.get(p.fromIdea) : null;
    const head = `<div class="phead"><div><button type="button" class="back" data-act="view" data-v="projects">${icon('back', 14)}全部项目</button>
      <div class="vh"><div><h1><span class="dot" style="--c:${cv(loose ? 'ink-3' : pColor(p))};display:inline-block;width:14px;height:14px;border-radius:5px;margin-right:10px;vertical-align:2px"></span>${loose ? '零散任务' : esc(p.name)}</h1></div>
      <div class="vh-acts">${loose ? '' : `<button class="btn btn-ai ai" type="button" data-act="ai-project" data-id="${esc(p.id)}">${icon('spark')}AI 补任务</button><button class="btn" type="button" data-act="edit-project" data-id="${esc(p.id)}">${icon('edit', 14)}编辑项目</button>`}</div></div></div>
      ${loose ? '<p class="goal">不属于任何项目的日常小事。</p>' : `<div class="row"><span class="pill">${AREA[p.area] || '工作'}</span><span class="pill ${p.status === 'done' ? 'pill-ok' : p.status === 'paused' ? '' : 'pill-lav'}">${PROJ_ST[p.status] || '进行中'}</span>
        ${p.start ? `<span class="due">${icon('cal', 12)}开始 ${esc(p.start)}</span>` : ''}${p.due ? `<span class="due ${p.status !== 'done' && p.due < TODAY() ? 'is-over' : ''}">${icon('cal', 12)}截止 ${esc(p.due)}${p.status === 'done' ? '' : `（${esc(dueLabel(p.due))}）`}</span>` : ''}<span class="mono" style="color:var(--ink-3);font-size:12px">更新于 ${esc(ago(s.last))}</span></div>
        ${p.goal ? `<p class="goal"><b style="color:var(--ink)">完成标准：</b>${esc(p.goal)}</p>` : '<p class="goal" style="color:var(--ink-3)">还没写完成标准。点「编辑项目」补一句「做到什么程度算完成」。</p>'}
        ${idea ? `<div class="src">源自灵感：<button type="button" class="linkish" data-open="ideas:${esc(idea.id)}">${esc(idea.text.length > 48 ? idea.text.slice(0, 48) + '…' : idea.text)}</button></div>` : ''}`}
      <div class="prog">${progBar(s)}<span class="mono">${s.done}/${s.total} 完成</span></div></div>
      <div class="toolbar"><div class="seg">${PTABS.map(([k, l]) => `<button type="button" class="${tab[0] === k ? 'is-on' : ''}" data-act="tab" data-v="${k}">${l}</button>`).join('')}</div>${mod.tools ? mod.tools(pid, true) : ''}</div>`;
    return head + mod.body(pid, true);
  },
  after(root) { const m = SY.mods[curTab()[2]]; if (m.after) m.after(root, S.ui.projectId); },
  before(root) { const m = SY.mods[curTab()[2]]; if (m.before) m.before(root, S.ui.projectId); },
  key(e) { const m = SY.mods[curTab()[2]]; return m.key ? m.key(e) : false; }
};
})();
