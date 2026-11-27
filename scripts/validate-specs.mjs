import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const FIELDS = [
  'schema_version',
  'id',
  'title',
  'kind',
  'status',
  'summary',
  'tags',
  'source_paths',
  'related_specs',
  'last_doc_update_at',
  'last_verified_at',
  'last_verified_code_commit',
  'last_source_change_commit',
  'last_source_change_at',
  'source_fingerprint',
];
const KINDS = new Set(['feature', 'workflow', 'architecture', 'interface', 'data']);
const STATUSES = new Set(['verified', 'partial', 'needs-review', 'stale', 'retired']);
const LINK_RE = /(?<!!)\[[^\]]*\]\(([^)]+)\)/g;

function isInside(child, parent) {
  const rel = path.relative(parent, child);
  return rel === '' || (!rel.startsWith('..') && !path.isAbsolute(rel));
}

function walkMd(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walkMd(full, out);
    else if (entry.isFile() && entry.name.endsWith('.md')) out.push(full);
  }
  return out.sort();
}

export function computeSourceFingerprint(repoRoot, sourcePaths) {
  const hash = createHash('sha256');
  for (const rel of [...sourcePaths].sort()) {
    const norm = Buffer.from(
      readFileSync(path.join(repoRoot, rel), 'utf8').replaceAll('\r\n', '\n'),
      'utf8',
    );
    const lenBuf = Buffer.alloc(8);
    lenBuf.writeBigUInt64BE(BigInt(norm.length));
    hash.update(Buffer.from(`${rel}\0`, 'utf8'));
    hash.update(lenBuf);
    hash.update(norm);
  }
  return `sha256:${hash.digest('hex')}`;
}

function parseFrontmatter(text) {
  const norm = text.replaceAll('\r\n', '\n');
  if (!norm.startsWith('---\n')) throw new Error('missing YAML frontmatter');
  const end = norm.indexOf('\n---\n', 4);
  if (end === -1) throw new Error('unclosed YAML frontmatter');
  const front = norm.slice(4, end);
  const body = norm.slice(end + 5);
  const meta = {};
  for (const line of front.split('\n')) {
    if (!line.trim() || line.trimStart().startsWith('#')) continue;
    const colon = line.indexOf(':');
    if (colon === -1 || line.startsWith(' ') || line.startsWith('\t')) {
      throw new Error('frontmatter must be flat key: value entries');
    }
    const key = line.slice(0, colon).trim();
    const rawVal = line.slice(colon + 1).trim();
    if (key in meta) throw new Error(`duplicate metadata key ${key}`);
    try {
      meta[key] = JSON.parse(rawVal);
    } catch {
      if (!rawVal || /^[[{'"]/.test(rawVal)) {
        throw new Error(`invalid value for ${key}; use JSON-compatible inline YAML`);
      }
      meta[key] = rawVal;
    }
  }
  return { meta, body };
}

function validIso(value) {
  return (
    typeof value === 'string' &&
    value.includes('T') &&
    (value.endsWith('Z') || /[+-]\d{2}:\d{2}$/.test(value)) &&
    !Number.isNaN(Date.parse(value))
  );
}

function extractLinks(filePath, content, docsRoot, errors) {
  const targets = [];
  for (const match of content.matchAll(LINK_RE)) {
    const raw = match[1].trim().split(/\s+/)[0].replace(/^<|>$/g, '');
    if (/^(?:https?:|mailto:|#)/i.test(raw)) continue;
    const local = decodeURIComponent(raw.split('#')[0].split('?')[0]);
    if (!local.toLowerCase().endsWith('.md')) continue;
    const target = path.resolve(path.dirname(filePath), local);
    if (!isInside(target, docsRoot)) errors.push(`${filePath}: link escapes docs/specs: ${raw}`);
    else if (!existsSync(target) || !statSync(target).isFile())
      errors.push(`${filePath}: broken link: ${raw}`);
    else targets.push(target);
  }
  return targets;
}

export function validateSpecs(repoRoot = process.cwd(), docsRel = 'docs/specs') {
  const root = path.resolve(repoRoot);
  const docsRoot = path.resolve(root, docsRel);
  const index = path.join(docsRoot, 'INDEX.md');
  if (!existsSync(docsRoot) || !statSync(docsRoot).isDirectory()) {
    return { ok: false, errors: [`docs root does not exist: ${docsRoot}`] };
  }
  if (!existsSync(index) || !statSync(index).isFile()) {
    return { ok: false, errors: [`missing routing index: ${index}`] };
  }
  const errors = [];
  const files = walkMd(docsRoot);
  const ids = new Map();
  const graph = new Map();
  const parsed = [];

  for (const file of files) {
    const content = readFileSync(file, 'utf8');
    graph.set(path.resolve(file), extractLinks(file, content, docsRoot, errors));
    const name = path.basename(file);
    if (name === 'INDEX.md' || name === 'README.md') continue;
    let doc;
    try {
      doc = parseFrontmatter(content);
    } catch (err) {
      errors.push(`${file}: ${err.message}`);
      continue;
    }
    const { meta, body } = doc;
    if (FIELDS.some((k) => !(k in meta))) {
      for (const k of FIELDS) if (!(k in meta)) errors.push(`${file}: missing required field ${k}`);
      continue;
    }
    if (typeof meta.id !== 'string' || !/^[a-z0-9]+(?:[.-][a-z0-9]+)*$/.test(meta.id)) {
      errors.push(`${file}: invalid id`);
    } else if (ids.has(meta.id)) {
      errors.push(`${file}: duplicate id ${meta.id}`);
    } else {
      ids.set(meta.id, file);
    }
    if (meta.schema_version !== 1) errors.push(`${file}: unsupported schema_version`);
    if (!KINDS.has(meta.kind)) errors.push(`${file}: invalid kind`);
    if (!STATUSES.has(meta.status)) errors.push(`${file}: invalid status`);
    for (const k of ['title', 'summary']) {
      if (typeof meta[k] !== 'string' || !meta[k].trim()) errors.push(`${file}: invalid ${k}`);
    }
    for (const k of ['tags', 'source_paths', 'related_specs']) {
      if (!Array.isArray(meta[k]) || !meta[k].every((x) => typeof x === 'string' && x)) {
        errors.push(`${file}: ${k} must be an array of nonempty strings`);
      }
    }
    for (const k of ['last_doc_update_at', 'last_verified_at', 'last_source_change_at']) {
      if (meta[k] !== 'unavailable' && !validIso(meta[k])) errors.push(`${file}: invalid ${k}`);
    }
    for (const k of ['last_verified_code_commit', 'last_source_change_commit']) {
      if (
        typeof meta[k] !== 'string' ||
        (meta[k] !== 'unavailable' && !/^[0-9a-f]{7,40}$/.test(meta[k]))
      ) {
        errors.push(`${file}: invalid ${k}`);
      }
    }
    if (
      typeof meta.source_fingerprint !== 'string' ||
      !/^sha256:[0-9a-f]{64}$/.test(meta.source_fingerprint)
    ) {
      errors.push(`${file}: invalid source_fingerprint`);
    }
    const safePaths = [];
    for (const src of meta.source_paths) {
      const target = path.resolve(root, src);
      if (!isInside(target, root) || !existsSync(target) || !statSync(target).isFile()) {
        errors.push(`${file}: missing or unsafe source file: ${src}`);
      } else {
        safePaths.push(src);
      }
    }
    if (safePaths.length === meta.source_paths.length && safePaths.length > 0) {
      if (computeSourceFingerprint(root, safePaths) !== meta.source_fingerprint) {
        errors.push(
          `${file}: source_fingerprint mismatch (source code changed; reverify documentation)`,
        );
      }
    } else if (meta.source_paths.length === 0 && meta.status === 'verified') {
      errors.push(`${file}: verified spec has no source_paths`);
    }
    if (!body.trimStart().startsWith('# '))
      errors.push(`${file}: body must begin with an H1 heading`);
    if (meta.status !== 'retired' && !body.includes('## Code map'))
      errors.push(`${file}: missing ## Code map`);
    parsed.push({ file, meta });
  }

  for (const { file, meta } of parsed) {
    for (const rel of meta.related_specs) {
      if (rel === meta.id) errors.push(`${file}: related_specs self-reference`);
      else if (!ids.has(rel)) errors.push(`${file}: related_specs references missing ID: ${rel}`);
    }
  }
  const reached = new Set();
  const queue = [path.resolve(index)];
  while (queue.length > 0) {
    const cur = queue.pop();
    if (reached.has(cur)) continue;
    reached.add(cur);
    for (const next of graph.get(cur) ?? []) queue.push(next);
  }
  for (const file of files) {
    if (path.basename(file) !== 'README.md' && !reached.has(path.resolve(file))) {
      errors.push(`${file}: unreachable from INDEX.md`);
    }
  }
  return { ok: errors.length === 0, errors };
}

if (import.meta.url === pathToFileURL(path.resolve(process.argv[1] ?? '')).href) {
  const res = validateSpecs(process.cwd());
  for (const err of res.errors) console.error('ERROR:', err);
  if (!res.ok) process.exit(1);
  console.log('validate:specs ok');
}
