import { useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle, Archive, ArrowDown, ArrowUp, Box, GitBranch, Check, ChevronDown,
  ChevronRight, CircleDot, Code2, Copy, File, FileDiff, FolderGit2, GitCommitHorizontal,
  GitFork, GitMerge, History, LoaderCircle, Minus, MoreHorizontal, Plus, RefreshCw, Search,
  Settings, Tag, Undo2, X
} from 'lucide-react';
import type { Commit, FileState, RepoSnapshot } from './types';
import { classifyRef, layoutGraph } from './graph';

type Selection = { type: 'file'; file: FileState; staged: boolean } | { type: 'commit'; commit: Commit } | null;

function relativeTime(date: string) {
  const delta = Date.now() - new Date(date).getTime();
  const minutes = Math.floor(delta / 60000);
  if (minutes < 1) return 'now';
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d`;
  return new Date(date).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

function initials(name: string) {
  return name.split(/\s+/).map(part => part[0]).join('').slice(0, 2).toUpperCase();
}

function StatusBadge({ file, staged }: { file: FileState; staged: boolean }) {
  const code = staged ? file.staged : file.working;
  const label = file.conflicted ? '!' : code === '?' ? 'U' : code === 'M' ? 'M' : code === 'A' ? 'A' : code === 'D' ? 'D' : code === 'R' ? 'R' : code;
  return <span className={`file-status status-${label}`}>{label}</span>;
}

function FileRow({ file, staged, selected, onSelect, onAction }: {
  file: FileState; staged: boolean; selected: boolean; onSelect(): void; onAction(): void;
}) {
  const pieces = file.path.split('/');
  return (
    <button className={`file-row ${selected ? 'selected' : ''}`} onClick={onSelect}>
      <File size={14} /><span className="file-name">{pieces.pop()}</span>
      <span className="file-dir">{pieces.join('/')}</span>
      <StatusBadge file={file} staged={staged} />
      <span className="row-action" onClick={e => { e.stopPropagation(); onAction(); }} title={staged ? 'Unstage' : 'Stage'}>
        {staged ? <Minus size={14} /> : <Plus size={14} />}
      </span>
    </button>
  );
}

function FileGroup({ title, files, staged, selection, onSelect, onAction, onAll }: {
  title: string; files: FileState[]; staged: boolean; selection: Selection;
  onSelect(file: FileState): void; onAction(file: FileState): void; onAll(): void;
}) {
  const [open, setOpen] = useState(true);
  return (
    <section className="file-group">
      <div className="group-title" onClick={() => setOpen(!open)}>
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <span>{title}</span><span className="count">{files.length}</span>
        {!!files.length && <button onClick={e => { e.stopPropagation(); onAll(); }}>{staged ? 'Unstage all' : 'Stage all'}</button>}
      </div>
      {open && files.map(file => <FileRow key={`${staged}-${file.path}`} file={file} staged={staged}
        selected={selection?.type === 'file' && selection.file.path === file.path && selection.staged === staged}
        onSelect={() => onSelect(file)} onAction={() => onAction(file)} />)}
      {open && files.length === 0 && <div className="empty-group"><Check size={13} /> Nothing here</div>}
    </section>
  );
}

function Sidebar({ repo, onCheckout, onCreate, onStashApply }: {
  repo: RepoSnapshot; onCheckout(name: string): void; onCreate(): void; onStashApply(index: number): void;
}) {
  const [branchesOpen, setBranchesOpen] = useState(true);
  const locals = repo.branches.filter(branch => !branch.remote);
  const remoteBranches = repo.branches.filter(branch => branch.remote && !branch.name.endsWith('/HEAD'));
  return (
    <aside className="sidebar">
      <div className="brand"><div className="brand-mark"><GitFork size={20} /></div><span>VGit</span></div>
      <nav className="primary-nav">
        <button className="active"><History size={16} /> History</button>
        <button><FileDiff size={16} /> Changes <span className="nav-count">{repo.files.length}</span></button>
        <button><Archive size={16} /> Stashes <span className="nav-count">{repo.stashes.length}</span></button>
      </nav>
      <div className="side-section">
        <div className="side-heading" onClick={() => setBranchesOpen(!branchesOpen)}>
          {branchesOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />} Branches
          <button title="New branch" onClick={e => { e.stopPropagation(); onCreate(); }}><Plus size={14} /></button>
        </div>
        {branchesOpen && locals.map(branch => (
          <button key={branch.name} className={`branch-row ${branch.current ? 'current' : ''}`} onClick={() => !branch.current && onCheckout(branch.name)}>
            <GitBranch size={14} /><span>{branch.name}</span>{branch.current && <CircleDot size={12} />}
          </button>
        ))}
      </div>
      <div className="side-section">
        <div className="side-heading"><ChevronDown size={14} /> Remotes</div>
        {repo.remotes.map(remote => <div className="remote-block" key={remote.name}>
          <div className="remote-row"><Box size={14} /><span>{remote.name}</span><small>{remoteBranches.filter(branch => branch.name.startsWith(`${remote.name}/`)).length}</small></div>
          {remoteBranches.filter(branch => branch.name.startsWith(`${remote.name}/`)).map(branch =>
            <div className="remote-branch-row" key={branch.name}><GitBranch size={12}/><span>{branch.name.slice(remote.name.length + 1)}</span></div>)}
        </div>)}
      </div>
      {!!repo.stashes.length && <div className="side-section">
        <div className="side-heading"><ChevronDown size={14} /> Stashes</div>
        {repo.stashes.map(stash => <button className="stash-row" key={stash.index} onClick={() => onStashApply(stash.index)} title="Apply stash">
          <Archive size={14} /><span>{stash.message}</span>
        </button>)}
      </div>}
      <div className="sidebar-bottom"><button><Settings size={15} /> Settings</button></div>
    </aside>
  );
}

function RefBadge({ value }: { value: string }) {
  const ref = classifyRef(value);
  return <span className={`ref-badge ref-${ref.kind}`} title={value}>
    {ref.kind === 'head' ? <CircleDot size={10}/> : ref.kind === 'tag' ? <Tag size={10}/> : ref.kind === 'remote' ? <Box size={10}/> : <GitBranch size={10}/>}
    {ref.label}
  </span>;
}

function Topology({ commit, row, isHead }: { commit: Commit; row: ReturnType<typeof layoutGraph>[number]; isHead: boolean }) {
  const x = (lane: number) => 16 + Math.min(lane, 5) * 17;
  return <svg className="topology" viewBox="0 0 112 58" preserveAspectRatio="none" aria-label={`${commit.parents.length} parent commit${commit.parents.length === 1 ? '' : 's'}`}>
    {row.edges.map((edge, index) => <path key={`${edge.from}-${edge.to}-${index}`}
      d={`M ${x(edge.from)} 0 C ${x(edge.from)} 22, ${x(edge.to)} 36, ${x(edge.to)} 58`}
      stroke={edge.color} className={edge.merge ? 'merge-edge' : ''}/>)}
    {isHead && <circle cx={x(row.lane)} cy="29" r="10" className="head-halo"/>}
    {commit.parents.length > 1
      ? <rect x={x(row.lane) - 5} y="24" width="10" height="10" rx="2" transform={`rotate(45 ${x(row.lane)} 29)`} className="merge-node"/>
      : <circle cx={x(row.lane)} cy="29" r="5" className={isHead ? 'commit-node head-node' : 'commit-node'}/>}
  </svg>;
}

function CommitGraph({ commits, selected, filter, onSelect }: { commits: Commit[]; selected?: string; filter: string; onSelect(c: Commit): void }) {
  const visible = useMemo(() => {
    const query = filter.toLowerCase();
    return commits.filter(c => !query || `${c.subject} ${c.author} ${c.hash} ${c.refs.join(' ')}`.toLowerCase().includes(query));
  }, [commits, filter]);
  const rows = useMemo(() => layoutGraph(visible), [visible]);
  return (
    <div className="commit-list">
      <div className="commit-columns"><span>Topology</span><span>Commit & references</span><span>Author</span><span>When</span></div>
      {visible.map((commit, index) => {
        const isHead = commit.refs.some(ref => ref.startsWith('HEAD -> '));
        return <button className={`commit-row ${selected === commit.hash ? 'selected' : ''} ${isHead ? 'head-row' : ''}`} key={commit.hash} onClick={() => onSelect(commit)}>
          <span className="graph-cell"><Topology commit={commit} row={rows[index]} isHead={isHead}/></span>
          <span className="commit-main">
            <span className="commit-message-line"><span className="commit-subject">{commit.subject}</span>{commit.parents.length > 1 && <span className="merge-label"><GitMerge size={10}/> merge</span>}</span>
            <span className="commit-meta-line"><span className="short-sha">{commit.shortHash}</span><span className="ref-list">{commit.refs.map(ref => <RefBadge value={ref} key={ref}/>)}</span></span>
          </span>
          <span className="author-cell"><span className="avatar">{initials(commit.author)}</span><span>{commit.author}</span></span>
          <span className="age-cell"><b>{relativeTime(commit.date)}</b><small>{new Date(commit.date).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })}</small></span>
        </button>;
      })}
      {!visible.length && <div className="empty-search"><Search size={24} /><span>No commits match your search</span></div>}
    </div>
  );
}

function DiffView({ text }: { text: string }) {
  if (!text) return <div className="diff-empty"><FileDiff size={32} /><h3>No textual diff</h3><p>This file may be untracked, binary, or unchanged in this view.</p></div>;
  return <pre className="diff-view">{text.split('\n').map((line, i) => {
    const kind = line.startsWith('+') && !line.startsWith('+++') ? 'add' : line.startsWith('-') && !line.startsWith('---') ? 'del' : line.startsWith('@@') ? 'hunk' : line.startsWith('diff ') || line.startsWith('index ') ? 'meta' : '';
    return <span className={kind} key={i}><b>{i + 1}</b>{line || ' '}</span>;
  })}</pre>;
}

function Inspector({ selection, content, loading, onDiscard }: { selection: Selection; content: string; loading: boolean; onDiscard(): void }) {
  return <aside className="inspector">
    {!selection ? <div className="inspector-empty"><GitCommitHorizontal size={38} /><h3>Select a commit or file</h3><p>Inspect changes, metadata, and patches here.</p></div> : <>
      <div className={`inspector-head ${selection.type === 'commit' ? 'commit-inspector-head' : ''}`}>
        <div><span className="eyebrow">{selection.type === 'commit' ? 'COMMIT DETAILS' : selection.staged ? 'STAGED CHANGE' : 'WORKING CHANGE'}</span>
          <h2>{selection.type === 'commit' ? selection.commit.subject : selection.file.path.split('/').pop()}</h2>
          <p>{selection.type === 'commit' ? selection.commit.author : selection.file.path}</p>
        </div>
        {selection.type === 'file' && !selection.staged && <button className="icon-button danger" title="Discard changes" onClick={onDiscard}><Undo2 size={16} /></button>}
      </div>
      {selection.type === 'commit' && <div className="commit-facts">
        <div><span>Commit</span><code>{selection.commit.hash}</code><button title="Copy full hash" onClick={() => navigator.clipboard.writeText(selection.commit.hash)}><Copy size={12}/></button></div>
        <div><span>Author</span><b>{selection.commit.author}</b><small>{selection.commit.email}</small></div>
        <div><span>Date</span><b>{new Date(selection.commit.date).toLocaleString()}</b></div>
        {!!selection.commit.refs.length && <div className="inspector-refs"><span>Points to</span><section>{selection.commit.refs.map(ref => <RefBadge value={ref} key={ref}/>)}</section></div>}
        <div><span>Parents</span><section>{selection.commit.parents.length ? selection.commit.parents.map(parent => <code key={parent}>{parent.slice(0, 8)}</code>) : <small>Root commit</small>}</section></div>
      </div>}
      <div className="diff-toolbar"><span>{selection.type === 'commit' ? 'Commit patch' : 'Changes'}</span><button><Code2 size={14} /> Unified</button><button><MoreHorizontal size={15} /></button></div>
      {loading ? <div className="loading"><LoaderCircle className="spin" size={22} /> Loading diff…</div> : <DiffView text={content} />}
    </>}
  </aside>;
}

function Welcome({ onOpen, loading, error }: { onOpen(): void; loading: boolean; error: string }) {
  return <main className="welcome">
    <div className="welcome-logo"><GitFork size={42} /></div><h1>See your history.<br/><em>Shape what comes next.</em></h1>
    <p>VGit brings your branches, commits, and working changes into one calm, visual workspace.</p>
    <button className="primary" onClick={onOpen} disabled={loading}>{loading ? <LoaderCircle className="spin" size={18} /> : <FolderGit2 size={18} />} Open a repository</button>
    {error && <div className="welcome-error"><AlertTriangle size={16} />{error}</div>}
    <div className="welcome-features"><span><GitCommitHorizontal size={17}/> Visual history</span><span><FileDiff size={17}/> Focused diffs</span><span><GitBranch size={17}/> Safe branching</span></div>
  </main>;
}

export default function App() {
  const [repo, setRepo] = useState<RepoSnapshot | null>(null);
  const [selection, setSelection] = useState<Selection>(null);
  const [content, setContent] = useState('');
  const [filter, setFilter] = useState('');
  const [message, setMessage] = useState('');
  const [amend, setAmend] = useState(false);
  const [loading, setLoading] = useState(false);
  const [diffLoading, setDiffLoading] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');

  const staged = repo?.files.filter(file => file.staged !== '.' && file.staged !== '?') || [];
  const unstaged = repo?.files.filter(file => file.working !== '.') || [];

  useEffect(() => {
    const recent = localStorage.getItem('vgit:lastRepo');
    window.vgit.initialRepository().then(initial => {
      if (initial) { setRepo(initial); localStorage.setItem('vgit:lastRepo', initial.path); }
      else if (recent) run(() => window.vgit.openRepository(recent), false).catch(() => localStorage.removeItem('vgit:lastRepo'));
    }).catch(err => setError(err instanceof Error ? err.message : String(err)));
  }, []);

  async function run(action: () => Promise<RepoSnapshot | null>, toast = true) {
    setLoading(true); setError('');
    try {
      const next = await action();
      if (next) { setRepo(next); localStorage.setItem('vgit:lastRepo', next.path); }
      if (toast) { setNotice('Operation completed'); window.setTimeout(() => setNotice(''), 2200); }
      return next;
    } catch (err) { setError(err instanceof Error ? err.message : String(err)); throw err; }
    finally { setLoading(false); }
  }

  async function selectFile(file: FileState, isStaged: boolean) {
    setSelection({ type: 'file', file, staged: isStaged }); setDiffLoading(true);
    try { setContent(await window.vgit.diff(file.path, isStaged)); } catch (err) { setContent(String(err)); } finally { setDiffLoading(false); }
  }
  async function selectCommit(commit: Commit) {
    setSelection({ type: 'commit', commit }); setDiffLoading(true);
    try { setContent(await window.vgit.showCommit(commit.hash)); } catch (err) { setContent(String(err)); } finally { setDiffLoading(false); }
  }
  async function mutate(action: () => Promise<RepoSnapshot>, success = 'Operation completed') {
    setLoading(true); setError('');
    try { const next = await action(); setRepo(next); setNotice(success); setTimeout(() => setNotice(''), 2200); }
    catch (err) { setError(err instanceof Error ? err.message : String(err)); }
    finally { setLoading(false); }
  }
  function createBranch() {
    const name = window.prompt('New branch name');
    if (name?.trim()) mutate(() => window.vgit.createBranch(name.trim()), `Created ${name.trim()}`);
  }
  function discardSelected() {
    if (selection?.type !== 'file') return;
    const untracked = selection.file.working === '?';
    if (window.confirm(`Discard ${untracked ? 'untracked file' : 'working changes in'} “${selection.file.path}”? This cannot be undone.`)) {
      mutate(() => window.vgit.discard(selection.file.path, untracked), 'Changes discarded'); setSelection(null);
    }
  }

  if (!repo) return <Welcome onOpen={() => { run(() => window.vgit.chooseRepository(), false).catch(() => {}); }} loading={loading} error={error} />;
  return <div className="app-shell">
    <Sidebar repo={repo} onCheckout={name => mutate(() => window.vgit.checkout(name), `Switched to ${name}`)} onCreate={createBranch}
      onStashApply={index => mutate(() => window.vgit.stashApply(index), 'Stash applied')} />
    <main className="workspace">
      <header className="toolbar">
        <button className="repo-picker" onClick={() => run(() => window.vgit.chooseRepository(), false).catch(() => {})}>
          <span className="repo-icon"><FolderGit2 size={17}/></span><span><b>{repo.name}</b><small>{repo.path}</small></span><ChevronDown size={14}/>
        </button>
        <div className="branch-pill"><GitBranch size={14}/><b>{repo.branch}</b>{repo.ahead > 0 && <span><ArrowUp size={11}/>{repo.ahead}</span>}{repo.behind > 0 && <span><ArrowDown size={11}/>{repo.behind}</span>}</div>
        <div className="toolbar-actions">
          <button onClick={() => mutate(() => window.vgit.fetch(), 'Fetch complete')} disabled={loading}><RefreshCw className={loading ? 'spin' : ''} size={15}/> Fetch</button>
          <button onClick={() => mutate(() => window.vgit.pull(), 'Repository updated')} disabled={loading}><ArrowDown size={15}/> Pull</button>
          <button onClick={() => mutate(() => window.vgit.push(), 'Changes pushed')} disabled={loading}><ArrowUp size={15}/> Push</button>
        </div>
        <button className="icon-button" title="Reveal repository" onClick={() => window.vgit.reveal()}><MoreHorizontal size={17}/></button>
      </header>
      {error && <div className="error-banner"><AlertTriangle size={15}/><span>{error}</span><button onClick={() => setError('')}><X size={14}/></button></div>}
      <div className="content-grid">
        <section className="changes-panel">
          <div className="panel-heading"><span>WORKING COPY</span><button title="Refresh" onClick={() => mutate(() => window.vgit.refresh(), 'Refreshed')}><RefreshCw size={14}/></button></div>
          <FileGroup title="Staged files" files={staged} staged selection={selection} onSelect={file => selectFile(file, true)}
            onAction={file => mutate(() => window.vgit.unstage(file.path), 'File unstaged')} onAll={() => mutate(() => window.vgit.unstageAll(), 'All files unstaged')} />
          <FileGroup title="Changes" files={unstaged} staged={false} selection={selection} onSelect={file => selectFile(file, false)}
            onAction={file => mutate(() => window.vgit.stage(file.path), 'File staged')} onAll={() => mutate(() => window.vgit.stageAll(), 'All files staged')} />
          <div className="commit-box">
            <textarea value={message} onChange={e => setMessage(e.target.value)} placeholder="Commit message" rows={4}/>
            <label><input type="checkbox" checked={amend} onChange={e => setAmend(e.target.checked)}/> Amend previous commit</label>
            <div className="commit-actions"><button className="stash-button" title="Stash all changes" onClick={() => mutate(() => window.vgit.stash(message), 'Changes stashed')}><Archive size={15}/></button>
              <button className="commit-button" disabled={loading || (!staged.length && !amend) || (!message.trim() && !amend)} onClick={() => mutate(() => window.vgit.commit(message, amend), 'Commit created').then(() => setMessage(''))}>
                <GitCommitHorizontal size={16}/> Commit {staged.length ? staged.length : ''}
              </button></div>
          </div>
        </section>
        <section className="history-panel">
          <div className="history-head">
            <div className="history-title"><span className="eyebrow">REPOSITORY MAP</span><h1>Commit history</h1><p>Follow how branches split, move, and merge over time.</p></div>
            <div className="history-stats">
              <span><GitCommitHorizontal size={14}/><b>{repo.commits.length}</b><small>commits</small></span>
              <span><GitBranch size={14}/><b>{repo.branches.filter(branch => !branch.remote).length}</b><small>branches</small></span>
              <span><GitMerge size={14}/><b>{repo.commits.filter(commit => commit.parents.length > 1).length}</b><small>merges</small></span>
            </div>
            <label className="search-box"><Search size={15}/><input value={filter} onChange={e => setFilter(e.target.value)} placeholder="Message, author, hash or branch"/></label>
          </div>
          <div className="graph-legend">
            <span><i className="legend-node head"/>HEAD</span><span><i className="legend-node"/>Commit</span>
            <span><i className="legend-line"/>Parent path</span><span><i className="legend-diamond"/>Merge</span>
            <small>Newest commits appear first</small>
          </div>
          <CommitGraph commits={repo.commits} selected={selection?.type === 'commit' ? selection.commit.hash : undefined} filter={filter} onSelect={selectCommit}/>
        </section>
        <Inspector selection={selection} content={content} loading={diffLoading} onDiscard={discardSelected}/>
      </div>
    </main>
    {notice && <div className="toast"><Check size={15}/>{notice}</div>}
  </div>;
}
