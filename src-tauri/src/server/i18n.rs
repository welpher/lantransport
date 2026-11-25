//! Locale catalogs shared with the desktop UI (locales/*.json).

use std::collections::HashMap;
use std::sync::OnceLock;

use super::http::Request;

struct Catalog {
    entries: HashMap<String, String>,
}

pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("zh-CN", "中文")];

fn catalogs() -> &'static HashMap<&'static str, Catalog> {
    static CATALOGS: OnceLock<HashMap<&'static str, Catalog>> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        let sources = [
            ("en", include_str!("../../../locales/en.json")),
            ("zh-CN", include_str!("../../../locales/zh-CN.json")),
        ];
        sources
            .into_iter()
            .map(|(code, json)| {
                let entries: HashMap<String, String> =
                    serde_json::from_str(json).expect("locale catalog is valid JSON");
                (code, Catalog { entries })
            })
            .collect()
    })
}

pub fn normalize(code: &str) -> Option<&'static str> {
    let code = code.trim();
    if code.is_empty() {
        return None;
    }
    if let Some((supported, _)) = LANGUAGES
        .iter()
        .find(|(supported, _)| supported.eq_ignore_ascii_case(code))
    {
        return Some(supported);
    }
    let primary = code
        .split(['-', '_'])
        .next()
        .unwrap_or(code)
        .to_ascii_lowercase();
    LANGUAGES
        .iter()
        .find(|(supported, _)| {
            supported
                .split('-')
                .next()
                .unwrap_or(supported)
                .eq_ignore_ascii_case(&primary)
        })
        .map(|(supported, _)| *supported)
}

pub fn resolve(cookie: Option<&str>, query: Option<&str>, accept: Option<&str>) -> &'static str {
    if let Some(locale) = query.and_then(normalize) {
        return locale;
    }
    if let Some(locale) = cookie.and_then(normalize) {
        return locale;
    }
    if let Some(header) = accept {
        for tag in header.split(',') {
            let tag = tag.split(';').next().unwrap_or_default().trim();
            if let Some(locale) = normalize(tag) {
                return locale;
            }
        }
    }
    "en"
}

pub fn from_request(request: &Request) -> &'static str {
    let cookie = request
        .header("cookie")
        .and_then(|header| cookie_value(header, "lt_lang"));
    let query = super::http::query_param(&request.target, "lang");
    resolve(cookie, query.as_deref(), request.header("accept-language"))
}

fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.trim())
}

pub fn t(locale: &str, key: &str) -> String {
    catalogs()
        .get(locale)
        .or_else(|| catalogs().get("en"))
        .and_then(|catalog| catalog.entries.get(key))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

pub fn t_args(locale: &str, key: &str, args: &[(&str, String)]) -> String {
    let mut text = t(locale, key);
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_supported_locales() {
        assert_eq!(normalize("zh"), Some("zh-CN"));
        assert_eq!(normalize("zh-Hans"), Some("zh-CN"));
        assert_eq!(normalize("EN-us"), Some("en"));
        assert_eq!(normalize("de"), None);
    }

    #[test]
    fn precedence_is_query_then_cookie_then_header() {
        assert_eq!(resolve(None, Some("zh"), Some("en-US")), "zh-CN");
        assert_eq!(resolve(Some("zh-CN"), None, Some("en-US")), "zh-CN");
        assert_eq!(resolve(None, None, Some("fr-FR, zh-CN;q=0.8")), "zh-CN");
        assert_eq!(resolve(None, None, Some("fr-FR")), "en");
    }

    #[test]
    fn translates_and_substitutes() {
        assert_eq!(t_args("en", "page.items", &[("n", "3".into())]), "3 items");
        assert_eq!(t("zh-CN", "page.download"), "下载");
        assert_eq!(t("en", "missing.key"), "missing.key");
    }
}
