export type FileState = { path: string; originalPath?: string; staged: string; working: string; conflicted: boolean };
export type Commit = { hash: string; shortHash: string; parents: string[]; author: string; email: string; date: string; subject: string; refs: string[] };
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

export type VGitAPI = {
  initialRepository(): Promise<RepoSnapshot | null>;
  chooseRepository(): Promise<RepoSnapshot | null>;
  openRepository(path: string): Promise<RepoSnapshot>;
  refresh(): Promise<RepoSnapshot>;
  diff(path: string, staged: boolean): Promise<string>;
  showCommit(hash: string): Promise<string>;
  stage(path: string): Promise<RepoSnapshot>;
  unstage(path: string): Promise<RepoSnapshot>;
  stageAll(): Promise<RepoSnapshot>;
  unstageAll(): Promise<RepoSnapshot>;
  discard(path: string, untracked: boolean): Promise<RepoSnapshot>;
  commit(message: string, amend: boolean): Promise<RepoSnapshot>;
  checkout(branch: string): Promise<RepoSnapshot>;
  createBranch(branch: string): Promise<RepoSnapshot>;
  fetch(): Promise<RepoSnapshot>;
  pull(): Promise<RepoSnapshot>;
  push(): Promise<RepoSnapshot>;
  stash(message: string): Promise<RepoSnapshot>;
  stashApply(index: number): Promise<RepoSnapshot>;
  reveal(): Promise<void>;
};

declare global { interface Window { vgit: VGitAPI } }
