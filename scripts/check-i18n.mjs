import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const dir = "locales";
const base = JSON.parse(readFileSync(join(dir, "en.json"), "utf8"));
const baseKeys = Object.keys(base).sort();
const placeholders = (text) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");

let failed = false;
for (const file of readdirSync(dir).filter((name) => name.endsWith(".json"))) {
  if (file === "en.json") continue;
  const catalog = JSON.parse(readFileSync(join(dir, file), "utf8"));
  const keys = Object.keys(catalog).sort();
  const missing = baseKeys.filter((key) => !(key in catalog));
  const extra = keys.filter((key) => !(key in base));
  if (missing.length) {
    failed = true;
    console.error(`${file}: missing ${missing.join(", ")}`);
  }
  if (extra.length) {
    failed = true;
    console.error(`${file}: unknown ${extra.join(", ")}`);
  }
  for (const key of baseKeys) {
    if (!(key in catalog)) continue;
    if (placeholders(base[key]) !== placeholders(catalog[key])) {
      failed = true;
      console.error(`${file}: placeholder mismatch in ${key}`);
    }
  }
  console.log(`${file}: ${keys.length} keys`);
}
if (failed) process.exit(1);
console.log("locales are in sync");
