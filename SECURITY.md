# Security Policy

## Supported versions

Security fixes are applied to the latest release and the `main` branch.

## Reporting a vulnerability

Please do not disclose security vulnerabilities in a public issue. Use the repository's **Security → Report a vulnerability** form to submit a private report.

Include the affected version, reproduction steps, expected impact, and any suggested mitigation. Please avoid including real credentials or private repository content. Maintainers will acknowledge a complete report as soon as practical and coordinate disclosure after a fix is available.

## Security design

VGit is a native Rust application. It is currently a UI prototype that does not execute Git, access repositories, or perform network operations; all displayed data is fictional and held in memory.

As real repository support lands, VGit will execute the installed system Git binary using argument arrays rather than interpolated shell commands, destructive operations will require explicit confirmation, and Git authentication will be delegated to the user's existing credential helpers and SSH agent. Secrets must never be stored in application logs or committed to this repository.
