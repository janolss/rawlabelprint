# Contributing

Thanks for considering a contribution to RawLabelPrint.

## Development

See the **Develop** / **Build** sections in [README.md](../README.md) for prerequisites and how to run the app (`npm install`, `npm run tauri dev`).

Before opening a PR, run:

```bash
npm run lint
npm test
```

## Pull requests

- Keep changes focused (one concern per PR).
- Describe *what* changed and *why*.
- Do not change SQL/config schemas or public HTTP APIs unless the PR explicitly calls that out.
- Security issues: follow [SECURITY.md](SECURITY.md) — do not open a public issue.

## Issues

- Include OS, app version, and steps to reproduce.
- For printer problems: network vs USB, and whether Compatible mode is on.
