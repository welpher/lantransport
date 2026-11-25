# Translating LanTransport

The UI ships in English and Chinese today. We want more, and the plan is to use
[Weblate](https://weblate.org) once the repository is public (hosted Weblate is
free for libre projects).

## Current state

Strings live in two places while the catalogs are being extracted:

- `src/i18n.ts` — desktop UI
- `src-tauri/src/server/listing.rs` — the web page served to visitors

## How to add a language today

1. Copy the strings for one locale and translate them.
2. Keep placeholders (`{n}`) and product names (`LanTransport`) unchanged.
3. Open a PR with the locale code (BCP 47, e.g. `pt-BR`).

## Planned structure

```
locales/
  en.json       # base language
  zh-CN.json
  ...
```

- Flat keys, ICU plurals: `{"items.count": "{n} items"}`
- One catalog used by both the Rust server (`include_str!`) and the TypeScript UI
- Web page language: `Accept-Language` + a switcher, remembered in `localStorage`
- Desktop language: follows the system by default, overridable in the app

## Rules

- Never translate `LanTransport`, `share` (the Basic auth username), or file names.
- Keep sentences short; the web page is used on phones.
- Don't add HTML to strings.
- If a screenshot helps, include one in the PR.
