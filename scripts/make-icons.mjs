// One source of truth for the logo: assets/mark.svg (the glyph, black, 1024).
// Everything else — tray icons, the app icon, the PWA icons — is derived here,
// so the menu bar can never drift away from the Dock icon again.
import { mkdir, readFile, writeFile } from "node:fs/promises";
import sharp from "sharp";

await mkdir("icons", { recursive: true });
await mkdir("public", { recursive: true });

const mark = await readFile("assets/mark.svg", "utf8");


// App icon: rounded gradient background with the mark in white.
const background = `<svg width="1024" height="1024" viewBox="0 0 1024 1024" xmlns="http://www.w3.org/2000/svg">
  <defs><linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="#3b82f6"/>
    <stop offset="1" stop-color="#1e40af"/>
  </linearGradient></defs>
  <rect width="1024" height="1024" rx="224" fill="url(#bg)"/>
</svg>`;
const icon = await sharp(Buffer.from(background))
  .composite([{ input: Buffer.from(mark.replaceAll("#000000", "#ffffff")) }])
  .png()
  .toBuffer();
await writeFile("assets/icon.png", icon);

// Tray icons, 44 px = 22 pt at 2x:
// - macOS treats menu bar images as templates, so it gets the glyph alone (black,
//   alpha only) and the system draws it black or white as the bar changes.
// - Windows and Linux have no such rule, so they get the app icon itself and the
//   tray matches the dock and the installer.
await sharp(Buffer.from(mark))
  .resize(44, 44)
  .png()
  .toFile("src-tauri/icons/tray-template.png");
await sharp(icon).resize(44, 44).png().toFile("src-tauri/icons/tray.png");

// Web copies: the desktop UI, the share page favicon and the web manifest.
await sharp(icon).resize(128, 128).png().toFile("public/icon.png");
await sharp(icon).resize(192, 192).png().toFile("public/icon-192.png");
await sharp(icon).resize(512, 512).png().toFile("public/icon-512.png");
