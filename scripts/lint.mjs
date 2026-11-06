import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

const bad = [];
const ignoredDirectories = new Set([
  'node_modules',
  'dist',
  'build',
  'out',
  'target',
  'bin',
  'obj',
  'coverage',
  '.git',
  '.test-dist',
  '.coverage-dist',
  'test-results',
  'playwright-report',
]);

function walk(dir) {
  if (!existsSync(dir)) return;
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (e.isDirectory() && ignoredDirectories.has(e.name)) continue;

    const f = path.join(dir, e.name);
    if (e.isDirectory()) walk(f);
    else if (/\.(?:ts|tsx|js|jsx|mjs|cjs)$/.test(f)) {
      const s = readFileSync(f, 'utf8');
      if (/\beval\s*\(/.test(s)) bad.push(`${f}: eval is forbidden`);
      if (/child_process[^\n]*exec\s*\(/.test(s))
        bad.push(`${f}: shell exec is forbidden; use spawn argv`);
    }
  }
}

for (const r of ['src', 'apps', 'packages', 'scripts']) {
  walk(r);
}

if (bad.length) {
  console.error(bad.join('\n'));
  process.exit(1);
}
console.log('lint ok');
