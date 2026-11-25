# Translating LanTransport

The catalogs live in [`locales/`](../locales). English is the base language; every
other file must have the same keys and the same `{placeholders}`.

## Current state

`locales/en.json` and `locales/zh-CN.json` are the single source of truth. The Rust
server embeds them at compile time (`include_str!`), and the desktop UI imports the
same files, so a translation fixes both surfaces at once.

The web page picks a language per visitor: `?lang=` → `lt_lang` cookie →
`Accept-Language` → English. The desktop app follows the system language and can be
overridden in the app.

## Adding a language by hand

1. Copy `locales/en.json` to `locales/<code>.json` using a BCP 47 code (`pt-BR`, `ja`).
2. Translate the values, keep the keys untouched.
3. Keep placeholders (`{n}`, `{size}`) and product names (`LanTransport`) unchanged.
4. Add the language to `LANGUAGES` in two places:
   - `src-tauri/src/server/i18n.rs`
   - `src/i18n.ts`
5. Run `pnpm i18n:check` and open a PR.

## Weblate

Once the repository is public, hosted Weblate (free for libre projects) can take
over the workflow:

1. Create the project at <https://hosted.weblate.org> and add a component.
2. Source: this GitHub repository.
3. File mask: `locales/*.json`
4. Base file: `locales/en.json`
5. File format: **JSON file (i18next)**, monolingual.

Weblate then opens pull requests with translated catalogs. CI runs
`pnpm i18n:check` on every PR, so key and placeholder mismatches fail fast.

## Rules

- Never translate `LanTransport`, `share` (the Basic auth username), or file names.
- Keep sentences short; the web page is used on phones.
- Don't put HTML in strings.
