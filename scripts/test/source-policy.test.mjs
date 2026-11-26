import assert from 'node:assert/strict';
import test from 'node:test';
import {
  EXCLUDED_EXTENSIONS,
  EXCLUDED_SEGMENTS,
  LIMITS,
  countPhysicalLines,
  looksArtificiallyCompressed,
  sourceLimit,
} from '../lib/source-policy.mjs';

test('source limits match the full programming language index', () => {
  assert.equal(LIMITS.get('.rs'), 300);
  assert.ok(EXCLUDED_SEGMENTS.has('node_modules'));
  assert.ok(EXCLUDED_EXTENSIONS.has('.json'));
  // 300-line group
  assert.equal(sourceLimit('helper/src/main.rs'), 300);
  assert.equal(sourceLimit('services/Auth/UserService.cs'), 300);
  assert.equal(sourceLimit('cmd/server/main.go'), 300);
  assert.equal(sourceLimit('scripts/worker.py'), 300);
  assert.equal(sourceLimit('db/schema.sql'), 300);

  // 350-line group
  assert.equal(sourceLimit('apps/core/src/runtime.ts'), 350);
  assert.equal(sourceLimit('apps/web-react/src/App.tsx'), 350);
  assert.equal(sourceLimit('apps/web/Component.vue'), 350);
  assert.equal(sourceLimit('apps/web/Widget.svelte'), 350);
  assert.equal(sourceLimit('apps/web/main.js'), 350);

  // 400-line group
  assert.equal(sourceLimit('apps/web/styles/base.css'), 400);
  assert.equal(sourceLimit('apps/web/styles/theme.scss'), 400);
  assert.equal(sourceLimit('apps/web/index.html'), 400);
  assert.equal(sourceLimit('Views/Home/Index.cshtml'), 400);
});

test('resource, data, binary, and generated paths are excluded', () => {
  assert.equal(sourceLimit('Resources/Strings.resx'), null);
  assert.equal(sourceLimit('config/settings.json'), null);
  assert.equal(sourceLimit('fixtures/seed.csv'), null);
  assert.equal(sourceLimit('state/app.db'), null);
  assert.equal(sourceLimit('state/app.sqlite3'), null);
  assert.equal(sourceLimit('src/types/env.d.ts'), null);
  assert.equal(sourceLimit('dist/apps/web/app.js'), null);
  assert.equal(sourceLimit('node_modules/x/index.js'), null);
  assert.equal(sourceLimit('target/release/build.rs'), null);
  assert.equal(sourceLimit('bin/Debug/net8.0/App.cs'), null);
  assert.equal(sourceLimit('obj/Debug/App.g.cs'), null);
});

test('physical line counting ignores a trailing empty split', () => {
  assert.equal(countPhysicalLines('a\nb\n'), 2);
  assert.equal(countPhysicalLines('a\nb'), 2);
  assert.equal(countPhysicalLines(''), 0);
});

test('artificially compressed source is rejected', () => {
  assert.equal(looksArtificiallyCompressed('const a=1;const b=2;const c=3;'.repeat(30)), true);
  assert.equal(looksArtificiallyCompressed('a'.repeat(550)), false);
  assert.equal(looksArtificiallyCompressed('const a = 1;\nconst b = 2;\n'), false);
});
