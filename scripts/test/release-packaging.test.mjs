import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';

const ROOT = process.cwd();
const read = (rel) => readFileSync(path.join(ROOT, rel), 'utf8');

test('README logo uses GitHub raw URL and author is set to the-long-ride', () => {
  const readme = read('README.md');
  assert.match(
    readme,
    /https:\/\/raw\.githubusercontent\.com\/the-long-ride\/opendrive\/main\/docs\/assets\/opendrive-logo\.svg/,
  );

  const cargo = read('Cargo.toml');
  assert.match(cargo, /authors\s*=\s*\["the-long-ride"\]/);

  const workflow = read('.github/workflows/release.yml');
  assert.match(workflow, /Maintainer:\s*the-long-ride/);
  assert.match(workflow, /Icon "docs\\assets\\opendrive\.ico"/);
  assert.match(workflow, /UninstallIcon "docs\\assets\\opendrive\.ico"/);
  assert.match(workflow, /Icon=opendrive/);
  assert.match(workflow, /CFBundleIconFile/);
  assert.match(workflow, /AppIcon\.icns/);
  assert.doesNotMatch(workflow, /for sz in 16 32 64 /);
});

test('generated ICO and PNG logo assets exist with valid headers', () => {
  const ico = readFileSync(path.join(ROOT, 'docs/assets/opendrive.ico'));
  assert.equal(ico[0], 0);
  assert.equal(ico[1], 0);
  assert.equal(ico[2], 1);
  assert.equal(ico[3], 0);
  assert.equal(ico.readUInt16LE(4), 6);

  const png = readFileSync(path.join(ROOT, 'docs/assets/opendrive-logo.png'));
  assert.deepEqual([...png.subarray(0, 4)], [0x89, 0x50, 0x4e, 0x47]);
});

test('llm.txt and error-codes spec document all OD-* diagnostic codes', () => {
  const llm = read('llm.txt');
  const spec = read('docs/specs/reference/error-codes.md');
  const errRs = read('crates/opendrive-core/src/error_codes.rs');
  const codes = [...errRs.matchAll(/code:\s*"(OD-[A-Z]+-\d+)"/g)].map((m) => m[1]);
  assert.ok(codes.length >= 20);
  for (const code of codes) {
    assert.ok(llm.includes(code), `llm.txt missing ${code}`);
    assert.ok(spec.includes(code), `error-codes.md missing ${code}`);
  }
});
