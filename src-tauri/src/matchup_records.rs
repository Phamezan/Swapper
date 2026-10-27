//! Compact, account-scoped draft-queue matchup history. Never infer missing lanes.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};

const PAGE_SIZE: usize = 20;
const MAX_STORE_BYTES: usize = 32 * 1024 * 1024;
static SYNC: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static DISK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatchRecord { champion: i64, enemy: i64, lane: String, win: bool }

#[derive(Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AccountHistory {
    games: BTreeMap<u64, MatchRecord>,
    /// Includes skipped games, so a resumed backfill does not fetch their details again.
    seen: BTreeSet<u64>,
    /// Most recent history row after a complete scan. Later scans stop here.
    head: Option<u64>,
    complete: bool,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Store { accounts: BTreeMap<String, AccountHistory> }

#[derive(Default, Debug, Serialize, PartialEq)]
pub(crate) struct Record { pub wins: u64, pub losses: u64 }

fn path() -> Option<PathBuf> { Some(crate::vault::data_root().ok()?.join("matchup-records.json")) }

fn load(path: &Path) -> Result<Store, String> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(_) => return Err("Could not read matchup records.".into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_STORE_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| "Could not read matchup records.")?;
    if bytes.len() > MAX_STORE_BYTES { return Err("Matchup record store is too large.".into()); }
    serde_json::from_slice(&bytes).map_err(|_| "Could not read matchup records.".into())
}

fn save(path: &Path, store: &Store) -> Result<(), String> {
    let bytes = serde_json::to_vec(store).map_err(|_| "Could not save matchup records.")?;
    if bytes.len() > MAX_STORE_BYTES { return Err("Matchup record store is too large.".into()); }
    let parent = path.parent().ok_or("Invalid record directory.")?;
    std::fs::create_dir_all(parent).map_err(|_| "Could not save matchup records.")?;
    let pending = path.with_extension("tmp");
    let result = std::fs::write(&pending, bytes).and_then(|_| std::fs::rename(&pending, path));
    let _ = std::fs::remove_file(pending);
    result.map_err(|_| "Could not save matchup records.".into())
}

fn saved_accounts(app: &tauri::AppHandle) -> Vec<String> {
    app.state::<crate::AppState>().config.lock().map(|c| c.accounts.iter().filter_map(|a| a.puuid.clone()).collect()).unwrap_or_default()
}

fn aggregate(store: &Store, saved: &[String], champion: i64, enemy: i64, lane: &str) -> Option<Record> {
    let mut result = Record::default();
    let mut available = false;
    for (puuid, account) in &store.accounts {
        if !saved.contains(puuid) { continue; }
        available = true;
        for game in account.games.values().filter(|g| g.champion == champion && g.enemy == enemy && g.lane == lane) {
            if game.win { result.wins += 1; } else { result.losses += 1; }
        }
    }
    available.then_some(result)
}

pub(crate) fn record(app: &tauri::AppHandle, champion: i64, enemy: i64, lane: &str) -> Option<Record> {
    let _guard = DISK.lock().unwrap_or_else(|e| e.into_inner());
    aggregate(&load(&path()?).ok()?, &saved_accounts(app), champion, enemy, lane)
}

fn position(player: &Value) -> Option<&'static str> {
    for key in ["teamPosition", "individualPosition"] {
        match player[key].as_str() {
            Some("TOP") => return Some("top"),
            Some("JUNGLE") => return Some("jungle"),
            Some("MIDDLE" | "MID") => return Some("mid"),
            Some("BOTTOM" | "BOT") => return Some("adc"),
            Some("UTILITY" | "SUPPORT") => return Some("support"),
            _ => {},
        }
    }
    // Reuse the app's verified Smite/support-item rules, without its CS/gold slot guesses.
    let anchored = crate::full_app::model::role(player);
    if matches!(anchored, "jungle" | "support") { return Some(anchored); }
    match player["timeline"]["lane"].as_str() {
        Some("TOP") => Some("top"),
        Some("MIDDLE" | "MID") => Some("mid"),
        Some("BOTTOM" | "BOT") => match player["timeline"]["role"].as_str() {
            Some("DUO_CARRY" | "CARRY") => Some("adc"),
            Some("DUO_SUPPORT" | "SUPPORT") => Some("support"),
            _ => None,
        },
        _ => None,
    }
}

fn draft_game(game: &Value) -> bool {
    game["mapId"] == 11 && matches!(game["queueId"].as_u64(), Some(400 | 420 | 440))
        && game["gameDuration"].as_u64().unwrap_or(0) >= 300
}

fn record_from_game(game: &Value, puuid: &str) -> Option<MatchRecord> {
    if !draft_game(game) { return None; }
    let players = game["participants"].as_array()?;
    if players.len() != 10 || players.iter().any(|p| p["stats"]["gameEndedInEarlySurrender"] == true)
        || [100,200].iter().any(|team| players.iter().filter(|p| p["teamId"] == *team).count() != 5) { return None; }
    let identities = game["participantIdentities"].as_array()?;
    let mut owners = identities.iter().filter(|p| p["player"]["puuid"].as_str() == Some(puuid));
    let identity = owners.next()?;
    if owners.next().is_some() { return None; }
    let participant_id = identity["participantId"].as_u64()?;
    let own = players.iter().find(|p| p["participantId"].as_u64() == Some(participant_id))?;
    let lane = position(own)?;
    let same_lane = |p: &&Value| position(p) == Some(lane);
    if players.iter().filter(|p| p["teamId"] == own["teamId"]).filter(same_lane).count() != 1 { return None; }
    let mut enemies = players.iter().filter(|p| p["teamId"] != own["teamId"]).filter(same_lane);
    let enemy = enemies.next()?;
    if enemies.next().is_some() { return None; }
    let champion = own["championId"].as_i64().filter(|id| *id > 0)?;
    let enemy = enemy["championId"].as_i64().filter(|id| *id > 0)?;
    Some(MatchRecord { champion, enemy, lane: lane.into(), win: own["stats"]["win"].as_bool()? })
}

/// Merge one page while holding the disk lock, so an end-of-game insert cannot be lost.
fn merge_at(path: &Path, puuid: &str, seen: &[u64], games: &[(u64, MatchRecord)], completed_head: Option<Option<u64>>) -> Result<(), String> {
    let _guard = DISK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = load(path)?;
    let account = store.accounts.entry(puuid.into()).or_default();
    account.seen.extend(seen);
    account.games.extend(games.iter().cloned());
    if let Some(head) = completed_head { account.complete = true; account.head = head; }
    save(path, &store)
}

pub(crate) fn remember(app: &tauri::AppHandle, game: &Value, puuid: &str) {
    if !saved_accounts(app).iter().any(|p| p == puuid) { return; }
    if let (Some(path), Some(id), Some(record)) = (path(), game["gameId"].as_u64(), record_from_game(game, puuid)) {
        // EOG roles may be provisional. History can replace this same game id later.
        if merge_at(&path, puuid, &[], &[(id, record)], None).is_ok() {
            let _ = app.emit("matchup_records_changed", ());
        }
    }
}

async fn sync(app: &tauri::AppHandle) -> Result<(), String> {
    let Ok(_sync) = SYNC.try_lock() else { return Ok(()) };
    let account_read = crate::full_app::lp::AccountRead::begin(app)?;
    let client = crate::runes::lcu().await.map_err(|_| "League Client unavailable.")?;
    let current = crate::runes::lcu_get::<Value>(&client, "/lol-summoner/v1/current-summoner").await.map_err(|_| "No signed-in account.")?;
    let puuid = current["puuid"].as_str().filter(|p| !p.is_empty() && p.len() <= 128
        && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')).ok_or("No signed-in account.")?;
    if !saved_accounts(app).iter().any(|p| p == puuid) { return Ok(()); }
    let path = path().ok_or("No data directory.")?;
    backfill(&path, puuid, |url| {
        let client = &client;
        async move { crate::runes::lcu_get::<Value>(client, &url).await.map_err(|_| "Match history unavailable.".to_string()) }
    }, |verified| {
        account_read.verify(puuid, verified)?;
        if !saved_accounts(app).iter().any(|p| p == puuid) { return Err("Account was removed.".into()); }
        Ok(())
    }).await?;
    let _ = app.emit("matchup_records_changed", ());
    Ok(())
}

async fn backfill<F, Fut>(path: &Path, puuid: &str, get: F, verify: impl Fn(&Value) -> Result<(), String>) -> Result<(), String>
where F: Fn(String) -> Fut, Fut: std::future::Future<Output = Result<Value,String>> {
    let mut held = {
        let _guard = DISK.lock().unwrap_or_else(|e| e.into_inner());
        load(&path)?.accounts.remove(puuid).unwrap_or_default()
    };
    let stop = held.complete.then_some(held.head).flatten();
    let (mut begin, mut newest) = (0, None);
    let mut visited = HashSet::new();
    loop {
        let url = format!("/lol-match-history/v1/products/lol/{puuid}/matches?begIndex={begin}&endIndex={}", begin + PAGE_SIZE - 1);
        let history = get(url).await?;
        let rows = history["games"]["games"].as_array().ok_or("Unexpected match history.")?;
        let mut seen = vec![];
        let mut games = vec![];
        let mut finished = rows.len() < PAGE_SIZE;
        // Some clients ignore pagination and repeat their latest 20 forever.
        let repeated = !rows.is_empty() && rows.iter().all(|r| r["gameId"].as_u64().is_some_and(|id| visited.contains(&id)));
        finished |= repeated;
        for row in rows {
            if repeated { break; }
            let id = row["gameId"].as_u64().filter(|id| *id > 0).ok_or("Unexpected game id.")?;
            if newest.is_none() { newest = Some(id); }
            if Some(id) == stop { finished = true; break; }
            // A server that clamps pagination must not send us around forever.
            if !visited.insert(id) { return Err("Match history repeated a page; backfill will retry later.".into()); }
            if held.seen.contains(&id) { continue; }
            let detail = if draft_game(row) && row["participants"].as_array().is_none_or(|p| p.len() != 10) {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let detail = get(format!("/lol-match-history/v1/games/{id}")).await?;
                if detail["gameId"].as_u64() != Some(id) { return Err("Unexpected match detail.".into()); }
                detail
            } else { row.clone() };
            if let Some(record) = record_from_game(&detail, puuid) { games.push((id,record)); }
            seen.push(id);
        }
        // The explicit PUUID URL and switch guard keep accounts separate even during a switch.
        let verified = get("/lol-summoner/v1/current-summoner".into()).await?;
        verify(&verified)?;
        merge_at(path, puuid, &seen, &games, finished.then_some(newest))?;
        held.seen.extend(seen);
        if finished { return Ok(()); }
        begin += rows.len();
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Independent of the full-app window: backfill each saved account while it is signed in.
pub(crate) fn spawn_watch(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let _ = sync(&app).await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

pub(crate) async fn after_game(app: tauri::AppHandle) {
    // Match history can file minutes after EOG; the regular watcher continues retrying.
    for delay in [20, 60] {
        tokio::time::sleep(Duration::from_secs(delay)).await;
        let _ = sync(&app).await;
    }
}

#[cfg(test)]
mod tests;
