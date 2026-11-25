import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import QRCode from "qrcode";

import { copy, type Copy, type Locale } from "./i18n";

interface Settings {
  root: string | null;
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
  folderLabel: $("folder-label"),
  runningHint: $("running-hint"),
  folderPath: $("folder-path"),
  choose: $<HTMLButtonElement>("choose"),
  reveal: $<HTMLButtonElement>("reveal"),
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
  copyLink: $<HTMLButtonElement>("copy-link"),
  maxUploadLabel: $("max-upload-label"),
  maxUpload: $<HTMLInputElement>("max-upload"),
  uploadHint: $("upload-hint"),
  remember: $<HTMLInputElement>("remember"),
  error: $("error"),
  primary: $<HTMLButtonElement>("primary"),
  runningCard: $("running-card"),
  urlsLabel: $("urls-label"),
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
let locale: Locale = "zh";
let busy = false;
let localError: string | null = null;
let qrUrl: string | null = null;

const draft = {
  root: null as string | null,
  port: 8000,
  allowLan: true,
  password: "",
  remember: false,
};

const text = (): Copy => copy[locale];

function detectLocale(): Locale {
  return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

function syncInputs(): void {
  const settings = snapshot?.settings;
  elements.port.value = String(settings?.port ?? draft.port);
  elements.lan.checked = settings?.allow_lan ?? draft.allowLan;
  elements.remember.checked = settings?.remember_password ?? draft.remember;
  elements.password.value = draft.password;
  elements.autoStart.checked = settings?.auto_start ?? false;
  elements.tray.checked = settings?.minimize_to_tray ?? true;
  renderFolder();
}

function renderFolder(): void {
  const path = draft.root ?? snapshot?.settings.root ?? null;
  const running = snapshot?.running ?? false;
  const label = text();
  elements.folderPath.textContent = path ?? label.noFolder;
  elements.folderPath.classList.toggle("empty", !path);
  elements.folderPath.title = path ?? "";
  elements.reveal.hidden = !path;
  elements.runningHint.textContent = running ? `:${snapshot?.port}` : "";
}

function apply(): void {
  const label = text();
  const running = snapshot?.running ?? false;
  document.documentElement.lang = locale === "zh" ? "zh-CN" : "en";
  elements.locale.value = locale;

  elements.tagline.textContent = label.tagline;
  elements.folderLabel.textContent = label.folder;
  elements.choose.textContent = label.choose;
  elements.reveal.textContent = label.reveal;
  elements.portLabel.textContent = label.port;
  elements.lanLabel.textContent = label.lan;
  elements.passwordLabel.textContent = label.password;
  elements.passwordHint.textContent = label.passwordHint;
  elements.rememberLabel.textContent = label.remember;
  elements.allowUploadsLabel.textContent = label.uploads;
  elements.uploadKeyLabel.textContent = label.uploadKey;
  elements.regenKey.textContent = label.regenerate;
  elements.copyLink.textContent = label.copyLink;
  elements.maxUploadLabel.textContent = label.maxUpload;
  elements.uploadHint.textContent = label.uploadHint;
  elements.urlsLabel.textContent = label.urls;
  elements.qrToggle.textContent = label.qr;
  elements.open.textContent = label.open;
  elements.autoStartLabel.textContent = label.autoStart;
  elements.trayLabel.textContent = label.tray;
  elements.security.textContent = label.security;
  elements.version.textContent = snapshot
    ? `${label.version} ${snapshot.version} · ${snapshot.platform}`
    : "";

  const settings = snapshot?.settings;
  elements.allowUploads.checked = settings?.allow_uploads ?? false;
  elements.uploadSettings.hidden = !(settings?.allow_uploads ?? false);
  elements.uploadKey.textContent = settings?.upload_key || "—";
  elements.copyKey.textContent = label.copy;
  elements.copyKey.disabled = !settings?.upload_key;
  elements.copyLink.disabled = !running || !settings?.upload_key;
  if (document.activeElement !== elements.maxUpload) {
    elements.maxUpload.value = String(settings?.max_upload_mb ?? 4096);
  }

  elements.status.classList.toggle("on", running);
  elements.statusText.textContent = busy ? label.starting : running ? label.running : label.stopped;

  elements.primary.textContent = running ? label.stop : label.start;
  elements.primary.classList.toggle("stop", running);
  elements.primary.disabled = busy;

  for (const control of [
    elements.choose,
    elements.port,
    elements.lan,
    elements.password,
    elements.remember,
  ]) {
    (control as HTMLInputElement | HTMLButtonElement).disabled = running || busy;
  }

  renderFolder();
  renderUrls();
  renderError();

  document.title = running && snapshot?.port ? `LanTransport · :${snapshot.port}` : "LanTransport";
}

function renderUrls(): void {
  const running = snapshot?.running ?? false;
  elements.runningCard.hidden = !running;
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
      button.textContent = copy[locale].copy;
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
  elements.error.textContent = text().errors[code] ?? code;
}

async function start(): Promise<void> {
  if (!draft.root) {
    localError = "no_folder";
    apply();
    return;
  }
  busy = true;
  localError = null;
  apply();
  try {
    snapshot = await invoke<Snapshot>("start_sharing", {
      root: draft.root,
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

async function chooseFolder(): Promise<void> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: text().folder,
    defaultPath: draft.root ?? undefined,
  });
  if (typeof selected === "string") {
    draft.root = selected;
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
  void flashCopy(`${base}/?key=${encodeURIComponent(key)}`, elements.copyLink);
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
  locale = stored === "zh" || stored === "en" ? stored : detectLocale();
  draft.root = snapshot.settings.root;
  draft.port = snapshot.settings.port;
  draft.allowLan = snapshot.settings.allow_lan;
  draft.password = snapshot.settings.password ?? "";
  draft.remember = snapshot.settings.remember_password;

  if (stored !== locale) {
    snapshot = await invoke<Snapshot>("update_prefs", { locale });
  }

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
    locale = elements.locale.value === "en" ? "en" : "zh";
    void updatePrefs({ locale });
  });
}

elements.choose.addEventListener("click", () => void chooseFolder());
elements.reveal.addEventListener("click", () => {
  const path = draft.root ?? snapshot?.settings.root;
  if (path) void revealItemInDir(path);
});
elements.primary.addEventListener("click", () => {
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
elements.copyLink.addEventListener("click", copyUploadLink);
elements.autoStart.addEventListener("change", () => {
  void updatePrefs({ autoStart: elements.autoStart.checked });
});
elements.tray.addEventListener("change", () => {
  void updatePrefs({ minimizeToTray: elements.tray.checked });
});

void boot();
