import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const IGNORE_REGEX =
  '(main\\.rs|types\\.rs|keyring\\.rs|fuse_sys\\.rs|cfapi_sys\\.rs|shell[/\\\\](window|tray|tray_loop)\\.rs)';

export function runRustCoverage(cwd = process.cwd()) {
  const args = [
    'llvm-cov',
    '--workspace',
    '--locked',
    '--fail-under-lines',
    '90',
    '--fail-under-functions',
    '90',
    '--fail-under-regions',
    '90',
    '--ignore-filename-regex',
    IGNORE_REGEX,
  ];
  const result = spawnSync('cargo', args, { cwd, stdio: 'inherit' });
  return result.status ?? 1;
}

if (import.meta.url === pathToFileURL(path.resolve(process.argv[1] ?? '')).href) {
  process.exit(runRustCoverage(process.cwd()));
}
