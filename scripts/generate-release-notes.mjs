import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const HTML_ESCAPE_MAP = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
};

export function escapeHtml(text) {
  return String(text).replace(/[&<>"']/g, (ch) => HTML_ESCAPE_MAP[ch]);
}

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

export function extractChangelog(changelogText, tag, version) {
  const headingRe = new RegExp(
    `^## (?:\\[(?:${escapeRegExp(tag)}|${escapeRegExp(version)})\\]|${escapeRegExp(tag)})(?:\\s|$)`,
  );
  const lines = changelogText.split(/\r?\n/);
  const collected = [];
  let capturing = false;

  for (const line of lines) {
    if (!capturing) {
      if (headingRe.test(line)) {
        capturing = true;
      }
      continue;
    }
    if (line.startsWith('## ')) {
      break;
    }
    collected.push(line);
  }

  const body = collected.join('\n').trim();
  return body || `Release ${tag}.`;
}

export function buildDownloadAssetsTable(tag, version, repo) {
  const assets = [
    ['Windows (x86_64)', 'NSIS Installer', `opendrive-${tag}-windows-x86_64-setup.exe`],
    ['Windows (x86_64)', 'Portable ZIP', `opendrive-${tag}-windows-x86_64-portable.zip`],
    ['macOS (Apple Silicon)', 'DMG Installer', `opendrive-${tag}-macos-arm64-installer.dmg`],
    ['macOS (Apple Silicon)', 'Portable ZIP', `opendrive-${tag}-macos-arm64-portable.zip`],
    ['Linux (x86_64)', 'AppImage', `opendrive-${tag}-linux-x86_64.AppImage`],
    ['Linux (x86_64)', 'Debian (.deb)', `opendrive_${version}_amd64.deb`],
    ['Linux (x86_64)', 'Portable Tarball', `opendrive-${tag}-linux-x86_64.tar.gz`],
    ['All', 'Checksums', 'SHA256SUMS.txt'],
  ];
  const baseUrl = `https://github.com/${repo}/releases/download/${encodeURIComponent(tag)}`;
  const rows = assets.map(
    ([os, pkg, file]) => `| ${os} | ${pkg} | [${file}](${baseUrl}/${encodeURIComponent(file)}) |`,
  );
  return ['Download assets:', '', '| OS | Package | File |', '|---|---|---|', ...rows].join('\n');
}

export function formatContributorsFlow(contributors) {
  if (contributors.length === 0) {
    return 'No contributors listed.';
  }
  const items = contributors.map((contributor) => {
    if (!contributor.login) {
      return `<span>${escapeHtml(contributor.name)}</span>`;
    }
    const login = contributor.login;
    const profileUrl = contributor.profileUrl || `https://github.com/${encodeURIComponent(login)}`;
    const rawAvatar =
      contributor.avatarUrl || `https://github.com/${encodeURIComponent(login)}.png`;
    const sizedAvatar = `${rawAvatar}${rawAvatar.includes('?') ? '&' : '?'}s=48`;
    const displayName = (contributor.name || '').trim();
    const label =
      displayName && displayName.toLowerCase() !== login.toLowerCase()
        ? `${displayName} (@${login})`
        : `@${login}`;
    const nbspLabel = escapeHtml(label).replaceAll(' ', '&nbsp;');
    return (
      `<a href="${escapeHtml(profileUrl)}">` +
      `<img src="${escapeHtml(sizedAvatar)}" width="24" height="24" align="absmiddle" alt="@${escapeHtml(login)}" />` +
      `&nbsp;${nbspLabel}</a>`
    );
  });
  return `<p>${items.join(' &nbsp; ')}</p>`;
}

export function renderReleaseNotes({ tag, version, repo, changelogBody, contributors }) {
  const body = (changelogBody || '').trim() || `Release ${tag}.`;
  return [
    '<details>',
    `<summary>Release notes for ${tag}</summary>`,
    '',
    body,
    '',
    '</details>',
    '',
    buildDownloadAssetsTable(tag, version, repo),
    '',
    'Contributors:',
    '',
    formatContributorsFlow(contributors),
    '',
  ].join('\n');
}

export function getPreviousTag(refSha, cwd = process.cwd()) {
  const result = spawnSync('git', ['describe', '--tags', '--abbrev=0', `${refSha}^`], {
    cwd,
    encoding: 'utf8',
  });
  return result.status === 0 ? result.stdout.trim() : '';
}

export function getGitFallbackContributors(prevTag, refSha, cwd = process.cwd()) {
  const range = prevTag ? `${prevTag}..${refSha}` : refSha;
  const result = spawnSync('git', ['log', range, '--format=%an'], {
    cwd,
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    return [];
  }
  const unique = new Map();
  for (const rawName of result.stdout.split(/\r?\n/)) {
    const name = rawName.trim();
    if (name && !unique.has(name.toLowerCase())) {
      unique.set(name.toLowerCase(), name);
    }
  }
  return [...unique.values()]
    .sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase(), 'en'))
    .map((name) => ({ name }));
}

async function fetchGithubCommits({ repo, prevTag, refSha, githubJson }) {
  const commits = [];
  let page = 1;
  while (true) {
    if (prevTag) {
      const url = `${repo.apiBase}/repos/${repo.slug}/compare/${encodeURIComponent(prevTag)}...${encodeURIComponent(refSha)}?per_page=100&page=${page}`;
      const data = await githubJson(url);
      const pageCommits = Array.isArray(data.commits) ? data.commits : [];
      commits.push(...pageCommits);
      if (
        pageCommits.length < 100 ||
        (data.total_commits !== undefined && commits.length >= data.total_commits)
      ) {
        break;
      }
    } else {
      const url = `${repo.apiBase}/repos/${repo.slug}/commits?sha=${encodeURIComponent(refSha)}&per_page=100&page=${page}`;
      const pageCommits = await githubJson(url);
      if (!Array.isArray(pageCommits) || pageCommits.length === 0) {
        break;
      }
      commits.push(...pageCommits);
      if (pageCommits.length < 100) {
        break;
      }
    }
    page += 1;
  }
  return commits;
}

export async function resolveContributors({
  repo,
  prevTag,
  refSha,
  token = '',
  apiBase = 'https://api.github.com',
  fetchImpl = fetch,
  cwd = process.cwd(),
}) {
  const headers = {
    Accept: 'application/vnd.github+json',
    'User-Agent': 'opendrive-release-notes',
    'X-GitHub-Api-Version': '2022-11-28',
  };
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }
  const githubJson = async (url) => {
    const res = await fetchImpl(url, { headers });
    if (!res.ok) {
      throw new Error(`GitHub API ${res.status} for ${url}`);
    }
    return res.json();
  };

  try {
    const commits = await fetchGithubCommits({
      repo: { slug: repo, apiBase },
      prevTag,
      refSha,
      githubJson,
    });
    const linked = new Map();
    const unlinked = new Map();

    for (const commit of commits) {
      const author = commit.author;
      const gitName = (commit.commit?.author?.name || '').trim();
      if (author && author.login) {
        if (author.type === 'Bot' || author.login.endsWith('[bot]')) {
          continue;
        }
        const key = author.login.toLowerCase();
        if (!linked.has(key)) {
          linked.set(key, {
            login: author.login,
            avatarUrl: author.avatar_url || '',
            profileUrl: author.html_url || '',
            gitName,
          });
        }
      } else if (gitName && !unlinked.has(gitName.toLowerCase())) {
        unlinked.set(gitName.toLowerCase(), { name: gitName });
      }
    }

    const resolvedLinked = [];
    const knownKeys = new Set();
    for (const user of linked.values()) {
      let name = user.gitName;
      try {
        const profile = await githubJson(`${apiBase}/users/${encodeURIComponent(user.login)}`);
        if (profile.name && profile.name.trim()) {
          name = profile.name.trim();
        }
      } catch {
        // Fall back to commit author name when user lookup fails.
      }
      knownKeys.add(user.login.toLowerCase());
      if (name) {
        knownKeys.add(name.toLowerCase());
      }
      resolvedLinked.push({
        login: user.login,
        name,
        avatarUrl: user.avatarUrl,
        profileUrl: user.profileUrl,
      });
    }

    const remainingUnlinked = [...unlinked.entries()]
      .filter(([key]) => !knownKeys.has(key))
      .map(([, entry]) => entry);

    return [...resolvedLinked, ...remainingUnlinked].sort((a, b) =>
      (a.login || a.name).toLowerCase().localeCompare((b.login || b.name).toLowerCase(), 'en'),
    );
  } catch {
    return getGitFallbackContributors(prevTag, refSha, cwd);
  }
}

export async function generateReleaseNotesFromEnv(
  env = process.env,
  cwd = process.cwd(),
  fetchImpl = fetch,
) {
  for (const key of ['TAG', 'VERSION', 'GITHUB_REPOSITORY']) {
    if (!env[key]) {
      throw new Error(`Missing required environment variable: ${key}`);
    }
  }

  const tag = env.TAG;
  const version = env.VERSION;
  const repo = env.GITHUB_REPOSITORY;
  const refSha = env.REF_SHA || 'HEAD';
  const token = env.GH_TOKEN || env.GITHUB_TOKEN || '';
  const apiBase = env.GITHUB_API_URL || 'https://api.github.com';
  const changelogPath = path.resolve(cwd, env.CHANGELOG_PATH || 'CHANGELOG.md');
  const outputPath = env.OUTPUT_PATH ? path.resolve(cwd, env.OUTPUT_PATH) : '';

  const changelogText = existsSync(changelogPath) ? readFileSync(changelogPath, 'utf8') : '';
  const changelogBody = extractChangelog(changelogText, tag, version);
  const prevTag = getPreviousTag(refSha, cwd);
  const contributors = await resolveContributors({
    repo,
    prevTag,
    refSha,
    token,
    apiBase,
    fetchImpl,
    cwd,
  });
  const notes = renderReleaseNotes({ tag, version, repo, changelogBody, contributors });

  if (outputPath) {
    writeFileSync(outputPath, notes, 'utf8');
  } else {
    process.stdout.write(notes);
  }
  return notes;
}

if (import.meta.url === pathToFileURL(path.resolve(process.argv[1] ?? '')).href) {
  generateReleaseNotesFromEnv().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
