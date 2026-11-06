import { spawnSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const DOCS_ONLY_FILES = new Set(['.prettierignore', '.prettierrc.json', 'LICENSE', 'LICENSE.md']);

const RUST_ROOT_FILES = new Set(['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml']);

const NODE_ROOT_FILES = new Set(['package.json', 'package-lock.json', 'knip.json']);

export function normalizePath(filePath) {
  return filePath.replaceAll('\\', '/').replace(/^\.\//, '');
}

function isDocsOrMeta(file) {
  if (file.startsWith('docs/')) return true;
  if (DOCS_ONLY_FILES.has(file)) return true;
  return /\.(?:md|mdx|txt|rst)$/i.test(file);
}

export function classifyChanges(rawFiles) {
  const files = rawFiles.map(normalizePath).filter(Boolean);
  if (files.length === 0) {
    return { node: false, rust: false };
  }

  let node = false;
  let rust = false;

  for (const file of files) {
    if (isDocsOrMeta(file)) {
      continue;
    }
    if (file.startsWith('crates/') || RUST_ROOT_FILES.has(file)) {
      rust = true;
      continue;
    }
    if (file.startsWith('scripts/') || NODE_ROOT_FILES.has(file)) {
      node = true;
      continue;
    }
    // Workflows, .gitignore, or unknown root files trigger both areas.
    return { node: true, rust: true };
  }

  return { node, rust };
}

export function getPrChangedFiles(baseSha, cwd = process.cwd()) {
  const result = spawnSync('git', ['diff', '--name-only', `${baseSha}...HEAD`], {
    cwd,
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    throw new Error(result.stderr);
  }
  return result.stdout.split(/\r?\n/).filter(Boolean);
}

function writeOutputs(areas) {
  const outputFile = process.env.GITHUB_OUTPUT;
  if (outputFile) {
    appendFileSync(outputFile, `node=${areas.node}\nrust=${areas.rust}\n`, 'utf8');
  }
  console.log(JSON.stringify(areas));
}

if (import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const eventName = process.env.GITHUB_EVENT_NAME ?? '';
  const baseSha = process.env.BASE_SHA ?? '';
  if (eventName !== 'pull_request' || !baseSha) {
    writeOutputs({ node: true, rust: true });
  } else {
    const changed = getPrChangedFiles(baseSha, process.cwd());
    writeOutputs(classifyChanges(changed));
  }
}
