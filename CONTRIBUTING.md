# Contributing to VGit

Thank you for helping improve VGit. Contributions of code, tests, documentation, design feedback, and reproducible bug reports are welcome.

VGit is a native Rust application built with [GPUI](https://www.gpui.rs/). It is currently a UI prototype backed by in-memory fixtures; see the [README](README.md) for current status and roadmap.

## Development setup

VGit requires Rust 1.98 or later and, on macOS, the Xcode Command Line Tools.

```bash
git clone https://github.com/anupkumarreddy/vgit.git
cd vgit
cargo run --locked --manifest-path native/Cargo.toml
```

The first build downloads and compiles GPUI and its dependencies, which takes several minutes.

Before opening a pull request, run:

```bash
cargo fmt --manifest-path native/Cargo.toml --check
cargo clippy --locked --manifest-path native/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path native/Cargo.toml
```

## Pull requests

- Keep each pull request focused on one problem.
- Add or update tests for behavior changes.
- Describe user-visible changes and include screenshots for UI changes.
- Format with `cargo fmt` and keep the build free of `clippy` warnings.
- Keep blocking Git and filesystem work off the UI thread.
- Invoke Git with argument arrays. Never concatenate user-controlled values into shell commands.
- Do not include credentials, private repositories, personal data, or proprietary source files in issues, fixtures, or commits.

By contributing, you agree that your contribution is licensed under the MIT License.
