import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import {
  countPhysicalLines,
  looksArtificiallyCompressed,
  sourceLimit,
} from './lib/source-policy.mjs';

function trackedFiles() {
  const result = spawnSync('git', ['ls-files', '-z'], { encoding: 'utf8' });
  if (result.status !== 0) {
    process.stderr.write(result.stderr || 'Unable to list tracked files.\n');
    process.exit(result.status ?? 1);
  }
  return result.stdout.split('\0').filter(Boolean);
}

// Temporary, file-specific ceilings for legacy files exceeding the normal 300-line limit.
// Any growth beyond the current baseline fails CI; new files keep the default limit.
const legacyCeilings = new Map([
  ['crates/opendrive-app/src/design/icons.rs', 337],
  ['crates/opendrive-app/src/design/widgets.rs', 313],
  ['crates/opendrive-app/src/desktop/mod.rs', 309],
  ['crates/opendrive-app/src/desktop/modals/guide.rs', 411],
  ['crates/opendrive-app/tests/bento_avatar_tray_tests.rs', 302],
  ['crates/opendrive-app/tests/desktop_render_tests.rs', 304],
  ['crates/opendrive-fs-windows/src/cfapi_sys.rs', 306],
]);

const failures = [];
for (const file of trackedFiles()) {
  if (!existsSync(file)) continue;
  const limit = sourceLimit(file);
  if (limit === null) continue;

  const text = readFileSync(file, 'utf8');
  const lines = countPhysicalLines(text);
  const ceiling = Math.max(limit, legacyCeilings.get(file) ?? limit);
  if (lines > ceiling) {
    failures.push(`${file}: ${lines} lines (limit ${ceiling})`);
  }
  if (looksArtificiallyCompressed(text)) {
    failures.push(`${file}: source appears artificially compressed`);
  }
}

if (failures.length > 0) {
  console.error(failures.join('\n'));
  process.exit(1);
}

console.log('lint:loc ok');
