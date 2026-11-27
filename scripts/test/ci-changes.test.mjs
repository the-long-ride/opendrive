import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { classifyChanges, getPrChangedFiles, normalizePath } from '../ci-changes.mjs';

test('normalizePath converts Windows backslashes and strips leading ./', () => {
  assert.equal(normalizePath('.\\scripts\\lint.mjs'), 'scripts/lint.mjs');
  assert.equal(
    normalizePath('crates\\opendrive-core\\src\\lib.rs'),
    'crates/opendrive-core/src/lib.rs',
  );
});

test('classifyChanges returns false/false for empty or docs-only changes', () => {
  assert.deepEqual(classifyChanges([]), { node: false, rust: false });
  assert.deepEqual(
    classifyChanges([
      'README.md',
      'AGENTS.md',
      'llm.txt',
      'docs/specs/01.md',
      'assets/demo-pictures/OpenDrive-Explorer.png',
      '.agents/notes.md',
      '.prettierrc.json',
      '.prettierignore',
    ]),
    { node: false, rust: false },
  );
});

test('classifyChanges routes rust-only, node-only, mixed, and workflow changes', () => {
  assert.deepEqual(classifyChanges(['crates\\opendrive-core\\src\\lib.rs', 'Cargo.toml']), {
    node: false,
    rust: true,
  });
  assert.deepEqual(classifyChanges(['scripts/lint.mjs', 'package.json']), {
    node: true,
    rust: false,
  });
  assert.deepEqual(classifyChanges(['crates/a.rs', 'scripts/lint.mjs']), {
    node: true,
    rust: true,
  });
  assert.deepEqual(classifyChanges(['.github/workflows/quality-gate.yml']), {
    node: true,
    rust: true,
  });
});

test('getPrChangedFiles and CLI entrypoint classify PR and non-PR events', () => {
  const cwd = mkdtempSync(path.join(os.tmpdir(), 'opendrive-ci-changes-'));
  try {
    const git = (args) =>
      spawnSync('git', args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
    git(['init', '-q']);
    git(['config', 'user.email', 'ci@example.com']);
    git(['config', 'user.name', 'CI']);
    writeFileSync(path.join(cwd, 'README.md'), 'init\n');
    git(['add', 'README.md']);
    git(['commit', '-qm', 'init']);
    const baseSha = git(['rev-parse', 'HEAD']).stdout.trim();

    writeFileSync(path.join(cwd, 'package.json'), '{}\n');
    git(['add', 'package.json']);
    git(['commit', '-qm', 'node change']);

    assert.deepEqual(getPrChangedFiles(baseSha, cwd), ['package.json']);
    assert.throws(() => getPrChangedFiles('0000000000000000000000000000000000000000', cwd));

    const outFile = path.join(cwd, 'github_output.txt');
    const scriptPath = path.resolve('scripts/ci-changes.mjs');

    const prRun = spawnSync(process.execPath, [scriptPath], {
      cwd,
      encoding: 'utf8',
      env: {
        ...process.env,
        GITHUB_EVENT_NAME: 'pull_request',
        BASE_SHA: baseSha,
        GITHUB_OUTPUT: outFile,
      },
    });
    assert.equal(prRun.status, 0, prRun.stderr);
    assert.match(readFileSync(outFile, 'utf8'), /node=true\nrust=false/);

    const pushRun = spawnSync(process.execPath, [scriptPath], {
      cwd,
      encoding: 'utf8',
      env: { ...process.env, GITHUB_EVENT_NAME: 'push', BASE_SHA: '' },
    });
    assert.equal(pushRun.status, 0, pushRun.stderr);
    assert.deepEqual(JSON.parse(pushRun.stdout), { node: true, rust: true });

    const bareEnv = { ...process.env };
    delete bareEnv.GITHUB_EVENT_NAME;
    delete bareEnv.BASE_SHA;
    const bareRun = spawnSync(process.execPath, [scriptPath], {
      cwd,
      encoding: 'utf8',
      env: bareEnv,
    });
    assert.equal(bareRun.status, 0, bareRun.stderr);
    assert.deepEqual(JSON.parse(bareRun.stdout), { node: true, rust: true });
  } finally {
    rmSync(cwd, { recursive: true, force: true });
  }
});
