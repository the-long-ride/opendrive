import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { computeSourceFingerprint, validateSpecs } from '../validate-specs.mjs';

const scriptPath = fileURLToPath(new URL('../validate-specs.mjs', import.meta.url));

function makeRepo(t, files) {
  const dir = mkdtempSync(path.join(tmpdir(), 'opendrive-specs-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  for (const [rel, content] of Object.entries(files)) {
    const abs = path.join(dir, rel);
    mkdirSync(path.dirname(abs), { recursive: true });
    writeFileSync(abs, content, 'utf8');
  }
  return dir;
}

function validFrontmatter(overrides = {}) {
  const meta = {
    schema_version: 1,
    id: 'spec-a',
    title: 'Spec A',
    kind: 'feature',
    status: 'verified',
    summary: 'Valid spec',
    tags: ['core'],
    source_paths: ['src/a.rs'],
    related_specs: [],
    last_doc_update_at: '2026-11-27T00:00:00Z',
    last_verified_at: '2026-11-27T00:00:00+00:00',
    last_verified_code_commit: 'abc1234',
    last_source_change_commit: 'unavailable',
    last_source_change_at: 'unavailable',
    source_fingerprint: `sha256:${'0'.repeat(64)}`,
    ...overrides,
  };
  return Object.entries(meta)
    .map(
      ([k, v]) => `${k}: ${typeof v === 'string' && v.trim() === v && v ? v : JSON.stringify(v)}`,
    )
    .join('\n');
}

test('computeSourceFingerprint normalizes CRLF to LF identically', (t) => {
  const repoLf = makeRepo(t, { 'a.rs': 'fn main() {}\n', 'b.rs': 'fn two() {}\n' });
  const repoCrlf = makeRepo(t, { 'a.rs': 'fn main() {}\r\n', 'b.rs': 'fn two() {}\r\n' });
  assert.equal(
    computeSourceFingerprint(repoLf, ['b.rs', 'a.rs']),
    computeSourceFingerprint(repoCrlf, ['a.rs', 'b.rs']),
  );
});

test('validateSpecs passes valid spec tree with links and CLI exit 0', (t) => {
  const dir = makeRepo(t, {
    'src/a.rs': 'pub fn a() {}\n',
    'docs/specs/README.md': 'ignored readme\n',
    'docs/specs/INDEX.md': '# Index\n\n- [A](features/a.md)\n- [Ext](https://example.com)\n',
  });
  const fp = computeSourceFingerprint(dir, ['src/a.rs']);
  const body = '\n# Spec A\n\n## Code map\n- [src/a.rs](../../src/a.rs)\n';
  writeFileSync(
    path.join(dir, 'docs/specs/features/a.md'),
    (() => {
      mkdirSync(path.join(dir, 'docs/specs/features'), { recursive: true });
      return `---\n# comment\n${validFrontmatter({ source_fingerprint: fp })}\n---\n${body}`;
    })(),
  );
  assert.deepEqual(validateSpecs(dir), { ok: true, errors: [] });
  const cli = spawnSync(process.execPath, [scriptPath], { cwd: dir, encoding: 'utf8' });
  assert.equal(cli.status, 0, cli.stderr);
  assert.match(cli.stdout, /validate:specs ok/);
});

test('validateSpecs catches missing roots, bad frontmatter, broken links, and fingerprint drift', (t) => {
  const empty = makeRepo(t, {});
  assert.equal(validateSpecs(empty).ok, false);

  const noIndex = makeRepo(t, { 'docs/specs/a.md': '# Orphan\n' });
  assert.match(validateSpecs(noIndex).errors[0], /missing routing index/);

  const badYaml = makeRepo(t, {
    'docs/specs/INDEX.md':
      '[Bad1](bad1.md) [Bad2](bad2.md) [Bad3](bad3.md) [Bad4](bad4.md) [Bad5](bad5.md)\n',
    'docs/specs/bad1.md': 'no frontmatter',
    'docs/specs/bad2.md': '---\nid: x\n',
    'docs/specs/bad3.md': '---\n  indented: 1\n---\n# H\n',
    'docs/specs/bad4.md': '---\nid: x\nid: y\n---\n# H\n',
    'docs/specs/bad5.md': '---\ntitle: [bad\n---\n# H\n',
  });
  assert.equal(validateSpecs(badYaml).ok, false);

  const missingFields = makeRepo(t, {
    'docs/specs/INDEX.md': '[M](m.md)\n',
    'docs/specs/m.md': '---\nid: only\n---\n',
  });
  assert.ok(validateSpecs(missingFields).errors.some((e) => e.includes('missing required field')));

  const badMeta = makeRepo(t, {
    'src/a.rs': 'pub fn a() {}\n',
    'docs/specs/INDEX.md': '[A](a.md) [B](b.md) [C](c.md) [D](d.md)\n',
    'docs/specs/a.md': `---\n${validFrontmatter({
      id: 'BAD ID',
      schema_version: 2,
      kind: 'bogus',
      status: 'bogus',
      title: ' ',
      tags: [''],
      last_doc_update_at: '2026-11-27',
      last_verified_code_commit: 'xyz',
      source_fingerprint: 'bad',
      source_paths: ['../escape.rs'],
      related_specs: ['BAD ID', 'missing-id'],
    })}\n---\nNo H1 or code map\n`,
    'docs/specs/b.md': `---\n${validFrontmatter({
      id: 'dup',
      status: 'retired',
      source_paths: ['src/a.rs'],
    })}\n---\n# B\n[Miss](missing.md) [Esc](../../outside.md)\n`,
    'docs/specs/c.md': `---\n${validFrontmatter({
      id: 'dup',
      source_paths: [],
      status: 'verified',
    })}\n---\n# C\n`,
    'docs/specs/d.md': `---\n${validFrontmatter({
      id: 'd',
      source_paths: ['src/a.rs'],
    })}\n---\n# D\n\n## Code map\n`,
    'docs/specs/orphan.md': `---\n${validFrontmatter({
      id: 'orphan',
      source_paths: [],
      status: 'retired',
    })}\n---\n# Orphan\n`,
  });
  const res = validateSpecs(badMeta);
  assert.equal(res.ok, false);
  for (const snippet of [
    'invalid id',
    'duplicate id dup',
    'unsupported schema_version',
    'missing or unsafe source file',
    'source_fingerprint mismatch',
    'verified spec has no source_paths',
    'broken link',
    'link escapes docs/specs',
    'unreachable from INDEX.md',
    'related_specs self-reference',
    'missing ID: missing-id',
    'H1 heading',
    'missing ## Code map',
  ]) {
    assert.ok(
      res.errors.some((e) => e.includes(snippet)),
      `expected error matching "${snippet}", got:\n${res.errors.join('\n')}`,
    );
  }
  const failCli = spawnSync(process.execPath, [scriptPath], { cwd: badMeta, encoding: 'utf8' });
  assert.equal(failCli.status, 1);
  assert.match(failCli.stderr, /ERROR:/);
});
