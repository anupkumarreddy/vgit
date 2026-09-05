# VGit

[![CI](https://github.com/anupkumarreddy/vgit/actions/workflows/ci.yml/badge.svg)](https://github.com/anupkumarreddy/vgit/actions/workflows/ci.yml)
[![CodeQL](https://github.com/anupkumarreddy/vgit/actions/workflows/codeql.yml/badge.svg)](https://github.com/anupkumarreddy/vgit/actions/workflows/codeql.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

VGit is an open-source visual desktop Git client written in Rust with
[GPUI](https://www.gpui.rs/). It presents commits, branches, tags, remotes,
hashes, authors, and merge paths as an approachable repository map.

> **Status: UI prototype.** The application currently renders a fictional
> repository held in memory. No Git commands, repository access, or network
> operations are implemented yet, and restarting resets the demo staging area.
> The Git layer is the next milestone. See
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

## Explore the prototype

- **Repository graph:** each visible branch keeps its own colored rail. Lane
  changes are drawn as a horizontal connector with small corners, and merge
  commits use a hollow inner dot so topology reads without relying on color.
- **History columns:** COMMIT, BRANCH, AUTHOR, MESSAGE, and WHEN are fixed
  widths so every row lines up, and the table scrolls sideways. **Columns**
  hides or shows any column except the message.
- **Branch selection:** the fixture carries eight branches and the graph draws
  five at a time. **Branches** chooses which. An edge into a hidden parent is
  redirected to the nearest visible ancestor, so the history stays connected.
- **Sidebar width:** drag the divider beside the editor. The sidebar opens at
  full width and yields to the editor when the window cannot hold both.
- **Editor:** tabs open per file. Selecting a file in the source tree opens it
  in its own tab, or focuses an existing one. `Cmd+1` and `Cmd+2` move between
  the diff and a source tab.
- **Repository sidebar:** stage and unstage files, collapse the source tree,
  and read the ref list in the repository state. Selecting a ref jumps the
  history to the commit it points at.
- **Appearance:** select the gear at the bottom of the activity bar, then choose
  Dark or Light. The choice is kept for the running session.
- **Shortcuts:** `Up`/`Down` moves through the visible commits, `Space` toggles
  the selected file's staging state, `Cmd+,` opens Settings, and `Escape`
  closes any open panel.

Patch snippets and change totals are illustrative fixtures, not computed Git
output.

## Validation

```bash
cargo fmt --manifest-path native/Cargo.toml --check
cargo clippy --locked --manifest-path native/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path native/Cargo.toml
```

The tests cover lane assignment and branch selection, edge routing geometry,
and the sidebar width clamp. They need no repository, since the fixture is
held in memory.

## Architecture

```text
native/src/main.rs    Desktop window, workspace views, in-memory interactions
native/src/demo.rs    Sample commits, branches, refs, files, and patch snippets
native/src/graph.rs   Lane assignment, edge routing, and canvas painting
native/src/theme.rs   Palette and shared visual primitives
```

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

Near-term work, in rough order:

1. Repository services that invoke system Git off the UI thread
2. Real commit topology, refs, and status replacing the in-memory fixtures
3. Staging, unstaging, and guarded discard against a real working tree
4. Commit, amend, branch creation, and switching
5. Fetch with pruning, fast-forward-only pull, and push with upstream setup
6. Stash creation and application
7. Search across commit message, author, hash, branch, and tag
8. Commit creation UI, text editing, adjustable pane sizes, and persistence

Later work includes interactive rebase, three-way conflict resolution, line and
hunk staging, reflog recovery, worktrees, submodules, Git LFS,
hosting-provider pull requests, operating-system keychain integration,
accessibility refinements, and signed packaged releases.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md) before submitting a pull request.

## License

VGit is available under the [MIT License](LICENSE).
