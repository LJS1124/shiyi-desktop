// "Possibly related" items: a plain-JS port of core/src/related.rs.
// Keep it in step with the Rust file; scripts/test_related.mjs checks that both agree.
(function (root) {
  'use strict';

  // Function-word bigrams and words that say nothing about the topic (verbatim from related.rs).
  const STOP = new Set([
    '可以', '一个', '一下', '一些', '能不', '不能', '能否', '我们', '你们', '他们', '什么', '这个', '那个', '这些', '没有', '不是', '如果', '因为', '所以', '但是',
    '还是', '就是', '已经', '自己', '时候', '怎么', '为了', '或者', '然后', '现在', '这样', '那样', '需要', '考虑', '感觉', '觉得', '应该', '是不', '有没', '的时',
    '里的', '中的', '上的', '下的', '了一', '的一', '一次', '每次', '比较', '然后', '其实', '可能', '试试', '看看', '顺便',
    'the', 'and', 'for', 'with', 'this', 'that', 'from', 'into', 'are', 'was', 'can',
  ]);
  const TRIM = '的了和与在是把被就也都很';
  const ALNUM = /[\p{Alphabetic}\p{N}]/u; // Rust char::is_alphanumeric

  const isCjk = (ch) => {
    const c = ch.codePointAt(0);
    return (c >= 0x3400 && c <= 0x4dbf) || (c >= 0x4e00 && c <= 0x9fff) || (c >= 0xf900 && c <= 0xfaff)
      || (c >= 0x3040 && c <= 0x30ff) || (c >= 0xac00 && c <= 0xd7af);
  };
  const hasCjk = (s) => [...s].some(isCjk);
  const len = (s) => [...s].length; // code points, like Rust chars().count()
  const sq = (x) => x * x;
  const str = (x) => (typeof x === 'string' ? x : '');
  const tagsOf = (doc) => (Array.isArray(doc && doc.tags) ? doc.tags.filter((t) => typeof t === 'string') : []);
  const projectOf = (doc) => str(doc && doc.projectId);
  // Code point order, as Rust compares UTF-8 bytes (JS < compares UTF-16 units).
  const cmpCp = (a, b) => {
    const x = [...a], y = [...b];
    for (let i = 0; i < Math.min(x.length, y.length); i++) {
      const d = x[i].codePointAt(0) - y[i].codePointAt(0);
      if (d) return d;
    }
    return x.length - y.length;
  };

  function features(text) {
    const out = new Set();
    let run = [];  // consecutive CJK characters
    let word = ''; // latin/digit word in progress
    const flushRun = () => { for (let i = 0; i + 1 < run.length; i++) out.add(run[i] + run[i + 1]); run = []; };
    const flushWord = () => { if (len(word) >= 2) out.add(word); word = ''; };
    for (const c of String(text ?? '').toLowerCase() + ' ') {
      if (isCjk(c)) { flushWord(); run.push(c); }
      else if (ALNUM.test(c)) { flushRun(); word += c; }
      else { flushRun(); flushWord(); }
    }
    for (const w of STOP) out.delete(w);
    return out;
  }

  // Adjacent shared bigrams of the query merge back into words ("路由" + "由器" -> "路由器").
  function sharedTerms(queryText, shared) {
    const sh = new Set(shared || []);
    const chars = [...String(queryText ?? '').toLowerCase()];
    const trim = (t) => {
      const a = [...t];
      let i = 0, j = a.length;
      while (i < j && TRIM.includes(a[i])) i++;
      while (j > i && TRIM.includes(a[j - 1])) j--;
      return a.slice(i, j).join('');
    };
    const terms = [];
    let i = 0;
    while (i + 1 < chars.length) {
      if (isCjk(chars[i]) && isCjk(chars[i + 1]) && sh.has(chars[i] + chars[i + 1])) {
        let j = i + 1;
        while (j + 1 < chars.length && isCjk(chars[j + 1]) && sh.has(chars[j] + chars[j + 1])) j++;
        const t = trim(chars.slice(i, j + 1).join(''));
        if (len(t) >= 2 && !terms.includes(t)) terms.push(t);
        i = j + 1;
      } else {
        i++;
      }
    }
    for (const w of sh) if (!hasCjk(w) && !terms.includes(w)) terms.push(w);
    terms.sort((a, b) => len(b) - len(a) || cmpCp(a, b));
    return terms.slice(0, 4);
  }

  // GUI-shaped documents: idea {text, note, tags, status, projectId}, project {name, goal, note},
  // task {title, note, status, projectId}, map {title, nodes: {id: {text}}, projectId}.
  function docText(type, doc) {
    const d = doc || {};
    let parts;
    switch (type) {
      case 'idea': parts = [d.text, d.note]; break;
      case 'project': parts = [d.name, d.goal, d.note]; break;
      case 'task': parts = [d.title, d.note]; break;
      case 'map': parts = [d.title]; break;
      default: parts = [];
    }
    let t = parts.map(str).join('\n');
    if (type === 'map' && d.nodes && typeof d.nodes === 'object' && !Array.isArray(d.nodes)) {
      for (const n of Object.values(d.nodes)) t += '\n' + str(n && n.text);
    }
    // [[wikilinks]] are explicit already; don't let their text count twice
    return t.replaceAll('[[', ' ').replaceAll(']]', ' ');
  }

  function candidate(it) {
    const status = str(it.doc && it.doc.status);
    switch (it.type) {
      case 'task': return status !== 'done';
      case 'idea': return status !== 'archived';
      case 'project': case 'map': return true;
      default: return false;
    }
  }

  // items: [{id, type, doc}] (all live ideas, projects, tasks, maps).
  // opts: {targetId | text, limit = 5, minScore, exclude (Set of ids already connected to the target)}.
  function rank(items, opts = {}) {
    const { targetId, text, limit = 5, minScore, exclude } = opts;
    const useTarget = targetId != null && targetId !== '';
    const all = Array.isArray(items) ? items : [];
    const target = useTarget ? all.find((it) => it.id === targetId) : undefined;
    if (useTarget && !target) return [];
    // The target joins the IDF corpus even when it is not a candidate itself.
    const corpus = all.filter((it) => candidate(it) || (useTarget && it.id === targetId));
    const feats = corpus.map((it) => features(docText(it.type, it.doc)));
    const df = new Map();
    for (const f of feats) for (const w of f) df.set(w, (df.get(w) || 0) + 1);
    const n = Math.max(corpus.length, 1);
    const idf = (w) => Math.log((n + 1) / ((df.get(w) || 0) + 1)) + 1;
    const norm = (f) => Math.sqrt([...f].reduce((s, w) => s + sq(idf(w)), 0));

    let qText, qFeats, qTags, qProject;
    if (useTarget) {
      qText = docText(target.type, target.doc);
      qFeats = feats[corpus.indexOf(target)];
      qTags = tagsOf(target.doc);
      qProject = projectOf(target.doc);
    } else {
      qText = String(text ?? '');
      qFeats = features(qText);
      qTags = [];
      qProject = '';
    }
    if (qFeats.size === 0 && qTags.length === 0) return [];

    const qn = norm(qFeats);
    const skip = new Set(useTarget && exclude ? exclude : []);
    const floor = minScore ?? (useTarget ? 0.12 : 0.08);
    const hits = [];
    corpus.forEach((it, i) => {
      if ((useTarget && it.id === targetId) || skip.has(it.id) || !candidate(it)) return;
      const f = feats[i];
      const common = [...qFeats].filter((w) => f.has(w));
      const dot = common.reduce((s, w) => s + sq(idf(w)), 0);
      const dn = norm(f);
      let score = qn > 0 && dn > 0 ? dot / (qn * dn) : 0;
      const tags = tagsOf(it.doc).filter((t) => qTags.includes(t));
      score += Math.min(tags.length * 0.12, 0.24);
      if (qProject && projectOf(it.doc) === qProject) score += 0.05;
      if (score < floor) return;
      hits.push({ id: it.id, type: it.type, score, shared: sharedTerms(qText, common), tags });
    });
    hits.sort((a, b) => b.score - a.score || cmpCp(a.id, b.id));
    return hits.slice(0, limit);
  }

  const api = { features, sharedTerms, docText, rank };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  if (root) { root.ShiyiRelated = api; if (root.SY) root.SY.relatedLib = api; }
})(typeof window !== 'undefined' ? window : globalThis);
