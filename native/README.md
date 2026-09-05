# VGit native desktop

A visual Git client written in Rust with GPUI. History, status, refs, diffs,
and the file tree come from the opened repository. See the root
[README](../README.md) for controls and the roadmap.

## Run and package

```sh
cargo run --locked --manifest-path native/Cargo.toml
./native/scripts/bundle-macos.sh
open 'out/VGit Preview.app'
```

Run from the repository root. The app discovers the repository in its working
directory; **Open** selects another. The unsigned macOS bundle is for local
development. Rust and Xcode Command Line Tools are required; runtime Metal
shader compilation avoids requiring full Xcode. Other platforms are unvalidated.

## Implementation

- `src/main.rs`: GPUI workspace, background reads, operation guard, and views.
- `src/git.rs`: Git subprocesses, status/log/ref parsing, and mutations.
- `src/graph.rs`: topology-derived rails and smooth connectors.
- `src/input.rs`: text selection, clipboard, Unicode editing, native IME input.
- `src/theme.rs`: dark/light palettes and shared primitives.

The workspace is arranged around the history. The left sidebar holds the
graph, then the selected commit with its actions, then the stash. The centre stacks
the editor above a dock holding the changes on the left, and the staged
changes above the commit message on the right. The right sidebar holds source control state, refs, and the file
tree. Commit actions enable only when Git would accept them, so revert is
unavailable while the index holds staged work and amend only applies to HEAD.

Git receives argument arrays and literal pathspecs. A file named `a[1].txt`
cannot select its neighbor `a1.txt` during discard or staging. Pull and push
use the configured upstream destination, even when it differs from the local
branch name. A new branch uses origin, or the sole remote; ambiguous remotes
produce an error.

Only one mutation runs at a time. Additional operations are rejected with a
status message while it runs. Both success and failure refresh repository
state, because an unsuccessful revert or stash operation can leave conflicts.
Read generations reject obsolete completions, and refresh reloads every open
editor tab. Commit selection follows its object ID across history updates.

Partially staged files appear in both change sections. Each row opens and
operates on its own index or working-tree version. Untracked text has an
additions preview; binary files have an explicit marker. Unstaging also works
before the first commit. Destructive controls require confirmation.

Input fields support cursor movement, Shift selection, select-all, cut/copy/
paste, grapheme-aware deletion, and native composition with UTF-16 ranges.
Messages remain available after cancellation or failure. Theme settings and
drafts are session-local. There is no file watcher; use refresh for external edits.

## Checks

```sh
cargo fmt --manifest-path native/Cargo.toml --check
cargo clippy --locked --manifest-path native/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path native/Cargo.toml
```

Integration tests create disposable repositories, including local bare remotes.
They cover path isolation, unborn HEAD, conflicts, staged/working versions,
tracking destinations, annotated refs, and existing Git operations. Unit tests
cover graph routing, distinct rails in busy histories, layout, and Unicode input.
