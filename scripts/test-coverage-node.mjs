import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

export function collectTestFiles(repoRoot = process.cwd()) {
  const dirs = ['scripts/test'];
  const files = [];

  for (const relDir of dirs) {
    const absDir = path.join(repoRoot, relDir);
    if (!existsSync(absDir)) continue;
    for (const entry of readdirSync(absDir, { withFileTypes: true })) {
      if (entry.isFile() && entry.name.endsWith('.test.mjs')) {
        files.push(path.join(relDir, entry.name).replaceAll('\\', '/'));
      }
    }
  }

  return files.sort();
}

export function runNodeCoverage(repoRoot = process.cwd()) {
  const testFiles = collectTestFiles(repoRoot);
  if (testFiles.length === 0) {
    console.error('No Node test files found.');
    return 1;
  }

  const c8Bin = path.join(repoRoot, 'node_modules', 'c8', 'bin', 'c8.js');
  const args = [
    c8Bin,
    '--check-coverage',
    '--lines',
    '95',
    '--statements',
    '95',
    '--functions',
    '95',
    '--branches',
    '95',
    '--exclude',
    'scripts/test/**',
    '--exclude',
    'scripts/launch-desktop.mjs',
    '--exclude',
    'scripts/test-coverage-*.mjs',
    process.execPath,
    '--test',
    ...testFiles,
  ];

  const result = spawnSync(process.execPath, args, { cwd: repoRoot, stdio: 'inherit' });
  return result.status ?? 1;
}

if (import.meta.url === pathToFileURL(path.resolve(process.argv[1] ?? '')).href) {
  process.exit(runNodeCoverage(process.cwd()));
}
