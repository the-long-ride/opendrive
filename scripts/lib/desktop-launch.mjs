import path from 'node:path';

export function resolveDesktopBinary(repoRoot, platform = process.platform) {
  const exe = platform === 'win32' ? 'opendrive.exe' : 'opendrive';
  return path.join(repoRoot, 'target', 'debug', exe);
}

export function buildCargoBuildArgs() {
  return ['build', '-p', 'opendrive-app'];
}

export function buildDesktopArgs(extraArgs = []) {
  return ['ui', ...extraArgs];
}
