const params = new URLSearchParams(location.search);
const running = params.get("mode") === "running";
const uploads = params.get("uploads") === "1";
const lang = params.get("lang") ?? "en";
const locale = lang === "zh" || lang === "zh-CN" ? "zh-CN" : "en";

const snapshot = {
  version: "0.1.0",
  platform: "macos",
  settings: {
    shares: params.get("empty") === "1" ? [] : ["/Users/me/Downloads", "/Volumes/Photos"],
    port: 8787,
    password: null,
    remember_password: false,
    auto_start: false,
    minimize_to_tray: true,
    allow_uploads: uploads,
    upload_key: uploads ? "k7m3qp9x2f" : "",
    max_upload_mb: 4096,
    locale,
    theme: (params.get("theme") as string) ?? "auto",
  },
  running,
  port: running ? 8000 : null,
  urls: running ? ["http://127.0.0.1:8787", "http://192.168.1.24:8787"] : [],
  started_at_ms: null,
  last_error: null,
  recent: [
    { name: "holiday.mp4", bytes: 1288490188, peer: "192.168.1.7", dir: "/Users/me/Downloads", at: 1795690000 },
    { name: "contract.pdf", bytes: 860160, peer: "192.168.1.9", dir: "/Volumes/Photos", at: 1795686000 },
    { name: "Screenshot 2026-11-26 at 19.02.11.png", bytes: 483328, peer: "192.168.1.12", dir: "/Users/me/Downloads", at: 1795681000 },
    // The folder is gone, so opening it has to show a localised message.
    { name: "old-report.pdf", bytes: 220200, peer: "192.168.1.20", dir: "/Volumes/Gone/2024", at: 1795676000 },
  ],
};

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (command === "check_port") return ((args?.port as number) !== 9999) as T;
  if (command === "get_snapshot") return structuredClone(snapshot) as T;
  if (command === "update_password") {
    snapshot.settings.password = (args?.password as string | null) ?? null;
    return structuredClone(snapshot) as T;
  }
  if (command === "update_prefs" || command === "update_upload") {
    Object.assign(snapshot.settings, args);
    return structuredClone(snapshot) as T;
  }
  if (command === "start_sharing") {
    snapshot.running = true;
    snapshot.port = 8000;
    snapshot.urls = ["http://127.0.0.1:8787", "http://192.168.1.24:8787"];
    return structuredClone(snapshot) as T;
  }
  if (command === "stop_sharing") {
    snapshot.running = false;
    snapshot.port = null;
    snapshot.urls = [];
    return structuredClone(snapshot) as T;
  }
  if (command === "open_folder_command") {
    const path = String(args?.path ?? "");
    if (path.startsWith("/Volumes/Gone")) throw "folder_gone";
    return null as T;
  }
  return null as T;
}

type Handler = (event: { payload: unknown }) => void;
const listeners = new Map<string, Set<Handler>>();

export async function listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> {
  const set = listeners.get(event) ?? new Set<Handler>();
  set.add(handler as Handler);
  listeners.set(event, set);
  return () => {
    set.delete(handler as Handler);
  };
}

/** Dev only: drive the UI from the console, e.g. `__emit('snapshot', snapshot)`. */
(globalThis as { __emit?: (event: string, payload: unknown) => void }).__emit = (event, payload) => {
  listeners.get(event)?.forEach((handler) => handler({ payload }));
};

export async function open(): Promise<string | null> {
  return null;
}

export async function writeText(): Promise<void> {}

export async function openUrl(): Promise<void> {}

export async function revealItemInDir(): Promise<void> {}

(globalThis as unknown as { __mockSnapshot: () => unknown }).__mockSnapshot = () => structuredClone(snapshot);
