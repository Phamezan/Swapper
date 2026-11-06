fn main() {
    // Embed the offline snapshot, including an empty index on fresh checkouts.
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("resources/guides");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = vec![("index.json".to_string(), root.join("index.json"))];
    if root.join("blind-picks.json").is_file() {
        files.push(("blind-picks.json".to_string(), root.join("blind-picks.json")));
    }
    if let Ok(entries) = std::fs::read_dir(root.join("champions")) {
        for path in entries.flatten().map(|e| e.path()) {
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else { continue };
            if path.extension().is_some_and(|ext| ext == "json") && path.file_stem().and_then(|s| s.to_str()).is_some_and(|s| s.parse::<u32>().is_ok()) {
                files.push((format!("champions/{name}"), path));
            }
        }
    }
    files.sort_by(|a,b| a.0.cmp(&b.0));
    let mut source = String::from("const BUNDLED_GUIDES: &[(&str, &[u8])] = &[\n");
    for (name, path) in files {
        source.push_str(&format!("({name:?}, include_bytes!({:?})),\n", path.to_str().unwrap()));
    }
    source.push_str("];\n");
    std::fs::write(std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("bundled_guides.rs"), source).unwrap();
    tauri_build::build()
}
