const params = new URLSearchParams(location.search);
const running = params.get("mode") === "running";
const uploads = params.get("uploads") === "1";
const lang = params.get("lang") ?? "en";
const locale = lang === "zh" || lang === "zh-CN" ? "zh-CN" : "en";

const snapshot = {
  version: "0.1.0",
  platform: "macos",
  settings: {
    shares: ["/Users/me/Downloads", "/Volumes/Photos"],
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
  return null as T;
}

export async function listen(): Promise<() => void> {
  return () => {};
}

export async function open(): Promise<string | null> {
  return null;
}

export async function writeText(): Promise<void> {}

export async function openUrl(): Promise<void> {}

export async function revealItemInDir(): Promise<void> {}

(globalThis as unknown as { __mockSnapshot: () => unknown }).__mockSnapshot = () => structuredClone(snapshot);
