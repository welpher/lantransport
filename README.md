# LanTransport

Share files across your local network from a small desktop app.

Pick a folder, press **Start sharing**, and anyone on the same Wi‑Fi can open the
link in a browser to browse and download. Read-only by default. People you give
an **upload key** to can also drop files into the folder they are viewing.

No account, no cloud, no relay: the files never leave your network.

```
┌──────────────────────────────────────────┐
│  LanTransport               ●  Sharing   │
│                                          │
│  Folder                                  │
│  /Users/me/Downloads           [Choose…] │
│  Port 8000      [x] Allow LAN access     │
│                                          │
│  [x] Allow uploads with a key            │
│      k7m3qp9x2f   [Copy] [Regenerate]    │
│                                          │
│  http://192.168.1.24:8000  [Copy]        │
└──────────────────────────────────────────┘
```

## Features

- **Browser UI** for any phone, tablet or computer — no app needed on the other side
- **Read-only by default**: browse, preview, download; Range requests so downloads resume
- **Optional password** for browsing (HTTP Basic, username `share`)
- **Key-gated uploads**: rotate the key anytime; uploads land in the folder being viewed
- **QR code** for the LAN link, one click to copy
- **Tray app**: keeps sharing while the window is closed; start/stop from the tray
- **Cross-platform**: macOS (Intel & Apple Silicon), Windows, Linux
- Light/dark follows the system; English and 中文 today, more via [Weblate](docs/TRANSLATING.md)
- Small installers: the app uses the system webview instead of bundling a browser

## Install

Download the installer for your platform from [Releases](../../releases).

| Platform | File |
|---|---|
| macOS | `.dmg` (Universal) |
| Windows | `.exe` (NSIS) |
| Linux | `.AppImage`, `.deb`, `.rpm` |

Builds are not code-signed yet, so macOS Gatekeeper and Windows SmartScreen will
warn on first launch. On macOS: right-click the app → **Open**. Verify the
checksums published with each release if you like.

## How it works

A small HTTP server is embedded in the app (Rust, no runtime to install). The UI
is Tauri 2 + the system webview. Requests are served straight from the chosen
folder; nothing is copied or indexed.

Security model, limits and known caveats live in [SECURITY.md](SECURITY.md).

## Development

Prerequisites: Node 20+, pnpm, Rust stable, and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
pnpm install
pnpm tauri dev          # desktop app with hot reload
pnpm tauri build        # installers in src-tauri/target/release/bundle
cargo test --manifest-path src-tauri/Cargo.toml
pnpm icons              # regenerate app/tray icons from assets/*.svg
pnpm preview:ui         # UI only, mocked IPC → http://localhost:1421/?mode=running&uploads=1
pnpm i18n:check         # verify locales/*.json keys and placeholders
```

Layout:

```
src/                     UI (vanilla TypeScript + Vite)
locales/                 one catalog per language, shared by app and web page
src-tauri/src/server/    embedded HTTP server (listing, downloads, uploads, auth)
src-tauri/src/           Tauri shell: tray, settings, commands
```

## Sponsor

LanTransport is free and MIT-licensed, with no telemetry and no account. Sponsorship
pays for the boring parts that keep releases trustworthy:

- macOS notarization and Windows code signing
- release infrastructure and a domain for update checks
- the occasional coffee while triaging issues

[![Sponsor on GitHub](https://img.shields.io/badge/Sponsor-%E2%99%A5-ea4aaa?logo=github&style=flat-square)](https://github.com/sponsors/welpher)

Other ways to help that cost nothing: star the repo, report bugs with your OS and
version, translate the UI (see `docs/TRANSLATING.md`), or tell one person who
keeps emailing themselves files.

## Roadmap

See [docs/ROADMAP.md](docs/ROADMAP.md). Near-term: code signing, auto-update,
per-folder links, upload history and a clipboard channel.

## License

MIT © 2026 welpher
