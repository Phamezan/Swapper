//! Custom mods: package validation, slots, the KR/CN block and which
//! RuneForge mod each handed-off file came from.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// shortcut: skinhack check skipped — neither ltk_fantome/ltk_modpkg metadata
// nor LTK Manager flags a package as a replica of a paid Riot skin, so there
// is no reliable signal to refuse on.

/// Which exclusive slot a mod occupies, if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Slot {
    Skin(String),
    Map,
    Hud,
    LoadingScreen,
    Other,
}

impl Slot {
    pub fn key(&self) -> String {
        match self {
            Slot::Skin(champion) => format!("skin:{}", champion.to_lowercase()),
            Slot::Map => "map".to_string(),
            Slot::Hud => "hud".to_string(),
            Slot::LoadingScreen => "loading".to_string(),
            Slot::Other => "other".to_string(),
        }
    }
    pub fn label(&self) -> String {
        match self {
            Slot::Skin(champion) => format!("Skin · {champion}"),
            Slot::Map => "Map".to_string(),
            Slot::Hud => "HUD".to_string(),
            Slot::LoadingScreen => "Loading screen".to_string(),
            Slot::Other => "Other".to_string(),
        }
    }
}

/// Derives the slot from package metadata. One skin per champion, one map,
/// one HUD and one loading screen; everything else shares freely.
pub fn slot_for(tags: &[String], champions: &[String], maps: &[String], category: Option<&str>) -> Slot {
    let text = |s: &str| s.to_lowercase();
    let has = |needle: &str| {
        tags.iter().any(|t| text(t).contains(needle))
            || category.is_some_and(|c| text(c).contains(needle))
    };
    if has("hud") {
        return Slot::Hud;
    }
    if has("loading") {
        return Slot::LoadingScreen;
    }
    if !maps.is_empty() || has("map") {
        return Slot::Map;
    }
    if let Some(champion) = champions.first().filter(|c| !c.trim().is_empty()) {
        return Slot::Skin(champion.clone());
    }
    if has("skin") {
        return Slot::Skin(category.unwrap_or("Unknown").to_string());
    }
    Slot::Other
}

/// Metadata read from a validated package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    pub name: String,
    pub author: String,
    pub version: String,
    pub category: Option<String>,
    pub champion: Option<String>,
    pub slot: Slot,
    pub format: String,
    pub wad_files: Vec<String>,
}

/// Where a RuneForge install came from, keyed in `runeforge.json` by the
/// SHA-256 of the file handed to LTK (LTK records it as `sourceSha256`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneForgeMeta {
    pub mod_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_patch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

fn runeforge_path(data_root: &Path) -> PathBuf {
    data_root.join("mods").join("runeforge.json")
}

pub fn load_runeforge(data_root: &Path) -> HashMap<String, RuneForgeMeta> {
    fs::read_to_string(runeforge_path(data_root))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_runeforge(data_root: &Path, sha256: &str, meta: RuneForgeMeta) -> Result<(), String> {
    let mut all = load_runeforge(data_root);
    all.insert(sha256.to_string(), meta);
    let path = runeforge_path(data_root);
    let text = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    path.parent().map(fs::create_dir_all).transpose().and_then(|_| fs::write(&path, text)).map_err(|e| e.to_string())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Validates a package file by parsing it with the LTK crates. Only real
/// `.fantome` / `.modpkg` packages pass; renamed executables and plain zips
/// are rejected.
pub fn validate_package(path: &Path) -> Result<PackageInfo, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "fantome" => validate_fantome(path),
        "modpkg" => validate_modpkg(path),
        _ => Err("Only .fantome and .modpkg files are supported.".to_string()),
    }
}

fn validate_fantome(path: &Path) -> Result<PackageInfo, String> {
    let file = fs::File::open(path).map_err(|e| format!("Couldn't open that file: {e}"))?;
    let mut reader = ltk_fantome::FantomeReader::new(file)
        .map_err(|_| "That file isn't a valid mod package.".to_string())?;
    let info = reader
        .read_info()
        .map_err(|_| "That file isn't a valid mod package.".to_string())?;
    if info.name.trim().is_empty() || reader.wad_names().is_empty() {
        return Err("That file isn't a valid mod package.".to_string());
    }
    // No executable payload may hide in a fantome.
    for name in reader.entry_names() {
        let lower = name.to_lowercase();
        if lower.ends_with(".exe") || lower.ends_with(".dll") || lower.ends_with(".bat") {
            return Err("That package contains an executable and can't be added.".to_string());
        }
    }
    let wad_files: Vec<String> = reader
        .entry_names()
        .filter(|n| n.to_uppercase().starts_with("WAD"))
        .map(str::to_string)
        .collect();
    let category = info.tags.first().cloned();
    let slot = slot_for(&info.tags, &info.champions, &info.maps, category.as_deref());
    Ok(PackageInfo {
        name: info.name.clone(),
        author: info.author.clone(),
        version: info.version.clone(),
        category,
        champion: info.champions.first().cloned(),
        slot,
        format: "fantome".to_string(),
        wad_files,
    })
}

fn validate_modpkg(path: &Path) -> Result<PackageInfo, String> {
    let file = fs::File::open(path).map_err(|e| format!("Couldn't open that file: {e}"))?;
    let mut pkg = ltk_modpkg::Modpkg::mount_from_reader(file)
        .map_err(|_| "That file isn't a valid mod package.".to_string())?;
    let meta = pkg
        .load_metadata()
        .map_err(|_| "That file isn't a valid mod package.".to_string())?;
    if meta.name.trim().is_empty() {
        return Err("That file isn't a valid mod package.".to_string());
    }
    let author = meta
        .authors
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let wad_files: Vec<String> = pkg.chunk_paths().values().cloned().collect();
    let category = meta.tags.first().cloned();
    let slot = slot_for(&meta.tags, &meta.champions, &meta.maps, category.as_deref());
    Ok(PackageInfo {
        name: if meta.display_name.trim().is_empty() { meta.name.clone() } else { meta.display_name.clone() },
        author,
        version: meta.version.to_string(),
        category,
        champion: meta.champions.first().cloned(),
        slot,
        format: "modpkg".to_string(),
        wad_files,
    })
}

/// What LTK Manager keeps in `mods/<slug>/` for an imported package: the
/// project name its slug comes from, `mod.config.json` and a thumbnail.
pub struct ModFiles {
    pub project_name: String,
    pub config: String,
    pub thumbnail: Option<(&'static str, Vec<u8>)>,
}

/// `mod.config.json` as LTK Manager writes it on import (its `ModProject` shape).
// shortcut: no licence, readme or hashtables, and layers sort by plain name;
// LTK shows them only. A modpkg's config is re-read by LTK itself (see `ltk`).
pub fn mod_files(path: &Path) -> Result<ModFiles, String> {
    const INVALID: &str = "That file isn't a valid mod package.";
    let base = || serde_json::json!({ "name": "base", "priority": 0, "description": "Base layer of the mod" });
    let file = fs::File::open(path).map_err(|e| format!("Couldn't open that file: {e}"))?;
    let (name, mut config, thumbnail) = if validate_package(path)?.format == "fantome" {
        let mut reader = ltk_fantome::FantomeReader::new(file).map_err(|_| INVALID.to_string())?;
        let mut info = reader.read_info().map_err(|_| INVALID.to_string())?;
        info.declare_layers(reader.layer_names().iter().map(String::as_str));
        let mut layers: Vec<&ltk_fantome::FantomeLayerInfo> = info.layers.values().filter(|l| !l.name.eq_ignore_ascii_case("base")).collect();
        layers.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.name.cmp(&b.name)));
        let mut table = vec![base()];
        for layer in layers {
            let mut entry = serde_json::json!({ "name": layer.name, "priority": layer.priority });
            if let Some(display) = &layer.display_name {
                entry["display_name"] = display.clone().into();
            }
            if !layer.string_overrides.is_empty() {
                entry["string_overrides"] = serde_json::to_value(&layer.string_overrides).map_err(|e| e.to_string())?;
            }
            table.push(entry);
        }
        let name = slugify(&info.name);
        let config = serde_json::json!({
            "name": name, "display_name": info.name, "version": info.version, "description": info.description,
            "authors": [info.author], "tags": info.tags, "champions": info.champions, "maps": info.maps, "layers": table,
        });
        let thumbnail = reader.read_image_png().ok().flatten().map(|png| ("thumbnail.png", png));
        (name, config, thumbnail)
    } else {
        let mut pkg = ltk_modpkg::Modpkg::mount_from_reader(file).map_err(|_| INVALID.to_string())?;
        let meta = pkg.load_metadata().map_err(|_| INVALID.to_string())?;
        let authors: Vec<serde_json::Value> = meta
            .authors
            .iter()
            .map(|a| match &a.role {
                Some(role) => serde_json::json!({ "name": a.name, "role": role }),
                None => a.name.clone().into(),
            })
            .collect();
        let config = serde_json::json!({
            "name": meta.name, "display_name": meta.display_name, "version": meta.version.to_string(),
            "description": meta.description.clone().unwrap_or_default(), "authors": authors,
            "tags": meta.tags, "champions": meta.champions, "maps": meta.maps, "layers": [base()],
        });
        (meta.name.clone(), config, None)
    };
    // LTK leaves empty lists out.
    if let Some(map) = config.as_object_mut() {
        map.retain(|key, value| key == "authors" || value.as_array().is_none_or(|list| !list.is_empty()));
    }
    Ok(ModFiles { project_name: name, config: serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?, thumbnail })
}

/// Lowercase ASCII letters and digits joined by single hyphens, like the
/// `slug` crate LTK Manager uses.
// shortcut: non-ASCII letters are dropped where `slug` transliterates them; a slug is only a folder name.
pub fn slugify(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Korea/China gating from the active account's platform/region.
pub fn region_blocked(platform: Option<&str>, region: Option<&str>) -> bool {
    let hit = |value: Option<&str>| {
        value.is_some_and(|v| {
            let v = v.trim().to_uppercase();
            v == "KR" || v == "CN" || v.starts_with("CN") || v == "KOREA" || v == "CHINA"
        })
    };
    hit(platform) || hit(region)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn fantome_package_is_accepted() {
        let dir = std::env::temp_dir().join(format!("swapper-mods-fantome-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cool.fantome");
        let info = serde_json::json!({"Name":"Cool Skin","Author":"Ada","Version":"1.2.3","Description":"","Champions":["Ahri"],"Tags":["champion-skin"]});
        write_zip(&path, &[
            ("META/info.json", info.to_string().as_bytes()),
            ("WAD/Ahri.wad.client/aaaa.bin", b"chunk"),
        ]);
        let parsed = validate_package(&path).expect("valid fantome");
        assert_eq!(parsed.name, "Cool Skin");
        assert_eq!(parsed.author, "Ada");
        assert_eq!(parsed.slot, Slot::Skin("Ahri".to_string()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn renamed_exe_and_plain_zip_are_rejected() {
        let dir = std::env::temp_dir().join(format!("swapper-mods-reject-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("evil.fantome");
        fs::write(&exe, b"MZ\x90\x00 not a zip").unwrap();
        assert!(validate_package(&exe).is_err(), "renamed exe must be rejected");
        let zip_path = dir.join("plain.fantome");
        write_zip(&zip_path, &[("readme.txt", b"hello")]);
        assert!(validate_package(&zip_path).is_err(), "plain zip must be rejected");
        let txt = dir.join("notes.txt");
        fs::write(&txt, b"hi").unwrap();
        assert!(validate_package(&txt).is_err(), "other extensions must be rejected");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn korea_and_china_are_blocked() {
        assert!(region_blocked(Some("KR"), Some("KR")));
        assert!(region_blocked(Some("CN1"), None));
        assert!(region_blocked(None, Some("China")));
        assert!(!region_blocked(Some("EUW1"), Some("EUW")));
        assert!(!region_blocked(None, None));
    }
}
