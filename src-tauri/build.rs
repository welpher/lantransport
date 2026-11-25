static LOCALES: &str = "locales";

fn main() {
    let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest"));
    let dir = manifest.join("..").join(LOCALES);
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut entries: Vec<(String, String)> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("locales directory") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let code = path
            .file_stem()
            .expect("stem")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(&path).expect("catalog readable");
        let value: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let name = value
            .get("language.name")
            .and_then(|value| value.as_str())
            .unwrap_or(&code)
            .to_string();
        entries.push((code, name));
    }
    entries.sort();

    let mut generated = String::new();
    generated.push_str("pub const LANGUAGES: &[(&str, &str)] = &[\n");
    for (code, name) in &entries {
        generated.push_str(&format!("    ({code:?}, {name:?}),\n"));
    }
    generated.push_str("];\n\npub const LOCALE_SOURCES: &[(&str, &str)] = &[\n");
    for (code, _) in &entries {
        generated.push_str(&format!(
            "    ({code:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../{LOCALES}/{code}.json\"))),\n"
        ));
    }
    generated.push_str("];\n");

    let out =
        std::path::PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("locales.rs");
    std::fs::write(out, generated).expect("write generated locales");
    tauri_build::build()
}
