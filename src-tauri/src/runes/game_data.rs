//! The League client's game data (`/lol-game-data/assets/...`), available with
//! League Client closed.
//!
//! Every catalog (runes, champions, items, summoner spells, per-champion files)
//! and every icon the app reads from the client's game-data plugin goes through
//! here, so one place decides the source:
//!
//! - JSON: the League Client, then the last copy on disk, then CommunityDragon.
//!   The client goes first so catalogs match its patch and language; callers
//!   keep catalogs in memory, so this runs about once an hour per file.
//! - Images: disk, then the League Client, then CommunityDragon. An asset
//!   path's bytes only change with a patch and the disk copy is cleared when the
//!   patch changes, so a disk hit just saves a client round trip. The client
//!   answering "no such asset" is final, so a running client never causes a CDN
//!   request.
//!
//! The disk copy lives in `%LOCALAPPDATA%\Swapper\game-data`, one file per asset
//! path, written atomically, cleared on a new patch and capped in total size
//! (oldest files go first). A missing, oversized or invalid file is a miss.
//! CommunityDragon mirrors the plugin 1:1 under `rcp-be-lol-game-data/global/default`
//! (in lower case). Its answers are untrusted: size-capped while downloading and
//! checked to be what the path says (JSON parses, PNG/JPEG magic bytes); callers
//! then parse JSON into their own typed shapes.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use serde::de::DeserializeOwned;

use super::RuneError;

const LCU_PREFIX: &str = "/lol-game-data/assets/";
// `latest` tracks live; a patch-pinned folder 404s for the first hours of a patch.
const CDRAGON_BASE: &str =
    "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default";
/// `items.json` is the largest catalog at well under this.
const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;
/// A centred splash is ~100 KB; icons are a few KB.
const MAX_IMAGE_BYTES: usize = 1024 * 1024;
/// Every champion's file and icons plus the catalogs fit several times over.
const MAX_DIR_BYTES: u64 = 128 * 1024 * 1024;
const MAX_RELATIVE_LEN: usize = 200;
const CDN_TIMEOUT: Duration = Duration::from_secs(6);
const PATCH_MARKER: &str = "patch";

/// A cold offline grid asks for a hundred icons at once; this keeps it polite.
static CDN_FETCHES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Disk,
    Lcu,
    Cdn,
}

/// A game-data JSON file, parsed.
pub(crate) async fn json<T: DeserializeOwned>(path: &str) -> Result<T, RuneError> {
    let text = text(path).await?;
    serde_json::from_str(&text).map_err(|_| RuneError::unavailable("Game data was unexpected."))
}

/// A game-data JSON file as text: client, disk, then CommunityDragon.
pub(crate) async fn text(path: &str) -> Result<String, RuneError> {
    sweep_once();
    let relative = relative(path);
    let lcu = async {
        let lcu = super::lcu().await?;
        super::lcu_get_text(&lcu, path).await.map(String::into_bytes)
    };
    let (bytes, source) = json_order(lcu, read(relative.clone()), cdn(relative.clone())).await?;
    let text = String::from_utf8(bytes).map_err(|_| RuneError::unavailable("Game data was unreadable."))?;
    if source != Source::Disk {
        write(relative, text.as_bytes().to_vec());
    }
    Ok(text)
}

/// A game-data image: disk, client, then CommunityDragon.
pub(crate) async fn bytes(path: &str) -> Result<Vec<u8>, RuneError> {
    sweep_once();
    let relative = relative(path);
    let (bytes, source) = image_order(read(relative.clone()), lcu_bytes(path), cdn(relative.clone())).await?;
    if source != Source::Disk {
        write(relative, bytes.clone());
    }
    Ok(bytes)
}

/// A game-data image from the client or CommunityDragon, without the disk copy,
/// for callers that keep their own.
pub(crate) async fn fetch_bytes(path: &str) -> Result<Vec<u8>, RuneError> {
    image_order(async { None }, lcu_bytes(path), cdn(relative(path)))
        .await
        .map(|(bytes, _)| bytes)
}

async fn lcu_bytes(path: &str) -> Result<Vec<u8>, RuneError> {
    let lcu = super::lcu().await?;
    super::lcu_get_bytes(&lcu, path).await
}

// Futures are lazy, so a source after the first hit is never polled.
async fn json_order(
    lcu: impl Future<Output = Result<Vec<u8>, RuneError>>,
    disk: impl Future<Output = Option<Vec<u8>>>,
    cdn: impl Future<Output = Result<Vec<u8>, RuneError>>,
) -> Result<(Vec<u8>, Source), RuneError> {
    if let Ok(bytes) = lcu.await {
        return Ok((bytes, Source::Lcu));
    }
    if let Some(bytes) = disk.await {
        return Ok((bytes, Source::Disk));
    }
    cdn.await.map(|bytes| (bytes, Source::Cdn))
}

async fn image_order(
    disk: impl Future<Output = Option<Vec<u8>>>,
    lcu: impl Future<Output = Result<Vec<u8>, RuneError>>,
    cdn: impl Future<Output = Result<Vec<u8>, RuneError>>,
) -> Result<(Vec<u8>, Source), RuneError> {
    if let Some(bytes) = disk.await {
        return Ok((bytes, Source::Disk));
    }
    match lcu.await {
        Ok(bytes) => return Ok((bytes, Source::Lcu)),
        Err(error @ RuneError::NotFound(_)) => return Err(error),
        Err(_) => {}
    }
    cdn.await.map(|bytes| (bytes, Source::Cdn))
}

/// The lower-case path below the plugin root, or `None` when the path is not a
/// plain game-data asset. Only this form reaches a file name or a CDN URL.
fn relative(path: &str) -> Option<String> {
    let rest = path.trim().strip_prefix(LCU_PREFIX)?.to_ascii_lowercase();
    let safe = !rest.is_empty()
        && rest.len() <= MAX_RELATIVE_LEN
        && rest.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '))
        });
    safe.then_some(rest)
}

fn file_name(relative: &str) -> String {
    relative.replace('/', "+")
}

/// Whether bytes are what the path claims and within its size cap.
fn valid(relative: &str, bytes: &[u8]) -> bool {
    if relative.ends_with(".json") {
        return bytes.len() <= MAX_JSON_BYTES
            && serde_json::from_slice::<serde::de::IgnoredAny>(bytes).is_ok();
    }
    let image = (relative.ends_with(".png") && bytes.starts_with(b"\x89PNG\r\n\x1a\n"))
        || ((relative.ends_with(".jpg") || relative.ends_with(".jpeg"))
            && bytes.starts_with(&[0xFF, 0xD8, 0xFF]));
    image && bytes.len() <= MAX_IMAGE_BYTES
}

fn limit(relative: &str) -> usize {
    if relative.ends_with(".json") {
        MAX_JSON_BYTES
    } else {
        MAX_IMAGE_BYTES
    }
}

fn dir() -> Option<PathBuf> {
    crate::vault::data_root().ok().map(|root| root.join("game-data"))
}

async fn read(relative: Option<String>) -> Option<Vec<u8>> {
    let (relative, dir) = (relative?, dir()?);
    tokio::task::spawn_blocking(move || load(&dir, &relative))
        .await
        .ok()
        .flatten()
}

/// Best effort, off the async runtime: a failed write only costs a refetch.
fn write(relative: Option<String>, bytes: Vec<u8>) {
    let (Some(relative), Some(dir)) = (relative, dir()) else {
        return;
    };
    tauri::async_runtime::spawn_blocking(move || store(&dir, &relative, &bytes));
}

fn load(dir: &Path, relative: &str) -> Option<Vec<u8>> {
    let path = dir.join(file_name(relative));
    if std::fs::metadata(&path).ok()?.len() > limit(relative) as u64 {
        return None;
    }
    std::fs::read(path).ok().filter(|bytes| valid(relative, bytes))
}

fn store(dir: &Path, relative: &str, bytes: &[u8]) {
    if !valid(relative, bytes) || std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let path = dir.join(file_name(relative));
    let pending = dir.join(format!("{}.{}.tmp", file_name(relative), uuid::Uuid::new_v4()));
    if std::fs::write(&pending, bytes).is_err() || std::fs::rename(&pending, &path).is_err() {
        let _ = std::fs::remove_file(pending);
    }
}

fn cdn_client() -> Result<&'static reqwest::Client, RuneError> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    let client = reqwest::Client::builder()
        .connect_timeout(CDN_TIMEOUT)
        .timeout(CDN_TIMEOUT)
        .build()
        .map_err(|_| RuneError::unavailable("Could not reach CommunityDragon."))?;
    Ok(CLIENT.get_or_init(|| client))
}

async fn cdn(relative: Option<String>) -> Result<Vec<u8>, RuneError> {
    let relative = relative.ok_or_else(|| RuneError::not_found("That game asset is unavailable."))?;
    let _permit = CDN_FETCHES
        .acquire()
        .await
        .map_err(|_| RuneError::unavailable("Could not reach CommunityDragon."))?;
    let unavailable = || RuneError::unavailable("Could not reach CommunityDragon.");
    let mut response = cdn_client()?
        .get(format!("{CDRAGON_BASE}/{relative}"))
        .send()
        .await
        .map_err(|_| unavailable())?;
    if !response.status().is_success() {
        return Err(RuneError::not_found("That game asset is unavailable."));
    }
    let cap = limit(&relative);
    if response.content_length().is_some_and(|len| len > cap as u64) {
        return Err(RuneError::not_found("That game asset is too large."));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        body.extend_from_slice(&chunk);
        if body.len() > cap {
            return Err(RuneError::not_found("That game asset is too large."));
        }
    }
    if !valid(&relative, &body) {
        return Err(RuneError::not_found("That game asset was not valid."));
    }
    Ok(body)
}

/// Once per run, in the background: clear the copy when the patch changed and
/// trim it to [`MAX_DIR_BYTES`]. Without a known patch only the trim runs.
fn sweep_once() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        tauri::async_runtime::spawn(async {
            let patch = super::patch::patches().await.map(|patches| patches.current);
            if let Some(dir) = dir() {
                let _ = tokio::task::spawn_blocking(move || sweep(&dir, patch.as_deref(), MAX_DIR_BYTES)).await;
            }
        });
    });
}

fn sweep(dir: &Path, patch: Option<&str>, max_bytes: u64) {
    // Created up front so the first run records its patch and keeps what it writes.
    let _ = std::fs::create_dir_all(dir);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let marker = dir.join(PATCH_MARKER);
    let recorded = std::fs::read_to_string(&marker).ok();
    let new_patch = patch.filter(|patch| recorded.as_deref() != Some(*patch));
    // Without a recorded patch this is the first run, and its files are this patch's.
    let clear = new_patch.is_some() && recorded.is_some();
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() != PATCH_MARKER)
        .filter_map(|entry| {
            let meta = entry.metadata().ok().filter(std::fs::Metadata::is_file)?;
            Some((meta.modified().ok()?, meta.len(), entry.path()))
        })
        .collect();
    if clear {
        for (_, _, path) in &files {
            let _ = std::fs::remove_file(path);
        }
        files.clear();
    }
    files.sort();
    let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
    for (_, len, path) in &files {
        if total <= max_bytes {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total -= len;
        }
    }
    if let Some(patch) = new_patch {
        let _ = std::fs::write(marker, patch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("swapper-game-data-{tag}-{}", uuid::Uuid::new_v4()))
    }

    fn ok(bytes: &[u8]) -> impl Future<Output = Result<Vec<u8>, RuneError>> {
        let bytes = bytes.to_vec();
        async move { Ok(bytes) }
    }

    async fn down() -> Result<Vec<u8>, RuneError> {
        Err(RuneError::unavailable("League Client is not connected."))
    }

    async fn never<T>() -> T {
        panic!("a later source was polled after an earlier one answered")
    }

    #[test]
    fn only_plain_game_data_paths_get_a_relative_form() {
        assert_eq!(
            relative("/lol-game-data/assets/ASSETS/Items/Icons2D/1001_Class_T1_BootsofSpeed.png").as_deref(),
            Some("assets/items/icons2d/1001_class_t1_bootsofspeed.png")
        );
        assert_eq!(relative("/lol-game-data/assets/v1/champions/1.json").as_deref(), Some("v1/champions/1.json"));
        for bad in [
            "",
            "/lol-game-data/assets/",
            "/lol-game-data/assets/../secret.png",
            "/lol-game-data/assets/v1//x.png",
            "/lol-game-data/assets/v1\\x.png",
            "/lol-game-data/assets/v1/x.png?y=1",
            "/lol-summoner/v1/current-summoner",
            "https://example.com/lol-game-data/assets/x.png",
        ] {
            assert!(relative(bad).is_none(), "{bad}");
        }
        assert!(relative(&format!("/lol-game-data/assets/{}.png", "a".repeat(300))).is_none());
        assert_eq!(file_name("v1/champions/1.json"), "v1+champions+1.json");
    }

    #[test]
    fn bytes_must_match_the_path_type() {
        assert!(valid("v1/x.png", PNG));
        assert!(valid("v1/x.jpg", &[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(valid("v1/x.json", br#"{"a":[1]}"#));
        assert!(!valid("v1/x.png", b"<html>404</html>"));
        assert!(!valid("v1/x.jpg", PNG));
        assert!(!valid("v1/x.json", b"{truncated"));
        assert!(!valid("v1/x.svg", b"<svg/>"));
        let huge = [PNG, &vec![0; MAX_IMAGE_BYTES]].concat();
        assert!(!valid("v1/x.png", &huge));
    }

    #[test]
    fn a_stored_file_round_trips_and_a_corrupt_one_is_a_miss() {
        let dir = temp_dir("roundtrip");
        store(&dir, "v1/champion-icons/1.png", PNG);
        assert_eq!(load(&dir, "v1/champion-icons/1.png").as_deref(), Some(PNG));
        // Invalid bytes are never written.
        store(&dir, "v1/champion-icons/2.png", b"not a png");
        assert!(load(&dir, "v1/champion-icons/2.png").is_none());
        // A file corrupted on disk reads as a miss.
        std::fs::write(dir.join(file_name("v1/items.json")), b"{\"cut").unwrap();
        assert!(load(&dir, "v1/items.json").is_none());
        assert!(load(&dir, "v1/missing.png").is_none());
        // No temp files are left behind.
        let leftovers = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).filter(|e| {
            e.file_name().to_string_lossy().ends_with(".tmp")
        });
        assert_eq!(leftovers.count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_new_patch_clears_the_copy_and_the_size_cap_evicts_the_oldest() {
        let dir = temp_dir("sweep");
        store(&dir, "old.png", PNG);
        std::thread::sleep(Duration::from_millis(30));
        store(&dir, "new.png", PNG);
        sweep(&dir, None, PNG.len() as u64);
        assert!(load(&dir, "old.png").is_none(), "oldest evicted first");
        assert!(load(&dir, "new.png").is_some());

        sweep(&dir, Some("16.20"), MAX_DIR_BYTES);
        assert!(load(&dir, "new.png").is_some(), "the first recorded patch keeps this run's files");
        store(&dir, "kept.png", PNG);
        sweep(&dir, Some("16.20"), MAX_DIR_BYTES);
        assert!(load(&dir, "kept.png").is_some(), "same patch keeps");
        sweep(&dir, Some("16.21"), MAX_DIR_BYTES);
        assert!(load(&dir, "kept.png").is_none(), "new patch clears");
        assert_eq!(std::fs::read_to_string(dir.join(PATCH_MARKER)).unwrap(), "16.21");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn images_come_from_disk_then_the_client_then_the_cdn() {
        let (_, source) = image_order(async { Some(PNG.to_vec()) }, never(), never()).await.unwrap();
        assert_eq!(source, Source::Disk);
        let (_, source) = image_order(async { None }, ok(PNG), never()).await.unwrap();
        assert_eq!(source, Source::Lcu);
        let (_, source) = image_order(async { None }, down(), ok(PNG)).await.unwrap();
        assert_eq!(source, Source::Cdn);
        // The running client saying "no such asset" is final.
        let missing = image_order(
            async { None },
            async { Err(RuneError::not_found("That rune asset is unavailable.")) },
            never(),
        )
        .await;
        assert!(matches!(missing, Err(RuneError::NotFound(_))));
    }

    #[tokio::test]
    async fn json_comes_from_the_client_then_disk_then_the_cdn() {
        let (_, source) = json_order(ok(b"[]"), never(), never()).await.unwrap();
        assert_eq!(source, Source::Lcu);
        let (_, source) = json_order(down(), async { Some(b"[]".to_vec()) }, never()).await.unwrap();
        assert_eq!(source, Source::Disk);
        let (_, source) = json_order(down(), async { None }, ok(b"[]")).await.unwrap();
        assert_eq!(source, Source::Cdn);
        assert!(json_order(down(), async { None }, down()).await.is_err());
    }

    /// The real loaders with League Client closed: cold (CDN) then warm (disk).
    /// Points `LOCALAPPDATA` at a temp folder, so run it alone:
    /// `cargo test --lib game_data::tests::loaders -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs the network and League Client closed"]
    async fn loaders_work_with_the_client_closed() {
        if super::super::lcu().await.is_ok() {
            eprintln!("League Client is running; skipping.");
            return;
        }
        let root = temp_dir("offline");
        std::env::set_var("LOCALAPPDATA", &root);
        let started = std::time::Instant::now();
        let catalog = super::super::data::catalog().await.expect("rune catalog");
        let names = super::super::data::champion_names().await.expect("champion names");
        let items = super::super::items::catalog().await.expect("item catalog");
        let spells = super::super::spells::catalog().await.expect("spell catalog");
        assert!(names.len() > 150 && items.names().len() > 100 && spells.len() > 5);
        assert!(catalog.asset_path(8005).is_some(), "Press the Attack");
        super::super::data::icon(8005).await.expect("rune icon");
        super::super::champion_icon::icon(266).await.expect("champion icon");
        super::super::items::icon(1001).await.expect("item icon");
        super::super::spells::icon(4).await.expect("spell icon");
        eprintln!("cold: {:?}", started.elapsed());
        // Writes are queued off-thread; give them a moment, then read straight from disk.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let dir = root.join("Swapper").join("game-data");
        for relative in ["v1/perks.json", "v1/items.json", "v1/champion-icons/266.png"] {
            assert!(load(&dir, relative).is_some(), "{relative} cached");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    /// Fetches real catalogs and icons from CommunityDragon. Run once with
    /// `cargo test --lib game_data::tests::community_dragon -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs the network"]
    async fn community_dragon_serves_the_catalogs_and_icons_offline_needs() {
        for path in [
            "/lol-game-data/assets/v1/perks.json",
            "/lol-game-data/assets/v1/perkstyles.json",
            "/lol-game-data/assets/v1/champion-summary.json",
            "/lol-game-data/assets/v1/items.json",
            "/lol-game-data/assets/v1/summoner-spells.json",
            "/lol-game-data/assets/v1/champions/266.json",
            "/lol-game-data/assets/v1/champion-icons/266.png",
            "/lol-game-data/assets/v1/perk-images/Styles/Precision/PressTheAttack/PressTheAttack.png",
            "/lol-game-data/assets/v1/perk-images/Styles/7201_Precision.png",
            "/lol-game-data/assets/v1/perk-images/StatMods/StatModsAdaptiveForceIcon.png",
            "/lol-game-data/assets/ASSETS/Items/Icons2D/1001_Class_T1_BootsofSpeed.png",
            "/lol-game-data/assets/DATA/Spells/Icons2D/Summoner_flash.png",
        ] {
            let started = std::time::Instant::now();
            let bytes = cdn(relative(path)).await.unwrap_or_else(|e| panic!("{path}: {e}"));
            eprintln!("{path}: {} bytes in {:?}", bytes.len(), started.elapsed());
        }
        // Paths the catalogs point at resolve too: base skin art and an ability icon.
        let champion: serde_json::Value =
            serde_json::from_slice(&cdn(relative("/lol-game-data/assets/v1/champions/266.json")).await.unwrap()).unwrap();
        for path in [
            champion["skins"][0]["tilePath"].as_str().unwrap(),
            champion["skins"][0]["splashPath"].as_str().unwrap(),
            champion["skins"][0]["loadScreenPath"].as_str().unwrap(),
            champion["spells"][0]["abilityIconPath"].as_str().unwrap(),
            champion["passive"]["abilityIconPath"].as_str().unwrap(),
        ] {
            let bytes = cdn(relative(path)).await.unwrap_or_else(|e| panic!("{path}: {e}"));
            eprintln!("{path}: {} bytes", bytes.len());
        }
    }
}
