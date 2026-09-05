# VGit native preview

A desktop UI prototype written entirely in Rust with GPUI. All repository data
is fictional and held in memory. No Git commands, repository access, or network
operations are implemented. Restarting resets the demo staging area.

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

- **Repository graph:** select a commit in the left sidebar. Local commits and
  branches use green, remote-tracking state uses blue, tags use purple, and
  merge commits use an amber double ring with a hollow center.
- **Editor:** use the Diff and Source tabs in the center panel. Cmd+1/Ctrl+1 and
  Cmd+2/Ctrl+2 switch between them.
- **Repository sidebar:** select files and stage or unstage them from the right
  panel. It also shows HEAD/upstream state and the sample source tree.
- **Appearance:** select the gear at the bottom of the activity bar, then choose
  Dark or Light. The choice is kept for the running preview session.
- **Shortcuts:** Up/Down selects commits, Space toggles the selected file's
  staging state, Cmd+,/Ctrl+, opens Settings, and Escape closes it.

Commit creation, real Git operations, text editing, search, adjustable pane
sizes, full accessibility, and persistence are deferred. Patch snippets and
change totals are illustrative fixtures, not computed Git output.

## Structure

```text
src/main.rs    Desktop window, workspace views, in-memory interactions
src/demo.rs    Sample commits, topology, files, and patch snippets
src/graph.rs   Native canvas painting of the sample commit DAG
src/theme.rs   Palette and shared visual primitives
```

The next implementation should introduce repository services behind these
views, replace fixture indices with stable Git identities, and move graph
layout out of the fixture data. Keep blocking Git work off the UI thread.

## Checks

```sh
cargo fmt --manifest-path native/Cargo.toml --check
cargo clippy --locked --manifest-path native/Cargo.toml -- -D warnings
```
