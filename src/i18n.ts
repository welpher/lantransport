import en from "../locales/en.json";
import zhCN from "../locales/zh-CN.json";

export const LANGUAGES = [
  { code: "en", name: "English" },
  { code: "zh-CN", name: "中文" },
];

const catalogs: Record<string, Record<string, string>> = {
  en: en as Record<string, string>,
  "zh-CN": zhCN as Record<string, string>,
};

export function normalizeLocale(code: string): string {
  const value = code.trim().toLowerCase();
  const exact = LANGUAGES.find((language) => language.code.toLowerCase() === value);
  if (exact) return exact.code;
  const primary = value.split(/[-_]/)[0];
  const match = LANGUAGES.find(
    (language) => language.code.split("-")[0].toLowerCase() === primary,
  );
  return match ? match.code : "en";
}

export function detectLocale(): string {
  const languages = navigator.languages?.length ? navigator.languages : [navigator.language];
  for (const language of languages) {
    const code = normalizeLocale(language);
    if (code !== "en" || language.toLowerCase().startsWith("en")) return code;
  }
  return "en";
}

export function translate(
  locale: string,
  key: string,
  vars?: Record<string, string | number>,
): string {
  const catalog = catalogs[locale] ?? catalogs.en;
  let text = catalog[key] ?? catalogs.en[key] ?? key;
  if (vars) {
    for (const [name, value] of Object.entries(vars)) {
      text = text.split(`{${name}}`).join(String(value));
    }
  }
  return text;
}
