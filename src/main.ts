import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import QRCode from "qrcode";

import { LANGUAGES, detectLocale, normalizeLocale, translate } from "./i18n";

interface Settings {
  shares: string[];
  port: number;
  allow_lan: boolean;
  password: string | null;
  remember_password: boolean;
  auto_start: boolean;
  minimize_to_tray: boolean;
  allow_uploads: boolean;
  upload_key: string;
  max_upload_mb: number;
  locale: string;
}

interface Snapshot {
  version: string;
  platform: string;
  settings: Settings;
  running: boolean;
  port: number | null;
  urls: string[];
  started_at_ms: number | null;
  last_error: string | null;
}

const $ = <T extends HTMLElement>(id: string): T => {
  const element = document.getElementById(id);
  if (!element) throw new Error(`missing element #${id}`);
  return element as T;
};

const elements = {
  tagline: $("tagline"),
  status: $("status"),
  statusText: $("status-text"),
  locale: $<HTMLSelectElement>("locale"),
  tabShare: $<HTMLButtonElement>("tab-share"),
  tabSettings: $<HTMLButtonElement>("tab-settings"),
  sharePanel: $("share-panel"),
  settingsPanel: $("settings-panel"),
  connectionLabel: $("connection-label"),
  uploadsGroupLabel: $("uploads-label"),
  behaviorLabel: $("behavior-label"),
  settingsLocked: $("settings-locked"),
  setupCard: $("setup-card"),
  foldersLabel: $("folders-label"),
  foldersHint: $("folders-hint"),
  folders: $("folders"),
  addFolder: $<HTMLButtonElement>("add-folder"),
  portLabel: $("port-label"),
  port: $<HTMLInputElement>("port"),
  lanLabel: $("lan-label"),
  lan: $<HTMLInputElement>("lan"),
  passwordLabel: $("password-label"),
  passwordHint: $("password-hint"),
  password: $<HTMLInputElement>("password"),
  rememberLabel: $("remember-label"),
  allowUploadsLabel: $("allow-uploads-label"),
  allowUploads: $<HTMLInputElement>("allow-uploads"),
  uploadSettings: $("upload-settings"),
  uploadKeyLabel: $("upload-key-label"),
  uploadKey: $("upload-key"),
  copyKey: $<HTMLButtonElement>("copy-key"),
  regenKey: $<HTMLButtonElement>("regen-key"),
  maxUploadLabel: $("max-upload-label"),
  maxUpload: $<HTMLInputElement>("max-upload"),
  uploadHint: $("upload-hint"),
  remember: $<HTMLInputElement>("remember"),
  error: $("error"),
  primary: $<HTMLButtonElement>("primary"),
  runningCard: $("running-card"),
  sharingLabel: $("sharing-label"),
  sharingFolder: $("sharing-folder"),
  liveLabel: $("live-label"),
  uploadShare: $("upload-share"),
  shareKeyLabel: $("share-key-label"),
  shareKey: $("share-key"),
  copyShareKey: $<HTMLButtonElement>("copy-share-key"),
  shareUploadLink: $<HTMLButtonElement>("share-upload-link"),
  stop: $<HTMLButtonElement>("stop"),
  urls: $<HTMLUListElement>("urls"),
  qrToggle: $<HTMLButtonElement>("qr-toggle"),
  qrWrap: $("qr-wrap"),
  qr: $<HTMLImageElement>("qr"),
  open: $<HTMLButtonElement>("open"),
  autoStartLabel: $("auto-start-label"),
  autoStart: $<HTMLInputElement>("auto-start"),
  trayLabel: $("tray-label"),
  tray: $<HTMLInputElement>("tray"),
  security: $("security"),
  sponsor: $<HTMLAnchorElement>("sponsor"),
  version: $("version"),
};

let snapshot: Snapshot | null = null;
let locale = "en";
let tab: "share" | "settings" = "share";
let busy = false;
let localError: string | null = null;
let qrUrl: string | null = null;

const draft = {
  shares: [] as string[],
  port: 8000,
  allowLan: true,
  password: "",
  remember: false,
};

const t = (key: string, vars?: Record<string, string | number>): string =>
  translate(locale, key, vars);

function text() {
  return {
    tagline: t("ui.tagline"),
    running: t("ui.status.running"),
    stopped: t("ui.status.stopped"),
    starting: t("ui.status.starting"),
    folder: t("ui.folder"),
    choose: t("ui.choose"),
    reveal: t("ui.reveal"),
    noFolder: t("ui.no_folder"),
    port: t("ui.port"),
    lan: t("ui.lan"),
    password: t("ui.password"),
    passwordHint: t("ui.password_hint"),
    remember: t("ui.remember"),
    start: t("ui.start"),
    stop: t("ui.stop"),
    urls: t("ui.urls"),
    copy: t("ui.copy"),
    copied: t("ui.copied"),
    qr: t("ui.qr"),
    open: t("ui.open_browser"),
    autoStart: t("ui.auto_start"),
    tray: t("ui.tray"),
    uploads: t("ui.uploads"),
    uploadKey: t("ui.upload_key"),
    regenerate: t("ui.regenerate"),
    copyLink: t("ui.copy_link"),
    maxUpload: t("ui.max_upload"),
    uploadHint: t("ui.upload_hint"),
    sharing: t("ui.sharing"),
    live: t("ui.live"),
    copyKey: t("ui.copy_key"),
    folders: t("ui.folders"),
    addFolder: t("ui.add_folder"),
    remove: t("ui.remove"),
    inbox: t("ui.inbox"),
    foldersHint: t("ui.folders_hint"),
    tabShare: t("ui.tab_share"),
    tabSettings: t("ui.tab_settings"),
    groupConnection: t("ui.group_connection"),
    groupUploads: t("ui.group_uploads"),
    groupBehavior: t("ui.group_behavior"),
    settingsLocked: t("ui.settings_locked"),
    security: t("ui.security"),
    version: t("ui.version"),
  };
}

function syncInputs(): void {
  const settings = snapshot?.settings;
  elements.port.value = String(settings?.port ?? draft.port);
  elements.lan.checked = settings?.allow_lan ?? draft.allowLan;
  elements.remember.checked = settings?.remember_password ?? draft.remember;
  elements.password.value = draft.password;
  elements.autoStart.checked = settings?.auto_start ?? false;
  elements.tray.checked = settings?.minimize_to_tray ?? true;
  renderFolders();
}

function folderName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function renderFolders(): void {
  const label = text();
  const rows = draft.shares.map((path, index) => {
    const row = document.createElement("div");
    row.className = "folder-item";
    const name = document.createElement("span");
    name.className = "folder-name";
    name.textContent = folderName(path);
    const full = document.createElement("span");
    full.className = "folder-path";
    full.textContent = path;
    full.title = path;
    row.append(name, full);
    if (index === 0) {
      const badge = document.createElement("span");
      badge.className = "badge";
      badge.textContent = label.inbox;
      row.append(badge);
    }
    const open = document.createElement("button");
    open.className = "icon-btn";
    open.textContent = "↗";
    open.title = label.reveal;
    open.setAttribute("aria-label", label.reveal);
    open.addEventListener("click", () => void revealItemInDir(path));
    const remove = document.createElement("button");
    remove.className = "icon-btn";
    remove.textContent = "×";
    remove.title = label.remove;
    remove.setAttribute("aria-label", label.remove);
    remove.addEventListener("click", () => {
      draft.shares.splice(index, 1);
      apply();
    });
    row.append(open, remove);
    return row;
  });
  if (!rows.length) {
    const empty = document.createElement("div");
    empty.className = "folders-empty";
    empty.textContent = label.noFolder;
    rows.push(empty);
  }
  elements.folders.replaceChildren(...rows);
}

function apply(): void {
  const label = text();
  const running = snapshot?.running ?? false;
  document.documentElement.lang = locale;
  elements.locale.value = locale;

  elements.tagline.textContent = label.tagline;
  elements.tabShare.textContent = label.tabShare;
  elements.tabSettings.textContent = label.tabSettings;
  elements.tabShare.classList.toggle("active", tab === "share");
  elements.tabSettings.classList.toggle("active", tab === "settings");
  elements.tabShare.setAttribute("aria-selected", String(tab === "share"));
  elements.tabSettings.setAttribute("aria-selected", String(tab === "settings"));
  elements.sharePanel.hidden = tab !== "share";
  elements.settingsPanel.hidden = tab !== "settings";
  elements.connectionLabel.textContent = label.groupConnection;
  elements.uploadsGroupLabel.textContent = label.groupUploads;
  elements.behaviorLabel.textContent = label.groupBehavior;
  elements.settingsLocked.textContent = label.settingsLocked;
  elements.settingsLocked.hidden = !running;
  elements.foldersLabel.textContent = label.folders;
  elements.foldersHint.textContent = label.foldersHint;
  elements.addFolder.textContent = label.addFolder;
  elements.portLabel.textContent = label.port;
  elements.lanLabel.textContent = label.lan;
  elements.passwordLabel.textContent = label.password;
  elements.passwordHint.textContent = label.passwordHint;
  elements.rememberLabel.textContent = label.remember;
  elements.allowUploadsLabel.textContent = label.uploads;
  elements.uploadKeyLabel.textContent = label.uploadKey;
  elements.regenKey.textContent = label.regenerate;
  elements.maxUploadLabel.textContent = label.maxUpload;
  elements.uploadHint.textContent = label.uploadHint;
  elements.sharingLabel.textContent = label.sharing;
  elements.liveLabel.textContent = label.live;
  elements.shareKeyLabel.textContent = label.uploadKey;
  elements.copyShareKey.textContent = label.copyKey;
  elements.shareUploadLink.textContent = label.copyLink;
  elements.stop.textContent = label.stop;
  elements.qrToggle.textContent = label.qr;
  elements.open.textContent = label.open;
  elements.autoStartLabel.textContent = label.autoStart;
  elements.trayLabel.textContent = label.tray;
  elements.security.textContent = label.security;
  elements.version.textContent = snapshot
    ? `${label.version} ${snapshot.version} · ${snapshot.platform}`
    : "";

  const settings = snapshot?.settings;
  elements.setupCard.hidden = running;
  elements.runningCard.hidden = !running;
  const shares = settings?.shares ?? [];
  elements.sharingFolder.textContent = shares.length > 1
    ? `${folderName(shares[0])} +${shares.length - 1}`
    : (shares[0] ?? "");
  const shareUploads = running
    && (settings?.allow_uploads ?? false)
    && Boolean(settings?.upload_key);
  elements.uploadShare.hidden = !shareUploads;
  elements.shareKey.textContent = settings?.upload_key ?? "";
  elements.stop.disabled = busy;
  elements.allowUploads.checked = settings?.allow_uploads ?? false;
  elements.uploadSettings.hidden = !(settings?.allow_uploads ?? false);
  elements.uploadKey.textContent = settings?.upload_key || "—";
  elements.copyKey.textContent = label.copy;
  elements.copyKey.disabled = !settings?.upload_key;
  if (document.activeElement !== elements.maxUpload) {
    elements.maxUpload.value = String(settings?.max_upload_mb ?? 4096);
  }

  elements.status.classList.toggle("on", running);
  elements.statusText.textContent = busy ? label.starting : running ? label.running : label.stopped;

  elements.primary.textContent = label.start;
  elements.primary.disabled = busy || running || draft.shares.length === 0;

  for (const control of [
    elements.addFolder,
    elements.port,
    elements.lan,
    elements.password,
    elements.remember,
  ]) {
    (control as HTMLInputElement | HTMLButtonElement).disabled = running || busy;
  }

  renderUrls();
  renderError();

  renderFolders();
  document.title = running && snapshot?.port ? `LanTransport · :${snapshot.port}` : "LanTransport";
}

function renderUrls(): void {
  const urls = snapshot?.urls ?? [];
  const key = urls.join("|");
  if (elements.urls.dataset.key === key) return;
  elements.urls.dataset.key = key;
  elements.urls.replaceChildren(
    ...urls.map((url) => {
      const item = document.createElement("li");
      const text = document.createElement("span");
      text.className = "url";
      text.textContent = url;
      const button = document.createElement("button");
      button.className = "btn copy";
      button.dataset.url = url;
      button.textContent = t("ui.copy");
      item.append(text, button);
      return item;
    }),
  );
  elements.qrWrap.hidden = true;
  qrUrl = null;
}

function renderError(): void {
  const code = localError ?? snapshot?.last_error ?? null;
  if (!code) {
    elements.error.hidden = true;
    return;
  }
  elements.error.hidden = false;
  const message = t(`error.${code}`);
  elements.error.textContent = message === `error.${code}` ? code : message;
}

async function start(): Promise<void> {
  if (!draft.shares.length) {
    localError = "no_folder";
    apply();
    return;
  }
  busy = true;
  localError = null;
  apply();
  try {
    snapshot = await invoke<Snapshot>("start_sharing", {
      shares: draft.shares,
      port: Number(elements.port.value),
      allowLan: elements.lan.checked,
      password: elements.password.value || null,
      rememberPassword: elements.remember.checked,
    });
    draft.remember = snapshot.settings.remember_password;
    syncInputs();
  } catch (error) {
    localError = String(error);
  }
  busy = false;
  apply();
}

async function stop(): Promise<void> {
  busy = true;
  localError = null;
  apply();
  try {
    snapshot = await invoke<Snapshot>("stop_sharing");
    syncInputs();
  } catch (error) {
    localError = String(error);
  }
  busy = false;
  apply();
}

async function updatePrefs(patch: Record<string, unknown>): Promise<void> {
  try {
    snapshot = await invoke<Snapshot>("update_prefs", patch);
    apply();
  } catch (error) {
    localError = String(error);
    apply();
  }
}

async function addFolder(): Promise<void> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: text().folders,
    defaultPath: draft.shares[draft.shares.length - 1] ?? undefined,
  });
  if (typeof selected === "string" && !draft.shares.includes(selected)) {
    draft.shares.push(selected);
    localError = null;
    apply();
  }
}

async function flashCopy(value: string, button: HTMLButtonElement): Promise<void> {
  try {
    await writeText(value);
    const original = button.textContent;
    button.textContent = text().copied;
    window.setTimeout(() => {
      button.textContent = original;
    }, 1200);
  } catch {
    localError = "io";
    apply();
  }
}

async function updateUpload(regenerate = false): Promise<void> {
  try {
    snapshot = await invoke<Snapshot>("update_upload", {
      allowUploads: elements.allowUploads.checked,
      maxUploadMb: Math.max(1, Math.min(1048576, Number(elements.maxUpload.value) || 4096)),
      regenerate,
    });
    localError = null;
  } catch (error) {
    localError = String(error);
  }
  apply();
}

function copyUploadLink(): void {
  const key = snapshot?.settings.upload_key;
  const urls = snapshot?.urls ?? [];
  if (!snapshot?.running || !key) {
    localError = "not_running";
    apply();
    return;
  }
  const base = urls.find((value) => !value.includes("127.0.0.1")) ?? urls[0];
  void flashCopy(`${base}/?key=${encodeURIComponent(key)}`, elements.shareUploadLink);
}

async function toggleQr(): Promise<void> {
  if (qrUrl) {
    elements.qrWrap.hidden = true;
    qrUrl = null;
    return;
  }
  const urls = snapshot?.urls ?? [];
  const url = urls.find((value) => !value.includes("127.0.0.1")) ?? urls[0];
  if (!url) return;
  const dataUrl = await QRCode.toDataURL(url, {
    margin: 1,
    width: 416,
    color: { dark: "#101828", light: "#ffffff" },
  });
  elements.qr.src = dataUrl;
  elements.qrWrap.hidden = false;
  qrUrl = url;
}

async function boot(): Promise<void> {
  snapshot = await invoke<Snapshot>("get_snapshot");
  const stored = snapshot.settings.locale;
  locale = stored === "auto" ? detectLocale() : normalizeLocale(stored);
  draft.shares = [...snapshot.settings.shares];
  draft.port = snapshot.settings.port;
  draft.allowLan = snapshot.settings.allow_lan;
  draft.password = snapshot.settings.password ?? "";
  draft.remember = snapshot.settings.remember_password;

  if (stored !== locale) {
    snapshot = await invoke<Snapshot>("update_prefs", { locale });
  }

  elements.locale.replaceChildren(
    ...LANGUAGES.map((language) => {
      const option = document.createElement("option");
      option.value = language.code;
      option.textContent = language.name;
      return option;
    }),
  );

  syncInputs();
  apply();

  await listen<Snapshot>("snapshot", (event) => {
    const wasRunning = snapshot?.running;
    snapshot = event.payload;
    if (wasRunning !== snapshot.running) {
      syncInputs();
    }
    apply();
  });
  elements.locale.addEventListener("change", () => {
    locale = normalizeLocale(elements.locale.value);
    void updatePrefs({ locale });
  });
}

elements.addFolder.addEventListener("click", () => void addFolder());
elements.tabShare.addEventListener("click", () => {
  tab = "share";
  apply();
});
elements.tabSettings.addEventListener("click", () => {
  tab = "settings";
  apply();
});
elements.primary.addEventListener("click", () => void start());
elements.stop.addEventListener("click", () => void stop());
elements.password.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !snapshot?.running) void start();
});
elements.urls.addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button.copy");
  if (button?.dataset.url) void flashCopy(button.dataset.url, button);
});
elements.sponsor.addEventListener("click", (event) => {
  event.preventDefault();
  void openUrl("https://github.com/sponsors/welpher");
});
elements.qrToggle.addEventListener("click", () => void toggleQr());
elements.open.addEventListener("click", () => {
  const url = snapshot?.urls.find((value) => !value.includes("127.0.0.1")) ?? snapshot?.urls[0];
  if (url) void openUrl(url);
});
elements.allowUploads.addEventListener("change", () => void updateUpload());
elements.regenKey.addEventListener("click", () => void updateUpload(true));
elements.maxUpload.addEventListener("change", () => void updateUpload());
elements.copyKey.addEventListener("click", () => {
  const key = snapshot?.settings.upload_key;
  if (key) void flashCopy(key, elements.copyKey);
});
elements.copyShareKey.addEventListener("click", () => {
  const key = snapshot?.settings.upload_key;
  if (key) void flashCopy(key, elements.copyShareKey);
});
elements.shareUploadLink.addEventListener("click", copyUploadLink);
elements.autoStart.addEventListener("change", () => {
  void updatePrefs({ autoStart: elements.autoStart.checked });
});
elements.tray.addEventListener("change", () => {
  void updatePrefs({ minimizeToTray: elements.tray.checked });
});

void boot();
