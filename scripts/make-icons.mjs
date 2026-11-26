import { mkdir } from "node:fs/promises";
import sharp from "sharp";

await mkdir("icons", { recursive: true });
await sharp("assets/icon.svg").resize(1024, 1024).png().toFile("assets/icon.png");
await sharp("assets/tray.svg").resize(44, 44).png().toFile("src-tauri/icons/tray.png");
await sharp("assets/tray-template.svg").resize(44, 44).png().toFile("src-tauri/icons/tray-template.png");
