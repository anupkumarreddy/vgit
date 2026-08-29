# Security Policy

## Supported versions

Security fixes are applied to the latest release and the `main` branch.

## Reporting a vulnerability

Please do not disclose security vulnerabilities in a public issue. Use the repository's **Security → Report a vulnerability** form to submit a private report.

Include the affected version, reproduction steps, expected impact, and any suggested mitigation. Please avoid including real credentials or private repository content. Maintainers will acknowledge a complete report as soon as practical and coordinate disclosure after a fix is available.

## Security design

VGit executes the installed system Git binary from Electron's main process using argument arrays. The renderer is sandboxed, has no Node.js integration, and receives only a narrow context-isolated API. Secrets must never be stored in application logs or committed to this repository.
