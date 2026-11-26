import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { runLint } from '../lint.mjs';

const lintScript = fileURLToPath(new URL('../lint.mjs', import.meta.url));

function withTempWorkspace(t, files) {
  const dir = mkdtempSync(path.join(tmpdir(), 'opendrive-lint-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const [relPath, content] of Object.entries(files)) {
    const abs = path.join(dir, relPath);
    mkdirSync(path.dirname(abs), { recursive: true });
    writeFileSync(abs, content, 'utf8');
  }
  return dir;
}

test('runLint passes on the repository and via CLI entrypoint', () => {
  assert.equal(runLint(process.cwd()), 0);
  const res = spawnSync(process.execPath, [lintScript], {
    cwd: process.cwd(),
    encoding: 'utf8',
  });
  assert.equal(res.status, 0, res.stderr);
  assert.match(res.stdout, /lint ok/);
});

test('runLint passes on clean temp workspace with ignored directories and valid manifests', (t) => {
  const dir = withTempWorkspace(t, {
    'scripts/clean.mjs': 'export function sum(a, b) { return a + b; }\n',
    'scripts/node_modules/ignored.js': 'const ignoredUnused = 1;\n',
    'crates/sample/Cargo.toml':
      '[package]\nname = "sample"\nversion.workspace = true\n[dependencies]\nanyhow = { workspace = true }\n',
    'Cargo.toml':
      '# workspace manifest\n[workspace.package]\nversion = "0.2.0"\n\n[workspace.dependencies]\nanyhow = "1"\n',
  });
  assert.equal(runLint(dir), 0);

  const emptyCrates = withTempWorkspace(t, {
    'crates/README.md': 'no crates here\n',
    'Cargo.toml': '[workspace.package]\nversion = "0.2.0"\n',
  });
  assert.equal(runLint(emptyCrates), 0);
});

test('runLint rejects eval, child_process exec, and unused JS symbols', (t) => {
  const cases = [
    ['eval is forbidden', `export function bad() { return ${['ev', 'al("1")'].join('')} ; }\n`],
    [
      'child_process exec is forbidden',
      `import { exec } from "node:child_process"; export function run() { return ${['ex', 'ec("ls")'].join('')}; }\n`,
    ],
    ['unused variable', 'const unusedVar = 42;\n'],
    ['unused class', 'class UnusedService {}\n'],
    ['unused function', 'function unusedHelper() { return 1; }\n'],
    ['unused private class member', 'export class Used { #unusedMethod() { return 1; } }\n'],
    ['unused parameter', 'export function add(a, unusedParam) { return a; }\n'],
  ];
  for (const [label, code] of cases) {
    const dir = withTempWorkspace(t, { 'scripts/bad.mjs': code });
    assert.equal(runLint(dir), 1, `expected failure for ${label}`);
  }
});

test('runLint rejects unused workspace.package fields and workspace.dependencies entries', (t) => {
  const dir = withTempWorkspace(t, {
    'Cargo.toml': [
      '[workspace.package]',
      'version = "0.2.0"',
      'authors = ["OpenDrive"]',
      '',
      '[workspace.dependencies]',
      'anyhow = "1"',
      'chrono = "0.4"',
      '',
    ].join('\n'),
    'crates/sample/Cargo.toml': [
      '[package]',
      'name = "sample"',
      'version.workspace = true',
      '',
      '[dependencies]',
      'anyhow.workspace = true',
      '',
    ].join('\n'),
  });
  const res = spawnSync(process.execPath, [lintScript], { cwd: dir, encoding: 'utf8' });
  assert.equal(res.status, 1);
  assert.match(res.stderr, /unused workspace\.package field 'authors'/);
  assert.match(res.stderr, /unused workspace dependency 'chrono'/);
});
