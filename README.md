# VGit

[![CI](https://github.com/anupkumarreddy/vgit/actions/workflows/ci.yml/badge.svg)](https://github.com/anupkumarreddy/vgit/actions/workflows/ci.yml)
[![CodeQL](https://github.com/anupkumarreddy/vgit/actions/workflows/codeql.yml/badge.svg)](https://github.com/anupkumarreddy/vgit/actions/workflows/codeql.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

VGit is an open-source visual desktop Git client written in Rust with
[GPUI](https://www.gpui.rs/). It presents commits, branches, tags, remotes,
hashes, authors, and merge paths as an approachable repository map.

> **Status: a working visual Git client.** VGit opens the repository it is
> launched from, or any other through **Open**, and every Git operation it
> implements is reachable from the interface. Anything that can lose work asks
> first, naming exactly what will be lost. See
> [Project status and roadmap](#project-status-and-roadmap).

VGit was previously an Electron, React, and TypeScript application. That
implementation was removed in favor of the native Rust rewrite; it remains in
Git history at commit `55f96ca` for reference.

## Requirements

- Rust 1.98 or later (install via [rustup](https://rustup.rs/))
- macOS with Xcode Command Line Tools

The prototype is developed on macOS. Windows and Linux are not yet validated.

## Run from source

```bash
git clone https://github.com/anupkumarreddy/vgit.git
cd vgit
cargo run --locked --manifest-path native/Cargo.toml
```

The first build downloads and compiles GPUI and its dependencies, which takes
several minutes. VGit enables GPUI's `runtime_shaders` feature so the offline
Metal compiler from full Xcode is not required.

Build a local macOS application bundle:

```bash
./native/scripts/bundle-macos.sh
open 'out/VGit Preview.app'
```

The local bundle is unsigned and intended for development only.

## Operations

| Where | What |
| --- | --- |
| Title bar | **Open** another repository, **Fetch**, **Pull** (fast-forward only), **Push**, and `⋯` for repository actions |
| History header | `↻` refresh, **Branches**, **Columns** |
| Repository panel | **actions** beside the selected commit: check out, revert, reset soft/mixed/hard, amend |
| Branches picker | choose which branches the graph draws, or **switch** to one |
| Repository picker | create a branch, stash and restore, delete untracked files |
| Change rows | `+`/`−` stage and unstage, `⟲` discards that file |
| Commit box | type a message, then **Commit** |

Hard reset, discard, clean, and amend open a confirmation naming what will be
lost. Pull is fast-forward only, so it can never quietly create a merge
commit. Push sets the upstream when the branch has none.

## Explore

- **Repository graph:** every branch keeps its own colored rail. Lanes are
  derived from the commit graph rather than stored on a commit, so the layout
  follows real topology. Lane changes are drawn as a horizontal connector with
  small corners, and merge commits use a hollow inner dot.
- **History columns:** COMMIT, BRANCH, AUTHOR, MESSAGE, and WHEN are fixed
  widths so every row lines up, and the table scrolls sideways. **Columns**
  hides or shows any column except the message.
- **Branch selection:** the history shows every ref by default. **Branches**
  narrows it to as many as five, which is what the graph can label clearly.
- **Changes:** the right sidebar lists what Git reports as changed and staged.
  The `+` and `−` on a row stage and unstage that path; the section headers
  stage or unstage everything.
- **Committing:** select the message box, type a message, and select Commit.
  The field is a minimal input that handles typing and backspace.
- **Diffs:** selecting a change opens `git diff` for that path. Selecting a
  file under FILES opens its current contents in its own tab.
- **Sidebar width:** drag the divider beside the editor. The sidebar opens at
  full width and yields to the editor when the window cannot hold both.
- **Appearance:** select the gear at the bottom of the activity bar, then choose
  Dark or Light. The choice is kept for the running session.
- **Shortcuts:** `Up`/`Down` moves through the history, `Cmd+1` opens the
  selected change as a diff, `Cmd+2` focuses a source tab, `Cmd+,` opens
  Settings, and `Escape` closes any open panel.

Patch snippets and change totals are illustrative fixtures, not computed Git
output.

## Validation

```bash
cargo fmt --manifest-path native/Cargo.toml --check
cargo clippy --locked --manifest-path native/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path native/Cargo.toml
```

The tests cover lane assignment over real commit graphs, edge routing
geometry, the source tree, diff hunk parsing, the sidebar width clamp, and the
Git layer. The Git tests build throwaway repositories in the temporary
directory and remove them afterwards; fetch and push run against a local bare
repository, so no test needs network access.

## Architecture

```text
native/src/main.rs    Desktop window, workspace views, and repository state
native/src/git.rs     Repository access: commands, parsers, and operations
native/src/graph.rs   Lane assignment, edge routing, and canvas painting
native/src/theme.rs   Palette and shared visual primitives
```

Git commands run with argument arrays rather than interpolated shell strings,
and every call blocks and runs on a background thread so the window stays
responsive. Lanes are derived from the commit graph rather than stored on a
commit, because a commit does not belong to a branch in Git.

Lanes are not stored on a commit. `graph::rows` derives them from the current
branch selection, which is what lets the gutter show five of eight branches
without rewriting the fixture.

The next implementation should introduce repository services behind these
views, replace fixture indices with stable Git identities, and move graph
layout out of the fixture data. Blocking Git work must stay off the UI thread.

VGit does not require an online account for local repository operations. Git
authentication will be delegated to the user's existing Git credential helpers
and SSH agent. See [SECURITY.md](SECURITY.md) for vulnerability reporting.

## Project status and roadmap

Done: the Git layer, and the interface reading and writing a real repository.
The history, diffs, changes, refs, and file tree all come from Git, and
staging, unstaging, and committing work. The fixture has been removed.

Near-term work, in rough order:

1. A real text editor for commit messages, replacing the minimal input
2. Progress reporting for long fetches and pushes
3. Search across commit message, author, hash, branch, and tag
4. Conflict resolution, line and hunk staging, and interactive rebase
5. Branch deletion, tag creation, and remote management
6. Recently opened repositories, and remembering the last one
7. Reflog recovery, worktrees, submodules, and Git LFS

Later work includes interactive rebase, three-way conflict resolution, line and
hunk staging, reflog recovery, worktrees, submodules, Git LFS,
hosting-provider pull requests, operating-system keychain integration,
accessibility refinements, and signed packaged releases.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md) before submitting a pull request.

## License

VGit is available under the [MIT License](LICENSE).
