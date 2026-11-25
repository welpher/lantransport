# Contributing

Thanks for helping.

```bash
pnpm install
pnpm tauri dev
pnpm test
```

- Keep the server logic in `src-tauri/src/server/` free of Tauri types so it stays testable.
- Add a test only for logic where a silent regression is costly: path resolution,
  auth, uploads, Range handling. UI copy and wiring do not need tests.
- Run `cargo fmt` and `cargo clippy -- -D warnings` before opening a PR.
- Security reports go to [SECURITY.md](SECURITY.md), never a public issue.
