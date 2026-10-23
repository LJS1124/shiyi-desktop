// Copies provider logos from the open-source LobeHub icon set (MIT,
// https://github.com/lobehub/lobe-icons) into src/assets/providers/<id>.svg.
// Run after: npm install --no-save @lobehub/icons-static-svg
// Missing files are fine: the settings page shows a letter badge instead.
import { existsSync, mkdirSync, copyFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const pkg = join(root, 'node_modules', '@lobehub', 'icons-static-svg');
const out = join(root, 'src', 'assets', 'providers');
// provider id → candidate icon names, preferred first (colour versions where they exist)
const WANT = {
  anthropic: ['anthropic', 'claude-color'],
  openai: ['openai'],
  gemini: ['gemini-color', 'gemini'],
  openrouter: ['openrouter'],
  deepseek: ['deepseek-color', 'deepseek'],
  dashscope: ['bailian-color', 'qwen-color', 'alibabacloud-color', 'qwen'],
  moonshot: ['kimi-color', 'moonshot', 'kimi'],
  zhipu: ['zhipu-color', 'chatglm-color', 'zhipu'],
  ark: ['doubao-color', 'volcengine-color', 'doubao'],
  siliconflow: ['siliconcloud-color', 'siliconcloud'],
  ollama: ['ollama'],
};
if (!existsSync(pkg)) { console.log('icon package not installed; skipping (letter badges will be shown)'); process.exit(0); }
const dir = ['icons', 'svg', ''].map(d => join(pkg, d)).find(d => existsSync(d) && readdirSync(d).some(f => f.endsWith('.svg')));
if (!dir) { console.log('no svg folder in the icon package; skipping'); process.exit(0); }
mkdirSync(out, { recursive: true });
for (const lic of ['LICENSE', 'LICENSE.md']) if (existsSync(join(pkg, lic))) copyFileSync(join(pkg, lic), join(out, 'LICENSE-lobe-icons'));
let n = 0;
for (const [id, names] of Object.entries(WANT)) {
  const hit = names.map(x => join(dir, x + '.svg')).find(existsSync);
  if (hit) { copyFileSync(hit, join(out, id + '.svg')); n++; } else console.log('no icon for', id);
}
const msg = `provider icons: ${n}/${Object.keys(WANT).length} copied to src/assets/providers`;
console.log(process.env.GITHUB_ACTIONS ? `::notice::${msg}` : msg);
