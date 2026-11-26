# Roadmap

## Now

- [x] Multiple folders merged into one listing, source labels on duplicate names
- [x] Read-only listing, search, breadcrumbs, sortable columns, Range downloads
- [x] Uploads land in the folder the visitor is browsing (older links: first folder's `uploads/`)
- [x] Optional browsing password
- [x] Key-gated uploads with size limit
- [x] ZIP download of a folder, streamed without touching disk
- [x] In-page refresh and remembered sort order on the web page
- [x] Tray, autostart on launch, minimize to tray, drop a folder on the window
- [x] zh / en UI; theme follows the system or can be pinned
- [x] locale catalogs shared by desktop app and web page
- [x] per-visitor language on the web page (Accept-Language + switcher)
- [x] upload link QR code
- [x] IPv6 (dual-stack listener, addresses listed) and a web manifest for phones
- [x] Resumable uploads: a dropped connection continues from the last byte
- [x] Desktop notification when a file is received
- [x] Select files on the web page and download just those as a ZIP

## Next

- [ ] Code signing + notarization (macOS), Authenticode (Windows)
- [ ] Auto-update via GitHub Releases
- [ ] Per-folder upload links instead of a global key
- [ ] Upload progress list + history in the desktop app
- [ ] Clipboard channel (text + images) between devices
- [ ] Connect hosted Weblate and open translation PRs

## Later

- [ ] Optional TLS with a self-signed cert
- [ ] Device list with per-device keys
- [ ] Read-only subfolders (per-folder policies)
- [ ] Linux tray fallbacks for desktops without AppIndicator

## Non-goals

- Public internet exposure or relay servers
- Accounts, cloud sync, or telemetry
