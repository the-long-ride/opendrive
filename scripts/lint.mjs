import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const oxlintBin = path.resolve(
  import.meta.dirname,
  '..',
  'node_modules',
  'oxlint',
  'bin',
  'oxlint',
);
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

function walkJsFiles(dir, files, bad) {
  if (!existsSync(dir)) return;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory() && ignoredDirectories.has(entry.name)) continue;

    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walkJsFiles(fullPath, files, bad);
    } else if (/\.(?:ts|tsx|js|jsx|mjs|cjs)$/.test(entry.name)) {
      files.push(fullPath);
      const text = readFileSync(fullPath, 'utf8');
      if (/\beval\s*\(/.test(text)) bad.push(`${fullPath}: eval is forbidden`);
      if (/child_process[^\n]*exec\s*\(/.test(text)) {
        bad.push(`${fullPath}: shell exec is forbidden; use spawn argv`);
      }
    }
  }
}

function extractTomlSectionKeys(tomlText, sectionHeader) {
  const keys = [];
  let inSection = false;
  for (const rawLine of tomlText.split(/\r?\n/)) {
    const line = rawLine.replace(/#.*$/, '').trim();
    if (!line) continue;
    if (line.startsWith('[')) {
      inSection = line === `[${sectionHeader}]`;
      continue;
    }
    if (!inSection) continue;
    const match = line.match(/^([A-Za-z0-9_-]+)\s*=/);
    if (match) keys.push(match[1]);
  }
  return keys;
}

function checkUnusedCargoWorkspace(rootDir, bad) {
  const rootManifestPath = path.join(rootDir, 'Cargo.toml');
  const cratesDir = path.join(rootDir, 'crates');
  if (!existsSync(rootManifestPath) || !existsSync(cratesDir)) return;

  const memberTomls = readdirSync(cratesDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => path.join(cratesDir, entry.name, 'Cargo.toml'))
    .filter(existsSync)
    .map((file) => readFileSync(file, 'utf8'));
  if (memberTomls.length === 0) return;

  const rootToml = readFileSync(rootManifestPath, 'utf8');
  for (const field of extractTomlSectionKeys(rootToml, 'workspace.package')) {
    const pattern = new RegExp(`^\\s*${field}\\.workspace\\s*=\\s*true\\b`, 'm');
    if (!memberTomls.some((toml) => pattern.test(toml))) {
      bad.push(`Cargo.toml: unused workspace.package field '${field}'`);
    }
  }
  for (const dep of extractTomlSectionKeys(rootToml, 'workspace.dependencies')) {
    const pattern = new RegExp(
      `^\\s*${dep}(?:\\.workspace\\s*=\\s*true|\\s*=\\s*\\{[^}]*\\bworkspace\\s*=\\s*true)`,
      'm',
    );
    if (!memberTomls.some((toml) => pattern.test(toml))) {
      bad.push(`Cargo.toml: unused workspace dependency '${dep}'`);
    }
  }
}

export function runLint(rootDir = process.cwd()) {
  const bad = [];
  const jsFiles = [];
  for (const root of ['src', 'apps', 'packages', 'scripts']) {
    walkJsFiles(path.join(rootDir, root), jsFiles, bad);
  }
  if (jsFiles.length > 0) {
    const result = spawnSync(
      process.execPath,
      [
        oxlintBin,
        '--deny',
        'no-unused-vars',
        '--deny',
        'no-unused-private-class-members',
        '--deny',
        'no-unused-labels',
        ...jsFiles,
      ],
      { cwd: rootDir, encoding: 'utf8' },
    );
    if (result.status !== 0) {
      bad.push([result.stdout, result.stderr].filter(Boolean).join('\n').trim());
    }
  }
  checkUnusedCargoWorkspace(rootDir, bad);
  if (bad.length > 0) {
    console.error(bad.join('\n'));
    return 1;
  }
  console.log('lint ok');
  return 0;
}

if (import.meta.url === pathToFileURL(path.resolve(process.argv[1] ?? '')).href) {
  process.exit(runLint(process.cwd()));
}
