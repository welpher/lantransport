const modules = import.meta.glob("../locales/*.json", { eager: true }) as Record<
  string,
  { default: Record<string, string> }
>;

export const LANGUAGES = Object.entries(modules)
  .map(([path, module]) => {
    const code = path.split("/").pop()?.replace(/\.json$/, "") ?? path;
    return { code, name: module.default["language.name"] ?? code };
  })
  .sort((a, b) => a.code.localeCompare(b.code));

const catalogs: Record<string, Record<string, string>> = {};
for (const [path, module] of Object.entries(modules)) {
  const code = path.split("/").pop()?.replace(/\.json$/, "") ?? path;
  catalogs[code] = module.default;
}

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
  const catalog = catalogs[locale] ?? catalogs.en ?? {};
  let text = catalog[key] ?? catalogs.en?.[key] ?? key;
  if (vars) {
    for (const [name, value] of Object.entries(vars)) {
      text = text.split(`{${name}}`).join(String(value));
    }
  }
  return text;
}
