# LanTransport

Share files across your local network from a small desktop app.

Pick a folder, press **Start sharing**, and anyone on the same Wi‑Fi can open the
link in a browser to browse and download. Read-only by default. People you give
an **upload key** can also drop files into the folder they are viewing.

No account, no cloud, no relay: the files never leave your network.

```
┌──────────────────────────────────────────────────────────┐
│                                                          │
│  ▪ LanTransport            ● Sharing    Theme  English   │
│    Turn any folder into a LAN file page                  │
│                                                          │
│    Share   Activity   Settings          [Stop sharing]   │
│                                                          │
│    Folders                                  [Add folder] │
│    /Users/me/Downloads                      [Open] [×]   │
│                                                          │
│    Sharing  /Users/me/Downloads                  ● Live  │
│    ┌──────┐   Links                          [Copy]      │
│    │ ▓▓▓▓ │   http://192.168.1.24:8123                   │
│    │ ▓▓▓▓ │   http://192.168.1.31:8123                   │
│    │ ▓▓▓▓ │   [Open in browser]                          │
│    └──────┘                                              │
│    Upload key  k7m3qp9x2f  [Copy key] [Upload link]      │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

## Features

- **Share several folders at once** — their contents appear as one merged listing;
  files with the same name are labeled by source folder
- **Browser UI** for any phone, tablet or computer — no app needed on the other side
- **Read-only by default**: browse, preview, download; Range requests so downloads resume
- **ZIP download from the folder row** — streamed, so a 2 GB folder starts
  immediately and nothing is written to disk
- **Sortable list**: name, size or modified time, ascending or descending — the choice is remembered, and **refresh** re-reads the folder without reloading the page
- **Optional password** for browsing (HTTP Basic, username `share`)
- **Key-gated uploads**: rotate the key anytime; files land in the folder the visitor
  is browsing (the first folder's `uploads/` is used by older links)
- **Folder upload**: drop a folder on the share page and its folders are
  recreated; empty folders included, 2000 files max per drop
- **QR code** for the LAN link, one click to copy
- **Tray app**: keeps sharing while the window is closed; start/stop from the tray
- **Always LAN-bound**: the server listens on your local network; there is no local-only mode to misconfigure
- **Cross-platform**: macOS (Intel & Apple Silicon), Windows, Linux
- **Light/dark** follows the system, or pin it from the header; English and 中文 today, more via [Weblate](docs/TRANSLATING.md)
- Small installers: the app uses the system webview instead of bundling a browser

## Screenshots

| Sharing | Settings | Activity |
|---|---|---|
| ![Sharing](docs/screenshots/app-running.png) | ![Settings](docs/screenshots/app-settings.png) | ![Activity](docs/screenshots/app-activity.png) |

| Phone | Desktop |
|---|---|
| ![Phone](docs/screenshots/web-mobile.png) | ![Desktop](docs/screenshots/web-desktop.png) |

## Install

### Homebrew (macOS)

```bash
brew install --cask welpher/tap/lantransport
```

### winget (Windows) — submitted, awaiting review

The manifest lives in [`packaging/winget`](packaging/winget) and the first pull
request is in the review queue. Until it lands, install the `.exe` from the
release below.

### Arch Linux — PKGBUILD included, not in the AUR yet

[`packaging/aur/PKGBUILD`](packaging/aur/PKGBUILD) builds from the AppImage; it
has not been submitted to the AUR. Meanwhile use the AppImage or the `.deb`.

### Direct downloads

| Platform | File |
|---|---|
| macOS | `.dmg` (universal) |
| Windows | `.exe` (NSIS) or `.msi` |
| Linux | `.AppImage`, `.deb`, `.rpm` |

Builds are not code-signed yet, so macOS Gatekeeper and Windows SmartScreen will
warn on first launch. On macOS: right-click the app → **Open**.

## How it works

A small HTTP server is embedded in the app (Rust, no runtime to install). The UI
is Tauri 2 + the system webview. Requests are served straight from the chosen
folder; nothing is copied or indexed.

Security model, limits and known caveats live in [SECURITY.md](SECURITY.md).

### Why does the browser say "Not secure"?

Because the page is plain HTTP on a local address. Nothing leaves your network,
but a browser has no way to know that: it marks every non-HTTPS page, and a LAN
IP cannot get a certificate that phones would trust (a self-signed one replaces
the small grey label with a full-page "your connection is not private" warning,
which is worse for guests).

What that means in practice:

- **Home or office Wi-Fi** — the traffic stays on your network; the label is
  cosmetic. Visitors only need the link.
- **Public Wi-Fi** (cafés, hotels, conferences) — someone on the same network can
  read the browsing password and upload key, both of which travel in plain text.
  Use a password, rotate the upload key afterwards, or share over a network you
  trust instead.

HTTPS with a verifiable certificate needs a domain name pointing at the LAN
address, which is why it is not offered here.

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
                         (drop a new <code>.json in and the pickers pick it up)
src-tauri/src/server/    embedded HTTP server (listing, downloads, uploads, auth)
src-tauri/src/           Tauri shell: tray, settings, commands
```

## Sponsor

LanTransport is free and MIT-licensed. Tips are voluntary and pay for the boring
parts that keep releases trustworthy: macOS notarization, Windows code signing,
and release infrastructure.

- [Alipay / WeChat Pay QR codes and other ways to help](SUPPORT.md)

[![Sponsor](https://img.shields.io/badge/Sponsor-%E2%99%A5-ea4aaa?logo=github&style=flat-square)](SUPPORT.md)

## Roadmap

See [docs/ROADMAP.md](docs/ROADMAP.md). Near-term: code signing, auto-update,
per-folder links, upload history and a clipboard channel.

## License

MIT © 2026 welpher
