# VGit

[![CI](https://github.com/anupkumarreddy/vgit/actions/workflows/ci.yml/badge.svg)](https://github.com/anupkumarreddy/vgit/actions/workflows/ci.yml)
[![CodeQL](https://github.com/anupkumarreddy/vgit/actions/workflows/codeql.yml/badge.svg)](https://github.com/anupkumarreddy/vgit/actions/workflows/codeql.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

VGit is an open-source visual desktop Git client built with Electron, React, and TypeScript. It presents commits, branches, tags, remotes, hashes, authors, and merge paths as an approachable repository map while retaining real system-Git behavior.

> VGit is under active development. Use a backup or a disposable repository when evaluating destructive workflows.

## Features

- Parent-derived commit topology with branch divergence and merge convergence
- Clear HEAD, local branch, remote branch, tag, and merge indicators
- Search by commit message, author, hash, branch, or tag
- Commit inspector with full hash, author, parent commits, references, and patch
- Staged, unstaged, untracked, renamed, deleted, and conflicted file states
- Unified working-tree, staged, and commit diffs
- Individual and bulk stage/unstage operations
- Commit and amend workflows
- Local branch creation and switching
- Fetch with pruning, fast-forward-only pull, and push with upstream setup
- Stash creation and application
- Guarded discard for tracked and untracked changes
- Repository, remote, upstream, ahead, and behind status

## Requirements

- Node.js 22 or later
- Git 2.23 or later (`git switch` support)
- macOS, Windows, or Linux supported by Electron

## Run from source

```bash
git clone https://github.com/anupkumarreddy/vgit.git
cd vgit
npm ci
npm run dev
```

Create and run the production renderer build:

```bash
npm run build
npm start
```

## Validation

```bash
npm run check
npm audit --audit-level=high
```

`npm run check` runs the Git/status and topology unit tests, TypeScript checks, Electron compilation, and production renderer build.

## Architecture and security

```text
Sandboxed React renderer
        │ narrow, context-isolated API
Electron preload bridge
        │ validated IPC operations
Electron main process
        │ execFile with argument arrays
Installed system Git
```

The renderer has no Node.js integration. Repository state and Git execution remain in the main process. Git commands use `execFile` with argument arrays instead of interpolated shell commands, and destructive file discard requires explicit confirmation.

VGit does not require an online account for local repository operations. Git authentication is delegated to the user's existing Git credential helpers and SSH agent.

See [SECURITY.md](SECURITY.md) for vulnerability reporting.

## Project status and roadmap

The current version is a functional source release. Planned work includes interactive rebase, three-way conflict resolution, line and hunk staging, reflog recovery, worktrees, submodules, Git LFS, hosting-provider pull requests, operating-system keychain integration, accessibility refinements, and signed packaged releases.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md) before submitting a pull request.

## License

VGit is available under the [MIT License](LICENSE).
