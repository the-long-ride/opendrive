import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';

const ROOT = process.cwd();

function read(relPath) {
  return readFileSync(path.join(ROOT, relPath), 'utf8');
}

test('package.json defines required quality-gate scripts', () => {
  const pkg = JSON.parse(read('package.json'));
  assert.equal(pkg.name, 'opendrive-workspace');
  assert.equal(pkg.private, true);
  assert.equal(pkg.type, 'module');

  const requiredScripts = [
    'desktop',
    'format',
    'format:check',
    'lint',
    'lint:source',
    'lint:loc',
    'lint:dead',
    'lint:rust',
    'test:scripts',
    'test:coverage:node',
    'test:coverage:rust',
    'test:coverage',
    'test:gate',
  ];
  for (const name of requiredScripts) {
    assert.equal(typeof pkg.scripts[name], 'string', `missing script ${name}`);
  }
  assert.match(
    pkg.scripts['lint:rust'],
    /cargo clippy --workspace --all-targets --locked -- -D warnings/,
  );
});

test('quality-gate workflow enforces Node 24, 95% coverage, Rust checks, and gate job', () => {
  const workflow = read('.github/workflows/quality-gate.yml');
  assert.match(workflow, /actions\/checkout@v6/);
  assert.match(workflow, /actions\/setup-node@v6/);
  assert.match(workflow, /Swatinem\/rust-cache@v2/);
  assert.match(workflow, /node-version:\s*['"]?24['"]?/);
  assert.match(workflow, /changes:/);
  assert.match(workflow, /static-checks:/);
  assert.match(workflow, /node-coverage:/);
  assert.match(workflow, /rust-quality:/);
  assert.match(workflow, /gate:/);
  assert.match(workflow, /if:\s*always\(\)/);
  assert.match(workflow, /cargo fmt --all -- --check/);
  assert.match(workflow, /cargo clippy --workspace --all-targets --locked -- -D warnings/);

  const nodeCov = read('scripts/test-coverage-node.mjs');
  for (const metric of ['--lines', '--statements', '--functions', '--branches']) {
    assert.ok(nodeCov.includes(metric), `missing ${metric} in test-coverage-node.mjs`);
  }
  assert.match(nodeCov, /'95'/);

  const rustCov = read('scripts/test-coverage-rust.mjs');
  assert.match(rustCov, /--fail-under-lines[\s\S]*95/);
  assert.match(rustCov, /--fail-under-functions[\s\S]*95/);
  assert.match(rustCov, /--fail-under-regions[\s\S]*95/);
});

test('toolchain, gitignore, prettierignore, launch.json, and knip config match policy', () => {
  const toolchain = read('rust-toolchain.toml');
  assert.match(toolchain, /channel\s*=\s*"stable"/);
  assert.match(toolchain, /components\s*=\s*\["rustfmt",\s*"clippy"\]/);

  const gitignore = read('.gitignore');
  assert.doesNotMatch(gitignore, /^Cargo\.lock$/m);
  assert.match(gitignore, /node_modules\//);
  assert.match(gitignore, /!\.vscode\/launch\.json/);

  const prettierIgnore = read('.prettierignore');
  assert.match(prettierIgnore, /docs\/specs\/\*\*\/\*\.md/);

  const launch = JSON.parse(read('.vscode/launch.json'));
  assert.equal(launch.configurations.length, 1);
  assert.match(launch.configurations[0].program, /launch-desktop\.mjs/);

  const knip = JSON.parse(read('knip.json'));
  assert.deepEqual(knip.project, ['scripts/**/*.mjs']);

  const locLint = read('scripts/loc-lint.mjs');
  assert.doesNotMatch(locLint, /legacyCeilings|allowlist|overrides/i);
  assert.match(locLint, /--others/);
  assert.match(locLint, /--exclude-standard/);
});
