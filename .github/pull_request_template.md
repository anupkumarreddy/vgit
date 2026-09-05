## Summary

Describe the change and the user problem it solves.

## Validation

- [ ] `cargo fmt --manifest-path native/Cargo.toml --check`
- [ ] `cargo clippy --locked --manifest-path native/Cargo.toml --all-targets -- -D warnings`
- [ ] `cargo test --locked --manifest-path native/Cargo.toml`
- [ ] UI changes were tested in the running application
- [ ] Screenshots are included for visual changes
- [ ] No credentials, private repository content, or generated build output is included

## Security impact

Describe any changes to Git command execution, filesystem access, credentials, or destructive operations. Write "None" when not applicable.
