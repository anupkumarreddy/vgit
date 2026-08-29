import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import path from 'node:path';

const runFile = promisify(execFile);
const MAX_BUFFER = 20 * 1024 * 1024;

export type FileState = {
  path: string;
  originalPath?: string;
  staged: string;
  working: string;
  conflicted: boolean;
};

export type Commit = {
  hash: string;
  shortHash: string;
  parents: string[];
  author: string;
  email: string;
  date: string;
  subject: string;
  refs: string[];
};

export type RepoSnapshot = {
  path: string;
  name: string;
  branch: string;
  upstream: string | null;
  ahead: number;
  behind: number;
  files: FileState[];
  commits: Commit[];
  branches: Array<{ name: string; current: boolean; remote: boolean }>;
  remotes: Array<{ name: string; url: string }>;
  stashes: Array<{ index: number; message: string }>;
};

export async function git(cwd: string, args: string[], allowFailure = false): Promise<string> {
  try {
    const { stdout } = await runFile('git', ['--no-pager', ...args], {
      cwd,
      encoding: 'utf8',
      maxBuffer: MAX_BUFFER,
      env: { ...process.env, GIT_TERMINAL_PROMPT: '0', LC_ALL: 'C' }
    });
    return stdout;
  } catch (error) {
    if (allowFailure) return '';
    const failure = error as { stderr?: string; stdout?: string; message?: string };
    throw new Error((failure.stderr || failure.stdout || failure.message || 'Git operation failed').trim());
  }
}

export function parseStatus(raw: string): FileState[] {
  const entries = raw.split('\0');
  const files: FileState[] = [];
  for (let i = 0; i < entries.length; i += 1) {
    const entry = entries[i];
    if (!entry || entry.startsWith('#')) continue;
    if (entry.startsWith('? ')) {
      files.push({ path: entry.slice(2), staged: '?', working: '?', conflicted: false });
      continue;
    }
    if (entry.startsWith('! ')) continue;
    const kind = entry[0];
    const fields = entry.split(' ');
    if (kind === '1') {
      files.push({ path: fields.slice(8).join(' '), staged: fields[1][0], working: fields[1][1], conflicted: false });
    } else if (kind === '2') {
      const currentPath = fields.slice(9).join(' ');
      const originalPath = entries[++i];
      files.push({ path: currentPath, originalPath, staged: fields[1][0], working: fields[1][1], conflicted: false });
    } else if (kind === 'u') {
      files.push({ path: fields.slice(10).join(' '), staged: fields[1][0], working: fields[1][1], conflicted: true });
    }
  }
  return files;
}

export function parseLog(raw: string): Commit[] {
  return raw.split('\x1e').filter(Boolean).map(record => {
    const [hash, shortHash, parents, author, email, date, subject, refs] = record.replace(/^\n/, '').split('\x1f');
    return {
      hash,
      shortHash,
      parents: parents ? parents.split(' ') : [],
      author,
      email,
      date,
      subject,
      refs: refs ? refs.split(', ').filter(Boolean) : []
    };
  });
}

export async function assertRepository(repoPath: string): Promise<string> {
  const resolved = path.resolve(repoPath);
  const root = (await git(resolved, ['rev-parse', '--show-toplevel'])).trim();
  return root;
}

export async function snapshot(repoPath: string): Promise<RepoSnapshot> {
  const root = await assertRepository(repoPath);
  const [status, log, branchRaw, remoteRaw, stashRaw] = await Promise.all([
    git(root, ['status', '--porcelain=v2', '--branch', '-z']),
    git(root, ['log', '--all', '--topo-order', '--date=iso-strict', '--max-count=500', '--pretty=format:%x1e%H%x1f%h%x1f%P%x1f%an%x1f%ae%x1f%aI%x1f%s%x1f%D'], true),
    git(root, ['branch', '--all', '--format=%(HEAD)%09%(refname)']),
    git(root, ['remote', '-v'], true),
    git(root, ['stash', 'list', '--format=%gd%x09%s'], true)
  ]);

  const header = status.split('\0').filter(line => line.startsWith('# '));
  const branch = header.find(line => line.startsWith('# branch.head '))?.slice(14) || 'HEAD';
  const upstream = header.find(line => line.startsWith('# branch.upstream '))?.slice(18) || null;
  const ab = header.find(line => line.startsWith('# branch.ab '))?.match(/\+(\d+) -(\d+)/);
  const remoteMap = new Map<string, string>();
  remoteRaw.split('\n').filter(Boolean).forEach(line => {
    const [name, url, type] = line.split(/\s+/);
    if (type === '(fetch)' && !remoteMap.has(name)) remoteMap.set(name, url);
  });

  return {
    path: root,
    name: path.basename(root),
    branch,
    upstream,
    ahead: Number(ab?.[1] || 0),
    behind: Number(ab?.[2] || 0),
    files: parseStatus(status),
    commits: parseLog(log),
    branches: branchRaw.split('\n').filter(Boolean).map(line => {
      const [marker, ref] = line.split('\t');
      const remote = ref.startsWith('refs/remotes/');
      const name = ref.replace(/^refs\/heads\//, '').replace(/^refs\/remotes\//, '');
      return { name, current: marker === '*', remote };
    }),
    remotes: [...remoteMap].map(([name, url]) => ({ name, url })),
    stashes: stashRaw.split('\n').filter(Boolean).map((line, index) => ({ index, message: line.split('\t').slice(1).join('\t') }))
  };
}
