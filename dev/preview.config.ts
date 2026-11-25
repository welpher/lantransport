import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vite";

const mock = fileURLToPath(new URL("./mock-tauri.ts", import.meta.url));

export default defineConfig({
  root: fileURLToPath(new URL("..", import.meta.url)),
  resolve: {
    alias: [
      { find: "@tauri-apps/api/core", replacement: mock },
      { find: "@tauri-apps/api/event", replacement: mock },
      { find: "@tauri-apps/plugin-dialog", replacement: mock },
      { find: "@tauri-apps/plugin-clipboard-manager", replacement: mock },
      { find: "@tauri-apps/plugin-opener", replacement: mock },
    ],
  },
  server: { port: 1421, strictPort: true },
  optimizeDeps: { include: ["qrcode"] },
});
