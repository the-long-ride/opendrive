import { spawn, spawnSync } from 'node:child_process';
import path from 'node:path';
import {
  buildCargoBuildArgs,
  buildDesktopArgs,
  resolveDesktopBinary,
} from './lib/desktop-launch.mjs';

function runStep(cmd, args, cwd) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { cwd, stdio: 'inherit' });
    child.on('error', reject);
    child.on('exit', (code, signal) => {
      resolve(code ?? (signal ? 1 : 0));
    });
  });
}

function stopChild(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) {
    return;
  }
  if (process.platform === 'win32' && child.pid) {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } else {
    child.kill('SIGTERM');
  }
}

async function main() {
  const repoRoot = path.resolve(import.meta.dirname, '..');
  const buildCode = await runStep('cargo', buildCargoBuildArgs(), repoRoot);
  if (buildCode !== 0) {
    process.exit(buildCode);
  }

  const binary = resolveDesktopBinary(repoRoot);
  const args = buildDesktopArgs(process.argv.slice(2));
  const appProc = spawn(binary, args, { cwd: repoRoot, stdio: 'inherit' });

  const forwardSignal = () => {
    stopChild(appProc);
  };
  process.on('SIGINT', forwardSignal);
  process.on('SIGTERM', forwardSignal);

  const exitCode = await new Promise((resolve, reject) => {
    appProc.on('error', reject);
    appProc.on('exit', (code, signal) => {
      resolve(code ?? (signal ? 1 : 0));
    });
  });
  process.exit(exitCode);
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : String(err));
  process.exit(1);
});
