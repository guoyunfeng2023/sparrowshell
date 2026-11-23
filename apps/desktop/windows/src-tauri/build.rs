use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=SPARROWSHELL_API_BASE_URL");
    let config_path = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"))
        .join("../config.example.json");
    println!("cargo:rerun-if-changed={}", config_path.display());

    let configured = env::var("SPARROWSHELL_API_BASE_URL")
        .ok()
        .or_else(|| {
            let source = fs::read_to_string(&config_path).ok()?;
            let config: serde_json::Value = serde_json::from_str(&source)
                .unwrap_or_else(|error| panic!("{} 不是有效 JSON：{error}", config_path.display()));
            Some(
                config
                    .get("apiBaseUrl")
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| panic!("{} 缺少有效的 apiBaseUrl", config_path.display()))
                    .to_owned(),
            )
        });
    if let Some(configured) = configured {
        println!("cargo:rustc-env=SPARROWSHELL_API_BASE_URL={configured}");
    }

    tauri_build::build()
}
