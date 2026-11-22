//! Korean font and custom mods, applied by LTK Manager. Swapper validates
//! packages, hands them to LTK Manager and turns LTK's mods on and off. See
//! `ltk` for the handoff, `library` for validation and slots and `patcher`
//! for finding and installing LTK Manager.

pub mod library;
pub mod ltk;
pub mod patcher;
pub mod runeforge;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use ltk::{LtkMod, Op};

fn custom_enabled(app: &AppHandle) -> bool {
    app.try_state::<crate::AppState>().is_some_and(|s| s.custom_mods_enabled())
}

/// KR/CN block from the active account's platform/region. The Korean font
/// is unaffected; only custom mods are gated.
pub fn active_region_blocked(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<crate::AppState>() else { return false };
    let Ok(config) = state.config.lock() else { return false };
    let active = config.accounts.iter().find(|a| Some(a.id) == config.active_id);
    active.is_some_and(|a| library::region_blocked(a.platform.as_deref(), a.region.as_deref()))
}

// ---- Korean font ----

/// How the bundled Korean font is recognised in LTK's library.
struct KoreanFont {
    sha256: String,
    name: String,
}

fn korean_font(app: &AppHandle) -> Option<&'static KoreanFont> {
    static FONT: OnceLock<Option<KoreanFont>> = OnceLock::new();
    FONT.get_or_init(|| {
        let path = &app.try_state::<crate::AppState>()?.bundled_korean_font;
        Some(KoreanFont {
            sha256: library::sha256_hex(&std::fs::read(path).ok()?),
            name: library::validate_package(path).ok()?.name,
        })
    })
    .as_ref()
}

fn is_korean(entry: &LtkMod, font: Option<&KoreanFont>) -> bool {
    font.is_some_and(|font| entry.sha256.as_deref() == Some(font.sha256.as_str()) || entry.name.eq_ignore_ascii_case(&font.name))
        || entry.slug.as_deref() == Some("korean-font")
}

/// Korean font on: hand the bundled font to LTK unless LTK already has it,
/// and make sure it is enabled. Off: disable it in LTK. Blocking.
pub fn korean_font_changed(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if patcher::manager_path().is_none() {
        return Ok(());
    }
    let font = korean_font(app);
    match ltk::planned_mods().into_iter().find(|m| is_korean(m, font)) {
        Some(entry) => ltk::queue(if enabled { Op::Enable(entry.id) } else { Op::Disable(entry.id) }),
        None if enabled => {
            let state = app.try_state::<crate::AppState>().ok_or("Swapper is not ready.")?;
            let file = state.bundled_korean_font.clone();
            let bytes = std::fs::read(&file).map_err(|e| format!("Couldn't open the Korean font: {e}"))?;
            let info = library::validate_package(&file)?;
            ltk::queue(Op::Import(Box::new(ltk::Import { id: uuid::Uuid::new_v4().to_string(), sha256: library::sha256_hex(&bytes), file, info })));
        }
        None => return Ok(()),
    }
    ltk::apply(app)
}

// ---- Mods Swapper handed to LTK ----

/// The custom mods Swapper added to LTK (`<data_root>/mods/handed.json`).
/// Custom mods off only ever touches these, never the user's own.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handed {
    #[serde(default)]
    pub mods: Vec<HandedMod>,
    /// Set while Custom mods are off or blocked: which of them were on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suspended: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandedMod {
    pub id: String,
    pub sha256: String,
}

impl Handed {
    fn owns(&self, entry: &LtkMod) -> bool {
        self.mods.iter().any(|h| h.id == entry.id || entry.sha256.as_deref() == Some(h.sha256.as_str()))
    }
}

fn handed_path(root: &Path) -> PathBuf {
    root.join("mods").join("handed.json")
}

pub fn load_handed(root: &Path) -> Handed {
    std::fs::read_to_string(handed_path(root)).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

pub fn save_handed(root: &Path, handed: &Handed) -> Result<(), String> {
    let path = handed_path(root);
    let text = serde_json::to_string_pretty(handed).map_err(|e| e.to_string())?;
    path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&path, text)).map_err(|e| e.to_string())
}

/// What Custom mods being `allowed` or not means for Swapper's mods: the
/// record to save and the changes to queue, or `None` when nothing changes.
/// Off remembers which were on and turns them off; on turns those back on.
pub fn custom_sync(handed: &Handed, mods: &[LtkMod], allowed: bool) -> Option<(Handed, Vec<Op>)> {
    let on = || mods.iter().filter(|m| m.enabled && handed.owns(m)).map(|m| m.id.clone());
    match (&handed.suspended, allowed) {
        (None, false) => {
            let was_on: Vec<String> = on().collect();
            let ops = was_on.iter().cloned().map(Op::Disable).collect();
            Some((Handed { suspended: Some(was_on), ..handed.clone() }, ops))
        }
        // Still off after a restart: anything of ours turned on meanwhile goes off again.
        (Some(_), false) => Some((handed.clone(), on().map(Op::Disable).collect::<Vec<_>>())).filter(|(_, ops)| !ops.is_empty()),
        (Some(was_on), true) => Some((Handed { suspended: None, ..handed.clone() }, was_on.iter().cloned().map(Op::Restore).collect())),
        (None, true) => None,
    }
}

/// Whether Custom mods were allowed at the last `sync_custom`.
static LAST_ALLOWED: Mutex<Option<bool>> = Mutex::new(None);

/// Suspends Swapper's mods in LTK when Custom mods are turned off or the
/// active account is KR/CN, and restores them when allowed again. Run by
/// the LTK worker; changes queue like any other and apply after a game.
pub fn sync_custom(app: &AppHandle) {
    if patcher::manager_path().is_none() {
        return;
    }
    let allowed = custom_enabled(app) && !active_region_blocked(app);
    let mut last = LAST_ALLOWED.lock().unwrap_or_else(|e| e.into_inner());
    if *last == Some(allowed) {
        return;
    }
    let Ok(root) = crate::vault::data_root() else { return };
    if let Some((next, ops)) = custom_sync(&load_handed(&root), &ltk::planned_mods(), allowed) {
        if let Err(error) = save_handed(&root, &next) {
            // Tried again next tick.
            eprintln!("Custom mods record: {error}");
            return;
        }
        ops.into_iter().for_each(ltk::queue);
    }
    *last = Some(allowed);
}

// ---- Library commands ----

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModView {
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub category: Option<String>,
    pub champion: Option<String>,
    pub slot_label: String,
    pub enabled: bool,
    pub source: Option<String>,
    pub licence: Option<String>,
    pub page_url: Option<String>,
    pub runeforge_mod_id: Option<String>,
    /// LTK Manager's health check: "healthy", "repairable", "unrepairable", or none yet.
    pub health: Option<String>,
    pub fatals: u32,
    pub errors: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModsView {
    pub mods: Vec<ModView>,
    pub region_blocked: bool,
    pub in_game: bool,
    pub applies_next_game: bool,
    /// A change waits for the user to close LTK Manager.
    pub needs_close: bool,
}

/// LTK's mods (without the Korean font, which has its own setting), as they
/// will be once queued changes apply.
fn view(app: &AppHandle) -> Result<ModsView, String> {
    let root = crate::vault::data_root()?;
    let sources = library::load_runeforge(&root);
    let font = korean_font(app);
    let health = ltk::health();
    let mods = ltk::planned_mods()
        .into_iter()
        .filter(|m| !is_korean(m, font))
        .map(|m| {
            let rf = m.sha256.as_ref().and_then(|sha| sources.get(sha));
            let checked = health.get(&m.id);
            ModView {
                health: checked.map(|h| h.verdict.clone()),
                fatals: checked.map_or(0, |h| h.fatals),
                errors: checked.map_or(0, |h| h.errors),
                slot_label: m.slot.label(),
                source: rf.map(|_| "runeforge".to_string()),
                licence: rf.and_then(|r| r.licence.clone()),
                page_url: rf.and_then(|r| r.page_url.clone()),
                runeforge_mod_id: rf.map(|r| r.mod_id.clone()),
                id: m.id,
                name: m.name,
                author: m.author,
                version: m.version,
                category: m.category,
                champion: m.champion,
                enabled: m.enabled,
            }
        })
        .collect();
    let status = ltk::status();
    Ok(ModsView {
        mods,
        region_blocked: active_region_blocked(app),
        in_game: ltk::in_game(),
        applies_next_game: status == ltk::Status::NextGame,
        needs_close: status == ltk::Status::NeedsClose,
    })
}

/// Custom mods need the setting on, a non-KR/CN account and LTK Manager.
fn guard(app: &AppHandle) -> Result<(), String> {
    if !custom_enabled(app) {
        return Err("Turn on Custom mods in Settings first.".to_string());
    }
    if active_region_blocked(app) {
        return Err("Custom mods aren't available in this region.".to_string());
    }
    if patcher::manager_path().is_none() {
        return Err("Install LTK Manager in Settings first.".to_string());
    }
    Ok(())
}

/// Queues `ops`, applies what can be applied now and returns the new view.
async fn change(app: AppHandle, ops: Vec<Op>) -> Result<ModsView, String> {
    guard(&app)?;
    tokio::task::spawn_blocking(move || {
        for op in ops {
            ltk::queue(op);
        }
        ltk::apply(&app)?;
        view(&app)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn mods_list(app: AppHandle) -> Result<ModsView, String> {
    view(&app)
}

/// Shared by "Add from file" and RuneForge installs: a copy of the validated
/// package waits in `incoming` until Swapper can add it to LTK's library.
fn import_validated(app: &AppHandle, path: &Path, info: library::PackageInfo) -> Result<ModsView, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't open that file: {e}"))?;
    let sha256 = library::sha256_hex(&bytes);
    let incoming = ltk::incoming_dir()?;
    let file = incoming.join(format!("{sha256}.{}", info.format));
    std::fs::create_dir_all(&incoming).and_then(|()| std::fs::write(&file, &bytes)).map_err(|e| {
        eprintln!("Mod copy failed: {e}");
        "Couldn't add that mod. Check disk space and try again.".to_string()
    })?;
    let id = uuid::Uuid::new_v4().to_string();
    // One LTK already holds is the user's (or already recorded), so it isn't recorded as ours.
    if !ltk::planned_mods().iter().any(|m| m.sha256.as_deref() == Some(sha256.as_str())) {
        let root = crate::vault::data_root()?;
        let mut handed = load_handed(&root);
        handed.mods.push(HandedMod { id: id.clone(), sha256: sha256.clone() });
        save_handed(&root, &handed)?;
    }
    ltk::queue(Op::Import(Box::new(ltk::Import { id, sha256, file, info })));
    ltk::apply(app)?;
    view(app)
}

#[tauri::command]
pub async fn mods_add(app: AppHandle, path: String) -> Result<ModsView, String> {
    guard(&app)?;
    tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&path);
        let info = library::validate_package(&path)?;
        import_validated(&app, &path, info)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- RuneForge commands ----

#[tauri::command]
pub async fn runeforge_list(
    page: u32,
    search: Option<String>,
    category: Option<String>,
    champion_id: Option<u64>,
    sort: Option<String>,
) -> Result<runeforge::RfList, String> {
    runeforge::list_mods(&runeforge::ListQuery {
        page,
        search: search.unwrap_or_default(),
        category: category.filter(|c| !c.is_empty()),
        champion_id,
        sort: sort.unwrap_or_else(|| "trending".to_string()),
    })
    .await
}

#[tauri::command]
pub async fn runeforge_mod(slug: String) -> Result<runeforge::RfDetail, String> {
    runeforge::mod_detail(&slug).await
}

#[tauri::command]
pub async fn runeforge_thumbnail(url: String) -> Result<String, String> {
    let root = crate::vault::data_root()?;
    runeforge::thumbnail_data_url(&root, &url).await
}

/// A library mod's thumbnail as a data URL (the CSP blocks file:// images):
/// LTK's own from the mod's folder, else the RuneForge one it was installed
/// from (cached), else none.
#[tauri::command]
pub async fn mods_thumbnail(id: String) -> Result<Option<String>, String> {
    let root = crate::vault::data_root()?;
    let lookup = root.clone();
    let (own, url) = tokio::task::spawn_blocking(move || match ltk::thumbnail(&id) {
        Some(bytes) => (Some(bytes), None),
        None => {
            let sources = library::load_runeforge(&lookup);
            let sha = ltk::planned_mods().into_iter().find(|m| m.id == id).and_then(|m| m.sha256);
            (None, sha.and_then(|sha| sources.get(&sha)?.thumbnail_url.clone()))
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Some(bytes) = own {
        return Ok(Some(runeforge::data_url(&bytes)));
    }
    let Some(url) = url else { return Ok(None) };
    Ok(runeforge::thumbnail_data_url(&root, &url).await.ok())
}

/// Opens a RuneForge page in the default browser (runeforge.dev only).
#[tauri::command]
pub fn runeforge_open(url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|_| "Couldn't open that link. Try again.".to_string())?;
    let host_ok = matches!(parsed.host_str(), Some(h) if h == "runeforge.dev" || h.ends_with(".runeforge.dev"));
    if parsed.scheme() != "https" || !host_ok {
        return Err("Couldn't open that link. Try again.".to_string());
    }
    crate::commands::guide::open_in_browser(&parsed)
}

#[tauri::command]
pub async fn runeforge_install(
    app: AppHandle,
    mod_id: String,
    slug: String,
    release_id: Option<String>,
) -> Result<ModsView, String> {
    guard(&app)?;
    let detail = runeforge::mod_detail(&slug).await?;
    if detail.id != mod_id {
        return Err(runeforge::UNREACHABLE.to_string());
    }
    if runeforge::is_skinhack(&detail.tags) {
        return Err("That mod is a skinhack and can't be installed.".to_string());
    }
    let release = match release_id.as_deref().filter(|r| !r.is_empty()) {
        Some(wanted) => detail
            .releases
            .iter()
            .find(|r| r.id == wanted)
            .ok_or_else(|| runeforge::UNREACHABLE.to_string())?,
        None => detail
            .releases
            .iter()
            .find(|r| Some(&r.id) == detail.latest_release_id.as_ref())
            .or_else(|| detail.releases.first())
            .ok_or_else(|| "That mod has no downloadable release.".to_string())?,
    };
    let artifact = release
        .artifacts
        .iter()
        .find(|a| a.primary)
        .or_else(|| release.artifacts.iter().find(|a| a.format == "fantome" || a.format == "modpkg"))
        .or_else(|| release.artifacts.first())
        .ok_or_else(|| "That mod has no downloadable file.".to_string())?;
    let temp = runeforge::download_to_temp(&detail, release, artifact).await?;
    let meta = library::RuneForgeMeta {
        mod_id: detail.id.clone(),
        release_id: Some(release.id.clone()),
        game_patch: release.game_patch.clone(),
        licence: detail.licence.clone(),
        page_url: Some(detail.page_url.clone()),
        thumbnail_url: detail.thumbnail_url.clone(),
    };
    tokio::task::spawn_blocking(move || {
        let result = (|| {
            let info = library::validate_package(&temp)?;
            let root = crate::vault::data_root()?;
            library::save_runeforge(&root, &library::sha256_hex(&std::fs::read(&temp).map_err(|e| e.to_string())?), meta)?;
            import_validated(&app, &temp, info)
        })();
        // The import keeps its own copy.
        let _ = std::fs::remove_file(&temp);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mods_set_enabled(app: AppHandle, id: String, enabled: bool) -> Result<ModsView, String> {
    change(app, vec![if enabled { Op::Enable(id) } else { Op::Disable(id) }]).await
}


#[tauri::command]
pub async fn mods_remove(app: AppHandle, id: String) -> Result<ModsView, String> {
    change(app, vec![Op::Remove(id)]).await
}

#[tauri::command]
pub async fn mods_disable_all(app: AppHandle) -> Result<ModsView, String> {
    let font = korean_font(&app);
    let ops = ltk::planned_mods()
        .into_iter()
        .filter(|m| m.enabled && !is_korean(m, font))
        .map(|m| Op::Disable(m.id))
        .collect();
    change(app, ops).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use library::Slot;

    fn entry(slug: &str, name: &str, sha: Option<&str>) -> LtkMod {
        LtkMod {
            id: slug.into(),
            slug: Some(slug.into()),
            format: "fantome".into(),
            name: name.into(),
            author: String::new(),
            version: String::new(),
            category: None,
            champion: None,
            slot: Slot::Other,
            enabled: true,
            sha256: sha.map(str::to_string),
        }
    }

    #[test]
    fn the_korean_font_is_found_by_hash_name_or_slug() {
        let font = KoreanFont { sha256: "f00".into(), name: "Korean Font".into() };
        assert!(is_korean(&entry("x", "Other", Some("f00")), Some(&font)));
        assert!(is_korean(&entry("x", "korean font", None), Some(&font)));
        assert!(is_korean(&entry("korean-font", "Renamed", None), None));
        assert!(!is_korean(&entry("ahri", "Ahri", Some("abc")), Some(&font)));
    }

    #[test]
    fn the_handed_record_round_trips_and_owns_by_id_or_hash() {
        let root = std::env::temp_dir().join(format!("swapper-handed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(load_handed(&root), Handed::default(), "no record yet");
        let handed = Handed {
            mods: vec![HandedMod { id: "a".into(), sha256: "sha-a".into() }],
            suspended: Some(vec!["a".into()]),
        };
        save_handed(&root, &handed).unwrap();
        assert_eq!(load_handed(&root), handed);
        assert!(handed.owns(&entry("a", "A", None)));
        assert!(handed.owns(&entry("re-adopted", "A", Some("sha-a"))));
        assert!(!handed.owns(&entry("users", "Theirs", Some("sha-x"))));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn custom_mods_off_suspends_only_swappers_mods_and_on_restores_them() {
        let handed = Handed {
            mods: vec![HandedMod { id: "ours-on".into(), sha256: "1".into() }, HandedMod { id: "ours-off".into(), sha256: "2".into() }],
            suspended: None,
        };
        let mut ours_off = entry("ours-off", "Ours off", Some("2"));
        ours_off.enabled = false;
        let mods = vec![entry("ours-on", "Ours on", Some("1")), ours_off, entry("users", "Theirs", Some("3")), entry("korean-font", "Korean Font", None)];

        assert_eq!(custom_sync(&handed, &mods, true), None, "allowed and nothing suspended");
        let (off, ops) = custom_sync(&handed, &mods, false).unwrap();
        assert_eq!(ops, vec![Op::Disable("ours-on".into())], "the user's mods and the font stay");
        assert_eq!(off.suspended, Some(vec!["ours-on".to_string()]));

        // After the disable applied, a restart while still off changes nothing.
        let mut applied = ltk::plan(&mods, &ops).mods;
        assert_eq!(custom_sync(&off, &applied, false), None);
        // One of ours turned back on meanwhile goes off again.
        applied[1].enabled = true;
        assert_eq!(custom_sync(&off, &applied, false).unwrap().1, vec![Op::Disable("ours-off".into())]);

        let (on, ops) = custom_sync(&off, &ltk::plan(&mods, &[Op::Disable("ours-on".into())]).mods, true).unwrap();
        assert_eq!(ops, vec![Op::Restore("ours-on".into())], "only what was on comes back");
        assert_eq!(on, handed);
    }
}
