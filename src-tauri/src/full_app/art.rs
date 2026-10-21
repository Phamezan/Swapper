//! Champion splash and tile art for the Fullsized App, read from the League
//! client's game data and kept on disk (`%LOCALAPPDATA%\Swapper\art`), so the
//! CSP stays local-only and a restart without League still shows the art.
use base64::Engine;
use serde_json::Value;

/// A centred splash is ~100 KB and a tile ~40 KB; anything bigger is refused.
const MAX_BYTES: u64 = 1024 * 1024;
const ASSET_PREFIX: &str = "/lol-game-data/assets/";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Splash,
    Tile,
    /// The tall loading-screen portrait (~308x560, ~60 KB).
    Loading,
}

impl Kind {
    pub fn parse(kind: &str) -> Option<Self> {
        match kind {
            "splash" => Some(Kind::Splash),
            "tile" => Some(Kind::Tile),
            "loading" => Some(Kind::Loading),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Splash => "splash",
            Kind::Tile => "tile",
            Kind::Loading => "loading",
        }
    }

    fn field(self) -> &'static str {
        match self {
            Kind::Splash => "splashPath",
            Kind::Tile => "tilePath",
            Kind::Loading => "loadScreenPath",
        }
    }
}

/// Only numeric ids and a fixed kind name reach the file name.
pub fn file_name(id: i64, kind: Kind) -> Option<String> {
    (id > 0).then(|| format!("{}-{id}.jpg", kind.name()))
}

/// The base skin's art path from `/lol-game-data/assets/v1/champions/{id}.json`,
/// accepted only inside the game-data asset tree.
pub fn base_skin_path(champion: &Value, kind: Kind) -> Option<String> {
    let path = champion["skins"].as_array()?.first()?[kind.field()].as_str()?;
    (path.starts_with(ASSET_PREFIX) && !path.contains("..")).then(|| path.to_string())
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() as u64 <= MAX_BYTES && bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

fn read_cached(path: &std::path::Path) -> Option<Vec<u8>> {
    if std::fs::metadata(path).ok()?.len() > MAX_BYTES {
        return None;
    }
    std::fs::read(path).ok().filter(|bytes| is_jpeg(bytes))
}

/// Art loads come in bursts (a grid of portraits); a few at a time keeps the
/// League Client's connection limit free for picks and the draft.
static ART_FETCHES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

async fn fetch(id: i64, kind: Kind) -> Result<Vec<u8>, String> {
    let _permit = ART_FETCHES.acquire().await.map_err(|e| e.to_string())?;
    let client = crate::runes::lcu().await.map_err(|e| e.to_string())?;
    let champion: Value =
        crate::runes::lcu_get(&client, &format!("/lol-game-data/assets/v1/champions/{id}.json"))
            .await
            .map_err(|e| e.to_string())?;
    let path = base_skin_path(&champion, kind).ok_or("Champion art is unavailable.")?;
    let bytes = crate::runes::lcu_get_bytes(&client, &path)
        .await
        .map_err(|e| e.to_string())?;
    is_jpeg(&bytes)
        .then_some(bytes)
        .ok_or_else(|| "Champion art is not a JPEG.".into())
}

#[tauri::command]
pub async fn champion_art(id: i64, kind: String) -> Result<String, String> {
    let kind = Kind::parse(&kind).ok_or("Unknown art kind.")?;
    let name = file_name(id, kind).ok_or("Unknown champion.")?;
    let path = crate::vault::data_root()?.join("art").join(name);
    let cached = {
        let path = path.clone();
        tokio::task::spawn_blocking(move || read_cached(&path))
            .await
            .unwrap_or(None)
    };
    let bytes = match cached {
        Some(bytes) => bytes,
        None => {
            let bytes = fetch(id, kind).await?;
            let copy = bytes.clone();
            // Best effort: a failed write only costs a refetch next time.
            let _ = tokio::task::spawn_blocking(move || {
                let parent = path.parent()?;
                std::fs::create_dir_all(parent).ok()?;
                let pending = path.with_extension("tmp");
                std::fs::write(&pending, copy).ok()?;
                std::fs::rename(pending, path).ok()
            })
            .await;
            bytes
        }
    };
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_numeric_ids_and_known_kinds_name_files() {
        assert_eq!(file_name(266, Kind::Splash).as_deref(), Some("splash-266.jpg"));
        assert_eq!(file_name(0, Kind::Tile), None);
        assert_eq!(file_name(-3, Kind::Tile), None);
        assert_eq!(Kind::parse("tile"), Some(Kind::Tile));
        assert_eq!(file_name(103, Kind::Loading).as_deref(), Some("loading-103.jpg"));
        assert_eq!(Kind::parse("../splash"), None);
    }

    #[test]
    fn art_paths_must_stay_inside_game_data_assets() {
        let champion = |path: &str| json!({"skins": [{"splashPath": path, "tilePath": path}]});
        let ok = "/lol-game-data/assets/ASSETS/Characters/Aatrox/Skins/Base/Images/aatrox_splash_centered_0.jpg";
        assert_eq!(base_skin_path(&champion(ok), Kind::Splash).as_deref(), Some(ok));
        assert_eq!(base_skin_path(&champion("/lol-summoner/v1/current-summoner"), Kind::Tile), None);
        assert_eq!(base_skin_path(&champion("/lol-game-data/assets/../../riotclient"), Kind::Tile), None);
        assert_eq!(base_skin_path(&json!({"skins": []}), Kind::Tile), None);
    }

    #[test]
    fn cache_accepts_only_bounded_jpegs() {
        assert!(is_jpeg(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(!is_jpeg(b"<html>"));
        assert!(!is_jpeg(&vec![0xFF; MAX_BYTES as usize + 1]));
        let path = std::env::temp_dir().join(format!("art-test-{}.jpg", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"not a jpeg").unwrap();
        assert!(read_cached(&path).is_none());
        std::fs::write(&path, [0xFF, 0xD8, 0xFF, 0xDB]).unwrap();
        assert!(read_cached(&path).is_some());
        std::fs::remove_file(path).unwrap();
    }
}
