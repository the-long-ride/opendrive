import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import {
  buildCargoBuildArgs,
  buildDesktopArgs,
  resolveDesktopBinary,
} from '../lib/desktop-launch.mjs';

test('resolveDesktopBinary returns platform-specific debug executable path', () => {
  const root = path.resolve('repo-root');
  assert.equal(
    resolveDesktopBinary(root, 'win32'),
    path.join(root, 'target', 'debug', 'opendrive.exe'),
  );
  assert.equal(
    resolveDesktopBinary(root, 'linux'),
    path.join(root, 'target', 'debug', 'opendrive'),
  );
  assert.equal(
    resolveDesktopBinary(root, 'darwin'),
    path.join(root, 'target', 'debug', 'opendrive'),
  );
  assert.ok(
    resolveDesktopBinary(root).endsWith(
      process.platform === 'win32' ? 'opendrive.exe' : 'opendrive',
    ),
  );
});

test('buildCargoBuildArgs and buildDesktopArgs produce native desktop launch args', () => {
  assert.deepEqual(buildCargoBuildArgs(), ['build', '-p', 'opendrive-app']);
  assert.deepEqual(buildDesktopArgs(), ['ui']);
  assert.deepEqual(buildDesktopArgs(['--help']), ['ui', '--help']);
});
