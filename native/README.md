# VGit native preview

A visual Git client written entirely in Rust with GPUI.

VGit opens the repository it is launched from and shows its real history,
diffs, changes, and refs. Staging, unstaging, and committing act on that
repository. There is no fixture data left in the application.

Not yet implemented: fetch, pull, push, branch switching, and the destructive
operations. The Git layer supports them all and they are covered by tests;
none of them are reachable from a control yet.

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

- **History:** every branch keeps its own colored rail. Lanes are derived from
  the commit graph, not stored on a commit, so the layout follows real
  topology. Merge commits use a hollow inner dot.
- **Columns:** COMMIT, BRANCH, AUTHOR, MESSAGE, and WHEN are fixed widths, so
  every row lines up, and the table scrolls sideways. **Columns** hides or
  shows any column except the message.
- **Branches:** the history shows every ref by default. **Branches** narrows it
  to as many as five, which is what the graph can label clearly.
- **Changes:** the right sidebar lists what Git reports as changed and staged.
  The `+` and `−` on a row stage and unstage that path for real, and the
  header buttons stage or unstage everything.
- **Committing:** select the message box, type, and select Commit. The message
  field is a minimal input, so it handles typing and backspace and nothing
  more.
- **Diffs:** selecting a change opens `git diff` for that path, with line
  numbers following the new side of each hunk. Selecting a file under FILES
  opens its current contents in a tab.
- **Refresh:** `↻` in the history header re-reads the repository. Every write
  reloads on its own.
- **Appearance:** the gear at the bottom of the activity bar chooses Dark or
  Light for the running session.
- **Shortcuts:** Up/Down moves through the history, Cmd+1/Ctrl+1 opens the
  selected change as a diff, Cmd+2/Ctrl+2 focuses a source tab, Cmd+,/Ctrl+,
  opens Settings, and Escape closes any open panel.

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

Staging, unstaging, and committing are wired to the interface. Fetch, pull,
push, branch switching, and the destructive operations are implemented and
tested but deliberately not reachable from a control yet.

The Git layer is covered by integration tests that build throwaway
repositories, including fetch and push against a local bare repository, so the
tests need no network access.

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
