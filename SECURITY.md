# Security

## Reporting a vulnerability

Please do not open a public issue. Use GitHub's **Report a vulnerability**
(Security → Advisories) or email the maintainer listed in `Cargo.toml`.

## Threat model

LanTransport is built for trusted local networks. It is not a way to expose
files to the public internet.

- **No TLS.** Traffic, including the browsing password and upload key, is sent in
  clear text over the LAN.
- **Browsing** is unauthenticated unless a password is set. Anyone who can reach
  the port can read the shared folder.
- **Uploads** require the upload key. It is compared in constant time and sent in
  the `X-Upload-Key` header (or a `?key=` link parameter). Keys are shown in the
  desktop app and stored in the page's `localStorage`.
- **Password storage**: the browsing password is saved as you type, in the app
  config file with `0600` permissions on Unix. It is stored in plain text; use the
  clear button in the field to remove it.
- **Path confinement**: every request is resolved with `canonicalize` per shared
  folder and must stay inside that folder. Hidden entries (`dotfiles`) are never listed or
  served, and upload file names are sanitized. Symlinks pointing outside the root
  are refused.
- **No TLS**: the page is served over plain HTTP on the LAN. The browsing
  password and the upload key are therefore readable by anyone who can sniff the
  network. That is fine on a network you control and a bad idea on public Wi-Fi.
- **Writes** are limited to uploads into the shared folder the visitor is
  browsing (older pages that send no target still write into the first folder's
  `uploads/`): no delete, rename or overwrite. Uploads are
  written to a hidden temp file and linked into place; name collisions get a
  ` (1)` suffix.
- **Active content** (HTML, SVG, XML and similar documents a browser can run
  script in) is always served as a download with `Content-Security-Policy:
  sandbox` and `X-Content-Type-Options: nosniff`, so an uploaded file cannot
  execute code on the share's origin.

## Hardening checklist

- Share only folders you are comfortable exposing to your network.
- Set a browsing password on untrusted networks (cafés, hotels, conferences).
- Rotate the upload key after a one-off exchange.
- Keep the app updated.

## Known limits

- No request-body limits beyond the configured max upload size.
- No TLS, no CORS, no rate limit on successful uploads (only failed keys are
  throttled).
- Windows reserved device names are rejected; other platform-specific filename
  quirks may still cause a failed upload.
