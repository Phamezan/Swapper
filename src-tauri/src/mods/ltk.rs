//! LTK Manager handoff. LTK Manager >= 1.32's patcher refuses any host but
//! LTK Manager, so Swapper never runs it: imports, on/off and remove edit
//! LTK's storage (`library.json`, `mods/<slug>/` and `mods/<slug>.<ext>`) the
//! way LTK's own import does. LTK reads its files at startup and writes them
//! while running, so they are only edited while LTK Manager is not running:
//! Swapper closes an LTK Manager that only sits in the tray, edits, and starts
//! it in the tray again; one whose window is open must be closed by the user.
//! Nothing changes during a game.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Emitter};

use super::library::{slot_for, PackageInfo, Slot};

const MANAGER_IMAGE: &str = "ltk-manager.exe";
const GAME_IMAGE: &str = "League of Legends.exe";
/// What setup turns on: start in the tray, start the patcher by itself, and
/// start at login (LTK registers that itself when it starts).
pub const SETUP_KEYS: [&str; 4] = ["alwaysStartPatcher", "startInTray", "minimizeToTray", "autoRun"];
/// How long a closed LTK Manager may take to exit before it is ended.
const CLOSE_WAIT: Duration = Duration::from_secs(10);
/// Names Windows refuses for a folder (LTK's list).
const RESERVED_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3",
    "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];
/// How often queued changes are retried.
const TICK: Duration = Duration::from_secs(3);

pub const NEXT_GAME: &str = "Applies next game.";
pub const CLOSE_TO_APPLY: &str = "Close LTK Manager to apply this.";
pub const CLOSE_TO_SET_UP: &str = "Close LTK Manager to finish setup.";
const WRITE_FAILED: &str = "Couldn't update LTK Manager. Try again.";

// ---- Files ----

/// LTK Manager's app data directory, where `settings.json` lives.
pub fn app_data_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("dev.leaguetoolkit.manager"))
}

fn settings_path() -> Option<PathBuf> {
    app_data_dir().map(|dir| dir.join("settings.json"))
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn read_settings() -> Option<Value> {
    read_json(&settings_path()?)
}

/// Where LTK keeps `library.json` and `mods/`: its `modStoragePath` when set,
/// else its app data directory.
pub fn storage_dir_from(app_data: &Path, settings: Option<&Value>) -> PathBuf {
    settings
        .and_then(|s| s.get("modStoragePath"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| app_data.to_path_buf())
}

fn storage_dir() -> Option<PathBuf> {
    Some(storage_dir_from(&app_data_dir()?, read_settings().as_ref()))
}

/// LTK's settings with the setup keys turned on. Every other key and value,
/// and the key order, stay as they were.
pub fn merge_setup(text: &str) -> Result<String, String> {
    let unreadable = || "Couldn't read LTK Manager's settings.".to_string();
    let mut value: Value = serde_json::from_str(text).map_err(|_| unreadable())?;
    let map = value.as_object_mut().ok_or_else(unreadable)?;
    for key in SETUP_KEYS {
        map.insert(key.to_string(), Value::Bool(true));
    }
    serde_json::to_string_pretty(&value).map_err(|_| unreadable())
}

pub fn is_set_up_in(settings: &Value) -> bool {
    SETUP_KEYS.iter().all(|key| settings.get(*key) == Some(&Value::Bool(true)))
}

/// Whether LTK Manager is set up to run in the tray and apply mods every game.
pub fn is_set_up() -> bool {
    read_settings().is_some_and(|settings| is_set_up_in(&settings))
}

/// Writes through a temp file and a rename, like LTK, so a reader never sees half a file.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let temp = path.with_extension("json.swapper-tmp");
    fs::write(&temp, text).and_then(|()| fs::rename(&temp, path)).map_err(|error| {
        eprintln!("LTK write failed for {}: {error}", path.display());
        let _ = fs::remove_file(&temp);
        WRITE_FAILED.to_string()
    })
}

// ---- Library ----

/// One mod in LTK Manager's library, as Swapper shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct LtkMod {
    pub id: String,
    pub slug: Option<String>,
    /// "fantome", "modpkg" or "unknown"; also the archive's extension.
    pub format: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub category: Option<String>,
    pub champion: Option<String>,
    pub slot: Slot,
    /// Enabled in LTK's active profile.
    pub enabled: bool,
    /// SHA-256 of the file it was imported from (`sourceSha256`).
    pub sha256: Option<String>,
}

fn library_path(storage: &Path) -> PathBuf {
    storage.join("library.json")
}

fn str_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn active_profile(doc: &Value) -> Option<&Value> {
    let active = doc.get("activeProfileId")?.as_str()?;
    doc.get("profiles")?.as_array()?.iter().find(|p| p.get("id").and_then(Value::as_str) == Some(active))
}

/// A slug or id used as a file name. LTK's file is not trusted with paths.
fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':'])
}

fn mod_dir(storage: &Path, entry: &LtkMod) -> PathBuf {
    storage.join("mods").join(entry.slug.as_deref().unwrap_or(&entry.id))
}

/// `mods/<slug>.<ext>`, or `archives/<id>.<ext>` for an entry from before slugs.
fn archive_path(storage: &Path, entry: &LtkMod) -> PathBuf {
    match &entry.slug {
        Some(slug) => storage.join("mods").join(format!("{slug}.{}", entry.format)),
        None => storage.join("archives").join(format!("{}.{}", entry.id, entry.format)),
    }
}

/// The mods in `doc` with their names from each `mod.config.json`.
pub fn read_mods(storage: &Path, doc: &Value) -> Vec<LtkMod> {
    let enabled: HashSet<String> = str_list(active_profile(doc).and_then(|p| p.get("enabledMods"))).into_iter().collect();
    let Some(entries) = doc.get("mods").and_then(Value::as_array) else { return Vec::new() };
    entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.to_string();
            let slug = entry.get("slug").and_then(Value::as_str).map(str::to_string);
            let format = entry.get("format").and_then(Value::as_str).unwrap_or("fantome").to_string();
            if !safe_name(&id) || slug.as_deref().is_some_and(|s| !safe_name(s)) || !safe_name(&format) {
                return None;
            }
            let mut found = LtkMod {
                enabled: enabled.contains(&id),
                sha256: entry.get("sourceSha256").and_then(Value::as_str).map(str::to_string),
                id,
                slug,
                format,
                name: String::new(),
                author: String::new(),
                version: String::new(),
                category: None,
                champion: None,
                slot: Slot::Other,
            };
            let config = read_json(&mod_dir(storage, &found).join("mod.config.json")).unwrap_or(Value::Null);
            let text = |key: &str| config.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string();
            found.name = [text("display_name"), text("name")]
                .into_iter()
                .find(|name| !name.is_empty())
                .unwrap_or_else(|| found.slug.clone().unwrap_or_else(|| found.id.clone()));
            found.version = text("version");
            // An author is a name or `{ "name", "role" }`.
            found.author = config
                .get("authors")
                .and_then(Value::as_array)
                .and_then(|authors| authors.first())
                .and_then(|author| author.as_str().or_else(|| author.get("name").and_then(Value::as_str)))
                .unwrap_or("")
                .to_string();
            let tags = str_list(config.get("tags"));
            let champions = str_list(config.get("champions"));
            found.category = tags.first().cloned();
            found.champion = champions.first().cloned();
            found.slot = slot_for(&tags, &champions, &str_list(config.get("maps")), found.category.as_deref());
            Some(found)
        })
        .collect()
}

/// Turns `id` on or off in the active profile only, like LTK's own toggle.
pub fn set_enabled(doc: &mut Value, id: &str, enabled: bool) {
    let Some(active) = doc.get("activeProfileId").and_then(Value::as_str).map(str::to_string) else { return };
    let Some(list) = doc
        .get_mut("profiles")
        .and_then(Value::as_array_mut)
        .and_then(|profiles| profiles.iter_mut().find(|p| p.get("id").and_then(Value::as_str) == Some(active.as_str())))
        .and_then(|profile| profile.get_mut("enabledMods"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    let on = list.iter().any(|v| v.as_str() == Some(id));
    if enabled && !on {
        list.push(Value::String(id.to_string()));
    } else if !enabled {
        list.retain(|v| v.as_str() != Some(id));
    }
}

/// Removes `id` from the index, every profile and every folder, like LTK's uninstall.
pub fn remove_entry(doc: &mut Value, id: &str) {
    let other = |v: &Value| v.as_str() != Some(id);
    if let Some(mods) = doc.get_mut("mods").and_then(Value::as_array_mut) {
        mods.retain(|m| m.get("id").and_then(Value::as_str) != Some(id));
    }
    for profile in doc.get_mut("profiles").and_then(Value::as_array_mut).into_iter().flatten() {
        for key in ["enabledMods", "modOrder"] {
            if let Some(list) = profile.get_mut(key).and_then(Value::as_array_mut) {
                list.retain(other);
            }
        }
        if let Some(states) = profile.get_mut("layerStates").and_then(Value::as_object_mut) {
            states.remove(id);
        }
    }
    for folder in doc.get_mut("folders").and_then(Value::as_array_mut).into_iter().flatten() {
        if let Some(list) = folder.get_mut("modIds").and_then(Value::as_array_mut) {
            list.retain(other);
        }
    }
}

/// Largest thumbnail read from LTK's mod folder.
const MAX_THUMBNAIL_BYTES: u64 = 2 * 1024 * 1024;

/// `entry`'s thumbnail as LTK keeps it beside `mod.config.json`: `thumbnail.webp`,
/// or the `thumbnail.png` older LTK builds and Swapper's import write. Only a
/// plain file (no link) under a plain slug inside `mods/`, at most 2 MB.
fn thumbnail_in(storage: &Path, entry: &LtkMod) -> Option<Vec<u8>> {
    use std::io::Read as _;
    if !safe_name(entry.slug.as_deref().unwrap_or(&entry.id)) {
        return None;
    }
    let dir = mod_dir(storage, entry);
    ["thumbnail.webp", "thumbnail.png"].into_iter().find_map(|name| {
        let path = dir.join(name);
        if !fs::symlink_metadata(&path).ok()?.is_file() {
            return None;
        }
        let mut bytes = Vec::new();
        fs::File::open(&path).ok()?.take(MAX_THUMBNAIL_BYTES + 1).read_to_end(&mut bytes).ok()?;
        (!bytes.is_empty() && bytes.len() as u64 <= MAX_THUMBNAIL_BYTES).then_some(bytes)
    })
}

/// The thumbnail of the library mod `id`, if LTK has one for it.
pub fn thumbnail(id: &str) -> Option<Vec<u8>> {
    let storage = storage_dir()?;
    let doc = read_json(&library_path(&storage))?;
    let entry = read_mods(&storage, &doc).into_iter().find(|m| m.id == id)?;
    thumbnail_in(&storage, &entry)
}

/// LTK Manager's own health check of one mod, from `mod-health-verdicts.json`
/// beside `library.json` (LTK's `StoredVerdictFile`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Health {
    /// "healthy", "repairable" or "unrepairable" (LTK's `ModHealth`).
    pub verdict: String,
    pub fatals: u32,
    pub errors: u32,
}

/// The verdicts in LTK's file by mod id. Read only; an entry of any other
/// shape is left out, so a missing file, entry or field reads as unknown.
pub fn health_from(doc: &Value) -> std::collections::HashMap<String, Health> {
    let count = |counts: Option<&Value>, key: &str| {
        counts.and_then(|c| c.get(key)).and_then(Value::as_u64).map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX))
    };
    let Some(verdicts) = doc.get("verdicts").and_then(Value::as_object) else { return Default::default() };
    verdicts
        .iter()
        .filter_map(|(id, verdict)| {
            let health = verdict.get("health")?.as_str()?;
            if !["healthy", "repairable", "unrepairable"].contains(&health) {
                return None;
            }
            let counts = verdict.get("counts").filter(|c| c.is_object());
            Some((id.clone(), Health { verdict: health.to_string(), fatals: count(counts, "fatals"), errors: count(counts, "errors") }))
        })
        .collect()
}

/// LTK Manager's health verdicts by mod id; empty when it has none.
pub fn health() -> std::collections::HashMap<String, Health> {
    storage_dir().and_then(|storage| read_json(&storage.join("mod-health-verdicts.json"))).map(|doc| health_from(&doc)).unwrap_or_default()
}

/// Deletes a mod's directory and archive. Called before the index is saved:
/// LTK drops an entry whose files are gone, but re-adopts a directory whose
/// entry is gone.
fn remove_files(storage: &Path, entry: &LtkMod) -> Result<(), String> {
    let failed = |error: std::io::Error| {
        eprintln!("LTK mod removal failed: {error}");
        "Couldn't remove that mod. Try again.".to_string()
    };
    // The names come from LTK's library.json: only delete inside `mods/`/`archives/` under a plain file name.
    let plain = |name: &str| !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':']);
    if !plain(entry.slug.as_deref().unwrap_or(&entry.id)) || !plain(&entry.id) || !plain(&entry.format) {
        eprintln!("LTK mod removal refused: unsafe name in library.json");
        return Err("Couldn't remove that mod. Remove it in LTK Manager.".to_string());
    }
    let dir = mod_dir(storage, entry);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(failed)?;
    }
    let archive = archive_path(storage, entry);
    if archive.exists() {
        fs::remove_file(&archive).map_err(failed)?;
    }
    Ok(())
}

/// Turns off the other enabled mods in `id`'s exclusive slot (one skin per
/// champion; one map, HUD and loading screen). Returns their names.
fn clear_slot(mods: &mut [LtkMod], id: &str) -> Vec<String> {
    let Some(key) = mods.iter().find(|m| m.id == id).map(|m| m.slot.clone()).filter(|s| *s != Slot::Other).map(|s| s.key()) else {
        return Vec::new();
    };
    let mut off = Vec::new();
    for other in mods.iter_mut().filter(|m| m.id != id && m.enabled && m.slot.key() == key) {
        other.enabled = false;
        off.push(other.name.clone());
    }
    off
}

// ---- Imports ----

/// A validated package Swapper adds to LTK's library itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// The id it gets in LTK's library (a UUID, like LTK's own).
    pub id: String,
    pub sha256: String,
    /// The package: Swapper's copy in `incoming_dir` (deleted once imported) or the bundled font.
    pub file: PathBuf,
    pub info: PackageInfo,
}

/// Where queued packages wait for LTK Manager.
pub fn incoming_dir() -> Result<PathBuf, String> {
    Ok(crate::vault::data_root()?.join("mods").join("incoming"))
}

/// Every name in `mods/` a new slug must avoid: the index's slugs and the
/// folders and archive names on disk, lowercase.
fn taken_slugs(doc: &Value, mods_dir: &Path) -> HashSet<String> {
    let mut taken: HashSet<String> = doc
        .get("mods")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("slug").and_then(Value::as_str))
        .map(str::to_ascii_lowercase)
        .collect();
    for path in fs::read_dir(mods_dir).into_iter().flatten().flatten().map(|e| e.path()) {
        let name = if path.is_dir() { path.file_name() } else { path.file_stem() };
        if let Some(name) = name.and_then(|n| n.to_str()) {
            taken.insert(name.to_ascii_lowercase());
        }
    }
    taken
}

/// LTK's slug for `project_name`: slugified, then `-2`, `-3`… past taken names.
pub fn assign_slug(project_name: &str, taken: &HashSet<String>) -> String {
    let base = Some(super::library::slugify(project_name)).filter(|s| !s.is_empty()).unwrap_or_else(|| "mod".to_string());
    if !RESERVED_NAMES.contains(&base.as_str()) && !taken.contains(&base) {
        return base;
    }
    (2u32..).map(|n| format!("{base}-{n}")).find(|candidate| !taken.contains(candidate)).unwrap_or(base)
}

/// Writes `import` into `storage` as LTK's import does (`mods/<slug>/` with
/// its config, `mods/<slug>.<ext>`) and adds its entry to `doc`: first in the
/// root folder and the active profile's order, and enabled there if `enabled`.
fn write_import(storage: &Path, doc: &mut Value, import: &Import, enabled: bool) -> Result<(), String> {
    let files = super::library::mod_files(&import.file)?;
    let mods_dir = storage.join("mods");
    let slug = assign_slug(&files.project_name, &taken_slugs(doc, &mods_dir));
    let dir = mods_dir.join(&slug);
    let archive = mods_dir.join(format!("{slug}.{}", import.info.format));
    // shortcut: the archive is stored as given; LTK's import also embeds chunk names and stores packed
    // WADs uncompressed, which only make it faster to read. Upgrade if LTK starts requiring them.
    let written = fs::create_dir_all(&dir)
        .and_then(|()| fs::write(dir.join("mod.config.json"), &files.config))
        .and_then(|()| files.thumbnail.as_ref().map_or(Ok(()), |(name, bytes)| fs::write(dir.join(name), bytes)))
        // After the config: LTK re-reads a modpkg's metadata from an archive newer than its config.
        .and_then(|()| fs::read(&import.file))
        .and_then(|bytes| fs::write(&archive, bytes));
    if let Err(error) = written {
        eprintln!("LTK import failed for {}: {error}", dir.display());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_file(&archive);
        return Err("Couldn't add that mod to LTK Manager. Try again.".to_string());
    }
    let id = Value::String(import.id.clone());
    let entry = serde_json::json!({
        "id": import.id,
        "installedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        "format": import.info.format,
        "storage": "archive",
        "slug": slug,
        "sourceSha256": import.sha256,
    });
    if let Some(mods) = doc.get_mut("mods").and_then(Value::as_array_mut) {
        mods.push(entry);
    }
    let first = |list: Option<&mut Value>| {
        if let Some(list) = list.and_then(Value::as_array_mut) {
            list.insert(0, id.clone());
        }
    };
    for folder in doc.get_mut("folders").and_then(Value::as_array_mut).into_iter().flatten() {
        if folder.get("id").and_then(Value::as_str) == Some("root") {
            first(folder.get_mut("modIds"));
        }
    }
    let active = doc.get("activeProfileId").and_then(Value::as_str).map(str::to_string);
    for profile in doc.get_mut("profiles").and_then(Value::as_array_mut).into_iter().flatten() {
        if profile.get("id").and_then(Value::as_str) == active.as_deref() {
            first(profile.get_mut("modOrder"));
            if enabled {
                first(profile.get_mut("enabledMods"));
            }
        }
    }
    Ok(())
}

/// A queued import as the mod it becomes.
fn import_entry(import: &Import) -> LtkMod {
    let info = &import.info;
    LtkMod {
        id: import.id.clone(),
        slug: None,
        format: info.format.clone(),
        name: info.name.clone(),
        author: info.author.clone(),
        version: info.version.clone(),
        category: info.category.clone(),
        champion: info.champion.clone(),
        slot: info.slot.clone(),
        enabled: false,
        sha256: Some(import.sha256.clone()),
    }
}

// ---- Queued changes ----

/// A change to LTK's library, applied when LTK Manager can be edited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Enable(String),
    Disable(String),
    /// Turns a mod back on without the slot rules (Custom mods allowed again).
    Restore(String),
    Remove(String),
    /// Adds a package, enabled with the slot rules. An archive LTK already
    /// holds (same SHA-256) is only enabled.
    Import(Box<Import>),
}

impl Op {
    fn key(&self) -> &str {
        match self {
            Op::Enable(key) | Op::Disable(key) | Op::Restore(key) | Op::Remove(key) => key,
            Op::Import(import) => &import.sha256,
        }
    }
}

/// The library after `ops`, what to delete and what to add.
#[derive(Debug, Default)]
pub struct Plan {
    pub mods: Vec<LtkMod>,
    pub removed: Vec<LtkMod>,
    pub imports: Vec<Import>,
    pub turned_off: Vec<String>,
    pub changed: bool,
}

fn enable(out: &mut Plan, id: &str) {
    if out.mods.iter().any(|m| m.id == id) {
        out.turned_off.extend(clear_slot(&mut out.mods, id));
        out.mods.iter_mut().filter(|m| m.id == id).for_each(|m| m.enabled = true);
    }
}

pub fn plan(before: &[LtkMod], ops: &[Op]) -> Plan {
    let mut out = Plan { mods: before.to_vec(), ..Plan::default() };
    for op in ops {
        match op {
            Op::Enable(id) => enable(&mut out, id),
            Op::Disable(id) => out.mods.iter_mut().filter(|m| &m.id == id).for_each(|m| m.enabled = false),
            Op::Restore(id) => out.mods.iter_mut().filter(|m| &m.id == id).for_each(|m| m.enabled = true),
            Op::Remove(id) => {
                if let Some(index) = out.imports.iter().position(|i| &i.id == id) {
                    // Not written yet: nothing to delete.
                    out.imports.remove(index);
                    out.mods.retain(|m| &m.id != id);
                } else if let Some(index) = out.mods.iter().position(|m| &m.id == id) {
                    out.removed.push(out.mods.remove(index));
                }
            }
            Op::Import(import) => match out.mods.iter().find(|m| m.sha256.as_deref() == Some(import.sha256.as_str())) {
                Some(found) => {
                    let id = found.id.clone();
                    enable(&mut out, &id);
                }
                None => {
                    out.mods.push(import_entry(import));
                    out.imports.push((**import).clone());
                    enable(&mut out, &import.id);
                }
            },
        }
    }
    out.changed = !out.removed.is_empty()
        || !out.imports.is_empty()
        || out.mods.iter().any(|m| before.iter().any(|b| b.id == m.id && b.enabled != m.enabled));
    out
}

/// Whether Swapper may edit LTK's files now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// LTK Manager is not running.
    Free,
    /// LTK Manager only sits in the tray: close it, edit, start it again.
    Restart(u32),
    /// Its window is open (or it is mid-restart): the user closes it.
    Blocked,
}

pub fn access(running: &[u32], window_visible: bool) -> Access {
    match running {
        [] => Access::Free,
        [pid] if !window_visible => Access::Restart(*pid),
        _ => Access::Blocked,
    }
}

/// What the last attempt to apply queued changes ran into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    NextGame,
    NeedsClose,
}

static PENDING: Mutex<Vec<Op>> = Mutex::new(Vec::new());
static SETUP_PENDING: AtomicBool = AtomicBool::new(false);
/// Setup started LTK Manager for its first run (it had no settings yet).
static SETUP_LAUNCHED: AtomicBool = AtomicBool::new(false);
static STATUS: Mutex<Status> = Mutex::new(Status::Idle);
/// One apply at a time (the worker and commands).
static APPLYING: Mutex<()> = Mutex::new(());

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// Queues `op`; a newer change to the same mod replaces an older one.
pub fn queue(op: Op) {
    let mut pending = lock(&PENDING);
    pending.retain(|o| o.key() != op.key());
    pending.push(op);
}

pub fn status() -> Status {
    *lock(&STATUS)
}

/// The short line Settings shows while a change waits, if any.
pub fn notice() -> Option<&'static str> {
    match status() {
        Status::Idle => None,
        Status::NextGame => Some(NEXT_GAME),
        Status::NeedsClose if SETUP_PENDING.load(Ordering::SeqCst) => Some(CLOSE_TO_SET_UP),
        Status::NeedsClose => Some(CLOSE_TO_APPLY),
    }
}

/// LTK's library as it will be once queued changes apply. Empty before LTK
/// has a library.
pub fn planned_mods() -> Vec<LtkMod> {
    let Some(storage) = storage_dir() else { return Vec::new() };
    let Some(doc) = read_json(&library_path(&storage)) else { return Vec::new() };
    plan(&read_mods(&storage, &doc), &lock(&PENDING)).mods
}

// ---- LTK Manager process ----

/// PIDs of running LTK Managers; `None` when that can't be told (then nothing is edited).
fn running_pids() -> Option<Vec<u32>> {
    crate::windows::process::matching(&[MANAGER_IMAGE]).ok().map(|found| found.into_iter().map(|p| p.pid).collect())
}

/// Starts LTK Manager unelevated (in the tray once set up). Started by
/// another program it relaunches itself through Explorer under a new PID.
fn launch() -> Result<(), String> {
    let exe = super::patcher::manager_path().ok_or("Install LTK Manager in Settings first.")?;
    let mut command = Command::new(&exe);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(drop).map_err(|error| {
        eprintln!("LTK Manager start failed: {error}");
        "Couldn't start LTK Manager. Try again.".to_string()
    })
}

/// `pid`'s Tauri windows (LTK Manager's main window) and whether one is
/// visible. Its single-instance and event windows count as visible to
/// Windows, so only Tauri's own windows are looked at.
fn main_windows(pid: u32) -> (Vec<windows_sys::Win32::Foundation::HWND>, bool) {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible};
    struct Probe {
        pid: u32,
        found: Vec<HWND>,
        visible: bool,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> windows_sys::core::BOOL {
        let probe = &mut *(lparam as *mut Probe);
        let mut owner = 0u32;
        GetWindowThreadProcessId(hwnd, &mut owner);
        if owner == probe.pid {
            let mut class = [0u16; 64];
            let len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
            if String::from_utf16_lossy(&class[..len]) == "Tauri Window" {
                probe.found.push(hwnd);
                probe.visible |= IsWindowVisible(hwnd) != 0;
            }
        }
        1
    }
    let mut probe = Probe { pid, found: Vec::new(), visible: false };
    unsafe { EnumWindows(Some(visit), &mut probe as *mut Probe as LPARAM) };
    (probe.found, probe.visible)
}

/// Closes a tray-only LTK Manager the way its window's close button does:
/// WM_CLOSE to its main window, which LTK doesn't intercept, so it exits and
/// shuts its patcher down. Ended only if it hasn't exited by the timeout,
/// and never during a game.
fn close(pid: u32) -> Result<(), String> {
    use crate::windows::process::{terminate, wait_for_exit};
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
    let failed = || "Couldn't restart LTK Manager. Try again.".to_string();
    for hwnd in main_windows(pid).0 {
        unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
    }
    if wait_for_exit(&[pid], CLOSE_WAIT) {
        return Ok(());
    }
    if in_game() {
        return Err(failed());
    }
    eprintln!("LTK Manager didn't close; ending it");
    if !terminate(&[pid]).is_empty() || !wait_for_exit(&[pid], CLOSE_WAIT) {
        return Err(failed());
    }
    Ok(())
}

// ---- Applying ----

/// Starts setup now or as soon as LTK Manager can be edited.
pub fn request_setup(app: &AppHandle) -> Result<(), String> {
    SETUP_PENDING.store(true, Ordering::SeqCst);
    SETUP_LAUNCHED.store(false, Ordering::SeqCst);
    apply(app)
}

/// Applies queued changes and setup if LTK Manager can be edited now.
pub fn apply(app: &AppHandle) -> Result<(), String> {
    let _guard = lock(&APPLYING);
    let before = status();
    let result = apply_locked();
    let (after, wrote) = match &result {
        Ok((status, wrote, turned_off)) => {
            if !turned_off.is_empty() {
                let _ = app.emit("mods_conflict", turned_off.join(", "));
            }
            (*status, *wrote)
        }
        Err(_) => (before, false),
    };
    *lock(&STATUS) = after;
    if result.is_err() || wrote || after != before {
        let _ = app.emit("mods_changed", ());
        // Refreshes the app state (Settings notice).
        let _ = app.emit("accounts_changed", ());
    }
    result.map(|_| ())
}

/// Nothing to do, or a game is loading or running: then nothing is touched.
pub fn gate(has_work: bool, in_game: bool) -> Option<Status> {
    if !has_work {
        Some(Status::Idle)
    } else if in_game {
        Some(Status::NextGame)
    } else {
        None
    }
}

/// A game is loading or running: the game process exists, or we can't tell.
/// Not the gameflow phase: a mod that crashes the game keeps the client in
/// InProgress/Reconnect while no game runs, and holding changes for that phase
/// meant the crashing mod could never be turned off.
pub(super) fn in_game() -> bool {
    crate::windows::process::matching(&[GAME_IMAGE]).map_or(true, |found| !found.is_empty())
}

/// The status after one attempt, whether LTK's files were written, and the
/// mods the slot rules turned off.
fn apply_locked() -> Result<(Status, bool, Vec<String>), String> {
    let setup = SETUP_PENDING.load(Ordering::SeqCst);
    let ops = lock(&PENDING).clone();
    if let Some(status) = gate(setup || !ops.is_empty(), in_game()) {
        return Ok((status, false, Vec::new()));
    }
    let storage = storage_dir().ok_or("Couldn't find LTK Manager's data.")?;
    let settings_file = settings_path().ok_or("Couldn't find LTK Manager's data.")?;
    let load = || {
        let doc = read_json(&library_path(&storage));
        let current = doc.as_ref().map(|doc| read_mods(&storage, doc)).unwrap_or_default();
        let planned = plan(&current, &ops);
        (doc, current, planned)
    };
    let (doc, current, planned) = load();
    let settings_text = fs::read_to_string(&settings_file).ok();
    let parsed = settings_text.as_deref().and_then(|t| serde_json::from_str::<Value>(t).ok());
    let write_setup = setup && parsed.as_ref().is_none_or(|s| !is_set_up_in(s));
    let write_settings = write_setup;

    let running = running_pids().ok_or(WRITE_FAILED)?;
    if setup && settings_text.is_none() {
        // A fresh install writes its settings on its first run.
        if !running.is_empty() {
            return Ok((Status::NeedsClose, false, Vec::new()));
        }
        if SETUP_LAUNCHED.swap(true, Ordering::SeqCst) {
            SETUP_PENDING.store(false, Ordering::SeqCst);
            return Err("Open LTK Manager once, then set it up.".to_string());
        }
        launch()?;
        return Ok((Status::NeedsClose, false, Vec::new()));
    }
    if !write_settings && !planned.changed {
        finish(&ops);
        SETUP_PENDING.store(false, Ordering::SeqCst);
        return Ok((Status::Idle, false, Vec::new()));
    }
    let visible = matches!(running.as_slice(), [pid] if main_windows(*pid).1);
    let restarted = match access(&running, visible) {
        Access::Blocked => return Ok((Status::NeedsClose, false, Vec::new())),
        Access::Restart(pid) => {
            close(pid)?;
            true
        }
        Access::Free => false,
    };
    // LTK may have saved its library as it closed.
    let (doc, current, planned) = if restarted { load() } else { (doc, current, planned) };

    let written = (|| -> Result<(), String> {
        if write_settings {
            // Re-read after a restart: LTK may have saved its settings as it closed.
            let mut text = fs::read_to_string(&settings_file).ok().or(settings_text.clone()).unwrap_or_default();
            text = merge_setup(&text)?;
            write_atomic(&settings_file, &text)?;
        }
        let (true, Some(mut doc)) = (planned.changed, doc) else { return Ok(()) };
        for entry in &planned.removed {
            remove_files(&storage, entry)?;
            remove_entry(&mut doc, &entry.id);
        }
        for entry in planned.mods.iter().filter(|m| current.iter().any(|c| c.id == m.id && c.enabled != m.enabled)) {
            set_enabled(&mut doc, &entry.id, entry.enabled);
        }
        let imported = apply_imports(&storage, &mut doc, &planned)?;
        let text = serde_json::to_string_pretty(&doc).map_err(|_| WRITE_FAILED.to_string());
        text.and_then(|text| write_atomic(&library_path(&storage), &text)).inspect_err(|_| {
            // Without its entry LTK would adopt the folder as a mod of its own.
            for entry in &imported {
                let _ = remove_files(&storage, entry);
            }
        })
    })();
    if let Err(error) = written {
        if restarted {
            let _ = launch();
        }
        return Err(error);
    }
    finish(&ops);
    SETUP_PENDING.store(false, Ordering::SeqCst);
    if restarted || write_settings || (is_set_up() && planned.mods.iter().any(|m| m.enabled)) {
        launch()?;
    }
    Ok((Status::Idle, true, planned.turned_off))
}

/// Writes the planned imports into `storage` and `doc`, returning them as
/// written so a failed save can take them back. A failed import takes back
/// the ones before it.
fn apply_imports(storage: &Path, doc: &mut Value, planned: &Plan) -> Result<Vec<LtkMod>, String> {
    let mut written = Vec::new();
    for import in &planned.imports {
        let enabled = planned.mods.iter().any(|m| m.id == import.id && m.enabled);
        if let Err(error) = write_import(storage, doc, import, enabled) {
            for entry in &written {
                let _ = remove_files(storage, entry);
            }
            return Err(error);
        }
        written.extend(read_mods(storage, doc).into_iter().filter(|m| m.id == import.id));
    }
    Ok(written)
}

/// Drops the applied ops, keeping changes queued meanwhile, and deletes
/// Swapper's copies of the packages they imported.
fn finish(applied: &[Op]) {
    let incoming = incoming_dir().ok();
    for op in applied {
        if let Op::Import(import) = op {
            if incoming.is_some() && import.file.parent() == incoming.as_deref() {
                let _ = fs::remove_file(&import.file);
            }
        }
    }
    let mut pending = lock(&PENDING);
    pending.retain(|op| !applied.contains(op));
}

/// Queues the packages a Swapper that quit left waiting; unreadable ones are deleted.
fn requeue_incoming() {
    let Ok(dir) = incoming_dir() else { return };
    for path in fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()) {
        let sha = fs::read(&path).map(|bytes| super::library::sha256_hex(&bytes));
        match (super::library::validate_package(&path), sha) {
            (Ok(info), Ok(sha256)) => queue(Op::Import(Box::new(Import { id: uuid::Uuid::new_v4().to_string(), sha256, file: path, info }))),
            _ => {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

/// Retries queued changes every few seconds: after a game ends, and once the
/// user closes LTK Manager. Also suspends or restores Swapper's mods when
/// Custom mods stop or start being allowed.
pub fn spawn_worker(app: AppHandle) {
    requeue_incoming();
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);
        super::sync_custom(&app);
        let idle = !SETUP_PENDING.load(Ordering::SeqCst) && lock(&PENDING).is_empty();
        if idle {
            continue;
        }
        if let Err(error) = apply(&app) {
            eprintln!("LTK Manager: {error}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removal_refuses_names_that_escape_the_mods_folder() {
        let storage = std::env::temp_dir().join(format!("swapper-ltk-unsafe-{}", std::process::id()));
        fs::create_dir_all(storage.join("mods").join("keep")).unwrap();
        for slug in ["", ".", "..", "../x", "a\\b", "C:"] {
            let entry = LtkMod {
                id: "id".into(), slug: Some(slug.into()), format: "fantome".into(), name: String::new(), author: String::new(),
                version: String::new(), category: None, champion: None, slot: Slot::Other, enabled: false, sha256: None,
            };
            assert!(remove_files(&storage, &entry).is_err(), "slug {slug:?} must be refused");
        }
        assert!(storage.join("mods").join("keep").is_dir(), "nothing was deleted");
        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn health_verdicts_are_read_defensively() {
        let doc: Value = serde_json::from_str(
            r#"{ "version": 4, "verdicts": {
                "ok": { "modId": "ok", "health": "healthy", "fixable": 0, "counts": { "fatals": 0, "errors": 0, "warnings": 2, "infos": 1 }, "checkedAt": "2026-11-18T10:00:00Z" },
                "map": { "modId": "map", "health": "repairable", "fixable": 1411, "counts": { "fatals": 1411, "errors": 3, "warnings": 0, "infos": 0 },
                         "rules": [{ "rule": "bin/property-type", "severity": "fatal", "count": 1411, "fixable": 1411 }], "checkedAt": "2026-11-18T10:00:00Z", "basis": { "build": "16.23" } },
                "odd": { "modId": "odd", "health": "exploded", "counts": {} },
                "bare": { "modId": "bare", "health": "unrepairable", "counts": "nope" },
                "junk": 7
            } }"#,
        )
        .unwrap();
        let health = health_from(&doc);
        assert_eq!(health.get("ok"), Some(&Health { verdict: "healthy".into(), fatals: 0, errors: 0 }));
        assert_eq!(health.get("map"), Some(&Health { verdict: "repairable".into(), fatals: 1411, errors: 3 }));
        assert_eq!(health.get("bare"), Some(&Health { verdict: "unrepairable".into(), fatals: 0, errors: 0 }), "bad counts read as none");
        assert!(!health.contains_key("odd") && !health.contains_key("junk"), "unknown shapes are left out");
        assert!(!health.contains_key("missing"));
        for malformed in [r#"[]"#, r#"{"verdicts": []}"#, r#"{"version": 4}"#, r#""text""#] {
            assert!(health_from(&serde_json::from_str(malformed).unwrap()).is_empty(), "{malformed}");
        }
    }

    #[test]
    fn thumbnails_are_read_from_the_mod_folder_only_and_bounded() {
        let storage = std::env::temp_dir().join(format!("swapper-ltk-thumb-{}", std::process::id()));
        let _ = fs::remove_dir_all(&storage);
        let mods = storage.join("mods");
        let entry = |slug: &str| LtkMod {
            id: "id".into(), slug: Some(slug.into()), format: "fantome".into(), name: String::new(), author: String::new(),
            version: String::new(), category: None, champion: None, slot: Slot::Other, enabled: false, sha256: None,
        };
        for (slug, file, bytes) in [("webp", "thumbnail.webp", vec![1u8; 4]), ("png", "thumbnail.png", vec![2u8; 4]), ("big", "thumbnail.webp", vec![0u8; MAX_THUMBNAIL_BYTES as usize + 1])] {
            fs::create_dir_all(mods.join(slug)).unwrap();
            fs::write(mods.join(slug).join(file), bytes).unwrap();
        }
        fs::write(storage.join("thumbnail.webp"), b"outside").unwrap();
        assert_eq!(thumbnail_in(&storage, &entry("webp")), Some(vec![1u8; 4]));
        assert_eq!(thumbnail_in(&storage, &entry("png")), Some(vec![2u8; 4]), "the legacy PNG is read too");
        assert_eq!(thumbnail_in(&storage, &entry("big")), None, "over 2 MB is skipped");
        assert_eq!(thumbnail_in(&storage, &entry("missing")), None);
        for slug in ["..", ".", "../mods/webp", "a\\b", "C:"] {
            assert_eq!(thumbnail_in(&storage, &entry(slug)), None, "slug {slug:?} must be refused");
        }
        let _ = fs::remove_dir_all(&storage);
    }

    /// Modeled on a real LTK Manager 1.33 `library.json` (v2), ids made up.
    const LIBRARY: &str = r#"{
  "version": 2,
  "mods": [
    { "id": "m-font", "installedAt": "2026-01-01T00:00:00Z", "format": "fantome", "storage": "archive", "slug": "korean-font", "harvest": { "namesAdded": 0, "unharvestable": 0 } },
    { "id": "m-ahri-a", "installedAt": "2026-01-01T00:00:00Z", "format": "fantome", "storage": "archive", "slug": "ahri-a", "sourceSha256": "aaa" },
    { "id": "m-ahri-b", "installedAt": "2026-01-01T00:00:00Z", "format": "modpkg", "storage": "archive", "slug": "ahri-b", "sourceSha256": "bbb" },
    { "id": "m-legacy", "installedAt": "2026-01-01T00:00:00Z", "format": "fantome", "storage": "archive" }
  ],
  "profiles": [
    { "id": "p-default", "name": "Default", "slug": "default", "enabledMods": ["m-font", "m-ahri-a"], "modOrder": ["m-font", "m-ahri-a", "m-ahri-b", "m-legacy"], "layerStates": { "m-ahri-b": {} }, "createdAt": "2026-01-01T00:00:00Z", "lastUsed": "2026-01-01T00:00:00Z" },
    { "id": "p-other", "name": "Other", "slug": "other", "enabledMods": ["m-ahri-b"], "modOrder": ["m-ahri-b"], "layerStates": {}, "createdAt": "2026-01-01T00:00:00Z", "lastUsed": "2026-01-01T00:00:00Z" }
  ],
  "activeProfileId": "p-default",
  "folders": [ { "id": "root", "name": "", "modIds": ["m-font", "m-ahri-a", "m-ahri-b", "m-legacy"] } ],
  "folderOrder": ["root"]
}"#;

    fn fixture(name: &str) -> PathBuf {
        let storage = std::env::temp_dir().join(format!("swapper-ltk-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&storage);
        let mods = storage.join("mods");
        for (slug, ext, config) in [
            ("korean-font", "fantome", r#"{"name":"korean-font","display_name":"Korean Font","version":"1.1.0","authors":["SSI6"],"layers":[]}"#),
            ("ahri-a", "fantome", r#"{"name":"ahri-a","display_name":"Ahri A","version":"1.0.0","authors":[{"name":"Ada","role":"Author"}],"champions":["Ahri"],"tags":["champion-skin"]}"#),
            ("ahri-b", "modpkg", r#"{"name":"ahri-b","display_name":"","version":"2.0.0","authors":[],"champions":["Ahri"]}"#),
        ] {
            fs::create_dir_all(mods.join(slug)).unwrap();
            fs::write(mods.join(slug).join("mod.config.json"), config).unwrap();
            fs::write(mods.join(format!("{slug}.{ext}")), b"archive").unwrap();
        }
        fs::create_dir_all(mods.join("m-legacy")).unwrap();
        fs::write(mods.join("m-legacy").join("mod.config.json"), r#"{"name":"old","display_name":"Old","version":"0.1","authors":[]}"#).unwrap();
        fs::create_dir_all(storage.join("archives")).unwrap();
        fs::write(storage.join("archives").join("m-legacy.fantome"), b"archive").unwrap();
        fs::write(library_path(&storage), LIBRARY).unwrap();
        storage
    }

    #[test]
    fn setup_merge_sets_the_four_keys_and_keeps_everything_else() {
        let text = r#"{"leaguePath":"C:/Riot Games/League of Legends","modStoragePath":null,"minimizeToTray":true,"startInTray":false,"autoRun":false,"builtinMods":{"mapSkin":"game"},"alwaysStartPatcher":false,"futureKey":[1,2]}"#;
        let merged: Value = serde_json::from_str(&merge_setup(text).unwrap()).unwrap();
        assert!(is_set_up_in(&merged));
        let original: Value = serde_json::from_str(text).unwrap();
        for (key, value) in original.as_object().unwrap() {
            if !SETUP_KEYS.contains(&key.as_str()) {
                assert_eq!(merged.get(key), Some(value), "{key} must be kept");
            }
        }
        let keys: Vec<&String> = merged.as_object().unwrap().keys().collect();
        assert_eq!(keys[0], "leaguePath", "key order is kept");
        assert_eq!(keys.last().unwrap().as_str(), "futureKey");
        assert!(!is_set_up_in(&original));
        assert!(merge_setup("not json").is_err());
        assert!(merge_setup("[1]").is_err());
    }

    #[test]
    fn storage_dir_follows_mod_storage_path() {
        let app = Path::new(r"C:\ltk");
        assert_eq!(storage_dir_from(app, None), app);
        assert_eq!(storage_dir_from(app, Some(&serde_json::json!({ "modStoragePath": null }))), app);
        assert_eq!(storage_dir_from(app, Some(&serde_json::json!({ "modStoragePath": "D:\\mods" }))), Path::new("D:\\mods"));
    }

    #[test]
    fn library_reads_names_authors_enabled_state_and_slots() {
        let storage = fixture("read");
        let doc: Value = serde_json::from_str(LIBRARY).unwrap();
        let mods = read_mods(&storage, &doc);
        let by_id = |id: &str| mods.iter().find(|m| m.id == id).unwrap().clone();
        assert_eq!(mods.len(), 4);
        let font = by_id("m-font");
        assert_eq!((font.name.as_str(), font.author.as_str(), font.version.as_str(), font.enabled), ("Korean Font", "SSI6", "1.1.0", true));
        let a = by_id("m-ahri-a");
        assert_eq!((a.author.as_str(), a.slot.clone(), a.sha256.as_deref()), ("Ada", Slot::Skin("Ahri".into()), Some("aaa")));
        let b = by_id("m-ahri-b");
        assert_eq!((b.name.as_str(), b.enabled), ("ahri-b", false), "falls back to name; enabled only counts the active profile");
        assert_eq!(by_id("m-legacy").name, "Old");
        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn unsafe_slugs_are_skipped() {
        let doc = serde_json::json!({ "mods": [{ "id": "x", "slug": "..\\..\\Windows", "format": "fantome" }], "profiles": [], "activeProfileId": "" });
        assert!(read_mods(Path::new("C:\\nowhere"), &doc).is_empty());
    }

    #[test]
    fn enable_disable_remove_round_trip_keeps_other_mods() {
        let storage = fixture("edit");
        let mut doc: Value = serde_json::from_str(LIBRARY).unwrap();
        set_enabled(&mut doc, "m-ahri-b", true);
        set_enabled(&mut doc, "m-ahri-b", true);
        set_enabled(&mut doc, "m-font", false);
        let mods = read_mods(&storage, &doc);
        assert!(mods.iter().find(|m| m.id == "m-ahri-b").unwrap().enabled);
        assert!(!mods.iter().find(|m| m.id == "m-font").unwrap().enabled);
        assert_eq!(str_list(doc["profiles"][0].get("enabledMods")), vec!["m-ahri-a", "m-ahri-b"]);
        assert_eq!(str_list(doc["profiles"][1].get("enabledMods")), vec!["m-ahri-b"], "other profiles are untouched");

        let legacy = mods.iter().find(|m| m.id == "m-legacy").unwrap().clone();
        let b = mods.iter().find(|m| m.id == "m-ahri-b").unwrap().clone();
        for entry in [&legacy, &b] {
            remove_files(&storage, entry).unwrap();
            remove_entry(&mut doc, &entry.id);
        }
        assert!(!storage.join("mods").join("ahri-b").exists() && !storage.join("mods").join("ahri-b.modpkg").exists());
        assert!(!storage.join("archives").join("m-legacy.fantome").exists() && !storage.join("mods").join("m-legacy").exists());
        assert!(storage.join("mods").join("ahri-a.fantome").exists(), "other mods' files stay");
        write_atomic(&library_path(&storage), &serde_json::to_string_pretty(&doc).unwrap()).unwrap();
        let saved = read_json(&library_path(&storage)).unwrap();
        assert_eq!(read_mods(&storage, &saved).iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["m-font", "m-ahri-a"]);
        assert_eq!(str_list(saved["profiles"][1].get("enabledMods")), Vec::<String>::new());
        assert_eq!(str_list(saved["profiles"][0].get("modOrder")), vec!["m-font", "m-ahri-a"]);
        assert!(saved["profiles"][0]["layerStates"].get("m-ahri-b").is_none());
        assert_eq!(str_list(saved["folders"][0].get("modIds")), vec!["m-font", "m-ahri-a"]);
        assert_eq!(saved["folderOrder"], serde_json::json!(["root"]), "unknown fields are kept");
        assert_eq!(saved["mods"][0]["harvest"]["namesAdded"], 0);
        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn enabling_a_skin_turns_off_the_other_skin_for_that_champion() {
        let storage = fixture("slots");
        let doc: Value = serde_json::from_str(LIBRARY).unwrap();
        let mods = read_mods(&storage, &doc);
        let out = plan(&mods, &[Op::Enable("m-ahri-b".into())]);
        let enabled: Vec<&str> = out.mods.iter().filter(|m| m.enabled).map(|m| m.id.as_str()).collect();
        assert_eq!(enabled, vec!["m-font", "m-ahri-b"], "the font (no slot) stays on");
        assert_eq!(out.turned_off, vec!["Ahri A".to_string()]);
        assert!(out.changed);
        assert!(!plan(&mods, &[Op::Disable("m-ahri-b".into())]).changed, "already off");
        assert!(!plan(&mods, &[Op::Enable("missing".into())]).changed);
        let _ = fs::remove_dir_all(&storage);
    }

    /// A small real `.fantome`: META/info.json and one WAD file.
    fn package(dir: &Path, name: &str, champion: &str) -> Import {
        use std::io::Write as _;
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("{}.fantome", name.replace(' ', "")));
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let info = serde_json::json!({"Name": name, "Author": "Ada", "Version": "1.0.0", "Description": "Test", "Champions": [champion], "Tags": ["champion-skin"]});
        zip.start_file("META/info.json", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(info.to_string().as_bytes()).unwrap();
        zip.start_file(format!("WAD/{champion}.wad.client/data/a.bin"), zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"chunk").unwrap();
        zip.finish().unwrap();
        Import {
            id: format!("new-{}", name.replace(' ', "-")),
            sha256: super::super::library::sha256_hex(&fs::read(&path).unwrap()),
            info: super::super::library::validate_package(&path).unwrap(),
            file: path,
        }
    }

    #[test]
    fn an_import_writes_ltk_files_and_entry_with_the_slot_rules() {
        let storage = fixture("import");
        let mut doc: Value = serde_json::from_str(LIBRARY).unwrap();
        // Slugifies to "ahri-a", which the fixture already has.
        let import = package(&storage.join("in"), "Ahri A", "Ahri");
        let out = plan(&read_mods(&storage, &doc), &[Op::Import(Box::new(import.clone()))]);
        assert!(out.changed);
        assert_eq!(out.turned_off, vec!["Ahri A".to_string()], "the other Ahri skin goes off");
        for entry in out.mods.iter().filter(|m| m.id == "m-ahri-a") {
            set_enabled(&mut doc, &entry.id, entry.enabled);
        }
        apply_imports(&storage, &mut doc, &out).unwrap();

        let mods = storage.join("mods");
        let config = read_json(&mods.join("ahri-a-2").join("mod.config.json")).unwrap();
        assert_eq!((config["name"].as_str(), config["display_name"].as_str()), (Some("ahri-a"), Some("Ahri A")));
        assert_eq!(config["authors"], serde_json::json!(["Ada"]));
        assert_eq!(config["layers"][0]["name"], "base");
        assert_eq!(fs::read(mods.join("ahri-a-2.fantome")).unwrap(), fs::read(&import.file).unwrap());
        let entry = doc["mods"].as_array().unwrap().last().unwrap();
        assert_eq!(entry["slug"], "ahri-a-2");
        assert_eq!((entry["storage"].as_str(), entry["format"].as_str()), (Some("archive"), Some("fantome")));
        assert_eq!(entry["sourceSha256"], import.sha256.as_str());
        assert!(entry["installedAt"].as_str().unwrap().ends_with('Z'));
        assert_eq!(doc["profiles"][0]["enabledMods"][0], import.id.as_str());
        assert_eq!(doc["profiles"][0]["modOrder"][0], import.id.as_str());
        assert_eq!(doc["folders"][0]["modIds"][0], import.id.as_str());
        assert_eq!(str_list(doc["profiles"][1].get("enabledMods")), vec!["m-ahri-b"], "other profiles are untouched");

        let read = read_mods(&storage, &doc);
        let added = read.iter().find(|m| m.id == import.id).unwrap();
        assert_eq!((added.name.as_str(), added.slot.clone(), added.enabled), ("Ahri A", Slot::Skin("Ahri".into()), true));
        let enabled: Vec<&str> = read.iter().filter(|m| m.enabled).map(|m| m.id.as_str()).collect();
        assert_eq!(enabled, vec!["m-font", import.id.as_str()]);

        // Removing it again with Swapper's own code leaves the fixture as it was.
        let removed = plan(&read, &[Op::Remove(import.id.clone())]);
        for entry in &removed.removed {
            remove_files(&storage, entry).unwrap();
            remove_entry(&mut doc, &entry.id);
        }
        assert!(!mods.join("ahri-a-2").exists() && !mods.join("ahri-a-2.fantome").exists());
        assert!(mods.join("ahri-a.fantome").exists());
        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn an_archive_ltk_already_has_is_only_enabled_and_a_pending_import_can_be_cancelled() {
        let storage = fixture("import-known");
        let doc: Value = serde_json::from_str(LIBRARY).unwrap();
        let mods = read_mods(&storage, &doc);
        let mut known = package(&storage.join("in"), "Known", "Ahri");
        known.sha256 = "bbb".into();
        let out = plan(&mods, &[Op::Import(Box::new(known))]);
        assert!(out.imports.is_empty());
        assert!(out.mods.iter().find(|m| m.id == "m-ahri-b").unwrap().enabled);

        let fresh = package(&storage.join("in"), "Fresh", "Lux");
        let cancelled = plan(&mods, &[Op::Import(Box::new(fresh.clone())), Op::Remove(fresh.id.clone())]);
        assert!(cancelled.imports.is_empty() && cancelled.removed.is_empty() && !cancelled.changed);
        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn slugs_never_collide_with_the_index_or_the_mods_folder() {
        let storage = fixture("slugs");
        fs::create_dir_all(storage.join("mods").join("Stray")).unwrap();
        let doc: Value = serde_json::from_str(LIBRARY).unwrap();
        let taken = taken_slugs(&doc, &storage.join("mods"));
        for name in ["korean-font", "ahri-a", "ahri-b", "m-legacy", "stray"] {
            assert!(taken.contains(name), "{name}");
        }
        assert_eq!(assign_slug("Ahri A", &taken), "ahri-a-2");
        assert_eq!(assign_slug("Stray", &taken), "stray-2", "case-insensitive");
        assert_eq!(assign_slug("Cool Skin!", &taken), "cool-skin");
        assert_eq!(assign_slug("Con", &taken), "con-2", "reserved Windows name");
        assert_eq!(assign_slug("???", &taken), "mod");
        let mut more = taken.clone();
        more.extend(["ahri-a-2".to_string(), "ahri-a-3".to_string()]);
        assert_eq!(assign_slug("ahri-a", &more), "ahri-a-4");
        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn restore_turns_a_mod_back_on_without_the_slot_rules() {
        let storage = fixture("restore");
        let doc: Value = serde_json::from_str(LIBRARY).unwrap();
        let out = plan(&read_mods(&storage, &doc), &[Op::Restore("m-ahri-b".into())]);
        assert!(out.turned_off.is_empty());
        assert!(out.mods.iter().filter(|m| m.id.starts_with("m-ahri")).all(|m| m.enabled));
        let _ = fs::remove_dir_all(&storage);
    }

    #[test]
    fn changes_wait_for_the_game_to_end() {
        assert_eq!(gate(true, true), Some(Status::NextGame), "game process running");
        assert_eq!(gate(true, false), None, "no game process: applies now, whatever the client's phase");
        assert_eq!(gate(false, true), Some(Status::Idle), "nothing queued");
    }

    #[test]
    fn a_tray_only_ltk_manager_is_restarted_and_an_open_window_waits() {
        assert_eq!(access(&[], false), Access::Free);
        assert_eq!(access(&[7], false), Access::Restart(7), "tray only, whoever started it");
        assert_eq!(access(&[7], true), Access::Blocked, "its window is open");
        assert_eq!(access(&[7, 8], false), Access::Blocked, "mid-relaunch");
    }

    /// Waits for LTK Manager to be back in the tray after a restart.
    fn tray_pid() -> u32 {
        for _ in 0..120 {
            std::thread::sleep(Duration::from_millis(500));
            if let Some([pid]) = running_pids().as_deref() {
                // Its startup (reconcile, save) settles first.
                std::thread::sleep(Duration::from_secs(8));
                assert_eq!(running_pids().as_deref(), Some(&[*pid][..]), "LTK Manager kept running");
                assert!(!main_windows(*pid).1, "LTK Manager came back with no window");
                return *pid;
            }
        }
        panic!("LTK Manager didn't come back");
    }

    #[test]
    #[ignore = "live: imports into and removes from this machine's LTK Manager (set SWAPPER_LIVE_LTK=1)"]
    fn live_silent_import_and_remove() {
        assert_eq!(std::env::var("SWAPPER_LIVE_LTK").as_deref(), Ok("1"));
        let storage = storage_dir().unwrap();
        let backup = storage.join("library.json.swapper-backup");
        if !backup.exists() {
            fs::copy(library_path(&storage), &backup).unwrap();
        }
        let before = read_json(&library_path(&storage)).unwrap();
        let dir = std::env::temp_dir().join(format!("swapper-live-ltk-{}", std::process::id()));
        let import = package(&dir, "Swapper Live Test", "Teemo");
        let id = import.id.clone();

        queue(Op::Import(Box::new(import)));
        assert_eq!(apply_locked().unwrap().0, Status::Idle);
        tray_pid();
        let listed = read_json(&library_path(&storage)).unwrap();
        let found = read_mods(&storage, &listed).into_iter().find(|m| m.id == id).expect("LTK kept the import");
        eprintln!("imported as {:?}, enabled {}", found.slug, found.enabled);
        assert!(found.enabled);

        queue(Op::Remove(id.clone()));
        assert_eq!(apply_locked().unwrap().0, Status::Idle);
        tray_pid();
        let after = read_json(&library_path(&storage)).unwrap();
        assert!(read_mods(&storage, &after).iter().all(|m| m.id != id));
        let slug = found.slug.unwrap();
        assert!(!storage.join("mods").join(&slug).exists() && !storage.join("mods").join(format!("{slug}.fantome")).exists());
        assert_eq!(after, before, "library.json is as it was");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_change_to_the_same_mod_replaces_the_queued_one() {
        let key = format!("queue-{}", std::process::id());
        queue(Op::Enable(key.clone()));
        queue(Op::Remove(key.clone()));
        let queued: Vec<Op> = lock(&PENDING).iter().filter(|o| o.key() == key).cloned().collect();
        assert_eq!(queued, vec![Op::Remove(key.clone())]);
        finish(&queued);
        assert!(!lock(&PENDING).iter().any(|o| o.key() == key));
    }
}
