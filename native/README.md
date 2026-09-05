# VGit native preview

A desktop workspace written entirely in Rust with GPUI.

The Git layer in `src/git.rs` is real: it runs the installed `git` binary and
parses its output. The **Live repository** panel in the right sidebar shows
that layer reading the working directory VGit was launched from, off the UI
thread.

Everything else on screen -- the history graph, diffs, file lists, and source
tree -- is still the in-memory fixture in `src/demo.rs`. Restarting resets the
demo staging area.

## Run

From the repository root:

```sh
cargo run --locked --manifest-path native/Cargo.toml
```

The first build downloads and compiles GPUI dependencies. On macOS, install Rust
and Xcode Command Line Tools. This prototype enables GPUI's `runtime_shaders`
feature so the offline Metal compiler from full Xcode is not required.

To produce a local macOS application bundle:

```sh
./native/scripts/bundle-macos.sh
open 'out/VGit Preview.app'
```

The local bundle is unsigned and intended for development. The prototype has
been developed on macOS; Windows and Linux packaging is not yet validated.

## Explore

- **Repository graph:** select a commit in the left sidebar. Each visible
  branch keeps its own colored rail, and lane changes are drawn as a
  horizontal connector with small corners rather than a long curve. Merge
  commits use a hollow inner dot so topology reads without relying on color.
- **History columns:** COMMIT, BRANCH, AUTHOR, MESSAGE, and WHEN are fixed
  widths, so every row lines up. The table is wider than the sidebar and
  scrolls sideways. Select **Columns** to hide or show any column except the
  message.
- **Branch selection:** the fixture carries eight branches and the gutter
  draws five at a time. Select **Branches** to choose which. Commits on a
  hidden branch disappear, and an edge into a hidden parent is redirected to
  the nearest visible ancestor, so the history never breaks apart.
- **Sidebar width:** drag the divider between the history and the editor. The
  sidebar opens at full width and yields to the editor when the window is too
  narrow to hold both.
- **Editor:** tabs open per file. Selecting a file in the source tree opens it
  in its own tab, or focuses the tab if it is already open. Cmd+1/Ctrl+1 and
  Cmd+2/Ctrl+2 move between the diff and a source tab.
- **Repository sidebar:** stage or unstage files, expand and collapse the
  source tree, and read the ref list in the repository state. Selecting a ref
  jumps the history to the commit it points at.
- **Appearance:** select the gear at the bottom of the activity bar, then
  choose Dark or Light. The choice is kept for the running preview session.
- **Shortcuts:** Up/Down moves through the visible commits, Space toggles the
  selected file's staging state, Cmd+,/Ctrl+, opens Settings, and Escape
  closes any open panel.

Commit creation, real Git operations, text editing, search, and persistence
are deferred. Patch snippets and change totals are illustrative fixtures, not
computed Git output.

## Structure

```text
src/main.rs    Desktop window, workspace views, in-memory interactions
src/git.rs     Real repository access: commands, parsers, and operations
src/demo.rs    Sample commits, branches, refs, files, and patch snippets
src/graph.rs   Lane assignment, edge routing, and canvas painting
src/theme.rs   Palette and shared visual primitives
```

## The Git layer

`src/git.rs` runs `git` with an argument array, never a shell string, so no
path, branch name, or commit message can be read as shell syntax. Every call
blocks, and the caller runs it off the UI thread; `Workspace::load_repository`
shows the pattern.

Implemented: repository discovery, status (including renames, conflicts,
detached HEAD, and ahead/behind), log with parents and refs, ref listing,
staging and unstaging, commit and amend, revert, reset in all three modes,
discard, clean, stash, branches, and fetch/pull/push.

Destructive operations are named plainly and kept separate: `reset` with
`ResetMode::Hard`, `discard`, and `clean` can lose uncommitted work.
`ResetMode::is_destructive` exists so callers can gate them behind a
confirmation before any of this reaches a button.

None of these operations are wired to the interface yet. They are covered by
integration tests that build throwaway repositories, including fetch and push
against a local bare repository, so the tests need no network access.

Lanes are not stored on a commit. `graph::rows` derives them from the current
branch selection, which is what lets the gutter show five of eight branches
without rewriting the fixture.

The next implementation should introduce repository services behind these
views, replace fixture indices with stable Git identities, and move graph
layout out of the fixture data. Keep blocking Git work off the UI thread.

## Checks

```sh
cargo fmt --manifest-path native/Cargo.toml --check
cargo clippy --locked --manifest-path native/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path native/Cargo.toml
```

The tests cover lane assignment and the branch selection, edge routing
geometry, and the sidebar width clamp. They need no repository, since the
fixture is in memory.
