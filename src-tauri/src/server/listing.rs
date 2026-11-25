//! HTML directory listing page.

use chrono::{DateTime, Local};

use super::http::{encode_segment, escape_html};
use super::i18n;

pub struct FileEntry {
    pub mount: usize,
    pub size: u64,
    pub modified: Option<DateTime<Local>>,
    pub label: String,
}

pub struct Entry {
    pub name: String,
    pub dir_mounts: Vec<usize>,
    pub files: Vec<FileEntry>,
}

pub fn human_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    for unit in UNITS {
        if value < 1024.0 || unit == UNITS[4] {
            return if unit == "B" {
                format!("{n} B")
            } else {
                format!("{value:.1} {unit}")
            };
        }
        value /= 1024.0;
    }
    format!("{n} B")
}

pub fn previewable(name: &str) -> bool {
    let extension = std::path::Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "avif"
            | "bmp"
            | "svg"
            | "pdf"
            | "mp4"
            | "webm"
            | "mov"
            | "m4v"
            | "mp3"
            | "wav"
            | "m4a"
            | "ogg"
            | "flac"
            | "txt"
            | "md"
            | "log"
            | "json"
            | "csv"
    )
}

pub fn ext_label(name: &str, is_dir: bool) -> String {
    if is_dir {
        return "DIR".into();
    }
    match std::path::Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
    {
        Some(ext) if !ext.is_empty() => ext.chars().take(5).collect::<String>().to_uppercase(),
        _ => "FILE".into(),
    }
}

pub fn render(
    dir_name: &str,
    url_path: &str,
    entries: &[Entry],
    total_size: u64,
    locale: &str,
    nonce: &str,
    uploads: bool,
) -> String {
    let t = |key: &str| i18n::t(locale, key);
    let t1 = |key: &str, value: String| {
        i18n::t_args(locale, key, &[("n", value.clone()), ("size", value)])
    };

    let mut rows = String::new();
    if url_path != "/" {
        let parent = url_path
            .trim_end_matches('/')
            .rsplit_once('/')
            .map_or("/", |(head, _)| head);
        let parent = if parent.is_empty() { "/" } else { parent };
        rows.push_str(&format!(
            "<tr class='dir' data-name='..' data-href='{}'><td><div class='name'><span class='ext'>DIR</span><a href='{}'>..</a></div></td><td class='size'>—</td><td class='time'>—</td><td class='act'></td></tr>",
            escape_html(&format!("{}/", parent.trim_end_matches('/'))),
            escape_html(&format!("{}/", parent.trim_end_matches('/')))
        ));
    }

    let mut dirs: Vec<&Entry> = entries
        .iter()
        .filter(|entry| !entry.dir_mounts.is_empty())
        .collect();
    let mut files: Vec<&Entry> = entries
        .iter()
        .filter(|entry| !entry.files.is_empty())
        .collect();
    dirs.sort_by_key(|entry| entry.name.to_lowercase());
    files.sort_by_key(|entry| entry.name.to_lowercase());

    for entry in &dirs {
        let name = escape_html(&entry.name);
        rows.push_str(&format!(
            "<tr class='dir' data-name='{}' data-href='{}/'><td><div class='name'><span class='ext'>DIR</span><a href='{}/'>{}/</a></div></td><td class='size'>—</td><td class='time'>—</td><td class='act'></td></tr>",
            escape_html(&entry.name.to_lowercase()),
            encode_segment(&entry.name),
            encode_segment(&entry.name),
            name
        ));
    }

    for entry in &files {
        let needs_src = entry.files.len() > 1 || !entry.dir_mounts.is_empty();
        for file in &entry.files {
            let href = if needs_src {
                format!("{}?src={}", encode_segment(&entry.name), file.mount)
            } else {
                encode_segment(&entry.name)
            };
            let download = if href.contains('?') {
                format!("{href}&download=1")
            } else {
                format!("{href}?download=1")
            };
            let source = if needs_src {
                format!("<span class='src'>{}</span>", escape_html(&file.label))
            } else {
                String::new()
            };
            let preview = if previewable(&entry.name) {
                format!(
                    "<a class='btn ghost' href='{href}' target='_blank' rel='noopener noreferrer'>{}</a>",
                    t("page.preview")
                )
            } else {
                String::new()
            };
            let modified = file.modified.map_or_else(
                || "—".to_string(),
                |time| time.format("%Y-%m-%d %H:%M").to_string(),
            );
            rows.push_str(&format!(
                "<tr data-name='{}' data-href='{download}'><td><div class='name'><span class='ext'>{}</span><span class='fname'>{}</span>{source}</div></td><td class='size'>{}</td><td class='time'>{}</td><td class='act'>{preview}<a class='btn' href='{download}'>{}</a></td></tr>",
                escape_html(&entry.name.to_lowercase()),
                escape_html(&ext_label(&entry.name, false)),
                escape_html(&entry.name),
                human_size(file.size),
                modified,
                t("page.download")
            ));
        }
    }

    let mut crumbs = format!("<a href='/'>{}</a>", t("page.home"));
    let mut walk = String::new();
    for part in url_path.split('/').filter(|part| !part.is_empty()) {
        walk.push('/');
        walk.push_str(&encode_segment(part));
        crumbs.push_str(&format!(
            "<span>/</span><a href='{walk}/'>{}</a>",
            escape_html(part)
        ));
    }

    let upload_button = if uploads {
        format!(
            r#"<button class="btn up-btn" id="key-open" type="button" hidden>{}</button><button class="btn up-btn" id="up-open" type="button" hidden>{}</button>"#,
            escape_html(&t("upload.set_key")),
            escape_html(&t("upload.open"))
        )
    } else {
        String::new()
    };

    let upload_modal = if uploads {
        format!(
            r#"<div class="modal" id="up-modal" hidden>
  <div class="modal-card" role="dialog" aria-modal="true" aria-label="{title}">
    <div class="modal-head">
      <strong>{title}</strong>
      <button class="icon-x" id="up-close" type="button" aria-label="{close}">×</button>
    </div>
    <div class="drop" id="up-drop">
      <div class="drop-copy">
        <strong>{drop}</strong>
        <span class="hint">{hint}</span>
      </div>
      <button class="btn" id="up-pick" type="button">{choose}</button>
      <input id="up-files" type="file" multiple hidden>
    </div>
    <div class="keyrow">
      <div class="keyfield">
        <input id="up-key" type="password" placeholder="{key}" autocomplete="off" spellcheck="false">
        <button class="clear-btn" id="up-clear" type="button" aria-label="{clear}" title="{clear}">×</button>
      </div>
    </div>
    <div class="up-status hint" id="up-status"></div>
    <div class="progress" id="up-progress" hidden><div class="progress-bar" id="up-bar"></div></div>
  </div>
</div>
<div class="drop-overlay" id="up-overlay" hidden><span>{dropping}</span></div>"#,
            title = escape_html(&t("upload.title")),
            close = escape_html(&t("upload.close")),
            hint = escape_html(&t("upload.hint")),
            drop = escape_html(&t("upload.drop")),
            choose = escape_html(&t("upload.choose")),
            key = escape_html(&t("upload.key")),
            clear = escape_html(&t("upload.clear")),
            dropping = escape_html(&t("upload.dropping")),
        )
    } else {
        String::new()
    };

    let upload_strings = format!(
        "const UP_MSG = {{ needKey: {}, failed: {}, done: {}, uploading: {} }};",
        js_string(&t("upload.need_key")),
        js_string(&t("upload.failed")),
        js_string(&t("upload.done")),
        js_string(&t("upload.uploading")),
    );

    let lang_options: String = i18n::LANGUAGES
        .iter()
        .map(|(code, name)| {
            format!(
                "<option value='{}'{}>{}</option>",
                escape_html(code),
                if *code == locale { " selected" } else { "" },
                escape_html(name)
            )
        })
        .collect();

    let empty = if entries.is_empty() {
        format!(
            "<tr><td colspan='4' class='empty'>{}</td></tr>",
            t("page.empty")
        )
    } else {
        String::new()
    };
    let filtered = t("page.filtered").replace('\'', "\\'");
    let items = t1("page.items", entries.len().to_string());
    let total = t1("page.total", human_size(total_size));
    let footer_key = if uploads {
        "page.footer_uploads"
    } else {
        "page.footer"
    };

    format!(
        r#"<!doctype html>
<html lang="{html_lang}">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="dark light">
<title>{title} · LanTransport</title>
<style>{STYLE}{UPLOAD_CSS}</style>
<div class="wrap">
  <div class="top">
    <a class="brand" href="/">
      <svg class="brand-mark" viewBox="0 0 24 24" aria-hidden="true"><path d="M3 6.5A2.5 2.5 0 0 1 5.5 4h4.2l2 2h6.8A2.5 2.5 0 0 1 21 8.5v9A2.5 2.5 0 0 1 18.5 20h-13A2.5 2.5 0 0 1 3 17.5v-11z"/></svg>
      LanTransport
    </a>
    <div class="top-actions">{upload_button}<select id="lang" aria-label="{lang_label}">{lang_options}</select></div>
  </div>
  <h1>{title}</h1>
  <div class="meta">{items} · {total}</div>
  <div class="crumbs">{crumbs}</div>
  <div class="bar"><input type="search" id="q" placeholder="{search}" autocomplete="off"></div>
  <div class="card list-card">
    <table>
      <thead><tr><th>{name}</th><th>{size}</th><th class="time-head">{modified}</th><th></th></tr></thead>
      <tbody id="list">{rows}{empty}</tbody>
    </table>
  </div>
  {upload_modal}
  <div class="foot" id="foot">{footer} · <a class="foot-brand" href="https://github.com/welpher/lantransport" target="_blank" rel="noreferrer noopener">LanTransport</a></div>
</div>
<script nonce="{nonce}">
{upload_strings}
const q = document.getElementById('q');
const rows = Array.from(document.querySelectorAll('#list tr'));
const foot = document.getElementById('foot');
const base = foot.firstChild.textContent;
const template = '{filtered}';
q.addEventListener('input', () => {{
  const term = q.value.trim().toLowerCase();
  let shown = 0;
  rows.forEach(row => {{
    const hit = !term || (row.dataset.name || '').includes(term);
    row.style.display = hit ? '' : 'none';
    if (hit && row.dataset.name !== '..') shown++;
  }});
  foot.firstChild.textContent = term ? template.replace('{{n}}', shown) : base;
}});
q.addEventListener('keydown', event => {{
  if (event.key === 'Escape') {{ q.value = ''; q.dispatchEvent(new Event('input')); }}
}});
document.querySelectorAll('#list tr[data-href]').forEach(row => {{
  row.addEventListener('click', event => {{
    if (event.target.closest('a, button')) return;
    window.location.href = row.getAttribute('data-href');
  }});
}});
const lang = document.getElementById('lang');
if (lang) {{
  if (new URLSearchParams(location.search).get('lang')) {{
    document.cookie = 'lt_lang=' + lang.value + '; Path=/; Max-Age=31536000; SameSite=Lax';
  }}
  lang.addEventListener('change', () => {{
    document.cookie = 'lt_lang=' + lang.value + '; Path=/; Max-Age=31536000; SameSite=Lax';
    const url = new URL(location.href);
    url.searchParams.set('lang', lang.value);
    location.href = url.toString();
  }});
}}
</script>
<script nonce="{nonce}">{UPLOAD_JS}</script>
"#,
        html_lang = escape_html(locale),
        title = escape_html(dir_name),
        lang_label = escape_html(&t("page.language")),
        lang_options = lang_options,
        items = escape_html(&items),
        total = escape_html(&total),
        crumbs = crumbs,
        search = escape_html(&t("page.search")),
        name = escape_html(&t("page.col.name")),
        size = escape_html(&t("page.col.size")),
        modified = escape_html(&t("page.col.modified")),
        rows = rows,
        empty = empty,
        footer = escape_html(&t(footer_key)),
        filtered = filtered,
        nonce = escape_html(nonce),
        upload_modal = upload_modal,
        upload_button = upload_button,
        upload_strings = upload_strings,
        STYLE = STYLE,
        UPLOAD_CSS = UPLOAD_CSS,
        UPLOAD_JS = UPLOAD_JS,
    )
}

fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

pub const STYLE: &str = r#"
:root {
  --bg: #f6f7f9;
  --card: #ffffff;
  --text: #1b1f24;
  --muted: #6a737d;
  --line: #e3e6ea;
  --accent: #2563eb;
  --row-hover: #f2f5fa;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #14171c;
    --card: #1b1f26;
    --text: #e6e9ee;
    --muted: #9aa4b2;
    --line: #2b313a;
    --accent: #6ea8fe;
    --row-hover: #222832;
  }
}
* { box-sizing: border-box; }
[hidden] { display: none !important; }
body {
  margin: 0;
  padding: 24px 16px 48px;
  background: var(--bg);
  color: var(--text);
  font: 15px/1.5 -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
        "Hiragino Sans GB", "Microsoft YaHei", sans-serif;
}
.wrap { max-width: 960px; margin: 0 auto; }
h1 { font-size: 20px; margin: 0 0 4px; font-weight: 600; word-break: break-all; }
.meta { color: var(--muted); font-size: 13px; margin-bottom: 18px; }
.crumbs { font-size: 14px; margin-bottom: 14px; word-break: break-all; }
.crumbs a { color: var(--accent); text-decoration: none; }
.crumbs a:hover { text-decoration: underline; }
.crumbs span { color: var(--muted); margin: 0 6px; }
.bar { display: flex; gap: 10px; align-items: center; margin-bottom: 14px; flex-wrap: wrap; }
input[type=search] {
  flex: 1 1 220px;
  padding: 9px 12px;
  border: 1px solid var(--line);
  border-radius: 9px;
  background: var(--card);
  color: var(--text);
  font-size: 14px;
  outline: none;
}
input[type=search]:focus { border-color: var(--accent); }
.card { background: var(--card); border: 1px solid var(--line); border-radius: 12px; overflow: hidden; }
table { width: 100%; border-collapse: collapse; }
th, td { padding: 11px 14px; text-align: left; border-bottom: 1px solid var(--line); }
th { font-size: 12px; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: .04em; }
tr:last-child td { border-bottom: none; }
tbody tr:hover { background: var(--row-hover); }
#list tr[data-href] { cursor: pointer; }
.name { display: flex; align-items: center; gap: 10px; min-width: 0; }
.name a { color: var(--text); text-decoration: none; overflow-wrap: anywhere; }
.name a:hover { color: var(--accent); }
.fname { color: var(--text); overflow-wrap: anywhere; }
.ext {
  flex: 0 0 auto;
  font: 600 10px/1 ui-monospace, SFMono-Regular, Menlo, monospace;
  letter-spacing: .04em;
  padding: 5px 6px;
  min-width: 42px;
  text-align: center;
  border-radius: 6px;
  color: var(--muted);
  background: var(--row-hover);
  border: 1px solid var(--line);
  text-transform: uppercase;
}
.dir .ext { color: var(--accent); border-color: transparent; background: rgba(37, 99, 235, .12); }
.size, .time { color: var(--muted); font-size: 13px; white-space: nowrap; font-variant-numeric: tabular-nums; }
.act { text-align: right; white-space: nowrap; display: flex; gap: 6px; justify-content: flex-end; align-items: center; }
.btn {
  display: inline-block;
  font-size: 13px;
  padding: 5px 11px;
  border-radius: 7px;
  border: 1px solid var(--line);
  color: var(--text);
  text-decoration: none;
  cursor: pointer;
}
.btn:hover { border-color: var(--accent); color: var(--accent); }
.empty { padding: 40px 16px; text-align: center; color: var(--muted); }
.foot { margin-top: 16px; font-size: 12px; color: var(--muted); text-align: center; }
.brand { color: inherit; text-decoration: none; opacity: .7; }
.src {
  flex: 0 0 auto;
  font-size: 10px;
  color: var(--muted);
  border: 1px solid var(--line);
  border-radius: 999px;
  padding: 2px 7px;
  margin-left: 2px;
}
.brand:hover { opacity: 1; text-decoration: underline; }
@media (max-width: 620px) {
  body { padding: 16px 10px 36px; }
  th, td { padding: 10px; }
  .time, .time-head { display: none; }
  .size { font-size: 12px; }
  .list-card { background: transparent; border: none; box-shadow: none; padding: 0; }
  .list-card table, .list-card tbody, .list-card tr, .list-card td { display: block; }
  .list-card thead { display: none; }
  .list-card tr {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 10px 12px;
    margin-bottom: 8px;
  }
  .list-card td { padding: 0; border: none; }
  .list-card td.size, .list-card td.time { display: inline; font-size: 12px; margin-right: 6px; }
  .list-card td.size::after { content: " ·"; color: var(--muted); }
  .list-card td.act { margin-top: 8px; }
  .list-card td.act {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  .list-card td.act .btn {
    flex: 1 1 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 8px 10px;
  }
  .list-card td.act .btn:last-child {
    border-color: var(--accent);
    color: var(--accent);
    font-weight: 600;
  }
  .list-card .name { flex-wrap: wrap; }
}


/* header, brand, upload dialog */
.top { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 14px; }
.brand {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  color: var(--text);
  text-decoration: none;
  font-size: 15px;
  font-weight: 650;
  letter-spacing: -0.01em;
  opacity: 1;
}
.brand:hover { opacity: 1; text-decoration: none; }
.brand-mark { width: 20px; height: 20px; flex: 0 0 auto; color: var(--accent); }
.top-actions { display: flex; align-items: center; gap: 8px; }
.top-actions select {
  appearance: none;
  border: 1px solid var(--line);
  background: var(--card);
  color: var(--text);
  font-size: 12.5px;
  padding: 6px 10px;
  border-radius: 8px;
  cursor: pointer;
}
.up-btn { display: inline-flex; align-items: center; gap: 6px; }
.modal {
  position: fixed;
  inset: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
  background: rgba(10, 14, 20, 0.45);
}
.modal-card {
  width: 100%;
  max-width: 520px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: 14px;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.28);
}
.modal-head { display: flex; align-items: center; justify-content: space-between; }
.modal-head strong { font-size: 14px; }
.icon-x {
  border: none;
  background: transparent;
  color: var(--muted);
  font-size: 20px;
  line-height: 1;
  padding: 2px 8px;
  border-radius: 7px;
  cursor: pointer;
}
.icon-x:hover { color: var(--text); background: var(--row-hover); }
.drop-overlay {
  position: fixed;
  inset: 12px;
  z-index: 30;
  display: flex;
  align-items: center;
  justify-content: center;
  border: 3px dashed var(--accent);
  border-radius: 18px;
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  color: var(--accent);
  font-size: 18px;
  font-weight: 600;
  pointer-events: none;
}
.foot-brand { color: inherit; text-decoration: none; opacity: .7; }
.foot-brand:hover { opacity: 1; text-decoration: underline; }
"#;

const UPLOAD_CSS: &str = r#"
.upload { gap: 10px; }
.up-head { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; }
.up-head strong { font-size: 13px; color: var(--muted); text-transform: uppercase; letter-spacing: .02em; font-weight: 600; }
.drop {
  display: flex; align-items: center; gap: 12px; flex-wrap: wrap;
  border: 1.5px dashed var(--line); border-radius: 12px;
  padding: 14px 16px; transition: border-color .15s, background .15s;
}
.drop-copy { flex: 1; min-width: 140px; display: flex; flex-direction: column; gap: 2px; }
.drop-copy strong { font-size: 13.5px; color: var(--text); font-weight: 600; }
.progress { height: 4px; background: var(--line); border-radius: 999px; overflow: hidden; }
.progress-bar { width: 0; height: 100%; background: var(--accent); transition: width .2s; }
.drop.over { border-color: var(--accent); background: rgba(37, 99, 235, .06); }
.keyrow { display: flex; gap: 8px; }
.keyfield { position: relative; flex: 1; min-width: 0; }
.keyfield input { width: 100%; padding-right: 34px; }
.clear-btn {
  position: absolute; right: 4px; top: 50%; transform: translateY(-50%);
  width: 26px; height: 26px; border: none; border-radius: 7px;
  background: transparent; color: var(--muted);
  font-size: 17px; line-height: 1; cursor: pointer;
}
.clear-btn:hover { background: var(--row-hover); color: var(--text); }
.keyrow input {
  flex: 1; padding: 9px 12px; border: 1px solid var(--line); border-radius: 9px;
  background: var(--card); color: var(--text); font-size: 13px; outline: none;
}
.keyrow input:focus { border-color: var(--accent); }
.up-status { min-height: 18px; }
"#;

const UPLOAD_JS: &str = r#"
(function () {
  const modal = document.getElementById('up-modal');
  if (!modal) return;
  const openBtn = document.getElementById('up-open');
  const keyBtn = document.getElementById('key-open');
  const clearBtn = document.getElementById('up-clear');
  const closeBtn = document.getElementById('up-close');
  const overlay = document.getElementById('up-overlay');
  const drop = document.getElementById('up-drop');
  const pick = document.getElementById('up-pick');
  const input = document.getElementById('up-files');
  const keyInput = document.getElementById('up-key');
  const status = document.getElementById('up-status');
  const progress = document.getElementById('up-progress');
  const bar = document.getElementById('up-bar');
  const storeKey = 'lantransport-upload-key';
  const params = new URLSearchParams(location.search);
  const fromUrl = params.get('key');
  if (fromUrl) {
    try { localStorage.setItem(storeKey, fromUrl); } catch (e) {}
    history.replaceState(null, '', location.pathname);
  }
  try { keyInput.value = localStorage.getItem(storeKey) || ''; } catch (e) {}
  const remember = () => { try { localStorage.setItem(storeKey, keyInput.value.trim()); } catch (e) {} };
  const syncActions = () => {
    const hasKey = keyInput.value.trim().length > 0;
    if (openBtn) openBtn.hidden = !hasKey;
    if (keyBtn) keyBtn.hidden = hasKey;
  };
  keyInput.addEventListener('input', () => { remember(); syncActions(); });
  clearBtn.addEventListener('click', () => {
    keyInput.value = '';
    try { localStorage.removeItem(storeKey); } catch (e) {}
    syncActions();
    keyInput.focus();
  });
  syncActions();

  const openModal = () => {
    modal.hidden = false;
    if (!keyInput.value.trim()) keyInput.focus();
  };
  const closeModal = () => { modal.hidden = true; };
  if (openBtn) openBtn.addEventListener('click', openModal);
  if (keyBtn) keyBtn.addEventListener('click', openModal);
  closeBtn.addEventListener('click', closeModal);
  modal.addEventListener('click', (event) => { if (event.target === modal) closeModal(); });
  document.addEventListener('keydown', (event) => { if (event.key === 'Escape' && !modal.hidden) closeModal(); });

  pick.addEventListener('click', () => input.click());
  input.addEventListener('change', () => {
    const files = Array.from(input.files || []);
    input.value = '';
    upload(files);
  });

  const hasFiles = (event) => Array.from((event.dataTransfer && event.dataTransfer.types) || []).includes('Files');
  let depth = 0;
  document.addEventListener('dragenter', (event) => {
    if (!hasFiles(event)) return;
    event.preventDefault();
    depth++;
    overlay.hidden = false;
  });
  document.addEventListener('dragover', (event) => {
    if (!hasFiles(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
  });
  document.addEventListener('dragleave', (event) => {
    if (!hasFiles(event)) return;
    depth = Math.max(0, depth - 1);
    if (depth === 0) overlay.hidden = true;
  });
  document.addEventListener('drop', (event) => {
    if (!hasFiles(event)) return;
    event.preventDefault();
    depth = 0;
    overlay.hidden = true;
    const files = Array.from(event.dataTransfer.files || []);
    if (!files.length) return;
    openModal();
    upload(files);
  });

  drop.addEventListener('dragover', (event) => { event.preventDefault(); drop.classList.add('over'); });
  drop.addEventListener('dragleave', () => drop.classList.remove('over'));
  drop.addEventListener('drop', (event) => {
    event.stopPropagation();
    event.preventDefault();
    drop.classList.remove('over');
    const files = Array.from(event.dataTransfer.files || []);
    if (files.length) upload(files);
  });

  function send(file, key) {
    return new Promise((resolve, reject) => {
      const xhr = new XMLHttpRequest();
      const url = '/upload?name=' + encodeURIComponent(file.name);
      xhr.open('PUT', url);
      xhr.setRequestHeader('X-Upload-Key', key);
      xhr.upload.onprogress = event => {
        if (event.lengthComputable) {
          const percent = Math.round(event.loaded / event.total * 100);
          status.textContent = file.name + ' · ' + percent + '%';
          progress.hidden = false;
          bar.style.width = percent + '%';
        }
      };
      xhr.onload = () => {
        if (xhr.status >= 200 && xhr.status < 300) return resolve();
        let message = '';
        try { message = JSON.parse(xhr.responseText).message || ''; } catch (e) {}
        reject(new Error(message || ('HTTP ' + xhr.status)));
      };
      xhr.onerror = () => reject(new Error('network'));
      xhr.send(file);
    });
  }

  async function upload(files) {
    if (!files.length) return;
    const key = keyInput.value.trim();
    if (!key) { status.textContent = UP_MSG.needKey; keyInput.focus(); return; }
    let failed = 0;
    let lastError = '';
    for (let index = 0; index < files.length; index++) {
      status.textContent = UP_MSG.uploading + ' ' + (index + 1) + '/' + files.length;
      try {
        await send(files[index], key);
      } catch (error) {
        failed++;
        lastError = error.message || UP_MSG.failed;
        status.textContent = files[index].name + ' · ' + lastError;
      }
    }
    status.textContent = failed ? lastError : UP_MSG.done;
    progress.hidden = true;
    bar.style.width = '0%';
    if (!failed) setTimeout(() => location.reload(), 600);
  }
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_human() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.5 KB");
    }

    #[test]
    fn escapes_names_in_listing() {
        let entries = vec![Entry {
            name: "<script>.txt".into(),
            dir_mounts: Vec::new(),
            files: vec![FileEntry {
                mount: 0,
                size: 1,
                modified: None,
                label: "share".into(),
            }],
        }];
        let page = render("x", "/", &entries, 1, "en", "abc", false);
        assert!(!page.contains("<script>.txt"));
        assert!(page.contains("&lt;script&gt;.txt"));
    }
}
