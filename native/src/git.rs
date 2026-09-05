//! Real repository access.
//!
//! Every command runs the installed `git` binary with an argument array, never
//! a shell string, so no repository path, branch name, or commit message can
//! be interpreted as shell syntax. Nothing here touches the interface: the
//! calls block, and the caller is expected to run them off the UI thread.
//!
//! Operations that can destroy uncommitted work are grouped under
//! [`Repository::discard`], [`Repository::reset`] and [`Repository::clean`] and
//! documented individually. They are deliberately explicit rather than
//! convenient.
#![allow(dead_code)]

use std::{
    fmt,
    path::{Path, PathBuf},
    process::Command,
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// `start` is not inside a Git working tree.
    NotARepository(PathBuf),
    /// The `git` binary could not be run at all.
    GitMissing(std::io::Error),
    /// Git ran and reported failure.
    Command {
        args: Vec<String>,
        status: Option<i32>,
        stderr: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotARepository(path) => {
                write!(f, "{} is not inside a Git repository", path.display())
            }
            Error::GitMissing(error) => write!(f, "could not run git: {error}"),
            Error::Command { args, stderr, .. } => {
                if stderr.is_empty() {
                    write!(f, "git {} failed", args.join(" "))
                } else {
                    write!(f, "git {} failed: {stderr}", args.join(" "))
                }
            }
        }
    }
}

impl std::error::Error for Error {}

/// Field and record separators for machine-readable `git log` output. Both are
/// control characters that cannot appear in a ref name and are vanishingly
/// unlikely in a commit message.
const FIELD: char = '\x1f';
const RECORD: char = '\x1e';

/// How a single path differs from HEAD and from the index.
///
/// The letters follow `git status --porcelain=v2`: `.` for unchanged, and
/// otherwise `M`, `A`, `D`, `R`, `C`, `T`, or `U`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStatus {
    pub path: String,
    /// Set for renames and copies: where the content came from.
    pub original_path: Option<String>,
    /// HEAD to index. `.` when the path is not staged.
    pub index: char,
    /// Index to working tree. `.` when the working tree matches the index.
    pub worktree: char,
    pub untracked: bool,
    pub ignored: bool,
    pub unmerged: bool,
}

impl FileStatus {
    /// Whether the path has content staged for the next commit.
    pub fn is_staged(&self) -> bool {
        !self.untracked && !self.ignored && !self.unmerged && self.index != '.'
    }

    /// Whether the working tree differs from the index.
    pub fn is_modified(&self) -> bool {
        self.untracked || self.unmerged || self.worktree != '.'
    }
}

/// A snapshot of the working tree and the branch it sits on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// The commit HEAD points at, absent in a repository with no commits.
    pub head: Option<String>,
    /// The checked-out branch, absent when HEAD is detached.
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub detached: bool,
    pub files: Vec<FileStatus>,
}

impl Status {
    pub fn staged(&self) -> impl Iterator<Item = &FileStatus> {
        self.files.iter().filter(|file| file.is_staged())
    }

    pub fn changed(&self) -> impl Iterator<Item = &FileStatus> {
        self.files.iter().filter(|file| file.is_modified())
    }

    pub fn has_conflicts(&self) -> bool {
        self.files.iter().any(|file| file.unmerged)
    }

    pub fn is_clean(&self) -> bool {
        self.files.iter().all(|file| file.ignored)
    }
}

/// One commit, as read from `git log`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub id: String,
    pub short_id: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    /// Author time, seconds since the epoch.
    pub timestamp: i64,
    /// Git's own relative rendering, such as "3 hours ago".
    pub relative_time: String,
    /// Ref names pointing at this commit, already stripped of decoration.
    pub refs: Vec<String>,
    pub subject: String,
}

impl Commit {
    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// Short name, such as `main`, `origin/main`, or `v0.3.0`.
    pub name: String,
    pub kind: RefKind,
    /// The commit the ref resolves to.
    pub target: String,
}

/// How far to move the working tree and index when resetting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    /// Move HEAD only. Changes stay staged.
    Soft,
    /// Move HEAD and reset the index. Changes stay in the working tree.
    Mixed,
    /// Move HEAD, index, and working tree. Uncommitted work is destroyed.
    Hard,
}

impl ResetMode {
    fn flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        }
    }

    /// Whether this mode can destroy uncommitted work.
    pub fn is_destructive(self) -> bool {
        self == ResetMode::Hard
    }
}

/// A Git working tree.
#[derive(Debug, Clone)]
pub struct Repository {
    root: PathBuf,
}

impl Repository {
    /// Finds the working tree containing `start`.
    pub fn discover(start: impl AsRef<Path>) -> Result<Self> {
        let start = start.as_ref();
        let output = Command::new("git")
            .arg("-C")
            .arg(start)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .map_err(Error::GitMissing)?;

        if !output.status.success() {
            return Err(Error::NotARepository(start.to_path_buf()));
        }

        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if root.is_empty() {
            return Err(Error::NotARepository(start.to_path_buf()));
        }
        Ok(Self {
            root: PathBuf::from(root),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Runs git and returns stdout, failing if git reports a non-zero status.
    fn run(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .output()
            .map_err(Error::GitMissing)?;

        if !output.status.success() {
            return Err(Error::Command {
                args: args.iter().map(|arg| arg.to_string()).collect(),
                status: output.status.code(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Runs git for its effect, discarding stdout.
    fn run_unit(&self, args: &[&str]) -> Result<()> {
        self.run(args).map(|_| ())
    }

    // ---- Reading -------------------------------------------------------

    /// The working tree and branch state.
    pub fn status(&self) -> Result<Status> {
        let raw = self.run(&[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
            "-z",
        ])?;
        Ok(parse_status(&raw))
    }

    /// The most recent `limit` commits reachable from every ref.
    pub fn log(&self, limit: usize) -> Result<Vec<Commit>> {
        let limit = format!("--max-count={limit}");
        let format = format!(
            "--pretty=format:%H{FIELD}%h{FIELD}%P{FIELD}%an{FIELD}%ae{FIELD}%at{FIELD}%ar{FIELD}%D{FIELD}%s{RECORD}"
        );
        // A repository with no commits has nothing to walk, and `git log`
        // treats that as an error rather than an empty list.
        if self.head_id()?.is_none() {
            return Ok(Vec::new());
        }
        let raw = self.run(&["log", "--all", "--topo-order", &limit, &format])?;
        Ok(parse_log(&raw))
    }

    /// The most recent `limit` commits reachable from `refs`.
    ///
    /// An empty `refs` walks every ref, which is what the history shows before
    /// a branch selection narrows it.
    pub fn log_refs(&self, refs: &[&str], limit: usize) -> Result<Vec<Commit>> {
        if self.head_id()?.is_none() {
            return Ok(Vec::new());
        }
        let limit = format!("--max-count={limit}");
        let format = format!(
            "--pretty=format:%H{FIELD}%h{FIELD}%P{FIELD}%an{FIELD}%ae{FIELD}%at{FIELD}%ar{FIELD}%D{FIELD}%s{RECORD}"
        );
        let mut args = vec!["log", "--topo-order", &limit, &format];
        if refs.is_empty() {
            args.push("--all");
        } else {
            // `--` keeps a branch named like a path from being read as one.
            args.extend(refs.iter().copied());
            args.push("--");
        }
        Ok(parse_log(&self.run(&args)?))
    }

    /// Every file tracked in the index, in Git's own sorted order.
    pub fn tracked_files(&self) -> Result<Vec<String>> {
        Ok(self
            .run(&["ls-files", "-z"])?
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Reads a file from the working tree.
    pub fn read_working_file(&self, path: &str) -> std::io::Result<String> {
        std::fs::read_to_string(self.root.join(path))
    }

    /// Every local branch, remote-tracking branch, and tag.
    pub fn references(&self) -> Result<Vec<Reference>> {
        let format = format!("--format=%(refname){FIELD}%(objectname)");
        let raw = self.run(&["for-each-ref", &format])?;
        Ok(parse_refs(&raw))
    }

    /// The commit HEAD points at, or `None` in a repository with no commits.
    pub fn head_id(&self) -> Result<Option<String>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["rev-parse", "--verify", "HEAD"])
            .output()
            .map_err(Error::GitMissing)?;

        if !output.status.success() {
            return Ok(None);
        }
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    }

    /// The checked-out branch, or `None` when HEAD is detached.
    pub fn current_branch(&self) -> Result<Option<String>> {
        let name = self.run(&["branch", "--show-current"])?.trim().to_string();
        Ok((!name.is_empty()).then_some(name))
    }

    /// The unified diff for one path. Pass `staged` to diff the index.
    pub fn diff(&self, path: &str, staged: bool) -> Result<String> {
        let mut args = vec!["diff", "--no-color"];
        if staged {
            args.push("--staged");
        }
        args.push("--");
        args.push(path);
        self.run(&args)
    }

    /// The file's contents at a commit.
    pub fn show(&self, commit: &str, path: &str) -> Result<String> {
        self.run(&["show", &format!("{commit}:{path}")])
    }

    // ---- The index -----------------------------------------------------

    pub fn stage(&self, paths: &[&str]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["add", "--"];
        args.extend_from_slice(paths);
        self.run_unit(&args)
    }

    pub fn stage_all(&self) -> Result<()> {
        self.run_unit(&["add", "--all"])
    }

    /// Removes paths from the index, leaving the working tree alone.
    pub fn unstage(&self, paths: &[&str]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["restore", "--staged", "--"];
        args.extend_from_slice(paths);
        self.run_unit(&args)
    }

    // ---- Committing ----------------------------------------------------

    pub fn commit(&self, message: &str) -> Result<()> {
        self.run_unit(&["commit", "--message", message])
    }

    /// Replaces the previous commit. The old commit stays in the reflog.
    pub fn amend(&self, message: &str) -> Result<()> {
        self.run_unit(&["commit", "--amend", "--message", message])
    }

    /// Amends without changing the message.
    pub fn amend_keep_message(&self) -> Result<()> {
        self.run_unit(&["commit", "--amend", "--no-edit"])
    }

    // ---- Undoing -------------------------------------------------------

    /// Records a new commit that undoes `commit`. History is preserved.
    pub fn revert(&self, commit: &str) -> Result<()> {
        self.run_unit(&["revert", "--no-edit", commit])
    }

    /// Applies the inverse of `commit` without committing it.
    pub fn revert_staged(&self, commit: &str) -> Result<()> {
        self.run_unit(&["revert", "--no-commit", commit])
    }

    /// Moves HEAD to `commit`.
    ///
    /// [`ResetMode::Hard`] destroys uncommitted work in the index and working
    /// tree. Confirm with the user before calling it.
    pub fn reset(&self, commit: &str, mode: ResetMode) -> Result<()> {
        self.run_unit(&["reset", mode.flag(), commit])
    }

    /// Throws away working-tree changes to `paths`. Destructive.
    pub fn discard(&self, paths: &[&str]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["restore", "--worktree", "--"];
        args.extend_from_slice(paths);
        self.run_unit(&args)
    }

    /// Deletes untracked files. Destructive, and not undoable through Git.
    pub fn clean(&self, include_directories: bool) -> Result<()> {
        let mut args = vec!["clean", "--force"];
        if include_directories {
            args.push("-d");
        }
        self.run_unit(&args)
    }

    // ---- Stashing ------------------------------------------------------

    pub fn stash_push(&self, message: &str, include_untracked: bool) -> Result<()> {
        let mut args = vec!["stash", "push"];
        if include_untracked {
            args.push("--include-untracked");
        }
        args.push("--message");
        args.push(message);
        self.run_unit(&args)
    }

    pub fn stash_pop(&self) -> Result<()> {
        self.run_unit(&["stash", "pop"])
    }

    pub fn stash_list(&self) -> Result<Vec<String>> {
        Ok(self
            .run(&["stash", "list", "--pretty=format:%gd %s"])?
            .lines()
            .map(str::to_string)
            .collect())
    }

    // ---- Branches ------------------------------------------------------

    pub fn branches(&self) -> Result<Vec<String>> {
        Ok(self
            .run(&["branch", "--format=%(refname:short)"])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    pub fn create_branch(&self, name: &str) -> Result<()> {
        self.run_unit(&["branch", name])
    }

    pub fn switch_branch(&self, name: &str) -> Result<()> {
        self.run_unit(&["switch", name])
    }

    pub fn create_and_switch(&self, name: &str) -> Result<()> {
        self.run_unit(&["switch", "--create", name])
    }

    /// Deletes a branch, refusing if it holds unmerged commits.
    pub fn delete_branch(&self, name: &str) -> Result<()> {
        self.run_unit(&["branch", "--delete", name])
    }

    // ---- Remotes -------------------------------------------------------

    pub fn remotes(&self) -> Result<Vec<String>> {
        Ok(self
            .run(&["remote"])?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    pub fn fetch(&self, remote: &str, prune: bool) -> Result<()> {
        let mut args = vec!["fetch", remote];
        if prune {
            args.push("--prune");
        }
        self.run_unit(&args)
    }

    /// Fast-forward only, so a pull can never create a merge commit
    /// unexpectedly.
    pub fn pull_fast_forward(&self, remote: &str, branch: &str) -> Result<()> {
        self.run_unit(&["pull", "--ff-only", remote, branch])
    }

    pub fn push(&self, remote: &str, branch: &str, set_upstream: bool) -> Result<()> {
        let mut args = vec!["push"];
        if set_upstream {
            args.push("--set-upstream");
        }
        args.push(remote);
        args.push(branch);
        self.run_unit(&args)
    }
}

// ---- Parsers -----------------------------------------------------------
//
// Kept as free functions over `&str` so they can be tested against captured
// output without a repository.

/// Parses `git status --porcelain=v2 --branch -z`.
pub fn parse_status(raw: &str) -> Status {
    let mut status = Status::default();
    // `-z` terminates every record with NUL, so the final split is empty.
    let mut parts = raw.split('\0').filter(|part| !part.is_empty());

    while let Some(part) = parts.next() {
        let mut chars = part.chars();
        match chars.next() {
            Some('#') => apply_header(part, &mut status),
            Some('1') => {
                if let Some(file) = parse_ordinary(part) {
                    status.files.push(file);
                }
            }
            Some('2') => {
                // A rename or copy carries its source as the next record.
                let original = parts.next().map(str::to_string);
                if let Some(mut file) = parse_ordinary(part) {
                    file.original_path = original;
                    status.files.push(file);
                }
            }
            Some('u') => {
                if let Some(path) = part.split(' ').nth(10) {
                    status.files.push(FileStatus {
                        path: path.to_string(),
                        original_path: None,
                        index: 'U',
                        worktree: 'U',
                        untracked: false,
                        ignored: false,
                        unmerged: true,
                    });
                }
            }
            Some(marker @ ('?' | '!')) => {
                if let Some(path) = part.get(2..) {
                    status.files.push(FileStatus {
                        path: path.to_string(),
                        original_path: None,
                        index: '.',
                        worktree: '?',
                        untracked: marker == '?',
                        ignored: marker == '!',
                        unmerged: false,
                    });
                }
            }
            _ => {}
        }
    }
    status
}

fn apply_header(line: &str, status: &mut Status) {
    let mut fields = line.split(' ');
    fields.next(); // "#"
    match (fields.next(), fields.next()) {
        (Some("branch.oid"), Some(oid)) => {
            if oid != "(initial)" {
                status.head = Some(oid.to_string());
            }
        }
        (Some("branch.head"), Some(name)) => {
            if name == "(detached)" {
                status.detached = true;
            } else {
                status.branch = Some(name.to_string());
            }
        }
        (Some("branch.upstream"), Some(name)) => status.upstream = Some(name.to_string()),
        (Some("branch.ab"), Some(ahead)) => {
            status.ahead = ahead.trim_start_matches('+').parse().unwrap_or(0);
            status.behind = fields
                .next()
                .and_then(|behind| behind.trim_start_matches('-').parse().ok())
                .unwrap_or(0);
        }
        _ => {}
    }
}

/// Parses a `1` or `2` record. The path is the last space-separated field, and
/// a path may itself contain spaces, so the leading fields are counted rather
/// than the trailing ones.
fn parse_ordinary(line: &str) -> Option<FileStatus> {
    let leading = if line.starts_with('2') { 9 } else { 8 };
    let mut fields = line.splitn(leading + 1, ' ');
    let _record = fields.next()?;
    let xy = fields.next()?;
    for _ in 0..(leading - 2) {
        fields.next()?;
    }
    let path = fields.next()?;

    let mut xy = xy.chars();
    Some(FileStatus {
        path: path.to_string(),
        original_path: None,
        index: xy.next().unwrap_or('.'),
        worktree: xy.next().unwrap_or('.'),
        untracked: false,
        ignored: false,
        unmerged: false,
    })
}

/// Parses the record-separated output written by [`Repository::log`].
pub fn parse_log(raw: &str) -> Vec<Commit> {
    raw.split(RECORD)
        .map(|record| record.trim_start_matches('\n'))
        .filter(|record| !record.is_empty())
        .filter_map(parse_commit)
        .collect()
}

fn parse_commit(record: &str) -> Option<Commit> {
    let fields: Vec<&str> = record.split(FIELD).collect();
    if fields.len() < 9 {
        return None;
    }
    Some(Commit {
        id: fields[0].to_string(),
        short_id: fields[1].to_string(),
        parents: fields[2].split_whitespace().map(str::to_string).collect(),
        author: fields[3].to_string(),
        email: fields[4].to_string(),
        timestamp: fields[5].parse().unwrap_or_default(),
        relative_time: fields[6].to_string(),
        refs: parse_decoration(fields[7]),
        // Anything past the ninth field belonged to the subject.
        subject: fields[8..].join(&FIELD.to_string()),
    })
}

/// Turns `%D` decoration such as `HEAD -> main, origin/main, tag: v1` into
/// plain ref names.
fn parse_decoration(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| {
            name.rsplit(" -> ")
                .next()
                .unwrap_or(name)
                .trim_start_matches("tag: ")
                .to_string()
        })
        .collect()
}

pub fn parse_refs(raw: &str) -> Vec<Reference> {
    raw.lines()
        .filter_map(|line| {
            let (refname, target) = line.split_once(FIELD)?;
            // Anything outside these three namespaces (refs/stash, notes, and
            // so on) is not a ref the interface shows.
            let (kind, name) = [
                (RefKind::Local, "refs/heads/"),
                (RefKind::Remote, "refs/remotes/"),
                (RefKind::Tag, "refs/tags/"),
            ]
            .into_iter()
            .find_map(|(kind, prefix)| Some((kind, refname.strip_prefix(prefix)?)))?;
            Some(Reference {
                name: name.to_string(),
                kind,
                target: target.trim().to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// A throwaway repository on disk, removed when the test ends.
    struct TempRepo {
        dir: PathBuf,
    }

    impl TempRepo {
        fn new(label: &str) -> Self {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir =
                std::env::temp_dir().join(format!("vgit-test-{}-{label}-{id}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("create temp dir");
            let repo = Self { dir };
            repo.git(&["init", "--initial-branch=main"]);
            // Never depend on the developer's global Git configuration.
            repo.git(&["config", "user.name", "VGit Test"]);
            repo.git(&["config", "user.email", "test@example.invalid"]);
            repo.git(&["config", "commit.gpgsign", "false"]);
            repo
        }

        fn bare(label: &str) -> Self {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir = std::env::temp_dir().join(format!(
                "vgit-test-{}-{label}-bare-{id}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("create temp dir");
            let repo = Self { dir };
            repo.git(&["init", "--bare", "--initial-branch=main"]);
            repo
        }

        /// Runs git for setup, panicking with git's own message on failure.
        fn git(&self, args: &[&str]) -> String {
            let output = Command::new("git")
                .arg("-C")
                .arg(&self.dir)
                .args(args)
                .output()
                .expect("run git");
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).into_owned()
        }

        fn write(&self, path: &str, contents: &str) {
            let full = self.dir.join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create parent");
            }
            fs::write(full, contents).expect("write file");
        }

        fn read(&self, path: &str) -> String {
            fs::read_to_string(self.dir.join(path)).expect("read file")
        }

        fn exists(&self, path: &str) -> bool {
            self.dir.join(path).exists()
        }

        fn commit_file(&self, path: &str, contents: &str, message: &str) {
            self.write(path, contents);
            self.git(&["add", "--", path]);
            self.git(&["commit", "--message", message]);
        }

        fn open(&self) -> Repository {
            Repository::discover(&self.dir).expect("discover repository")
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    // ---- Discovery -----------------------------------------------------

    #[test]
    fn discovery_finds_the_working_tree_root_from_a_subdirectory() {
        let temp = TempRepo::new("discover");
        temp.commit_file("src/main.rs", "fn main() {}\n", "Initial commit");

        let from_root = temp.open();
        let from_subdir = Repository::discover(temp.dir.join("src")).expect("discover");
        assert_eq!(from_root.root(), from_subdir.root());
    }

    #[test]
    fn discovery_fails_outside_a_repository() {
        let dir = std::env::temp_dir().join(format!("vgit-not-a-repo-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create dir");
        let result = Repository::discover(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert!(matches!(result, Err(Error::NotARepository(_))));
    }

    // ---- Status --------------------------------------------------------

    #[test]
    fn status_separates_staged_from_unstaged_changes() {
        let temp = TempRepo::new("status");
        temp.commit_file("tracked.txt", "one\n", "Initial commit");
        temp.write("tracked.txt", "two\n");
        temp.write("staged.txt", "new\n");
        temp.git(&["add", "--", "staged.txt"]);
        temp.write("untracked.txt", "loose\n");

        let status = temp.open().status().expect("status");

        assert_eq!(status.branch.as_deref(), Some("main"));
        assert!(!status.detached);
        assert!(status.head.is_some());

        let staged: Vec<&str> = status.staged().map(|file| file.path.as_str()).collect();
        assert_eq!(staged, vec!["staged.txt"]);

        let changed: Vec<&str> = status.changed().map(|file| file.path.as_str()).collect();
        assert!(changed.contains(&"tracked.txt"), "{changed:?}");
        assert!(changed.contains(&"untracked.txt"), "{changed:?}");

        let untracked = status
            .files
            .iter()
            .find(|file| file.path == "untracked.txt")
            .expect("untracked entry");
        assert!(untracked.untracked);
        assert!(!untracked.is_staged());
    }

    #[test]
    fn a_clean_repository_reports_no_changes() {
        let temp = TempRepo::new("clean");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        let status = temp.open().status().expect("status");
        assert!(status.is_clean(), "{:?}", status.files);
        assert_eq!(status.staged().count(), 0);
    }

    #[test]
    fn status_reports_paths_containing_spaces() {
        let temp = TempRepo::new("spaces");
        temp.commit_file("a name with spaces.txt", "hello\n", "Initial commit");
        temp.write("a name with spaces.txt", "changed\n");

        let status = temp.open().status().expect("status");
        let paths: Vec<&str> = status.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["a name with spaces.txt"]);
    }

    #[test]
    fn status_reports_a_rename_with_its_original_path() {
        let temp = TempRepo::new("rename");
        temp.commit_file("before.txt", "stable contents\n", "Initial commit");
        temp.git(&["mv", "before.txt", "after.txt"]);

        let status = temp.open().status().expect("status");
        let renamed = status
            .files
            .iter()
            .find(|file| file.path == "after.txt")
            .expect("renamed entry");
        assert_eq!(renamed.original_path.as_deref(), Some("before.txt"));
        assert!(renamed.is_staged());
    }

    #[test]
    fn status_reports_an_empty_repository_without_a_head() {
        let temp = TempRepo::new("empty");
        let status = temp.open().status().expect("status");
        assert!(status.head.is_none());
        assert_eq!(status.branch.as_deref(), Some("main"));
    }

    #[test]
    fn status_reports_a_detached_head() {
        let temp = TempRepo::new("detached");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        temp.commit_file("b.txt", "b\n", "Second commit");
        temp.git(&["checkout", "--detach", "HEAD~1"]);

        let status = temp.open().status().expect("status");
        assert!(status.detached);
        assert!(status.branch.is_none());
    }

    #[test]
    fn status_reports_conflicts_during_a_merge() {
        let temp = TempRepo::new("conflict");
        temp.commit_file("shared.txt", "base\n", "Initial commit");
        temp.git(&["switch", "--create", "other"]);
        temp.commit_file("shared.txt", "from other\n", "Other side");
        temp.git(&["switch", "main"]);
        temp.commit_file("shared.txt", "from main\n", "Main side");

        // The merge is expected to fail; the conflict is the point.
        let _ = Command::new("git")
            .arg("-C")
            .arg(&temp.dir)
            .args(["merge", "other"])
            .output()
            .expect("run merge");

        let status = temp.open().status().expect("status");
        assert!(status.has_conflicts(), "{:?}", status.files);
        let conflicted = status
            .files
            .iter()
            .find(|file| file.unmerged)
            .expect("conflicted entry");
        assert_eq!(conflicted.path, "shared.txt");
        assert!(!conflicted.is_staged(), "a conflict is not staged content");
    }

    // ---- Log -----------------------------------------------------------

    #[test]
    fn log_reads_commits_newest_first_with_parents() {
        let temp = TempRepo::new("log");
        temp.commit_file("a.txt", "a\n", "First");
        temp.commit_file("b.txt", "b\n", "Second");
        temp.commit_file("c.txt", "c\n", "Third");

        let commits = temp.open().log(10).expect("log");
        let subjects: Vec<&str> = commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, vec!["Third", "Second", "First"]);

        assert!(commits[2].parents.is_empty(), "the root has no parent");
        assert_eq!(commits[0].parents, vec![commits[1].id.clone()]);
        assert_eq!(commits[0].author, "VGit Test");
        assert_eq!(commits[0].email, "test@example.invalid");
        assert!(commits[0].timestamp > 0);
        assert!(!commits[0].short_id.is_empty());
        assert!(commits[0].id.starts_with(&commits[0].short_id));
    }

    #[test]
    fn log_records_both_parents_of_a_merge() {
        let temp = TempRepo::new("merge");
        temp.commit_file("base.txt", "base\n", "Initial commit");
        temp.git(&["switch", "--create", "side"]);
        temp.commit_file("side.txt", "side\n", "Side commit");
        temp.git(&["switch", "main"]);
        temp.commit_file("main.txt", "main\n", "Main commit");
        temp.git(&["merge", "--no-ff", "--no-edit", "side"]);

        let commits = temp.open().log(20).expect("log");
        let merge = commits
            .iter()
            .find(|commit| commit.is_merge())
            .expect("a merge commit");
        assert_eq!(merge.parents.len(), 2);
    }

    #[test]
    fn log_carries_ref_names_without_decoration_syntax() {
        let temp = TempRepo::new("decoration");
        temp.commit_file("a.txt", "a\n", "Tagged commit");
        temp.git(&["tag", "v1.0.0"]);

        let commits = temp.open().log(10).expect("log");
        let refs = &commits[0].refs;
        assert!(refs.iter().any(|name| name == "main"), "{refs:?}");
        assert!(refs.iter().any(|name| name == "v1.0.0"), "{refs:?}");
        assert!(
            refs.iter().all(|name| !name.contains("->")),
            "decoration arrows leaked into {refs:?}"
        );
    }

    #[test]
    fn log_handles_a_subject_containing_separators() {
        let temp = TempRepo::new("subject");
        temp.commit_file("a.txt", "a\n", "Fix: handle a, b -> c properly");

        let commits = temp.open().log(10).expect("log");
        assert_eq!(commits[0].subject, "Fix: handle a, b -> c properly");
    }

    #[test]
    fn log_is_empty_in_a_repository_with_no_commits() {
        let temp = TempRepo::new("nolog");
        assert!(temp.open().log(10).expect("log").is_empty());
    }

    // ---- References ----------------------------------------------------

    #[test]
    fn references_classify_branches_remotes_and_tags() {
        let temp = TempRepo::new("refs");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        temp.git(&["tag", "v0.1.0"]);
        temp.git(&["branch", "feature/one"]);

        let refs = temp.open().references().expect("references");
        let local: Vec<&str> = refs
            .iter()
            .filter(|r| r.kind == RefKind::Local)
            .map(|r| r.name.as_str())
            .collect();
        assert!(local.contains(&"main"), "{local:?}");
        assert!(local.contains(&"feature/one"), "{local:?}");

        let tags: Vec<&str> = refs
            .iter()
            .filter(|r| r.kind == RefKind::Tag)
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(tags, vec!["v0.1.0"]);
        assert!(refs.iter().all(|r| !r.target.is_empty()));
    }

    // ---- The index and committing --------------------------------------

    #[test]
    fn staging_and_unstaging_move_a_path_in_and_out_of_the_index() {
        let temp = TempRepo::new("stage");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        temp.write("a.txt", "changed\n");
        let repo = temp.open();

        repo.stage(&["a.txt"]).expect("stage");
        assert_eq!(repo.status().expect("status").staged().count(), 1);

        repo.unstage(&["a.txt"]).expect("unstage");
        let status = repo.status().expect("status");
        assert_eq!(status.staged().count(), 0);
        assert_eq!(status.changed().count(), 1, "the edit survives unstaging");
    }

    #[test]
    fn committing_records_the_staged_content_and_clears_the_index() {
        let temp = TempRepo::new("commit");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        temp.write("b.txt", "b\n");
        let repo = temp.open();

        repo.stage(&["b.txt"]).expect("stage");
        repo.commit("Add b").expect("commit");

        let commits = repo.log(10).expect("log");
        assert_eq!(commits[0].subject, "Add b");
        assert!(repo.status().expect("status").is_clean());
    }

    #[test]
    fn amending_replaces_the_previous_commit() {
        let temp = TempRepo::new("amend");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        temp.commit_file("b.txt", "b\n", "Typo in mesage");
        let repo = temp.open();

        let before = repo.log(10).expect("log").len();
        repo.amend("Fix the message").expect("amend");
        let commits = repo.log(10).expect("log");

        assert_eq!(commits.len(), before, "amend must not add a commit");
        assert_eq!(commits[0].subject, "Fix the message");
    }

    #[test]
    fn staging_a_path_with_spaces_does_not_split_it() {
        let temp = TempRepo::new("stage-spaces");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        temp.write("two words.txt", "content\n");
        let repo = temp.open();

        repo.stage(&["two words.txt"]).expect("stage");
        let staged: Vec<String> = repo
            .status()
            .expect("status")
            .staged()
            .map(|f| f.path.clone())
            .collect();
        assert_eq!(staged, vec!["two words.txt".to_string()]);
    }

    // ---- Undoing -------------------------------------------------------

    #[test]
    fn revert_adds_a_commit_that_undoes_the_change() {
        let temp = TempRepo::new("revert");
        temp.commit_file("a.txt", "original\n", "Initial commit");
        temp.commit_file("a.txt", "replaced\n", "Replace contents");
        let repo = temp.open();

        let target = repo.log(10).expect("log")[0].id.clone();
        repo.revert(&target).expect("revert");

        assert_eq!(temp.read("a.txt"), "original\n");
        let commits = repo.log(10).expect("log");
        assert_eq!(commits.len(), 3, "history is preserved, not rewritten");
        assert!(commits[0].subject.starts_with("Revert"));
    }

    #[test]
    fn a_soft_reset_moves_head_but_keeps_the_change_staged() {
        let temp = TempRepo::new("soft");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        temp.commit_file("b.txt", "b\n", "Second commit");
        let repo = temp.open();

        repo.reset("HEAD~1", ResetMode::Soft).expect("reset");

        assert_eq!(repo.log(10).expect("log").len(), 1);
        assert_eq!(repo.status().expect("status").staged().count(), 1);
        assert!(temp.exists("b.txt"), "a soft reset keeps the file");
    }

    #[test]
    fn a_mixed_reset_leaves_the_change_in_the_working_tree() {
        let temp = TempRepo::new("mixed");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        temp.commit_file("b.txt", "b\n", "Second commit");
        let repo = temp.open();

        repo.reset("HEAD~1", ResetMode::Mixed).expect("reset");

        let status = repo.status().expect("status");
        assert_eq!(status.staged().count(), 0);
        assert_eq!(status.changed().count(), 1);
        assert!(temp.exists("b.txt"));
    }

    /// The destructive one: it must actually destroy, so the interface can
    /// warn about it accurately.
    #[test]
    fn a_hard_reset_discards_the_commit_and_its_file() {
        let temp = TempRepo::new("hard");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        temp.commit_file("b.txt", "b\n", "Second commit");
        let repo = temp.open();

        repo.reset("HEAD~1", ResetMode::Hard).expect("reset");

        assert_eq!(repo.log(10).expect("log").len(), 1);
        assert!(repo.status().expect("status").is_clean());
        assert!(!temp.exists("b.txt"), "a hard reset removes the file");
        assert!(ResetMode::Hard.is_destructive());
        assert!(!ResetMode::Soft.is_destructive());
        assert!(!ResetMode::Mixed.is_destructive());
    }

    #[test]
    fn discard_throws_away_a_working_tree_edit() {
        let temp = TempRepo::new("discard");
        temp.commit_file("a.txt", "original\n", "Initial commit");
        temp.write("a.txt", "unwanted\n");
        let repo = temp.open();

        repo.discard(&["a.txt"]).expect("discard");

        assert_eq!(temp.read("a.txt"), "original\n");
        assert!(repo.status().expect("status").is_clean());
    }

    // ---- Stashing ------------------------------------------------------

    #[test]
    fn a_stash_round_trip_restores_the_working_tree() {
        let temp = TempRepo::new("stash");
        temp.commit_file("a.txt", "original\n", "Initial commit");
        temp.write("a.txt", "work in progress\n");
        let repo = temp.open();

        repo.stash_push("wip", false).expect("stash push");
        assert_eq!(temp.read("a.txt"), "original\n");
        assert_eq!(repo.stash_list().expect("stash list").len(), 1);

        repo.stash_pop().expect("stash pop");
        assert_eq!(temp.read("a.txt"), "work in progress\n");
        assert!(repo.stash_list().expect("stash list").is_empty());
    }

    // ---- Branches ------------------------------------------------------

    #[test]
    fn branches_can_be_created_switched_and_deleted() {
        let temp = TempRepo::new("branch");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        let repo = temp.open();

        repo.create_and_switch("feature/one").expect("switch");
        assert_eq!(
            repo.current_branch().expect("branch").as_deref(),
            Some("feature/one")
        );

        repo.switch_branch("main").expect("switch back");
        assert_eq!(
            repo.current_branch().expect("branch").as_deref(),
            Some("main")
        );

        let names = repo.branches().expect("branches");
        assert!(names.contains(&"feature/one".to_string()), "{names:?}");

        repo.delete_branch("feature/one").expect("delete");
        assert!(
            !repo
                .branches()
                .expect("branches")
                .contains(&"feature/one".to_string())
        );
    }

    // ---- Remotes -------------------------------------------------------

    /// Fetch and push against a second local repository, so the test needs no
    /// network access.
    #[test]
    fn push_and_fetch_exchange_commits_with_a_remote() {
        let origin = TempRepo::bare("origin");
        let temp = TempRepo::new("push");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        temp.git(&["remote", "add", "origin", &origin.dir.to_string_lossy()]);
        let repo = temp.open();

        assert_eq!(repo.remotes().expect("remotes"), vec!["origin".to_string()]);
        repo.push("origin", "main", true).expect("push");

        let status = repo.status().expect("status");
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 0);

        // A second clone pushes another commit, which the first then fetches.
        let other = TempRepo::new("clone");
        other.git(&["remote", "add", "origin", &origin.dir.to_string_lossy()]);
        other.git(&["fetch", "origin"]);
        other.git(&["reset", "--hard", "origin/main"]);
        other.commit_file("b.txt", "b\n", "From the other clone");
        other.git(&["push", "origin", "main"]);

        repo.fetch("origin", true).expect("fetch");
        let status = repo.status().expect("status");
        assert_eq!(status.behind, 1, "the fetched commit is not yet merged");
        assert_eq!(status.ahead, 0);

        repo.pull_fast_forward("origin", "main").expect("pull");
        assert_eq!(temp.read("b.txt"), "b\n");
        assert_eq!(repo.status().expect("status").behind, 0);
    }

    #[test]
    fn ahead_counts_local_commits_that_are_not_pushed() {
        let origin = TempRepo::bare("origin-ahead");
        let temp = TempRepo::new("ahead");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        temp.git(&["remote", "add", "origin", &origin.dir.to_string_lossy()]);
        let repo = temp.open();
        repo.push("origin", "main", true).expect("push");

        temp.commit_file("b.txt", "b\n", "Unpushed work");
        let status = repo.status().expect("status");
        assert_eq!(status.ahead, 1);
        assert_eq!(status.behind, 0);
    }

    // ---- Failure handling ----------------------------------------------

    #[test]
    fn a_failing_command_reports_git_own_message() {
        let temp = TempRepo::new("failure");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        let repo = temp.open();

        let error = repo
            .switch_branch("does-not-exist")
            .expect_err("should fail");
        match error {
            Error::Command { ref stderr, .. } => {
                assert!(!stderr.is_empty(), "git's message was dropped");
            }
            other => panic!("unexpected error: {other:?}"),
        }
        // The message is shown to the user, so it must not be empty.
        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn committing_with_nothing_staged_fails_rather_than_creating_an_empty_commit() {
        let temp = TempRepo::new("empty-commit");
        temp.commit_file("a.txt", "a\n", "Initial commit");
        let repo = temp.open();

        assert!(repo.commit("nothing to record").is_err());
        assert_eq!(repo.log(10).expect("log").len(), 1);
    }

    // ---- Parsers, without a repository ---------------------------------

    #[test]
    fn log_refs_narrows_the_history_to_the_given_branches() {
        let temp = TempRepo::new("logrefs");
        temp.commit_file("base.txt", "base\n", "Base");
        temp.git(&["switch", "--create", "side"]);
        temp.commit_file("side.txt", "side\n", "Only on side");
        temp.git(&["switch", "main"]);
        temp.commit_file("main.txt", "main\n", "Only on main");
        let repo = temp.open();

        let everything: Vec<String> = repo
            .log_refs(&[], 50)
            .expect("log")
            .into_iter()
            .map(|c| c.subject)
            .collect();
        assert!(everything.contains(&"Only on side".to_string()));
        assert!(everything.contains(&"Only on main".to_string()));

        let only_main: Vec<String> = repo
            .log_refs(&["main"], 50)
            .expect("log")
            .into_iter()
            .map(|c| c.subject)
            .collect();
        assert!(only_main.contains(&"Only on main".to_string()));
        assert!(
            !only_main.contains(&"Only on side".to_string()),
            "the side branch leaked into a main-only log"
        );
        assert!(
            only_main.contains(&"Base".to_string()),
            "shared history is kept"
        );
    }

    #[test]
    fn tracked_files_lists_the_index_and_skips_untracked_paths() {
        let temp = TempRepo::new("lsfiles");
        temp.commit_file("src/main.rs", "fn main() {}\n", "Initial commit");
        temp.commit_file("docs/readme.md", "# hi\n", "Docs");
        temp.write("untracked.txt", "loose\n");

        let files = temp.open().tracked_files().expect("ls-files");
        assert!(files.contains(&"src/main.rs".to_string()), "{files:?}");
        assert!(files.contains(&"docs/readme.md".to_string()), "{files:?}");
        assert!(!files.contains(&"untracked.txt".to_string()), "{files:?}");
    }

    #[test]
    fn reading_a_working_file_returns_its_current_contents() {
        let temp = TempRepo::new("readfile");
        temp.commit_file("a.txt", "committed\n", "Initial commit");
        temp.write("a.txt", "edited\n");
        let repo = temp.open();
        assert_eq!(repo.read_working_file("a.txt").expect("read"), "edited\n");
        assert!(repo.read_working_file("missing.txt").is_err());
    }

    #[test]
    fn a_diff_shows_the_working_tree_change() {
        let temp = TempRepo::new("diff");
        temp.commit_file("a.txt", "one\n", "Initial commit");
        temp.write("a.txt", "two\n");
        let diff = temp.open().diff("a.txt", false).expect("diff");
        assert!(diff.contains("-one"), "{diff}");
        assert!(diff.contains("+two"), "{diff}");
    }

    #[test]
    fn the_status_parser_reads_branch_headers() {
        let raw = "# branch.oid abc123\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -3\0";
        let status = parse_status(raw);
        assert_eq!(status.head.as_deref(), Some("abc123"));
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 3);
    }

    #[test]
    fn the_status_parser_treats_an_initial_repository_as_headless() {
        let status = parse_status("# branch.oid (initial)\0# branch.head main\0");
        assert!(status.head.is_none());
    }

    #[test]
    fn the_log_parser_ignores_a_truncated_record() {
        let record = format!("only{FIELD}three{FIELD}fields{RECORD}");
        assert!(parse_log(&record).is_empty());
    }

    #[test]
    fn the_ref_parser_skips_refs_it_does_not_recognize() {
        let raw = format!("refs/stash{FIELD}abc\nrefs/heads/main{FIELD}def\n");
        let refs = parse_refs(&raw);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].name, "main");
    }
}
