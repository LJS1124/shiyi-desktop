/* 十一遗 burndown: remaining work vs ideal line, scope line, velocity projection, daily completions */
(() => {
'use strict';
const SY = window.SY, S = SY.S;
const { esc, getP, scopeTasks, TODAY, addDays, dayDiff, parseYmd, isYmd, tsDay, md, est } = SY;
let cache = null;

function niceStep(max, count = 4) {
  if (max <= 0) return 1;
  const raw = max / count, p = Math.pow(10, Math.floor(Math.log10(raw))), n = raw / p;
  return (n <= 1 ? 1 : n <= 2 ? 2 : n <= 5 ? 5 : 10) * p;
}
const fmt = v => (Math.abs(v - Math.round(v)) < 0.05 ? String(Math.round(v)) : v.toFixed(1));

function compute(scope) {
  const tasks = scopeTasks(scope), p = getP(scope), today = TODAY();
  const w = t => (S.ui.burnMetric === 'est' ? est(t).te : 1);
  const created = t => tsDay(t.createdAt || Date.now());
  let start;
  if (scope === 'all') start = addDays(today, -29);
  else if (p && isYmd(p.start)) start = p.start;
  else start = tasks.map(created).concat(p && p.createdAt ? [tsDay(p.createdAt)] : []).reduce((a, b) => (a < b ? a : b), today);
  if (start > today) start = today;
  if (dayDiff(start, today) > 365) start = addDays(today, -365);
  const n = dayDiff(start, today) + 1, series = [];
  for (let i = 0; i < n; i++) {
    const d = addDays(start, i), endTs = parseYmd(d).getTime() + 864e5 - 1, dayStart = endTs - 864e5 + 1;
    let sc = 0, dn = 0, dd = 0;
    for (const t of tasks) {
      const wt = w(t);
      if ((t.createdAt || 0) <= endTs) sc += wt;
      if (t.status === 'done' && t.doneAt && t.doneAt <= endTs) { dn += wt; if (t.doneAt >= dayStart) dd += wt; }
    }
    series.push({ d, scope: sc, done: dn, rem: Math.max(0, sc - dn), day: dd });
  }
  const last = series[series.length - 1];
  const back = series[Math.max(0, series.length - 8)];
  const span7 = Math.max(1, Math.min(7, series.length - 1));
  const velocity = Math.max(0, (last.done - back.done) / span7);
  const projDays = velocity > 0 ? Math.ceil(last.rem / velocity) : null;
  const projDate = projDays != null && last.rem > 0 ? addDays(today, projDays) : last.rem === 0 ? today : null;
  const due = p && isYmd(p.due) ? p.due : null;
  let end = [today, due || today, projDate && dayDiff(today, projDate) <= 150 ? projDate : today].reduce((a, b) => (a > b ? a : b));
  end = addDays(end, 2);
  return { tasks, p, today, start, end, series, last, velocity, projDate, due };
}

SY.mods.burndown = {
  tools() {
    return `<div class="seg seg-sm">${[['count', '按任务数'], ['est', '按预估工期（天）']].map(([k, l]) => `<button type="button" class="${S.ui.burnMetric === k ? 'is-on' : ''}" data-act="burn-metric" data-v="${k}">${l}</button>`).join('')}</div>`;
  },
  body(scope) {
    const c = cache = compute(scope);
    if (!c.tasks.length) return `<div class="empty-big clay"><h3>还没有任务可以统计</h3><p>燃尽图根据任务的创建时间和完成时间自动生成，不需要额外录入。先给项目加几件任务吧。</p></div>`;
    const unit = S.ui.burnMetric === 'est' ? '天' : '件';
    const { series, last, velocity, projDate, due, start, end, today } = c;
    const W = 1000, H = 340, L = 50, R = 26, Tp = 18, B = 36;
    const total = Math.max(1, dayDiff(start, end));
    const X = d => L + dayDiff(start, d) / total * (W - L - R);
    const maxV = Math.max(1, ...series.map(s => s.scope)) * 1.12, step = Math.max(S.ui.burnMetric === 'est' ? 0.5 : 1, niceStep(maxV));
    const top = Math.ceil(maxV / step) * step;
    const Y = v => Tp + (1 - v / top) * (H - Tp - B);
    let grid = '';
    for (let v = 0; v <= top + 1e-9; v += step) grid += `<line class="gridl" x1="${L}" x2="${W - R}" y1="${Y(v)}" y2="${Y(v)}"/><text x="${L - 8}" y="${Y(v) + 4}" text-anchor="end">${fmt(v)}</text>`;
    const xStep = Math.max(1, Math.ceil(total / 9));
    for (let i = 0; i <= total; i += xStep) { const d = addDays(start, i); grid += `<text x="${X(d)}" y="${H - B + 20}" text-anchor="middle">${md(d)}</text>`; }
    const pts = series.map(s => `${X(s.d).toFixed(1)},${Y(s.rem).toFixed(1)}`).join(' ');
    const area = `M${X(series[0].d)},${Y(0)} L${pts.split(' ').join(' L')} L${X(series[series.length - 1].d)},${Y(0)} Z`;
    const scopePts = series.map(s => `${X(s.d).toFixed(1)},${Y(s.scope).toFixed(1)}`).join(' ');
    const ideal = due && due > start ? `<line class="l-ideal" x1="${X(start)}" y1="${Y(series[0].scope)}" x2="${X(due)}" y2="${Y(0)}"/>` : '';
    const proj = projDate && last.rem > 0 && projDate <= end ? `<line class="l-proj" x1="${X(today)}" y1="${Y(last.rem)}" x2="${X(projDate)}" y2="${Y(0)}"/>` : '';
    const dueL = due && due >= start && due <= end ? `<line class="due-l" x1="${X(due)}" x2="${X(due)}" y1="${Tp}" y2="${H - B}"/><text x="${X(due) + 4}" y="${Tp + 10}" style="fill:var(--ink-2)">截止 ${md(due)}</text>` : '';
    const svg = `<svg class="chart" id="burn-svg" viewBox="0 0 ${W} ${H}" preserveAspectRatio="xMidYMid meet" role="img" aria-label="燃尽图">
      ${grid}<line class="axis" x1="${L}" x2="${W - R}" y1="${Y(0)}" y2="${Y(0)}"/>
      <path class="a-actual" d="${area}"/>${ideal}<polyline class="l-scope" points="${scopePts}"/><polyline class="l-actual" points="${pts}"/>${proj}
      <line class="today" x1="${X(today)}" x2="${X(today)}" y1="${Tp}" y2="${H - B}"/>${dueL}
      <circle class="end-dot" cx="${X(today)}" cy="${Y(last.rem)}" r="6"/>
      <line class="hover-l" id="burn-hl" x1="0" x2="0" y1="${Tp}" y2="${H - B}" visibility="hidden"/>
      <rect id="burn-hit" x="${L}" y="${Tp}" width="${W - L - R}" height="${H - Tp - B}" fill="transparent" style="pointer-events:all"/></svg>`;
    const BH = 110, maxD = Math.max(1, ...series.map(s => s.day));
    const bw = Math.max(2, (W - L - R) / total * 0.62);
    const bars = series.map(s => s.day > 0 ? `<rect x="${(X(s.d) - bw / 2).toFixed(1)}" y="${(10 + (1 - s.day / maxD) * (BH - 34)).toFixed(1)}" width="${bw.toFixed(1)}" height="${(s.day / maxD * (BH - 34)).toFixed(1)}"/>` : '').join('');
    const svg2 = `<svg class="chart" viewBox="0 0 ${W} ${BH}" role="img" aria-label="每日完成量"><line class="axis" x1="${L}" x2="${W - R}" y1="${BH - 24}" y2="${BH - 24}"/><g class="bars">${bars}</g>
      <text x="${L - 8}" y="${14}" text-anchor="end">${fmt(maxD)}</text><text x="${L - 8}" y="${BH - 20}" text-anchor="end">0</text>
      <line class="today" x1="${X(today)}" x2="${X(today)}" y1="6" y2="${BH - 24}"/></svg>`;
    const pct = last.scope ? Math.round(last.done / last.scope * 100) : 0;
    let projTxt = '—', projCls = 't-lav', projSub = '最近 7 天还没有完成记录';
    if (last.rem === 0) { projTxt = '已完成'; projCls = 't-mint'; projSub = '全部任务都完成了'; }
    else if (projDate) {
      projTxt = md(projDate);
      if (due) { const diff = dayDiff(due, projDate); projCls = diff > 0 ? 't-rose' : 't-mint'; projSub = diff > 0 ? `比截止晚 ${diff} 天` : diff < 0 ? `比截止提前 ${-diff} 天` : '刚好赶上截止'; }
      else projSub = `按最近 7 天的速度推算`;
    }
    return `<div class="tiles">
      <div class="tile clay t-lav"><span class="k">剩余</span><span class="v">${fmt(last.rem)}<small style="font-size:14px;margin-left:4px">${unit}</small></span><span class="d">总量 ${fmt(last.scope)} ${unit}</span></div>
      <div class="tile clay t-mint"><span class="k">已完成</span><span class="v">${pct}%</span><span class="d">${fmt(last.done)} ${unit}</span></div>
      <div class="tile clay t-butter"><span class="k">近 7 天速度</span><span class="v">${velocity.toFixed(1)}</span><span class="d">${unit} / 天</span></div>
      <div class="tile clay ${projCls}"><span class="k">预计完成</span><span class="v">${projTxt}</span><span class="d">${esc(projSub)}</span></div>
    </div>
    <div class="chart-card clay"><h3>剩余工作量</h3><div class="cap">从 ${start} 到今天，每天结束时还剩多少${S.ui.burnMetric === 'est' ? '预估工期' : '件任务'}没完成。总量线上升说明中途加了任务。</div>
      <div class="legend"><span><i style="--c:var(--lav)"></i>实际剩余</span><span><i class="dash" style="--c:var(--mint)"></i>理想燃尽${due ? '' : '（设置项目截止日期后显示）'}</span><span><i style="--c:var(--peach)"></i>总量</span><span><i class="dash" style="--c:var(--lav)"></i>按当前速度的预测</span><span><i style="--c:var(--rose)"></i>今天</span></div>
      ${svg}<div class="c-tip" id="burn-tip" hidden></div></div>
    <div class="chart-card clay"><h3>每日完成量</h3>${svg2}</div>`;
  },
  after(root) {
    const svg = root.querySelector('#burn-svg'); if (!svg || !cache) return;
    const hit = svg.querySelector('#burn-hit'), hl = svg.querySelector('#burn-hl'), tip = root.querySelector('#burn-tip');
    const { series, start, end, due } = cache; const total = Math.max(1, dayDiff(start, end));
    hit.addEventListener('mousemove', e => {
      const r = svg.getBoundingClientRect(), vb = 1000 / r.width;
      const vx = (e.clientX - r.left) * vb, frac = (vx - 50) / (1000 - 50 - 26);
      const i = Math.round(frac * total);
      if (i < 0 || i >= series.length) { tip.hidden = true; hl.setAttribute('visibility', 'hidden'); return; }
      const s = series[i], xv = 50 + i / total * (1000 - 50 - 26);
      hl.setAttribute('x1', xv); hl.setAttribute('x2', xv); hl.setAttribute('visibility', 'visible');
      let ideal = '';
      if (due && due > start) { const k = dayDiff(start, s.d) / dayDiff(start, due); ideal = `<br>理想剩余 <b>${fmt(Math.max(0, series[0].scope * (1 - k)))}</b>`; }
      tip.innerHTML = `<b>${s.d}</b><br>剩余 <b>${fmt(s.rem)}</b> · 总量 <b>${fmt(s.scope)}</b><br>当天完成 <b>${fmt(s.day)}</b>${ideal}`;
      const card = tip.parentElement.getBoundingClientRect();
      tip.hidden = false; tip.style.left = Math.min(card.width - 180, e.clientX - card.left + 14) + 'px'; tip.style.top = (e.clientY - card.top + 10) + 'px';
    });
    hit.addEventListener('mouseleave', () => { tip.hidden = true; hl.setAttribute('visibility', 'hidden'); });
  }
};
SY.acts['burn-metric'] = el => { S.ui.burnMetric = el.dataset.v; SY.saveUI(); SY.renderAll(); };
SY.views.burndown = SY.chartView('burndown', '燃尽图', '根据任务的创建和完成时间自动生成。看剩余量能不能在截止日期前降到零。');
})();
