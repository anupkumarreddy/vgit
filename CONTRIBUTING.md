# Contributing to VGit

Thank you for helping improve VGit. Contributions of code, tests, documentation, design feedback, and reproducible bug reports are welcome.

## Development setup

VGit requires Node.js 22 or later and a working Git installation.

```bash
git clone https://github.com/anupkumarreddy/vgit.git
cd vgit
npm ci
npm run dev
```

Before opening a pull request, run:

```bash
npm run check
npm audit --audit-level=high
```

## Pull requests

- Keep each pull request focused on one problem.
- Add or update tests for behavior changes.
- Describe user-visible changes and include screenshots for UI changes.
- Preserve the Electron security boundary: renderer code must not receive unrestricted Node.js or shell access.
- Invoke Git with argument arrays. Never concatenate user-controlled values into shell commands.
- Do not include credentials, private repositories, personal data, or proprietary source files in issues, fixtures, or commits.

By contributing, you agree that your contribution is licensed under the MIT License.
