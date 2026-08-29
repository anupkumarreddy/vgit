import { app, BrowserWindow, dialog, ipcMain, shell } from 'electron';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { git, snapshot, assertRepository } from './git.js';

const dirname = path.dirname(fileURLToPath(import.meta.url));
let mainWindow: BrowserWindow | null = null;
let activeRepo: string | null = null;

function requireRepo(): string {
  if (!activeRepo) throw new Error('Open a repository first.');
  return activeRepo;
}

async function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1480,
    height: 940,
    minWidth: 1080,
    minHeight: 700,
    titleBarStyle: process.platform === 'darwin' ? 'hiddenInset' : 'default',
    backgroundColor: '#0b0f14',
    webPreferences: {
      preload: path.join(dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true
    }
  });
  if (process.env.VITE_DEV_SERVER_URL) await mainWindow.loadURL(process.env.VITE_DEV_SERVER_URL);
  else await mainWindow.loadFile(path.join(dirname, '../dist/index.html'));
}

function handle<T extends unknown[], R>(channel: string, fn: (...args: T) => Promise<R>) {
  ipcMain.handle(channel, async (_event, ...args: T) => fn(...args));
}

app.whenReady().then(() => {
  handle('repo:initial', async () => {
    const requested = process.env.VGIT_INITIAL_REPO;
    if (!requested) return null;
    activeRepo = await assertRepository(requested);
    return snapshot(activeRepo);
  });
  handle('repo:choose', async () => {
    const result = await dialog.showOpenDialog(mainWindow!, { properties: ['openDirectory'] });
    if (result.canceled || !result.filePaths[0]) return null;
    activeRepo = await assertRepository(result.filePaths[0]);
    return snapshot(activeRepo);
  });
  handle('repo:open', async (repoPath: string) => {
    activeRepo = await assertRepository(repoPath);
    return snapshot(activeRepo);
  });
  handle('repo:refresh', async () => snapshot(requireRepo()));
  handle('repo:diff', async (filePath: string, staged: boolean) => {
    const repo = requireRepo();
    return git(repo, ['diff', '--no-ext-diff', '--no-color', ...(staged ? ['--cached'] : []), '--', filePath]);
  });
  handle('repo:showCommit', async (hash: string) => git(requireRepo(), ['show', '--stat', '--patch', '--no-ext-diff', '--no-color', hash]));
  handle('repo:stage', async (filePath: string) => { await git(requireRepo(), ['add', '--', filePath]); return snapshot(requireRepo()); });
  handle('repo:unstage', async (filePath: string) => { await git(requireRepo(), ['reset', '--', filePath]); return snapshot(requireRepo()); });
  handle('repo:stageAll', async () => { await git(requireRepo(), ['add', '-A']); return snapshot(requireRepo()); });
  handle('repo:unstageAll', async () => { await git(requireRepo(), ['reset']); return snapshot(requireRepo()); });
  handle('repo:discard', async (filePath: string, untracked: boolean) => {
    if (untracked) await git(requireRepo(), ['clean', '-f', '--', filePath]);
    else await git(requireRepo(), ['restore', '--worktree', '--', filePath]);
    return snapshot(requireRepo());
  });
  handle('repo:commit', async (message: string, amend: boolean) => {
    if (!message.trim() && !amend) throw new Error('A commit message is required.');
    await git(requireRepo(), ['commit', ...(amend ? ['--amend'] : []), ...(message.trim() ? ['-m', message.trim()] : ['--no-edit'])]);
    return snapshot(requireRepo());
  });
  handle('repo:checkout', async (branch: string) => { await git(requireRepo(), ['switch', branch]); return snapshot(requireRepo()); });
  handle('repo:createBranch', async (branch: string) => { await git(requireRepo(), ['switch', '-c', branch]); return snapshot(requireRepo()); });
  handle('repo:fetch', async () => { await git(requireRepo(), ['fetch', '--all', '--prune']); return snapshot(requireRepo()); });
  handle('repo:pull', async () => { await git(requireRepo(), ['pull', '--ff-only']); return snapshot(requireRepo()); });
  handle('repo:push', async () => {
    const repo = requireRepo();
    const state = await snapshot(repo);
    await git(repo, state.upstream ? ['push'] : ['push', '-u', 'origin', state.branch]);
    return snapshot(repo);
  });
  handle('repo:stash', async (message: string) => { await git(requireRepo(), ['stash', 'push', '-u', '-m', message || 'VGit stash']); return snapshot(requireRepo()); });
  handle('repo:stashApply', async (index: number) => { await git(requireRepo(), ['stash', 'apply', `stash@{${index}}`]); return snapshot(requireRepo()); });
  handle('repo:reveal', async () => { await shell.openPath(requireRepo()); });
  createWindow();
  app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); });
});

app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit(); });
