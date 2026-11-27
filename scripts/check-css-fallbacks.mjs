// color-mix() is Safari 16.2 / Chrome 111 / Firefox 113. Anything older drops the
// whole declaration, and Safari on macOS 12-15 is older, so every color-mix()
// must have a plain declaration of the same property in front of it.
import { readFileSync } from "node:fs";

const sources = [
  ["src/styles.css", readFileSync("src/styles.css", "utf8")],
  ["src-tauri/src/server/listing.rs", readFileSync("src-tauri/src/server/listing.rs", "utf8")],
];

function cssBlocks(text) {
  // Comments first: a comment without a ";" would otherwise swallow the
  // declaration that follows it.
  const stripped = text.replace(/\/\*[\s\S]*?\*\//g, "");
  return stripped.split(/[{}]/).filter((_, index) => index % 2 === 1);
}

const problems = [];
for (const [name, text] of sources) {
  const blocks = name.endsWith(".rs") ? cssBlocks(extractStrings(text)) : cssBlocks(text);
  for (const block of blocks) {
    const seen = new Set();
    for (const raw of block.split(";")) {
      const declaration = raw.trim();
      if (!declaration || declaration.startsWith("/*")) continue;
      const colon = declaration.indexOf(":");
      if (colon < 0) continue;
      const property = declaration.slice(0, colon).trim();
      const value = declaration.slice(colon + 1).trim();
      if (value.includes("color-mix(")) {
        if (!seen.has(property)) {
          problems.push(`${name}: "${property}: ${value}" has no plain fallback above it`);
        }
      } else {
        seen.add(property);
      }
    }
  }
}

function extractStrings(text) {
  const parts = [];
  for (const marker of ['pub const STYLE: &str = r#"', 'const UPLOAD_CSS: &str = r#"']) {
    const start = text.indexOf(marker);
    if (start < 0) continue;
    const from = start + marker.length;
    parts.push(text.slice(from, text.indexOf('"#;', from)));
  }
  return parts.join("\n");
}

if (problems.length) {
  console.error("CSS fallbacks missing:");
  for (const problem of problems) console.error("  " + problem);
  process.exit(1);
}
console.log("css fallbacks: every color-mix() has a plain declaration in front of it");
