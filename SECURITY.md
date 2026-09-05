# Security Policy

## Supported versions

Security fixes are applied to the latest release and the `main` branch.

## Reporting a vulnerability

Please do not disclose security vulnerabilities in a public issue. Use the repository's **Security → Report a vulnerability** form to submit a private report.

Include the affected version, reproduction steps, expected impact, and any suggested mitigation. Please avoid including real credentials or private repository content. Maintainers will acknowledge a complete report as soon as practical and coordinate disclosure after a fix is available.

## Security design

VGit is a native Rust application. It executes the installed system Git binary from `native/src/git.rs`, always with an argument array and never through a shell, so no repository path, branch name, or commit message can be interpreted as shell syntax. VGit opens the repository it is launched from. It reads history, diffs, status, and refs, and writes to the repository when staging, unstaging, or committing.

Git commands block, and callers run them off the UI thread. Git authentication is delegated to the user's existing credential helpers and SSH agent; VGit does not prompt for, store, or transmit credentials.

Operations that can destroy work — `reset` with `ResetMode::Hard`, `discard`, `clean`, and `amend` — are named plainly and are reachable only through a confirmation that names what will be lost. No destructive operation runs directly from a button. Pull is fast-forward only, so it cannot create a merge commit unexpectedly, and `clean` is the one operation Git itself cannot undo. Secrets must never be stored in application logs or committed to this repository.
