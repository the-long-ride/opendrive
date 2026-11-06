import path from 'node:path';

export const LIMITS = new Map([
  // Group A: 300 lines (Systems, Backend, Scripting, Query & IDL)
  ['.rs', 300],
  ['.cs', 300],
  ['.go', 300],
  ['.py', 300],
  ['.pyi', 300],
  ['.rb', 300],
  ['.java', 300],
  ['.kt', 300],
  ['.kts', 300],
  ['.swift', 300],
  ['.c', 300],
  ['.h', 300],
  ['.cc', 300],
  ['.cpp', 300],
  ['.cxx', 300],
  ['.hh', 300],
  ['.hpp', 300],
  ['.hxx', 300],
  ['.zig', 300],
  ['.lua', 300],
  ['.sh', 300],
  ['.bash', 300],
  ['.zsh', 300],
  ['.ps1', 300],
  ['.psm1', 300],
  ['.psd1', 300],
  ['.sql', 300],
  ['.proto', 300],
  ['.graphql', 300],
  ['.gql', 300],

  // Group B: 350 lines (Application Languages, Web Logic & UI Components)
  ['.ts', 350],
  ['.mts', 350],
  ['.cts', 350],
  ['.js', 350],
  ['.mjs', 350],
  ['.cjs', 350],
  ['.tsx', 350],
  ['.jsx', 350],
  ['.vue', 350],
  ['.svelte', 350],
  ['.astro', 350],
  ['.dart', 350],
  ['.php', 350],
  ['.scala', 350],
  ['.sc', 350],
  ['.ex', 350],
  ['.exs', 350],
  ['.erl', 350],
  ['.hrl', 350],
  ['.hs', 350],
  ['.ml', 350],
  ['.mli', 350],
  ['.fs', 350],
  ['.fsi', 350],
  ['.fsx', 350],
  ['.r', 350],
  ['.jl', 350],
  ['.sol', 350],

  // Group C: 400 lines (Stylesheets & HTML/Razor Markup Templates)
  ['.css', 400],
  ['.scss', 400],
  ['.sass', 400],
  ['.less', 400],
  ['.html', 400],
  ['.htm', 400],
  ['.cshtml', 400],
  ['.razor', 400],
]);

export const EXCLUDED_SEGMENTS = new Set([
  'dist',
  'build',
  'out',
  'target',
  'bin',
  'obj',
  'coverage',
  'node_modules',
  'vendor',
  'artifacts',
  'test-results',
  'playwright-report',
  '.git',
  '.test-dist',
  '.coverage-dist',
  '.coverage-v8',
  '.vscode-test',
  '.work-tracking',
  '.next',
  '.nuxt',
  '.svelte-kit',
  '.turbo',
  '__pycache__',
  '.venv',
  'venv',
]);

export const EXCLUDED_EXTENSIONS = new Set([
  '.resx',
  '.resources',
  '.json',
  '.jsonc',
  '.json5',
  '.jsonl',
  '.ndjson',
  '.csv',
  '.tsv',
  '.parquet',
  '.avro',
  '.yaml',
  '.yml',
  '.toml',
  '.xml',
  '.plist',
  '.ini',
  '.cfg',
  '.conf',
  '.properties',
  '.env',
  '.db',
  '.sqlite',
  '.sqlite3',
  '.sqlitedb',
  '.mdb',
  '.ldf',
  '.wal',
  '.shm',
  '.lock',
  '.sum',
  '.mod',
  '.md',
  '.mdx',
  '.txt',
  '.rst',
  '.adoc',
  '.svg',
  '.png',
  '.jpg',
  '.jpeg',
  '.gif',
  '.webp',
  '.ico',
  '.bmp',
  '.tiff',
  '.woff',
  '.woff2',
  '.ttf',
  '.otf',
  '.eot',
  '.mp3',
  '.mp4',
  '.wav',
  '.ogg',
  '.webm',
  '.pdf',
  '.zip',
  '.tar',
  '.gz',
  '.tgz',
  '.7z',
  '.rar',
  '.wasm',
  '.dll',
  '.exe',
  '.so',
  '.dylib',
  '.a',
  '.lib',
  '.pdb',
  '.bin',
  '.dat',
  '.patch',
]);

const EXCLUDED_SUFFIXES = [
  '.d.ts',
  '.min.js',
  '.min.css',
  '.bundle.js',
  '.g.cs',
  '.g.i.cs',
  '.designer.cs',
  '.pb.go',
  '_pb2.py',
  '.generated.ts',
];

export function sourceLimit(file) {
  const normalized = file.replaceAll('\\', '/');
  const lower = normalized.toLowerCase();
  const parts = normalized.split('/').filter(Boolean);
  if (parts.some((part) => EXCLUDED_SEGMENTS.has(part) || part.startsWith('.test-dist-'))) {
    return null;
  }
  if (EXCLUDED_SUFFIXES.some((suffix) => lower.endsWith(suffix))) {
    return null;
  }
  const ext = path.extname(lower);
  if (EXCLUDED_EXTENSIONS.has(ext)) {
    return null;
  }
  return LIMITS.get(ext) ?? null;
}

export function countPhysicalLines(text) {
  if (text.length === 0) return 0;
  return text.endsWith('\n') ? text.split('\n').length - 1 : text.split('\n').length;
}

export function looksArtificiallyCompressed(text) {
  const lines = text.split(/\r?\n/).filter(Boolean);
  return lines.some((line) => line.length > 500 && (line.match(/[;{}]/g) ?? []).length >= 20);
}
