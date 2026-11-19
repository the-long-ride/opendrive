import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const gate = fileURLToPath(new URL('../release-gate.sh', import.meta.url));

function run(command, args, cwd, env = process.env) {
  return spawnSync(command, args, { cwd, env, encoding: 'utf8' });
}

function git(cwd, ...args) {
  const result = run('git', args, cwd);
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim();
}

function setup() {
  const dir = mkdtempSync(join(tmpdir(), 'opendrive-release-'));
  git(dir, 'init', '-b', 'main');
  git(dir, 'config', 'user.email', 'ci@example.org');
  git(dir, 'config', 'user.name', 'CI');
  writeFileSync(
    join(dir, 'Cargo.toml'),
    '[workspace]\nresolver = "2"\n[workspace.package]\nversion = "0.1.0"\n',
  );
  git(dir, 'add', 'Cargo.toml');
  git(dir, 'commit', '-m', 'init');
  const head = git(dir, 'rev-parse', 'HEAD');
  git(dir, 'tag', '-a', 'v0.1.0', '-m', 'release 0.1.0');
  git(dir, 'update-ref', 'refs/remotes/origin/main', head);
  return { dir, head };
}

function runGate(dir, options = {}) {
  const output = join(dir, 'output');
  writeFileSync(output, '');
  const env = {
    ...process.env,
    ARTIFACT_ONLY: String(options.artifactOnly ?? false),
    REQUESTED_TAG: options.tag ?? '',
    REF_TYPE: options.refType ?? 'branch',
    GITHUB_REF_NAME: options.refName ?? 'main',
    GITHUB_OUTPUT: output,
  };
  const result = run('bash', [gate], dir, env);
  return { ...result, outputs: readFileSync(output, 'utf8') };
}

test('accepts annotated tag v0.1.0 and resolves its commit', () => {
  const { dir, head } = setup();
  try {
    const result = runGate(dir, { refType: 'tag', refName: 'v0.1.0' });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.outputs, /tag=v0\.1\.0\n/);
    assert.match(result.outputs, /version=0\.1\.0\n/);
    assert.ok(result.outputs.includes('ref_sha=' + head + '\n'));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('accepts explicit manual tag input on main', () => {
  const { dir } = setup();
  try {
    const result = runGate(dir, { tag: 'v0.1.0' });
    assert.equal(result.status, 0, result.stderr);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('rejects unknown tags instead of silently releasing current branch', () => {
  const { dir } = setup();
  try {
    const result = runGate(dir, { tag: 'v0.2.0' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /does not exist/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('rejects missing and malformed tags', () => {
  const { dir } = setup();
  try {
    for (const tag of ['', 'v0x1x0', 'v0.1.0;echo HI']) {
      const result = runGate(dir, { tag });
      assert.equal(result.status, 1);
      assert.match(result.stderr, /Enter an existing version tag/);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('rejects tags whose workspace version does not match', () => {
  const { dir } = setup();
  try {
    git(dir, 'tag', '-a', 'v0.2.0', '-m', 'wrong version');
    const result = runGate(dir, { tag: 'v0.2.0' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Version mismatch/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('artifact-only builds do not need an existing release tag', () => {
  const { dir, head } = setup();
  try {
    const result = runGate(dir, { artifactOnly: true });
    assert.equal(result.status, 0, result.stderr);
    assert.ok(result.outputs.includes('tag=v0.1.0-manual-' + head.slice(0, 7) + '\n'));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
