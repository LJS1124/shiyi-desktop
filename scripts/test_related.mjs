#!/usr/bin/env node
// Checks that src/js/related.js ranks exactly like the `shiyi related` CLI.
// Usage: node scripts/test_related.mjs <db path>
// Pass a copy of the database: the CLI opens the file it is given.
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const { rank } = require('../src/js/related.js');

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const cli = path.join(repo, 'core', 'target', 'debug', 'shiyi');
const LIMIT = 6;
const TEXT_QUERIES = ['想做一个回放工具看 agent 怎么失败的', '书房路由器信号', '用 LLM 写周报'];

const dbArg = process.argv[2];
if (!dbArg) {
  console.error('usage: node scripts/test_related.mjs <db path>');
  process.exit(2);
}
const db = path.resolve(dbArg);
if (!existsSync(db)) {
  console.error(`no database at ${db}`);
  process.exit(2);
}

function shiyi(args) {
  const out = JSON.parse(execFileSync(cli, [...args, '--db', db, '--compact'], { encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 }));
  if (out.ok === false) throw new Error(JSON.stringify(out));
  return out;
}

// Database shape -> GUI shape: project_id becomes projectId, everything else keeps its name.
const KINDS = [['ideas', 'idea'], ['projects', 'project'], ['tasks', 'task'], ['maps', 'map']];
const snap = shiyi(['export']);
const docs = [];
for (const [plural, type] of KINDS) {
  for (const row of snap[plural] || []) {
    const { project_id, ...rest } = row;
    docs.push({ id: row.id, type, doc: { ...rest, projectId: project_id ?? null } });
  }
}

// Ids already connected to the target, as `shiyi get` reports them (tag edges ignored).
function connectedIds(id) {
  const g = shiyi(['get', id]);
  const ids = [...(g.links_out || []).map((e) => e.target), ...(g.links_in || []).map((e) => e.source)];
  return new Set(ids.filter((x) => typeof x === 'string' && !x.startsWith('tag:')));
}

const same = (a, b) => a.length === b.length && a.every((x, i) => x === b[i]);
let queries = 0, bad = 0, hitsCompared = 0, exactScores = 0;

function check(label, cliItems, hits) {
  queries++;
  const problems = [];
  const cliIds = cliItems.map((x) => x.id);
  const jsIds = hits.map((h) => h.id);
  if (!same(cliIds, jsIds)) problems.push(`ids cli=[${cliIds}] js=[${jsIds}]`);
  for (let i = 0; i < Math.min(cliItems.length, hits.length); i++) {
    const c = cliItems[i], h = hits[i];
    hitsCompared++;
    const js = Math.round(h.score * 100) / 100;
    if (c.score === js) exactScores++;
    if (!(Math.abs(c.score - js) <= 0.011)) problems.push(`${c.id} score cli=${c.score} js=${js}`);
    if (!same(c.shared ?? [], h.shared)) problems.push(`${c.id} shared cli=${JSON.stringify(c.shared)} js=${JSON.stringify(h.shared)}`);
    if (!same(c.shared_tags ?? [], h.tags)) problems.push(`${c.id} shared_tags cli=${JSON.stringify(c.shared_tags)} js=${JSON.stringify(h.tags)}`);
  }
  if (problems.length) {
    bad++;
    console.log(`DIFF ${label}: ${problems.join('; ')}`);
  } else {
    console.log(`ok   ${label} (${hits.length} items)`);
  }
}

function fail(label, e) {
  queries++;
  bad++;
  console.log(`DIFF ${label}: ${String(e.message).split('\n')[0]}`);
}

for (const it of docs) {
  const label = `related ${it.id}`;
  try {
    const cliItems = shiyi(['related', it.id, '--limit', String(LIMIT)]).items;
    const hits = rank(docs, { targetId: it.id, limit: LIMIT, exclude: connectedIds(it.id) });
    check(label, cliItems, hits);
  } catch (e) {
    fail(label, e);
  }
}

for (const q of TEXT_QUERIES) {
  const label = `text "${q}"`;
  try {
    const cliItems = shiyi(['related', '--text', q, '--limit', String(LIMIT)]).items;
    const hits = rank(docs, { text: q, limit: LIMIT });
    check(label, cliItems, hits);
  } catch (e) {
    fail(label, e);
  }
}

console.log(`${queries} queries: ${queries - bad} ok, ${bad} DIFF; ${hitsCompared} hits compared, ${exactScores} scores equal exactly after rounding`);
process.exit(bad ? 1 : 0);
