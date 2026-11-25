//! HTML directory listing page.

use chrono::{DateTime, Local};

use super::http::{encode_segment, escape_html};
use super::i18n;

pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<DateTime<Local>>,
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
            "<tr class='dir' data-name='..'><td><div class='name'><span class='ext'>DIR</span><a href='{}'>..</a></div></td><td class='size'>—</td><td class='time'>—</td><td class='act'></td></tr>",
            escape_html(&format!("{}/", parent.trim_end_matches('/')))
        ));
    }

    let mut dirs: Vec<&Entry> = entries.iter().filter(|entry| entry.is_dir).collect();
    let mut files: Vec<&Entry> = entries.iter().filter(|entry| !entry.is_dir).collect();
    dirs.sort_by_key(|entry| entry.name.to_lowercase());
    files.sort_by_key(|entry| entry.name.to_lowercase());

    for entry in &dirs {
        let name = escape_html(&entry.name);
        rows.push_str(&format!(
            "<tr class='dir' data-name='{}'><td><div class='name'><span class='ext'>DIR</span><a href='{}/'>{}/</a></div></td><td class='size'>—</td><td class='time'>—</td><td class='act'></td></tr>",
            escape_html(&entry.name.to_lowercase()),
            encode_segment(&entry.name),
            name
        ));
    }

    for entry in &files {
        let href = encode_segment(&entry.name);
        let modified = entry.modified.map_or_else(
            || "—".to_string(),
            |time| time.format("%Y-%m-%d %H:%M").to_string(),
        );
        rows.push_str(&format!(
            "<tr data-name='{}'><td><div class='name'><span class='ext'>{}</span><a href='{}'>{}</a></div></td><td class='size'>{}</td><td class='time'>{}</td><td class='act'><a class='btn' href='{}?download=1'>{}</a></td></tr>",
            escape_html(&entry.name.to_lowercase()),
            escape_html(&ext_label(&entry.name, false)),
            href,
            escape_html(&entry.name),
            human_size(entry.size),
            modified,
            href,
            t("page.download")
        ));
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

    let upload_panel = if uploads {
        format!(
            r#"<div class="card upload" id="up">
  <div class="up-head"><strong>{title}</strong><span class="hint">{hint}</span></div>
  <div class="drop" id="up-drop">
    <button class="btn" id="up-pick" type="button">{choose}</button>
    <input id="up-files" type="file" multiple hidden>
  </div>
  <div class="keyrow">
    <input id="up-key" type="password" placeholder="{key}" autocomplete="off" spellcheck="false">
    <button class="btn ghost" id="up-save" type="button">{save}</button>
  </div>
  <div class="up-status hint" id="up-status"></div>
</div>"#,
            title = escape_html(&t("upload.title")),
            hint = escape_html(&t("upload.hint")),
            choose = escape_html(&t("upload.choose")),
            save = escape_html(&t("upload.save")),
            key = escape_html(&t("upload.key")),
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
    <h1>{title}</h1>
    <select id="lang" aria-label="{lang_label}">{lang_options}</select>
  </div>
  <div class="meta">{items} · {total}</div>
  <div class="crumbs">{crumbs}</div>
  <div class="bar"><input type="search" id="q" placeholder="{search}" autocomplete="off"></div>
  {upload_panel}
  <div class="card">
    <table>
      <thead><tr><th>{name}</th><th>{size}</th><th class="time-head">{modified}</th><th></th></tr></thead>
      <tbody id="list">{rows}{empty}</tbody>
    </table>
  </div>
  <div class="foot" id="foot">{footer} · <a class="brand" href="https://github.com/welpher/lantransport" target="_blank" rel="noreferrer noopener">LanTransport</a></div>
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
        upload_panel = upload_panel,
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
.name { display: flex; align-items: center; gap: 10px; min-width: 0; }
.name a { color: var(--text); text-decoration: none; overflow-wrap: anywhere; }
.name a:hover { color: var(--accent); }
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
.act { text-align: right; white-space: nowrap; }
.btn {
  display: inline-block;
  font-size: 13px;
  padding: 5px 11px;
  border-radius: 7px;
  border: 1px solid var(--line);
  color: var(--text);
  text-decoration: none;
}
.btn:hover { border-color: var(--accent); color: var(--accent); }
.empty { padding: 40px 16px; text-align: center; color: var(--muted); }
.foot { margin-top: 16px; font-size: 12px; color: var(--muted); text-align: center; }
.brand { color: inherit; text-decoration: none; opacity: .7; }
.brand:hover { opacity: 1; text-decoration: underline; }
@media (max-width: 620px) {
  body { padding: 16px 10px 36px; }
  th, td { padding: 10px; }
  .time, .time-head { display: none; }
  .size { font-size: 12px; }
}
"#;

const UPLOAD_CSS: &str = r#"
.upload { gap: 10px; }
.up-head { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; }
.up-head strong { font-size: 13px; color: var(--muted); text-transform: uppercase; letter-spacing: .02em; font-weight: 600; }
.drop {
  display: flex; align-items: center; gap: 12px;
  border: 1.5px dashed var(--line); border-radius: 12px;
  padding: 16px; transition: border-color .15s, background .15s;
}
.drop.over { border-color: var(--accent); background: rgba(37, 99, 235, .06); }
.keyrow { display: flex; gap: 8px; }
.keyrow input {
  flex: 1; padding: 9px 12px; border: 1px solid var(--line); border-radius: 9px;
  background: var(--card); color: var(--text); font-size: 13px; outline: none;
}
.keyrow input:focus { border-color: var(--accent); }
.up-status { min-height: 18px; }
"#;

const UPLOAD_JS: &str = r#"
(function () {
  const box = document.getElementById('up');
  if (!box) return;
  const drop = document.getElementById('up-drop');
  const pick = document.getElementById('up-pick');
  const input = document.getElementById('up-files');
  const keyInput = document.getElementById('up-key');
  const save = document.getElementById('up-save');
  const status = document.getElementById('up-status');
  const dir = location.pathname;
  const storeKey = 'lantransport-upload-key';
  const params = new URLSearchParams(location.search);
  const fromUrl = params.get('key');
  if (fromUrl) {
    try { localStorage.setItem(storeKey, fromUrl); } catch (e) {}
    history.replaceState(null, '', location.pathname);
  }
  try { keyInput.value = localStorage.getItem(storeKey) || ''; } catch (e) {}
  const remember = () => { try { localStorage.setItem(storeKey, keyInput.value.trim()); } catch (e) {} };
  if (save) save.addEventListener('click', remember);
  keyInput.addEventListener('change', remember);
  pick.addEventListener('click', () => input.click());
  input.addEventListener('change', () => { const files = Array.from(input.files || []); input.value = ''; upload(files); });
  ['dragenter', 'dragover'].forEach(name => drop.addEventListener(name, event => { event.preventDefault(); drop.classList.add('over'); }));
  ['dragleave', 'drop'].forEach(name => drop.addEventListener(name, event => { event.preventDefault(); drop.classList.remove('over'); }));
  drop.addEventListener('drop', event => upload(Array.from(event.dataTransfer.files || [])));

  function send(file, key) {
    return new Promise((resolve, reject) => {
      const xhr = new XMLHttpRequest();
      const url = '/upload?dir=' + encodeURIComponent(dir) + '&name=' + encodeURIComponent(file.name);
      xhr.open('PUT', url);
      xhr.setRequestHeader('X-Upload-Key', key);
      xhr.upload.onprogress = event => {
        if (event.lengthComputable) {
          status.textContent = file.name + ' · ' + Math.round(event.loaded / event.total * 100) + '%';
        }
      };
      xhr.onload = () => (xhr.status >= 200 && xhr.status < 300) ? resolve() : reject(new Error(String(xhr.status)));
      xhr.onerror = () => reject(new Error('network'));
      xhr.send(file);
    });
  }

  async function upload(files) {
    if (!files.length) return;
    const key = keyInput.value.trim();
    if (!key) { status.textContent = UP_MSG.needKey; keyInput.focus(); return; }
    remember();
    let failed = 0;
    for (let index = 0; index < files.length; index++) {
      status.textContent = UP_MSG.uploading + ' ' + (index + 1) + '/' + files.length;
      try { await send(files[index], key); } catch (error) { failed++; }
    }
    status.textContent = failed ? UP_MSG.failed : UP_MSG.done;
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
            is_dir: false,
            size: 1,
            modified: None,
        }];
        let page = render("x", "/", &entries, 1, "en", "abc", false);
        assert!(!page.contains("<script>.txt"));
        assert!(page.contains("&lt;script&gt;.txt"));
    }
}
