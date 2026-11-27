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
            // SVG is served as a download because browsers can run script in
            // it, so an inline "preview" would just download the file.
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
        // Build the parent href from encoded segments, like every other link:
        // the incoming url_path is already decoded, so names with '#' or '%'
        // would otherwise produce a link the browser mis-parses.
        let mut parent = String::new();
        let parts: Vec<&str> = url_path
            .trim_end_matches('/')
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            parent.push('/');
            parent.push_str(&encode_segment(part));
        }
        parent.push('/');
        rows.push_str(&format!(
            "<tr class='dir' data-name='..' data-href='{parent}'><td><div class='name'><span class='ext'>DIR</span><a href='{parent}'>..</a></div></td><td class='size'>—</td><td class='time'>—</td><td class='act'></td></tr>",
            parent = escape_html(&parent)
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
            "<tr class='dir' data-kind='dir' data-name='{}' data-href='{}/'><td><div class='name'><span class='ext'>DIR</span><a href='{}/'>{}/</a></div></td><td class='size'>—</td><td class='time'>—</td><td class='act'><a class='btn' href='{}/?zip=1' download title='{zip_hint}'>{download}</a></td></tr>",
            escape_html(&entry.name.to_lowercase()),
            encode_segment(&entry.name),
            encode_segment(&entry.name),
            name,
            encode_segment(&entry.name),
            zip_hint = t("page.zip_hint"),
            download = t("page.download")
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
            let epoch = file.modified.map(|time| time.timestamp()).unwrap_or(0);
            rows.push_str(&format!(
                "<tr data-kind='file' data-name='{}' data-size='{}' data-time='{}' data-href='{download}'><td><div class='name'><span class='ext'>{}</span><span class='fname'>{}</span>{source}</div></td><td class='size'>{}</td><td class='time'>{}</td><td class='act'>{preview}<a class='btn' href='{download}'>{}</a></td></tr>",
                escape_html(&entry.name.to_lowercase()),
                file.size,
                epoch,
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

    let drag_hint = if uploads {
        format!(
            "<p class=\"drag-hint\"><svg viewBox=\"0 0 24 24\" width=\"15\" height=\"15\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"M12 16V4\"/><path d=\"m7.5 8.5 4.5-4.5 4.5 4.5\"/><path d=\"M4 16v3a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-3\"/></svg>{}</p>",
            escape_html(&t("upload.drag_hint"))
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
    <div class="keyrow">
      <div class="keyfield">
        <input id="up-key" type="password" placeholder="{key}" autocomplete="off" spellcheck="false">
        <button class="clear-btn" id="up-clear" type="button" aria-label="{clear}" title="{clear}" hidden>×</button>
      </div>
    </div>
    <div class="up-area" id="up-area" hidden>
      <div class="drop" id="up-drop">
        <span class="drop-icon" aria-hidden="true"><svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h3.4l2 2h7.6A2.5 2.5 0 0 1 21 9.5v8A2.5 2.5 0 0 1 18.5 20h-13A2.5 2.5 0 0 1 3 17.5v-10z"/><path d="M12 11v5"/><path d="M9.8 13.8 12 16l2.2-2.2"/></svg></span>
        <div class="drop-copy">
          <strong>{drop}</strong>
          <span class="hint">{hint}</span>
        </div>
        <button class="btn" id="up-pick" type="button">{choose}</button>
        <input id="up-files" type="file" multiple hidden>
      </div>
      <div class="up-status hint" id="up-status"></div>
      <div class="progress" id="up-progress" hidden><div class="progress-bar" id="up-bar"></div></div>
      <button class="btn ghost" id="up-retry" type="button" hidden>{retry}</button>
    </div>
  </div>
</div>
<div class="drop-overlay" id="up-overlay" hidden><span>{dropping}</span></div>"#,
            title = escape_html(&t("upload.title")),
            close = escape_html(&t("upload.close")),
            hint = escape_html(&t("upload.hint")),
            drop = escape_html(&t("upload.drop")),
            choose = escape_html(&t("upload.choose")),
            key = escape_html(&t("upload.key")),
            retry = escape_html(&t("upload.retry")),
            clear = escape_html(&t("upload.clear")),
            dropping = escape_html(&t("upload.dropping")),
        )
    } else {
        String::new()
    };

    let select_strings = format!(
        "const SELECT_MSG = {{ selected: {}, all: {} }};",
        js_string(&t("page.selected")),
        js_string(&t("page.select_all")),
    );

    let upload_strings = format!(
        "const UP_MSG = {{ needKey: {}, failed: {}, done: {}, uploading: {}, retrying: {} }};\nconst UP_RETRIES = 3;",
        js_string(&t("upload.need_key")),
        js_string(&t("upload.failed")),
        js_string(&t("upload.done")),
        js_string(&t("upload.uploading")),
        js_string(&t("upload.retrying")),
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
<link rel="icon" href="/icon.png" type="image/png">
<link rel="manifest" href="/manifest.webmanifest">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
<meta name="apple-mobile-web-app-capable" content="yes">
<meta name="apple-mobile-web-app-title" content="LanTransport">
<meta name="apple-mobile-web-app-status-bar-style" content="default">
<meta name='theme-color' content='#f6f7f9' media='(prefers-color-scheme: light)'>
<meta name='theme-color' content='#14171c' media='(prefers-color-scheme: dark)'>
<title>{title} · LanTransport</title>
<style>{STYLE}{UPLOAD_CSS}</style>
<div class="topline" id="topline"></div>
<div class="top">
  <div class="top-inner">
    <a class="brand" href="/">
      <img class="brand-mark" src="/icon.png" alt="" width="40" height="40" draggable="false">
      LanTransport
    </a>
    <div class="top-actions">{upload_button}<select id="lang" aria-label="{lang_label}">{lang_options}</select></div>
  </div>
</div>
<div class="wrap">
  <h1>{title}</h1>
  <div class="meta" id="meta">{items} · {total}</div>
  <div class="crumbs">{crumbs}</div>
  <div class="bar">
    <input type="search" id="q" placeholder="{search}" autocomplete="off">
    <select id="sort-select" class="sort-select" aria-label="{sort_label}">
      <option value="name:asc">{name} ↑</option>
      <option value="name:desc">{name} ↓</option>
      <option value="size:asc">{size} ↑</option>
      <option value="size:desc">{size} ↓</option>
      <option value="time:asc">{modified} ↑</option>
      <option value="time:desc">{modified} ↓</option>
    </select>
    <button class="btn" id="select-mode" type="button">{select_label}</button>
    <button class="btn icon-btn" id="refresh" type="button" title="{refresh_label}" aria-label="{refresh_label}">&#8635;</button>
  </div>
  <div class="selectbar" id="selectbar" hidden>
    <label class="pick-all"><input type="checkbox" id="pick-all"><span>{select_all}</span></label>
    <span class="hint" id="select-info"></span>
    <button class="btn" id="select-download" type="button">{download_label}</button>
    <button class="btn ghost" id="select-cancel" type="button">{cancel_label}</button>
  </div>
  {drag_hint}
  <div class="card list-card">
    <table>
      <thead><tr><th><button class="sort" type="button" data-sort="name">{name}</button></th><th><button class="sort" type="button" data-sort="size">{size}</button></th><th class="time-head"><button class="sort" type="button" data-sort="time">{modified}</button></th><th></th></tr></thead>
      <tbody id="list">{rows}{empty}</tbody>
    </table>
  </div>
  {upload_modal}
  <div class="foot" id="foot">{footer} · <a class="foot-brand" href="https://github.com/welpher/lantransport" target="_blank" rel="noreferrer noopener">LanTransport</a></div>
</div>
<script nonce="{nonce}">
{select_strings}
{upload_strings}{PAGE_JS}
</script>
<script nonce="{nonce}">{UPLOAD_JS}</script>
"#,
        html_lang = escape_html(locale),
        title = escape_html(dir_name),
        lang_label = escape_html(&t("page.language")),
        refresh_label = escape_html(&t("page.refresh")),
        lang_options = lang_options,
        items = escape_html(&items),
        total = escape_html(&total),
        crumbs = crumbs,
        search = escape_html(&t("page.search")),
        sort_label = escape_html(&t("page.sort")),
        name = escape_html(&t("page.col.name")),
        size = escape_html(&t("page.col.size")),
        modified = escape_html(&t("page.col.modified")),
        rows = rows,
        empty = empty,
        footer = escape_html(&t(footer_key)),
        PAGE_JS = page_js(&filtered),
        nonce = escape_html(nonce),
        upload_modal = upload_modal,
        upload_button = upload_button,
        drag_hint = drag_hint,
        select_label = escape_html(&t("page.select")),
        select_all = escape_html(&t("page.select_all")),
        download_label = escape_html(&t("page.download")),
        cancel_label = escape_html(&t("page.cancel")),
        select_strings = select_strings,
        upload_strings = upload_strings,
        STYLE = STYLE,
        UPLOAD_CSS = UPLOAD_CSS,
        UPLOAD_JS = UPLOAD_JS,
    )
}

fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

/// Client behaviour for the listing: filter, sort, row clicks, language and the
/// in-page refresh. Kept out of the `format!` string so braces stay readable;
/// `__FILTERED__` is the already-escaped "showing {n} files" template.
const PAGE_JS: &str = r#"
const q = document.getElementById('q');
const list = document.getElementById('list');
const meta = document.getElementById('meta');
const foot = document.getElementById('foot');
const sortSelect = document.getElementById('sort-select');
const base = foot.firstChild.textContent;
const template = '__FILTERED__';

// Remember the sort choice the same way the language is remembered.
const readCookie = (name) => {
  const hit = document.cookie.split('; ').find(part => part.startsWith(name + '='));
  return hit ? decodeURIComponent(hit.slice(name.length + 1)) : '';
};
const savedSort = readCookie('lt_sort');
const initialSort = /^(name|size|time):(asc|desc)$/.test(savedSort) ? savedSort : 'name:asc';
const state = { key: initialSort.split(':')[0], dir: initialSort.split(':')[1] };
let rows = [];
let groups = { parent: [], dir: [], file: [] };

function applyFilter() {
  const term = q.value.trim().toLowerCase();
  let shown = 0;
  rows.forEach(row => {
    const hit = !term || (row.dataset.name || '').includes(term);
    row.style.display = hit ? '' : 'none';
    if (hit && row.dataset.name !== '..') shown++;
  });
  foot.firstChild.textContent = term ? template.replace('{n}', shown) : base;
}

function compare(a, b) {
  const sign = state.dir === 'desc' ? -1 : 1;
  if (state.key === 'size' || state.key === 'time') {
    const left = Number(a.dataset[state.key]);
    const right = Number(b.dataset[state.key]);
    if (Number.isFinite(left) && Number.isFinite(right)) return (left - right) * sign;
  }
  return a.dataset.name.localeCompare(b.dataset.name, undefined, { numeric: true }) * sign;
}

function applySort(key, dir, remember) {
  state.key = key;
  state.dir = dir;
  if (remember !== false) {
    document.cookie = 'lt_sort=' + key + ':' + dir + '; Path=/; Max-Age=31536000; SameSite=Lax';
  }
  list.replaceChildren(...groups.parent, ...groups.dir.slice().sort(compare), ...groups.file.slice().sort(compare));
  document.querySelectorAll('th .sort').forEach(button => {
    const active = button.dataset.sort === key;
    button.classList.toggle('active', active);
    button.dataset.dir = active ? dir : '';
  });
  if (sortSelect) sortSelect.value = key + ':' + dir;
}

// First rows slide in; a long folder only animates the first screenful.
function staggerRows() {
  const shown = rows.filter(row => row.offsetParent !== null).slice(0, 20);
  shown.forEach((row, index) => {
    row.style.animationDelay = (index * 12) + 'ms';
    row.classList.add('row-in');
  });
}

function setBusy(on) {
  const line = document.getElementById('topline');
  if (!line) return;
  if (on) {
    line.hidden = false;
    line.classList.remove('done');
    line.classList.toggle('run', true);
    return;
  }
  line.classList.add('run');
  line.classList.add('done');
  window.setTimeout(() => {
    line.hidden = true;
    line.classList.remove('run', 'done');
  }, 400);
}

function bindRows() {
  rows = Array.from(list.querySelectorAll('tr'));
  groups = { parent: [], dir: [], file: [] };
  rows.forEach(row => {
    const kind = row.dataset.kind || 'file';
    (groups[kind] || groups.file).push(row);
  });
  list.querySelectorAll('tr[data-href]').forEach(row => {
    row.addEventListener('click', event => {
      if (event.target.closest('a, button, input')) return;
      if (selecting) {
        if (row.dataset.kind !== 'file') return;
        togglePick(row.dataset.name);
        return;
      }
      window.location.href = row.getAttribute('data-href');
    });
  });
  if (selecting) attachPicks();
  applySort(state.key, state.dir, false);
  applyFilter();
  staggerRows();
}

q.addEventListener('input', applyFilter);
q.addEventListener('keydown', event => {
  if (event.key === 'Escape') { q.value = ''; applyFilter(); }
  if (event.key === 'Enter') {
    const term = q.value.trim();
    if (!term) return;
    let dirParam = location.pathname;
    try { dirParam = decodeURIComponent(location.pathname); } catch (e) {}
    location.href = dirParam + '?find=' + encodeURIComponent(term);
  }
});
document.querySelectorAll('th .sort').forEach(button => {
  button.addEventListener('click', () => {
    const key = button.dataset.sort;
    const dir = state.key === key ? (state.dir === 'asc' ? 'desc' : 'asc') : (key === 'name' ? 'asc' : 'desc');
    applySort(key, dir);
  });
});
if (sortSelect) {
  sortSelect.value = initialSort;
  sortSelect.addEventListener('change', () => {
    const [key, dir] = sortSelect.value.split(':');
    applySort(key, dir);
  });
}

const refresh = document.getElementById('refresh');

// Re-reading the list in place keeps the scroll position and the search term;
// the upload flow calls this too, so it must live at the top level rather than
// inside the button's block.
async function refreshListing() {
  if (!refresh || refresh.classList.contains('busy')) return;
  refresh.classList.add('busy');
  setBusy(true);
  try {
    const response = await fetch(location.href, { cache: 'no-store' });
    if (!response.ok) throw new Error('status ' + response.status);
    const doc = new DOMParser().parseFromString(await response.text(), 'text/html');
    const nextList = doc.getElementById('list');
    const nextMeta = doc.getElementById('meta');
    if (!nextList || !nextMeta) throw new Error('missing list');
    list.replaceChildren(...Array.from(nextList.childNodes));
    meta.textContent = nextMeta.textContent;
    bindRows();
  } catch (error) {
    location.reload();
    return;
  } finally {
    refresh.classList.remove('busy');
    setBusy(false);
  }
}

if (refresh) refresh.addEventListener('click', () => { void refreshListing(); });

const selectModeBtn = document.getElementById('select-mode');
const selectBar = document.getElementById('selectbar');
const selectInfo = document.getElementById('select-info');
const selectAllBox = document.getElementById('pick-all');
const selectDownload = document.getElementById('select-download');
const selectCancel = document.getElementById('select-cancel');
let selecting = false;
const picked = new Set();

function humanBytes(bytes) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = Number(bytes) || 0;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++; }
  return (unit === 0 ? Math.round(value) : value.toFixed(1)) + ' ' + units[unit];
}

function fileRows() {
  return rows.filter(row => row.dataset.kind === 'file');
}

function refreshPicked() {
  let bytes = 0;
  picked.forEach(name => {
    const row = rows.find(item => item.dataset.kind === 'file' && item.dataset.name === name);
    if (row) bytes += Number(row.dataset.size) || 0;
  });
  selectInfo.textContent = SELECT_MSG.selected
    .replace('{n}', String(picked.size))
    .replace('{size}', humanBytes(bytes));
  selectDownload.disabled = picked.size === 0;
  const files = fileRows();
  selectAllBox.checked = files.length > 0 && files.every(row => picked.has(row.dataset.name));
}

function togglePick(name) {
  if (picked.has(name)) picked.delete(name);
  else picked.add(name);
  fileRows().forEach(row => {
    if (row.dataset.name !== name) return;
    const box = row.querySelector('.row-pick');
    if (box) box.checked = picked.has(name);
  });
  refreshPicked();
}

function attachPicks() {
  fileRows().forEach(row => {
    if (row.querySelector('.row-pick')) return;
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.className = 'row-pick';
    box.checked = picked.has(row.dataset.name);
    box.addEventListener('click', event => event.stopPropagation());
    box.addEventListener('change', () => togglePick(row.dataset.name));
    const cell = row.querySelector('.name');
    if (cell) cell.prepend(box);
  });
}

function leaveSelect() {
  selecting = false;
  picked.clear();
  selectBar.hidden = true;
  selectModeBtn.hidden = false;
  document.querySelectorAll('.row-pick').forEach(box => box.remove());
}

if (selectModeBtn) {
  selectModeBtn.addEventListener('click', () => {
    selecting = true;
    picked.clear();
    selectBar.hidden = false;
    selectModeBtn.hidden = true;
    attachPicks();
    refreshPicked();
  });
  selectCancel.addEventListener('click', leaveSelect);
  selectAllBox.addEventListener('change', () => {
    picked.clear();
    if (selectAllBox.checked) fileRows().forEach(row => picked.add(row.dataset.name));
    attachPicks();
    fileRows().forEach(row => {
      const box = row.querySelector('.row-pick');
      if (box) box.checked = picked.has(row.dataset.name);
    });
    refreshPicked();
  });
  selectDownload.addEventListener('click', () => {
    if (!picked.size) return;
    const names = Array.from(picked).join(',');
    location.href = location.pathname + '?zip=1&files=' + encodeURIComponent(names);
  });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape' && selecting) leaveSelect();
  });
}

const lang = document.getElementById('lang');
if (lang) {
  if (new URLSearchParams(location.search).get('lang')) {
    document.cookie = 'lt_lang=' + lang.value + '; Path=/; Max-Age=31536000; SameSite=Lax';
  }
  lang.addEventListener('change', () => {
    document.cookie = 'lt_lang=' + lang.value + '; Path=/; Max-Age=31536000; SameSite=Lax';
    const url = new URL(location.href);
    url.searchParams.set('lang', lang.value);
    location.href = url.toString();
  });
}

bindRows();
"#;

fn page_js(filtered: &str) -> String {
    PAGE_JS.replace("__FILTERED__", filtered)
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
  --ok: #16a34a;
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
    --ok: #3fb950;
  }
}
* { box-sizing: border-box; }
[hidden] { display: none !important; }
/* Hide the scrollbar itself: reserving a slot always left a slim strip on the
   right, and letting the scrollbar appear would shift the layout sideways.
   With it hidden nothing is reserved and nothing can appear later, so the
   listing never jumps. Wheel, trackpad, keyboard and touch still scroll. */
html {
  overflow-y: scroll;
  scrollbar-width: none;
}
::-webkit-scrollbar { width: 0; height: 0; }
body {
  margin: 0;
  padding: 0;
  background: var(--bg);
  color: var(--text);
  font: 15px/1.5 -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
        "Hiragino Sans GB", "Microsoft YaHei", sans-serif;
}
.wrap { max-width: 992px; margin: 0 auto; padding: 0 16px 48px; }
h1 { font-size: 19px; margin: 0 0 4px; font-weight: 600; letter-spacing: -0.01em; word-break: break-all; }
.meta { color: var(--muted); font-size: 13px; margin-bottom: 18px; }
.crumbs { font-size: 14px; margin-bottom: 14px; word-break: break-all; }
.crumbs a { color: var(--accent); text-decoration: none; }
.crumbs a:hover { text-decoration: underline; }
.crumbs span { color: var(--muted); margin: 0 6px; }
.bar { display: flex; gap: 10px; align-items: center; margin-bottom: 14px; flex-wrap: wrap; }
input[type=search] {
  flex: 1 1 220px;
  min-width: 0;
  height: 32px;
  padding: 0 12px;
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
th .sort { background: none; border: none; padding: 0; margin: 0; font: inherit; color: inherit; text-transform: inherit; letter-spacing: inherit; cursor: pointer; user-select: none; }
th .sort:hover { color: var(--accent); }
th .sort.active { color: var(--accent); }
th .sort.active::after { content: " ↑"; }
th .sort.active[data-dir="desc"]::after { content: " ↓"; }
.sort-select { display: none; }
.selectbar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  margin-bottom: 14px;
}
.selectbar .hint { color: var(--muted); font-size: 13px; }
.pick-all { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; color: var(--muted); }
.pick-all input { width: 16px; height: 16px; accent-color: var(--accent); }
.row-pick { width: 16px; height: 16px; margin-right: 10px; flex: 0 0 auto; accent-color: var(--accent); }
#select-download[disabled] { opacity: .5; cursor: default; }
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
/* Keep this a table cell: display:flex would drop vertical-align and the
   buttons would hug the top of a two-line name. */
.act { text-align: right; white-space: nowrap; }
.act .btn + .btn { margin-left: 6px; }
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
  .wrap { padding: 0 10px 36px; }
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
  .sort-select {
    display: block;
    padding: 9px 10px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--card);
    color: var(--text);
    font-size: 16px;
  }
  input[type="search"] {
    font-size: 16px;
  }
}


/* header, brand, upload dialog */
/* Full-bleed sticky bar: the border spans the whole viewport, the content
   inside stays aligned with .wrap. */
.top {
  position: sticky;
  top: 0;
  z-index: 10;
  background: var(--bg);
  margin-bottom: 16px;
  border-bottom: 1px solid var(--line);
}
.top-inner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  max-width: 992px;
  margin: 0 auto;
  padding: 16px 16px 14px;
}
.brand {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  color: var(--text);
  text-decoration: none;
  font-size: 16px;
  font-weight: 650;
  letter-spacing: -0.01em;
  opacity: 1;
}
.brand:hover { opacity: .85; text-decoration: none; }
.brand-mark { width: 40px; height: 40px; flex: 0 0 auto; border-radius: 10px; }
@media (max-width: 620px) {
  .top { margin-bottom: 14px; }
  .top-inner { padding: 14px 10px 12px; }
  .brand { font-size: 15px; }
  .brand-mark { width: 32px; height: 32px; border-radius: 8px; }
  h1 { font-size: 17.5px; }
}
.top-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; justify-content: flex-end; }
.icon-btn { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 32px; padding: 0; font-size: 16px; line-height: 1; }
.icon-btn.busy { animation: spin .9s linear infinite; pointer-events: none; opacity: .6; }
@keyframes spin { to { transform: rotate(360deg); } }
.top-actions select,
.sort-select {
  appearance: none;
  border: 1px solid var(--line);
  background: var(--card);
  color: var(--text);
  font-size: 12.5px;
  height: 32px;
  padding: 0 10px;
  border-radius: 8px;
  cursor: pointer;
}
select:focus,
select:focus-visible {
  outline: none;
  box-shadow: none;
}
select:focus-visible { border-color: var(--accent); }
.up-btn { display: inline-flex; align-items: center; gap: 6px; }
/* Every control in the top bar and the toolbar is the same height. */
.top-actions .btn,
.bar .btn,
.top-actions select { height: 32px; padding: 0 11px; }
.top-actions select { padding: 0 10px; }
.modal {
  position: fixed;
  inset: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
  background: rgba(10, 14, 20, 0.45);
}
.modal-card {
  width: 100%;
  max-width: 420px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: 14px;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.28);
}
.up-area { display: flex; flex-direction: column; gap: 12px; }
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
  inset: 0;
  z-index: 40;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in srgb, var(--bg) 78%, transparent);
  backdrop-filter: blur(2px);
  pointer-events: none;
}
.drop-overlay span {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 14px 22px;
  border: 2px dashed var(--accent);
  border-radius: 14px;
  background: var(--card);
  box-shadow: 0 10px 30px rgba(16, 24, 40, .18);
  color: var(--accent);
  font-size: 16px;
  font-weight: 600;
}
.foot-brand { color: inherit; text-decoration: none; opacity: .7; }
.foot-brand:hover { opacity: 1; text-decoration: underline; }

/* iOS Safari zooms the page when a focused field is under 16px */
@media (max-width: 620px) {
  .top-actions select,
  .sort-select { font-size: 16px; height: 36px; }
  input[type="search"] { font-size: 16px; height: 36px; flex: 1 1 140px; }
  .top-actions .btn,
  .bar .btn { height: 36px; }
  .icon-btn { width: 36px; height: 36px; }
}

/* motion: short, purposeful, and off for anyone who asked for less */
@keyframes row-in { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: none; } }
@keyframes pop { 0% { transform: scale(1); } 45% { transform: scale(1.06); } 100% { transform: scale(1); } }
.row-in { animation: row-in .22s ease-out both; }
.btn.pop { animation: pop .35s ease-out; }
.topline {
  position: fixed;
  top: 0;
  left: 0;
  height: 2px;
  width: 0;
  z-index: 60;
  background: var(--accent);
  transition: width .35s ease-out, opacity .3s ease-out;
}
.topline.run { width: 78%; }
.topline.done { width: 100%; opacity: 0; }
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.001ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.001ms !important;
  }
}
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
.progress {
  position: relative;
  height: 6px;
  border-radius: 999px;
  background: var(--line);
  overflow: hidden;
  transition: box-shadow .3s ease;
}
.progress-bar {
  width: 0;
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, var(--accent), color-mix(in srgb, var(--accent) 62%, #ffffff));
  transition: width .2s linear, background-color .3s ease;
  position: relative;
  overflow: hidden;
}
/* a sheen that keeps moving while bytes are flowing */
.progress:not(.done) .progress-bar::after {
  content: "";
  position: absolute;
  inset: 0;
  background: linear-gradient(90deg, transparent, rgba(255, 255, 255, .45), transparent);
  width: 40%;
  animation: sheen 1.1s linear infinite;
}
@keyframes sheen { from { transform: translateX(-100%); } to { transform: translateX(350%); } }
.progress.done { box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 18%, transparent); }
.progress.done .progress-bar {
  background: var(--ok);
  animation: none;
}
.progress.done .progress-bar::after { display: none; }
.up-status.done { color: var(--ok); font-weight: 600; }
.drag-hint {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: -6px 0 12px;
  color: var(--muted);
  font-size: 12.5px;
}
.drop-icon { display: flex; flex: 0 0 auto; color: var(--accent); transition: transform .15s ease; }
.drop.over { border-color: var(--accent); background: rgba(37, 99, 235, .06); }
.drop.over .drop-icon { transform: translateY(-2px) scale(1.06); }
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

@media (max-width: 620px) {
  .keyrow input { font-size: 16px; }
}
"#;

const UPLOAD_JS: &str = r#"
(function () {
  const modal = document.getElementById('up-modal');
  if (!modal) return;
  const openBtn = document.getElementById('up-open');
  const keyBtn = document.getElementById('key-open');
  const clearBtn = document.getElementById('up-clear');
  const area = document.getElementById('up-area');
  const closeBtn = document.getElementById('up-close');
  const overlay = document.getElementById('up-overlay');
  const drop = document.getElementById('up-drop');
  const pick = document.getElementById('up-pick');
  const input = document.getElementById('up-files');
  const keyInput = document.getElementById('up-key');
  const status = document.getElementById('up-status');
  const retryBtn = document.getElementById('up-retry');
  let retryFiles = [];
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
    if (clearBtn) clearBtn.hidden = !hasKey;
    if (area) area.hidden = !hasKey;
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
    // The dialog has its own drop zone; dismiss the page-wide banner if a drag
    // opened it a moment earlier.
    overlay.hidden = true;
    depth = 0;
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
  // The dialog has its own drop zone; a second "drop to upload" banner on top of
  // it just collides with the dialog's text.
  const modalOpen = () => !modal.hidden;
  document.addEventListener('dragenter', (event) => {
    if (!hasFiles(event)) return;
    event.preventDefault();
    depth++;
    if (!modalOpen()) overlay.hidden = false;
  });
  document.addEventListener('dragover', (event) => {
    if (!hasFiles(event)) return;
    event.preventDefault();
    if (overlay.hidden && !modalOpen()) overlay.hidden = false;
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

  function humanSpeed(bytesPerSecond) {
    if (!isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '—';
    const units = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
    let value = bytesPerSecond;
    let unit = 0;
    while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++; }
    return (unit === 0 ? Math.round(value) : value.toFixed(1)) + ' ' + units[unit];
  }

  function formatEta(seconds) {
    if (!isFinite(seconds) || seconds <= 0) return '—';
    if (seconds < 1) return '<1s';
    if (seconds < 60) return Math.round(seconds) + 's';
    const minutes = Math.floor(seconds / 60);
    return minutes + 'm ' + Math.round(seconds % 60) + 's';
  }

  function uploadUrl(file) {
    // location.pathname is already percent-encoded; encode it once more only
    // after decoding, or a folder called "a b" arrives as "a%20b" and fails.
    let dirParam = location.pathname;
    try { dirParam = decodeURIComponent(location.pathname); } catch (e) {}
    return '/upload?dir=' + encodeURIComponent(dirParam) + '&name=' + encodeURIComponent(file.name);
  }

  // Ask how much of this file already reached the folder; a dropped connection
  // then continues from there instead of sending everything again.
  async function bytesAlreadyThere(url, key) {
    try {
      const response = await fetch(url, { method: 'HEAD', headers: { 'X-Upload-Key': key }, cache: 'no-store' });
      if (!response.ok) return 0;
      const value = Number(response.headers.get('X-Upload-Offset'));
      return Number.isFinite(value) && value > 0 ? value : 0;
    } catch (error) {
      return 0;
    }
  }

  async function send(file, key, onProgress) {
    const url = uploadUrl(file);
    const offset = Math.min(await bytesAlreadyThere(url, key), file.size);
    if (file.size > 0 && offset >= file.size) return;
    return new Promise((resolve, reject) => {
      const xhr = new XMLHttpRequest();
      xhr.open('PUT', url);
      xhr.setRequestHeader('X-Upload-Key', key);
      xhr.setRequestHeader('X-Upload-Total', String(file.size));
      if (offset > 0) xhr.setRequestHeader('X-Upload-Offset', String(offset));
      xhr.upload.onprogress = event => {
        if (event.lengthComputable) onProgress(offset + event.loaded);
      };
      xhr.onload = () => {
        if (xhr.status >= 200 && xhr.status < 300) return resolve();
        let message = '';
        try { message = JSON.parse(xhr.responseText).message || ''; } catch (e) {}
        reject(new Error(message || ('HTTP ' + xhr.status)));
      };
      xhr.onerror = () => reject(new Error('network'));
      xhr.send(offset > 0 ? file.slice(offset) : file);
    });
  }

  async function upload(files) {
    if (!files.length) return;
    setBusy(true);
    const key = keyInput.value.trim();
    if (!key) { status.textContent = UP_MSG.needKey; keyInput.focus(); return; }
    retryFiles = [];
    retryBtn.hidden = true;
    const total = files.reduce((sum, file) => sum + file.size, 0);
    let sent = 0;
    let failed = 0;
    let lastError = '';
    const startedAt = performance.now();
    const report = (index, loaded) => {
      const current = sent + loaded;
      const percent = total > 0 ? Math.min(100, Math.round(current / total * 100)) : 100;
      const elapsed = Math.max(0.2, (performance.now() - startedAt) / 1000);
      const speed = current / elapsed;
      const eta = speed > 0 && total > current ? (total - current) / speed : 0;
      status.textContent = (index + 1) + '/' + files.length + ' · ' + percent + '% · ' + humanSpeed(speed) + ' · ' + formatEta(eta);
      progress.hidden = false;
      bar.style.width = percent + '%';
    };
    const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
    for (let index = 0; index < files.length; index++) {
      report(index, 0);
      let attempt = 0;
      while (true) {
        try {
          await send(files[index], key, loaded => report(index, loaded));
          sent += files[index].size;
          break;
        } catch (error) {
          // The next attempt asks the server how much arrived and continues
          // from there, so a flaky Wi-Fi blip usually repairs itself.
          attempt++;
          if (attempt <= UP_RETRIES) {
            status.textContent = UP_MSG.retrying
              .replace('{n}', String(attempt))
              .replace('{total}', String(UP_RETRIES));
            await sleep(attempt === 1 ? 1000 : 3000);
            continue;
          }
          failed++;
          lastError = error.message || UP_MSG.failed;
          retryFiles.push(files[index]);
          status.textContent = files[index].name + ' · ' + lastError;
          break;
        }
      }
    }
    if (failed) {
      progress.hidden = true;
      bar.style.width = '0%';
      status.textContent = failed + ' / ' + files.length + ' · ' + lastError;
      retryBtn.hidden = false;
      progress.classList.remove('done');
      status.classList.remove('done');
    } else {
      bar.style.width = '100%';
      progress.classList.add('done');
      status.classList.add('done');
      status.textContent = '\u2713 ' + UP_MSG.done;
      setTimeout(() => {
        progress.classList.remove('done');
        status.classList.remove('done');
        progress.hidden = true;
        bar.style.width = '0%';
      }, 1600);
      // In-place refresh: the new file appears without a page flash, and the
      // tick above stays on screen long enough to be seen.
      setTimeout(() => { void refreshListing(); }, 800);
    }
  }

  retryBtn.addEventListener('click', () => {
    if (retryFiles.length) void upload(retryFiles);
  });
})();
"#;

/// One match from a recursive search, with the path relative to the folder the
/// visitor is standing in.
pub struct Found {
    pub path: String,
    pub size: u64,
    pub modified: Option<String>,
    pub href: String,
}

const FIND_DEPTH: usize = 8;
const FIND_VISITED: usize = 20_000;
const FIND_MATCHES: usize = 500;

pub fn collect_matches(roots: &[std::path::PathBuf], term: &str) -> Vec<Found> {
    let needle = term.to_lowercase();
    let mut found = Vec::new();
    let mut visited = 0usize;
    for (index, root) in roots.iter().enumerate() {
        walk_find(root, "", 0, &needle, index, &mut found, &mut visited);
        if found.len() >= FIND_MATCHES {
            break;
        }
    }
    found.sort_by_key(|item| item.path.to_lowercase());
    found
}

fn walk_find(
    dir: &std::path::Path,
    prefix: &str,
    depth: usize,
    needle: &str,
    mount: usize,
    found: &mut Vec<Found>,
    visited: &mut usize,
) {
    if depth > FIND_DEPTH || *visited > FIND_VISITED || found.len() >= FIND_MATCHES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<_> = entries.flatten().collect();
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        *visited += 1;
        if *visited > FIND_VISITED || found.len() >= FIND_MATCHES {
            return;
        }
        let name = child.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(child.path()) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        let relative = format!("{prefix}{name}");
        if meta.is_dir() {
            walk_find(
                &child.path(),
                &format!("{relative}/"),
                depth + 1,
                needle,
                mount,
                found,
                visited,
            );
            continue;
        }
        if !meta.is_file() || !name.to_lowercase().contains(needle) {
            continue;
        }
        let href = format!(
            "{}?src={mount}&download=1",
            relative
                .split('/')
                .map(encode_segment)
                .collect::<Vec<_>>()
                .join("/")
        );
        found.push(Found {
            path: relative,
            size: meta.len(),
            modified: meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|value| {
                    chrono::DateTime::from_timestamp(value.as_secs() as i64, 0)
                        .map(|time| {
                            time.with_timezone(&Local)
                                .format("%Y-%m-%d %H:%M")
                                .to_string()
                        })
                        .unwrap_or_else(|| "—".into())
                }),
            href,
        });
    }
}

pub fn render_find(term: &str, found: &[Found], locale: &str, nonce: &str) -> String {
    let t = |key: &str| i18n::t(locale, key);
    let t1 = |key: &str, value: &str| i18n::t_args(locale, key, &[("n", value.to_string())]);
    let rows: String = found
        .iter()
        .map(|item| {
            format!(
                "<tr><td class='name'><a href='{}'>{}</a></td><td class='size'>{}</td><td class='time'>{}</td><td class='act'><a class='btn' href='{}'>{}</a></td></tr>",
                escape_html(&item.href),
                escape_html(&item.path),
                human_size(item.size),
                escape_html(item.modified.as_deref().unwrap_or("—")),
                escape_html(&item.href),
                escape_html(&t("page.download"))
            )
        })
        .collect();
    let body = if found.is_empty() {
        format!(
            "<tr><td colspan='4' class='empty'>{}</td></tr>",
            t("page.find_empty")
        )
    } else {
        rows
    };
    format!(
        r#"<!doctype html>
<html lang="{lang}">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="dark light">
<link rel="icon" href="/icon.png" type="image/png">
<title>{title} · LanTransport</title>
<style>{STYLE}</style>
<div class="wrap">
  <div class="top"><div class="top-inner">
    <a class="brand" href="./"><img class="brand-mark" src="/icon.png" alt="" width="40" height="40" draggable="false">LanTransport</a>
    <div class="top-actions"><a class="btn" href="./">{back}</a></div>
  </div></div>
  <h1>{heading}</h1>
  <div class="meta">{count}</div>
  <div class="card list-card"><table>
    <thead><tr><th>{name}</th><th>{size}</th><th class="time-head">{modified}</th><th></th></tr></thead>
    <tbody>{body}</tbody>
  </table></div>
</div>
<script nonce="{nonce}"></script>
"#,
        lang = escape_html(locale),
        title = escape_html(&t("page.find_title")),
        STYLE = STYLE,
        back = escape_html(&t("page.find_back")),
        heading = escape_html(&format!("{}: {}", t("page.find_title"), term)),
        count = escape_html(&t1("page.find_count", &found.len().to_string())),
        name = escape_html(&t("page.col.name")),
        size = escape_html(&t("page.col.size")),
        modified = escape_html(&t("page.col.modified")),
        body = body,
        nonce = escape_html(nonce),
    )
}

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
