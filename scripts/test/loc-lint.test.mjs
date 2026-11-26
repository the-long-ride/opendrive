import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const lintScript = fileURLToPath(new URL('../loc-lint.mjs', import.meta.url));

function runGit(cwd, args) {
  const result = spawnSync('git', args, { cwd, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
}

test('LOC lint reports tracked and untracked oversized source with its configured limit', () => {
  const cwd = mkdtempSync(path.join(tmpdir(), 'loc-lint-'));
  mkdirSync(path.join(cwd, 'src'));
  writeFileSync(path.join(cwd, 'src', 'big.tsx'), 'export const x = 1;\n'.repeat(351));
  writeFileSync(path.join(cwd, 'src', 'big.rs'), 'pub const X: i32 = 1;\n'.repeat(301));
  runGit(cwd, ['init', '-q']);
  runGit(cwd, ['add', 'src/big.tsx']);

  const result = spawnSync(process.execPath, [lintScript], {
    cwd,
    encoding: 'utf8',
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /src\/big\.tsx: 351 lines \(limit 350\)/);
  assert.match(result.stderr, /src\/big\.rs: 301 lines \(limit 300\)/);
});

test('LOC lint ignores gitignored, generated, and resource files (.resx, .json, .csv, .db)', () => {
  const cwd = mkdtempSync(path.join(tmpdir(), 'loc-lint-'));
  mkdirSync(path.join(cwd, 'src'));
  mkdirSync(path.join(cwd, 'dist'));
  writeFileSync(path.join(cwd, '.gitignore'), 'src/ignored.js\n');
  writeFileSync(path.join(cwd, 'src', 'ok.ts'), 'export const ok = true;\n');
  writeFileSync(path.join(cwd, 'src', 'ignored.js'), 'const x = 1;\n'.repeat(351));
  writeFileSync(path.join(cwd, 'dist', 'generated.js'), 'const x = 1;\n'.repeat(351));
  writeFileSync(path.join(cwd, 'src', 'Strings.resx'), '<root>\n'.repeat(1000));
  writeFileSync(path.join(cwd, 'src', 'data.json'), '{\n'.repeat(1000));
  writeFileSync(path.join(cwd, 'src', 'rows.csv'), 'a,b,c\n'.repeat(1000));
  writeFileSync(path.join(cwd, 'src', 'fixture.db'), 'sqlite\n'.repeat(1000));
  runGit(cwd, ['init', '-q']);
  runGit(cwd, [
    'add',
    '.gitignore',
    'src/ok.ts',
    'dist/generated.js',
    'src/Strings.resx',
    'src/data.json',
    'src/rows.csv',
    'src/fixture.db',
  ]);

  const result = spawnSync(process.execPath, [lintScript], {
    cwd,
    encoding: 'utf8',
  });

  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /lint:loc ok/);
});

test('LOC lint rejects artificially compressed files and non-git directories', () => {
  const cwd = mkdtempSync(path.join(tmpdir(), 'loc-lint-comp-'));
  const nonGitResult = spawnSync(process.execPath, [lintScript], { cwd, encoding: 'utf8' });
  assert.notEqual(nonGitResult.status, 0);

  mkdirSync(path.join(cwd, 'src'));
  writeFileSync(path.join(cwd, 'src', 'compressed.js'), 'const x = 1; '.repeat(45) + '\n');
  writeFileSync(path.join(cwd, 'src', 'deleted.ts'), 'export const gone = true;\n');
  runGit(cwd, ['init', '-q']);
  runGit(cwd, ['add', 'src/compressed.js', 'src/deleted.ts']);
  rmSync(path.join(cwd, 'src', 'deleted.ts'));

  const compResult = spawnSync(process.execPath, [lintScript], { cwd, encoding: 'utf8' });
  assert.equal(compResult.status, 1);
  assert.match(compResult.stderr, /source appears artificially compressed/);
});
