import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  buildDownloadAssetsTable,
  escapeHtml,
  extractChangelog,
  formatContributorsFlow,
  generateReleaseNotesFromEnv,
  getGitFallbackContributors,
  getPreviousTag,
  renderReleaseNotes,
  resolveContributors,
} from '../generate-release-notes.mjs';

const scriptPath = fileURLToPath(new URL('../generate-release-notes.mjs', import.meta.url));

let sharedRepoDir = '';

function getSharedRepo() {
  if (sharedRepoDir) return sharedRepoDir;
  const dir = mkdtempSync(path.join(tmpdir(), 'opendrive-relnotes-'));
  const git = (...args) => {
    const res = spawnSync('git', args, { cwd: dir, encoding: 'utf8' });
    assert.equal(res.status, 0, res.stderr);
  };
  git('init', '-b', 'main');
  writeFileSync(path.join(dir, 'README.md'), 'init\n');
  git('add', 'README.md');
  git('-c', 'user.email=ci@example.org', '-c', 'user.name=Initial Author', 'commit', '-m', 'init');
  git('tag', 'v0.1.0');

  writeFileSync(path.join(dir, 'CHANGELOG.md'), '## [v0.2.0] - 2026-11-26\n\n- Added stuff\n');
  git('add', 'CHANGELOG.md');
  git(
    '-c',
    'user.email=ci@example.org',
    '-c',
    'user.name=Thế Long',
    'commit',
    '-m',
    'feat: v0.2.0',
  );

  writeFileSync(path.join(dir, 'extra.txt'), 'more\n');
  git('add', 'extra.txt');
  git('-c', 'user.email=ci@example.org', '-c', 'user.name=the-long-ride', 'commit', '-m', 'chore');
  git('tag', 'v0.2.0');
  sharedRepoDir = dir;
  return dir;
}

test.after(() => {
  if (sharedRepoDir) rmSync(sharedRepoDir, { recursive: true, force: true });
});

test('extractChangelog matches tag/version headings, ignores prefix collisions, and falls back', () => {
  const md = [
    '# Changelog',
    '## [v0.2.0-rc1] - 2026-11-20',
    '- RC item',
    '## [v0.2.0] - 2026-11-26',
    '### Added',
    '- Feature A',
    '## [0.1.5]',
    '- Patch B',
    '## v0.1.0',
    '- Initial',
  ].join('\n');

  assert.equal(extractChangelog(md, 'v0.2.0', '0.2.0'), '### Added\n- Feature A');
  assert.equal(extractChangelog(md, 'v0.1.5', '0.1.5'), '- Patch B');
  assert.equal(extractChangelog(md, 'v0.1.0', '0.1.0'), '- Initial');
  assert.equal(extractChangelog(md, 'v9.9.9', '9.9.9'), 'Release v9.9.9.');
});

test('buildDownloadAssetsTable renders all 8 asset filenames as plain download links', () => {
  const table = buildDownloadAssetsTable('v0.2.0', '0.2.0', 'the-long-ride/opendrive');
  const expectedFiles = [
    'opendrive-v0.2.0-windows-x86_64-setup.exe',
    'opendrive-v0.2.0-windows-x86_64-portable.zip',
    'opendrive-v0.2.0-macos-arm64-installer.dmg',
    'opendrive-v0.2.0-macos-arm64-portable.zip',
    'opendrive-v0.2.0-linux-x86_64.AppImage',
    'opendrive_0.2.0_amd64.deb',
    'opendrive-v0.2.0-linux-x86_64.tar.gz',
    'SHA256SUMS.txt',
  ];
  for (const file of expectedFiles) {
    const url = `https://github.com/the-long-ride/opendrive/releases/download/v0.2.0/${file}`;
    assert.ok(table.includes(`[${file}](${url})`), `missing link for ${file}`);
    assert.ok(!table.includes(`\`${file}\``), `unexpected backticks around ${file}`);
  }
});

test('escapeHtml and formatContributorsFlow format clickable flow badges and fallbacks', () => {
  assert.equal(escapeHtml(`A & <B> "C" 'D'`), 'A &amp; &lt;B&gt; &quot;C&quot; &#39;D&#39;');
  assert.equal(formatContributorsFlow([]), 'No contributors listed.');

  const html = formatContributorsFlow([
    {
      login: 'the-long-ride',
      name: 'Thế Long',
      avatarUrl: 'https://avatars.githubusercontent.com/u/73347418?v=4',
      profileUrl: 'https://github.com/the-long-ride',
    },
    { login: 'octocat', name: 'octocat' },
    { name: 'Git <Author>' },
  ]);

  assert.ok(html.startsWith('<p>') && html.endsWith('</p>'));
  assert.ok(
    html.includes(
      '<a href="https://github.com/the-long-ride"><img src="https://avatars.githubusercontent.com/u/73347418?v=4&amp;s=48" width="24" height="24" align="absmiddle" alt="@the-long-ride" />&nbsp;Thế&nbsp;Long&nbsp;(@the-long-ride)</a>',
    ),
  );
  assert.ok(
    html.includes(
      ' &nbsp; <a href="https://github.com/octocat"><img src="https://github.com/octocat.png?s=48" width="24" height="24" align="absmiddle" alt="@octocat" />&nbsp;@octocat</a>',
    ),
  );
  assert.ok(html.includes(' &nbsp; <span>Git &lt;Author&gt;</span>'));
});

test('renderReleaseNotes omits Change log heading and renders details, assets, and contributors', () => {
  const notes = renderReleaseNotes({
    tag: 'v0.2.0',
    version: '0.2.0',
    repo: 'the-long-ride/opendrive',
    changelogBody: '',
    contributors: [{ login: 'the-long-ride', name: 'Thế Long' }],
  });
  assert.ok(!notes.includes('Change log for version'));
  assert.ok(
    notes.startsWith(
      '<details>\n<summary>Release notes for v0.2.0</summary>\n\nRelease v0.2.0.\n\n</details>',
    ),
  );
  assert.ok(notes.includes('Download assets:\n\n| OS | Package | File |'));
  assert.ok(notes.includes('Contributors:\n\n<p><a href="https://github.com/the-long-ride">'));
});

test('resolveContributors handles pagination, dedup, bots, unlinked names, and initial tag', async () => {
  const dir = process.cwd();
  const page1Commits = Array.from({ length: 100 }, (_, idx) => ({
    author: {
      login: 'the-long-ride',
      type: 'User',
      avatar_url: 'https://avatars.githubusercontent.com/u/73347418?v=4',
      html_url: 'https://github.com/the-long-ride',
    },
    commit: { author: { name: idx % 2 === 0 ? 'Thế Long' : 'the-long-ride' } },
  }));
  const page2Commits = [
    {
      author: { login: 'dependabot[bot]', type: 'Bot' },
      commit: { author: { name: 'dependabot' } },
    },
    { author: null, commit: { author: { name: 'Thế Long' } } },
    { author: null, commit: { author: { name: 'Unlinked Dev' } } },
    { author: null, commit: { author: { name: 'Unlinked Dev' } } },
  ];

  const seenAuth = [];
  const compareFetch = async (url, init) => {
    seenAuth.push(init.headers.Authorization);
    if (url.includes('/compare/') && url.includes('&page=1')) {
      return { ok: true, json: async () => ({ total_commits: 104, commits: page1Commits }) };
    }
    if (url.includes('/compare/') && url.includes('&page=2')) {
      return { ok: true, json: async () => ({ total_commits: 104, commits: page2Commits }) };
    }
    if (url.endsWith('/users/the-long-ride')) {
      return { ok: true, json: async () => ({ name: 'Thế Long' }) };
    }
    throw new Error(`Unexpected URL: ${url}`);
  };

  const compareList = await resolveContributors({
    repo: 'the-long-ride/opendrive',
    prevTag: 'v0.1.0',
    refSha: 'v0.2.0',
    token: 'tok-123',
    fetchImpl: compareFetch,
    cwd: dir,
  });
  assert.equal(seenAuth[0], 'Bearer tok-123');
  assert.deepEqual(compareList, [
    {
      login: 'the-long-ride',
      name: 'Thế Long',
      avatarUrl: 'https://avatars.githubusercontent.com/u/73347418?v=4',
      profileUrl: 'https://github.com/the-long-ride',
    },
    { name: 'Unlinked Dev' },
  ]);

  const initialFetch = async (url) => {
    if (url.includes('/commits?') && url.includes('&page=1')) {
      return {
        ok: true,
        json: async () => [
          {
            author: { login: 'alice', type: 'User' },
            commit: { author: { name: 'Alice Fallback' } },
          },
        ],
      };
    }
    return { ok: false, status: 404, json: async () => ({}) };
  };
  const initialList = await resolveContributors({
    repo: 'the-long-ride/opendrive',
    prevTag: '',
    refSha: 'v0.1.0',
    fetchImpl: initialFetch,
    cwd: dir,
  });
  assert.deepEqual(initialList, [
    { login: 'alice', name: 'Alice Fallback', avatarUrl: '', profileUrl: '' },
  ]);

  const emptyInitialList = await resolveContributors({
    repo: 'the-long-ride/opendrive',
    prevTag: '',
    refSha: 'v0.1.0',
    fetchImpl: async () => ({ ok: true, json: async () => [] }),
    cwd: dir,
  });
  assert.deepEqual(emptyInitialList, []);
});

test('getPreviousTag and git fallback work on temp repo and non-git directories', async (t) => {
  const dir = getSharedRepo();
  assert.equal(getPreviousTag('v0.2.0', dir), 'v0.1.0');
  assert.equal(getPreviousTag('v0.1.0', dir), '');

  const fallback = await resolveContributors({
    repo: 'the-long-ride/opendrive',
    prevTag: 'v0.1.0',
    refSha: 'v0.2.0',
    fetchImpl: async () => ({ ok: false, status: 500, json: async () => ({}) }),
    cwd: dir,
  });
  assert.deepEqual(fallback, [{ name: 'Thế Long' }, { name: 'the-long-ride' }]);

  const initialFallback = getGitFallbackContributors('', 'v0.1.0', dir);
  assert.deepEqual(initialFallback, [{ name: 'Initial Author' }]);

  const nonGitDir = mkdtempSync(path.join(tmpdir(), 'opendrive-nongit-'));
  t.after(() => rmSync(nonGitDir, { recursive: true, force: true }));
  assert.deepEqual(getGitFallbackContributors('', 'HEAD', nonGitDir), []);
});

test('generateReleaseNotesFromEnv and CLI entrypoint handle file, stdout, missing changelog, and errors', async (t) => {
  const dir = getSharedRepo();
  const server = createServer((req, res) => {
    res.setHeader('Content-Type', 'application/json');
    if (req.url?.includes('/compare/')) {
      const author = {
        login: 'the-long-ride',
        type: 'User',
        avatar_url: 'https://avatars.githubusercontent.com/u/73347418?v=4',
        html_url: 'https://github.com/the-long-ride',
      };
      return res.end(
        JSON.stringify({
          total_commits: 1,
          commits: [{ author, commit: { author: { name: 'the-long-ride' } } }],
        }),
      );
    }
    if (req.url === '/users/the-long-ride') return res.end(JSON.stringify({ name: 'Thế Long' }));
    res.statusCode = 404;
    res.end('{}');
  });

  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => {
    server.close();
    server.closeAllConnections?.();
  });
  const apiUrl = `http://127.0.0.1:${server.address().port}`;
  const baseEnv = {
    ...process.env,
    TAG: 'v0.2.0',
    VERSION: '0.2.0',
    REF_SHA: 'v0.2.0',
    GITHUB_REPOSITORY: 'the-long-ride/opendrive',
    GITHUB_API_URL: apiUrl,
  };
  const runCli = (extraEnv) =>
    new Promise((resolve, reject) => {
      const child = spawn(process.execPath, [scriptPath], {
        cwd: dir,
        env: { ...baseEnv, ...extraEnv },
      });
      let stdout = '';
      let stderr = '';
      child.stdout.on('data', (chunk) => {
        stdout += chunk;
      });
      child.stderr.on('data', (chunk) => {
        stderr += chunk;
      });
      child.on('error', reject);
      child.on('close', (status) => resolve({ status, stdout, stderr }));
    });

  const cliFileRun = await runCli({ OUTPUT_PATH: 'out.md' });
  assert.equal(cliFileRun.status, 0, cliFileRun.stderr);
  const written = readFileSync(path.join(dir, 'out.md'), 'utf8');
  assert.ok(!written.includes('Change log for version'));
  assert.ok(written.includes('- Added stuff'));
  assert.ok(written.includes('Thế&nbsp;Long&nbsp;(@the-long-ride)'));

  const cliStdoutRun = await runCli({ CHANGELOG_PATH: 'MISSING.md' });
  assert.equal(cliStdoutRun.status, 0, cliStdoutRun.stderr);
  assert.ok(cliStdoutRun.stdout.includes('Release v0.2.0.'));

  const cliErrRun = await runCli({ TAG: '' });
  assert.equal(cliErrRun.status, 1);
  assert.match(cliErrRun.stderr, /Missing required environment variable: TAG/);

  await assert.rejects(
    generateReleaseNotesFromEnv({ TAG: 'v0.2.0', VERSION: '0.2.0' }, dir),
    /Missing required environment variable: GITHUB_REPOSITORY/,
  );
});

test('release workflow delegates release notes generation to scripts/generate-release-notes.mjs', () => {
  const workflow = readFileSync(path.resolve('.github/workflows/release.yml'), 'utf8');
  assert.match(workflow, /actions\/setup-node@v6/);
  assert.match(workflow, /node scripts\/generate-release-notes\.mjs/);
  assert.doesNotMatch(workflow, /Change log for version/);
});
