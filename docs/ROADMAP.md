# Roadmap

## Now

- [x] Multiple folders merged into one listing, source labels on duplicate names
- [x] Read-only listing, search, breadcrumbs, Range downloads
- [x] Uploads land in the first folder's `uploads/`
- [x] Optional browsing password
- [x] Key-gated uploads with size limit
- [x] Tray, autostart on launch, minimize to tray
- [x] zh / en UI, light / dark
- [x] locale catalogs shared by desktop app and web page
- [x] per-visitor language on the web page (Accept-Language + switcher)

## Next

- [ ] Code signing + notarization (macOS), Authenticode (Windows)
- [ ] Auto-update via GitHub Releases
- [ ] Per-folder upload links instead of a global key
- [ ] Upload progress list + history in the desktop app
- [ ] QR code for upload links (currently browse link)
- [ ] Clipboard channel (text + images) between devices
- [ ] Connect hosted Weblate and open translation PRs

## Later

- [ ] Optional TLS with a self-signed cert
- [ ] Range uploads / resumable uploads
- [ ] Device list with per-device keys
- [ ] Read-only subfolders (per-folder policies)
- [ ] Linux tray fallbacks for desktops without AppIndicator

## Non-goals

- Public internet exposure or relay servers
- Accounts, cloud sync, or telemetry
