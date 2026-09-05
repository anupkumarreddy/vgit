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

- **Repository graph:** select a commit in the left sidebar. Local commits and
  branches use green, remote-tracking state uses blue, tags use purple, and
  merge commits use an amber double ring with a hollow center.
- **Editor:** use the Diff and Source tabs in the center panel. `Cmd+1` and
  `Cmd+2` switch between them.
- **Repository sidebar:** select files and stage or unstage them from the right
  panel. It also shows HEAD/upstream state and the sample source tree.
- **Appearance:** select the gear at the bottom of the activity bar, then choose
  Dark or Light. The choice is kept for the running session.
- **Shortcuts:** `Up`/`Down` selects commits, `Space` toggles the selected
  file's staging state, `Cmd+,` opens Settings, and `Escape` closes it.

Patch snippets and change totals are illustrative fixtures, not computed Git
output.

## Validation

```bash
cargo fmt --manifest-path native/Cargo.toml --check
cargo clippy --locked --manifest-path native/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path native/Cargo.toml
```

## Architecture

```text
native/src/main.rs    Desktop window, workspace views, in-memory interactions
native/src/demo.rs    Sample commits, topology, files, and patch snippets
native/src/graph.rs   Native canvas painting of the sample commit DAG
native/src/theme.rs   Palette and shared visual primitives
```

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
