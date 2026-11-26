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
  password: string | null;
  auto_start: boolean;
  minimize_to_tray: boolean;
  allow_uploads: boolean;
  upload_key: string;
  max_upload_mb: number;
  locale: string;
  theme: string;
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
  theme: $<HTMLSelectElement>("theme"),
  settingsLocked: $("settings-locked"),
  setupCard: $("setup-card"),
  foldersLabel: $("folders-label"),
  foldersHint: $("folders-hint"),
  folders: $("folders"),
  addFolder: $<HTMLButtonElement>("add-folder"),
  portLabel: $("port-label"),
  portState: $("port-state"),
  port: $<HTMLInputElement>("port"),
  passwordLabel: $("password-label"),
  passwordHint: $("password-hint"),
  password: $<HTMLInputElement>("password"),
  passwordClear: $<HTMLButtonElement>("password-clear"),
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
  error: $("error"),
  run: $<HTMLButtonElement>("run"),
  runningCard: $("running-card"),
  sharingLabel: $("sharing-label"),
  sharingFolder: $("sharing-folder"),
  liveLabel: $("live-label"),
  uploadShare: $("upload-share"),
  shareKeyLabel: $("share-key-label"),
  shareKey: $("share-key"),
  copyShareKey: $<HTMLButtonElement>("copy-share-key"),
  shareUploadLink: $<HTMLButtonElement>("share-upload-link"),
  shareQr: $<HTMLButtonElement>("share-qr"),
  shareQrWrap: $("share-qr-wrap"),
  shareQrImg: $<HTMLImageElement>("share-qr-img"),
  urls: $<HTMLUListElement>("urls"),
  networkNote: $("network-note"),
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
let qrFor: string | null = null;
let shareQrUrl: string | null = null;
let selectedUrl: string | null = null;
let passwordTimer: number | null = null;
let networkNoteTimer: number | null = null;
let dropHintTimer: number | null = null;
let networkNote = false;
let portTimer: number | null = null;
let portStatus: "unknown" | "free" | "used" = "unknown";

const portValue = (): number => Number(elements.port.value);

async function checkPort(): Promise<void> {
  if (snapshot?.running) return;
  const port = portValue();
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    portStatus = "unknown";
    apply();
    return;
  }
  try {
    const free = await invoke<boolean>("check_port", { port });
    if (portValue() !== port) return;
    portStatus = free ? "free" : "used";
  } catch {
    portStatus = "unknown";
  }
  apply();
}

function schedulePortCheck(): void {
  if (portTimer !== null) window.clearTimeout(portTimer);
  portTimer = window.setTimeout(() => void checkPort(), 350);
}

async function persistPassword(password: string | null): Promise<void> {
  try {
    snapshot = await invoke<Snapshot>("update_password", { password });
    apply();
  } catch {
    /* keep the field as the user typed it */
  }
}

function schedulePasswordSave(): void {
  draft.password = elements.password.value;
  updatePasswordClear();
  if (passwordTimer !== null) window.clearTimeout(passwordTimer);
  const value = elements.password.value || null;
  passwordTimer = window.setTimeout(() => void persistPassword(value), 350);
}

const draft = {
  shares: [] as string[],
  port: 8787,
  password: "",
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
    portAvailable: t("ui.port_available"),
    portInUse: t("ui.port_in_use"),
    password: t("ui.password"),
    passwordHint: t("ui.password_hint"),
    clear: t("ui.clear"),
    start: t("ui.start"),
    stop: t("ui.stop"),
    urls: t("ui.urls"),
    copy: t("ui.copy"),
    copied: t("ui.copied"),
    qr: t("ui.qr"),
    open: t("ui.open_browser"),
    badgeLan: t("ui.badge_lan"),
    badgeLocal: t("ui.badge_local"),
    autoStart: t("ui.auto_start"),
    tray: t("ui.tray"),
    uploads: t("ui.uploads"),
    uploadKey: t("ui.upload_key"),
    regenerate: t("ui.regenerate"),
    copyLink: t("ui.copy_link"),
    uploadQr: t("ui.upload_qr"),
    maxUpload: t("ui.max_upload"),
    uploadHint: t("ui.upload_hint"),
    sharing: t("ui.sharing"),
    live: t("ui.live"),
    copyKey: t("ui.copy_key"),
    folders: t("ui.folders"),
    addFolder: t("ui.add_folder"),
    remove: t("ui.remove"),
    foldersHint: t("ui.folders_hint"),
    dropAdded: t("ui.drop_added"),
    dropKnown: t("ui.drop_known"),
    tabShare: t("ui.tab_share"),
    tabSettings: t("ui.tab_settings"),
    groupConnection: t("ui.group_connection"),
    groupUploads: t("ui.group_uploads"),
    groupBehavior: t("ui.group_behavior"),
    theme: t("ui.theme"),
    themeAuto: t("ui.theme_auto"),
    themeLight: t("ui.theme_light"),
    themeDark: t("ui.theme_dark"),
    settingsLocked: t("ui.settings_locked"),
    networkChanged: t("ui.network_changed"),
    security: t("ui.security"),
    version: t("ui.version"),
  };
}

function updatePasswordClear(): void {
  elements.passwordClear.hidden = elements.password.value.length === 0;
}

function syncInputs(): void {
  const settings = snapshot?.settings;
  elements.port.value = String(settings?.port ?? draft.port);
  elements.password.value = draft.password;
  updatePasswordClear();
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
      void persistShares();
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
  if (dropHintTimer === null) elements.foldersHint.textContent = label.foldersHint;
  elements.addFolder.textContent = label.addFolder;
  elements.portLabel.textContent = label.port;
  elements.portState.textContent =
    portStatus === "free" ? label.portAvailable : portStatus === "used" ? label.portInUse : "";
  elements.portState.classList.toggle("warn", portStatus === "used");
  elements.passwordLabel.textContent = label.password;
  elements.passwordHint.textContent = label.passwordHint;
  elements.passwordClear.textContent = "×";
  elements.passwordClear.title = label.clear;
  elements.passwordClear.setAttribute("aria-label", label.clear);
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
  elements.networkNote.hidden = !(running && networkNote);
  elements.networkNote.textContent = label.networkChanged;
  elements.shareQr.textContent = label.uploadQr;
  elements.run.textContent = running ? label.stop : label.start;
  elements.run.classList.toggle("stop", running);
  elements.open.textContent = label.open;
  elements.autoStartLabel.textContent = label.autoStart;
  elements.trayLabel.textContent = label.tray;
  elements.theme.setAttribute("aria-label", label.theme);
  const themeOptions: Array<[string, string]> = [
    ["auto", label.themeAuto],
    ["light", label.themeLight],
    ["dark", label.themeDark],
  ];
  for (const [value, text] of themeOptions) {
    const option = elements.theme.querySelector(`option[value="${value}"]`);
    if (option) option.textContent = text;
  }
  elements.theme.value = snapshot?.settings.theme ?? "auto";
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
  if (!shareUploads) {
    elements.shareQrWrap.hidden = true;
    shareQrUrl = null;
  }
  elements.shareKey.textContent = settings?.upload_key ?? "";
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

  const portInvalid = !Number.isInteger(portValue()) || portValue() < 1 || portValue() > 65535;
  elements.run.disabled =
    busy || (!running && (draft.shares.length === 0 || portInvalid || portStatus === "used"));

  for (const control of [
    elements.addFolder,
    elements.port,
    elements.password,
  ]) {
    (control as HTMLInputElement | HTMLButtonElement).disabled = running || busy;
  }

  renderUrls();
  renderError();
  renderFolders();

  if (running) {
    const current = selectedUrlValue();
    if (current) void ensureQr(current, label.qr);
  }
  document.title = running && snapshot?.port ? `LanTransport · :${snapshot.port}` : "LanTransport";
}

function isLoopback(url: string): boolean {
  return url.includes("127.0.0.1") || url.includes("localhost") || url.includes("[::1]");
}

function selectedUrlValue(): string | null {
  const urls = snapshot?.urls ?? [];
  if (selectedUrl && urls.includes(selectedUrl)) return selectedUrl;
  return urls.find((url) => !isLoopback(url)) ?? urls[0] ?? null;
}

async function ensureQr(url: string, alt: string): Promise<void> {
  if (qrFor === url && elements.qr.src) return;
  try {
    const dataUrl = await QRCode.toDataURL(url, {
      margin: 1,
      width: 336,
      color: { dark: "#101828", light: "#ffffff" },
    });
    elements.qr.src = dataUrl;
    elements.qr.alt = alt;
    qrFor = url;
  } catch {
    qrFor = null;
  }
}

function renderUrls(): void {
  const urls = snapshot?.urls ?? [];
  const current = selectedUrlValue();
  const key = `${urls.join("|")}#${current ?? ""}`;
  if (elements.urls.dataset.key === key) return;
  elements.urls.dataset.key = key;
  const label = text();
  elements.urls.replaceChildren(
    ...urls.map((url) => {
      const item = document.createElement("li");
      item.classList.toggle("selected", url === current);
      item.tabIndex = 0;
      item.setAttribute("role", "button");
      item.setAttribute("aria-selected", String(url === current));
      item.addEventListener("click", (event) => {
        if ((event.target as HTMLElement).closest("button")) return;
        selectedUrl = url;
        apply();
      });
      item.addEventListener("keydown", (event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        event.preventDefault();
        selectedUrl = url;
        apply();
      });
      const text = document.createElement("span");
      text.className = "url";
      text.textContent = url;
      const badge = document.createElement("span");
      badge.className = "src";
      badge.textContent = isLoopback(url) ? label.badgeLocal : label.badgeLan;
      const button = document.createElement("button");
      button.className = "btn copy";
      button.dataset.url = url;
      button.textContent = label.copy;
      item.append(text, badge, button);
      return item;
    }),
  );
  qrFor = null;
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
      password: elements.password.value || null,
    });
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
    void checkPort();
  } catch (error) {
    localError = String(error);
  }
  busy = false;
  apply();
}

function applyTheme(theme: string): void {
  const root = document.documentElement;
  if (theme === "light" || theme === "dark") {
    root.dataset.theme = theme;
  } else {
    delete root.dataset.theme;
  }
  // Match the native title bar too; unavailable on some platforms, hence the catch.
  void (async () => {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      await getCurrentWindow().setTheme(theme === "auto" ? null : (theme as "light" | "dark"));
    } catch {
      /* preview browser or platform without theming */
    }
  })();
}

async function persistShares(): Promise<void> {
  try {
    snapshot = await invoke<Snapshot>("update_prefs", { shares: [...draft.shares] });
    draft.shares = [...snapshot.settings.shares];
    localError = null;
  } catch (error) {
    localError = String(error);
  }
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
    void persistShares();
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
  const base = urls.find((value) => !isLoopback(value)) ?? urls[0];
  void flashCopy(`${base}/?key=${encodeURIComponent(key)}`, elements.shareUploadLink);
}



async function boot(): Promise<void> {
  snapshot = await invoke<Snapshot>("get_snapshot");
  const stored = snapshot.settings.locale;
  locale = stored === "auto" ? detectLocale() : normalizeLocale(stored);
  draft.shares = [...snapshot.settings.shares];
  draft.port = snapshot.settings.port;
  draft.password = snapshot.settings.password ?? "";

  if (stored !== locale) {
    snapshot = await invoke<Snapshot>("update_prefs", { locale });
  }

  applyTheme(snapshot.settings.theme);

  elements.theme.replaceChildren(
    ...["auto", "light", "dark"].map((value) => {
      const option = document.createElement("option");
      option.value = value;
      return option;
    }),
  );

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
  void checkPort();

  await listen<[number, number]>("drop-result", (event) => {
    const [added, known] = event.payload;
    const label = text();
    elements.foldersHint.textContent =
      added > 0 ? label.dropAdded : known > 0 ? label.dropKnown : label.dropKnown;
    if (dropHintTimer !== null) window.clearTimeout(dropHintTimer);
    dropHintTimer = window.setTimeout(() => {
      dropHintTimer = null;
      apply();
    }, 4000);
  });

  await listen<Snapshot>("snapshot", (event) => {
    const previous = snapshot;
    const wasRunning = previous?.running ?? false;
    const previousShares = JSON.stringify(previous?.settings.shares ?? []);
    snapshot = event.payload;
    if (wasRunning !== snapshot.running) {
      syncInputs();
    } else if (
      previousShares !== JSON.stringify(snapshot.settings.shares) &&
      // Only adopt the external list when the fields on screen have no edits of
      // their own; a folder dropped on the window must not wipe a half-typed key.
      JSON.stringify(draft.shares) === previousShares
    ) {
      draft.shares = [...snapshot.settings.shares];
      renderFolders();
    }
    const urlsChanged =
      wasRunning && JSON.stringify(previous?.urls ?? []) !== JSON.stringify(snapshot.urls);
    if (urlsChanged) {
      networkNote = true;
      if (networkNoteTimer !== null) window.clearTimeout(networkNoteTimer);
      networkNoteTimer = window.setTimeout(() => {
        networkNote = false;
        apply();
      }, 8000);
    }
    apply();
  });
  elements.locale.addEventListener("change", () => {
    locale = normalizeLocale(elements.locale.value);
    void updatePrefs({ locale });
  });
}

elements.addFolder.addEventListener("click", () => void addFolder());
elements.port.addEventListener("input", schedulePortCheck);
elements.port.addEventListener("blur", () => void checkPort());
elements.password.addEventListener("input", schedulePasswordSave);
elements.passwordClear.addEventListener("click", () => {
  elements.password.value = "";
  draft.password = "";
  updatePasswordClear();
  if (passwordTimer !== null) window.clearTimeout(passwordTimer);
  void persistPassword(null);
  elements.password.focus();
});
elements.tabShare.addEventListener("click", () => {
  tab = "share";
  apply();
});
elements.tabSettings.addEventListener("click", () => {
  tab = "settings";
  apply();
});
elements.run.addEventListener("click", () => {
  if (snapshot?.running) void stop();
  else void start();
});
elements.password.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !snapshot?.running) void start();
});
elements.urls.addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button.copy");
  if (button?.dataset.url) void flashCopy(button.dataset.url, button);
});
elements.sponsor.addEventListener("click", (event) => {
  event.preventDefault();
  void openUrl("https://github.com/welpher/lantransport/blob/main/SUPPORT.md");
});
elements.open.addEventListener("click", () => {
  const url = selectedUrlValue();
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
elements.shareQr.addEventListener("click", () => {
  void (async () => {
    if (shareQrUrl) {
      elements.shareQrWrap.hidden = true;
      shareQrUrl = null;
      return;
    }
    const key = snapshot?.settings.upload_key;
    const base = selectedUrlValue();
    if (!key || !base) {
      localError = "not_running";
      apply();
      return;
    }
    const url = `${base}/?key=${encodeURIComponent(key)}`;
    const dataUrl = await QRCode.toDataURL(url, {
      margin: 1,
      width: 336,
      color: { dark: "#101828", light: "#ffffff" },
    });
    elements.shareQrImg.src = dataUrl;
    elements.shareQrWrap.hidden = false;
    shareQrUrl = url;
  })();
});
elements.autoStart.addEventListener("change", () => {
  void updatePrefs({ autoStart: elements.autoStart.checked });
});
elements.tray.addEventListener("change", () => {
  void updatePrefs({ minimizeToTray: elements.tray.checked });
});
elements.theme.addEventListener("change", () => {
  applyTheme(elements.theme.value);
  void updatePrefs({ theme: elements.theme.value });
});

void boot();
