//! Comps backend: lobby/saved team, per-player champion pools from the League
//! client (last-20 match history + full champion mastery), and per-account
//! persistence for the saved roster and the user's own pool overrides.
//! Teammate lookups that fail return an error on that player only.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(6);
/// "Played this season or last", approximated as the last 18 months.
const RECENT_MS: u64 = 540 * 24 * 3600 * 1000;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Member { pub puuid: String, pub riot_id: String }

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Overrides { #[serde(default)] pub added: Vec<Added>, #[serde(default)] pub hidden: Vec<i64> }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Added { pub champion_id: i64, pub role: String }

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context { pub me: Option<Member>, pub lobby: Vec<Member>, pub roster: Vec<RosterMember>, pub suggested: Vec<Suggested>, pub overrides: Overrides }

/// One roster friend, shared across all signed-in accounts (never "You").
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RosterMember { pub puuid: String, pub riot_id: String, #[serde(default)] pub nickname: Option<String>, #[serde(default)] pub roles: Vec<String> }
/// A frequent Flex/Clash teammate found over the season, with shared games.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggested { pub puuid: String, pub riot_id: String, pub games: u64 }

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolEntry { pub champion_id: i64, pub role: String, pub reason: String, pub games: Option<u64>, pub mastery_points: Option<u64>, pub mastery_level: Option<u64> }
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pool { pub puuid: String, pub entries: Vec<PoolEntry>, pub error: Option<String>, pub profile_icon_id: Option<i64> }

#[derive(Clone, Default, Serialize, Deserialize)]
struct Stored { #[serde(default)] overrides: Overrides }
type ByAccount = BTreeMap<String, Stored>;
static FILE_LOCK: Mutex<()> = Mutex::new(());
static CACHE: Mutex<Option<HashMap<String, (std::time::Instant, Pool)>>> = Mutex::new(None);
static SUGGEST_CACHE: Mutex<Option<HashMap<String, (std::time::Instant, Vec<Suggested>)>>> = Mutex::new(None);
const POOL_CACHE_TTL: Duration = Duration::from_secs(30 * 60);
const SUGGEST_CACHE_TTL: Duration = Duration::from_secs(10 * 60);
fn should_cache_suggestions(timed_out: bool) -> bool { !timed_out }

fn store_path() -> Option<std::path::PathBuf> { Some(crate::vault::data_root().ok()?.join("comps-by-account.json")) }
fn load_all(path: &Path) -> ByAccount {
    if std::fs::metadata(path).map_or(true, |m| m.len() > 1024 * 1024) { return ByAccount::new(); }
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}
fn load_all_for_write(path: &Path) -> Result<ByAccount, String> {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ByAccount::new()),
        Err(e) => Err(e.to_string()),
        Ok(m) if m.len() > 1024 * 1024 => { backup_bad(path)?; Ok(ByAccount::new()) }
        Ok(_) => match std::fs::read(path) {
            Ok(b) => match serde_json::from_slice(&b) { Ok(v) => Ok(v), Err(_) => { backup_bad(path)?; Ok(ByAccount::new()) } },
            Err(e) => Err(e.to_string()),
        },
    }
}
/// Keep the unreadable file as `.bad`; refuse to overwrite it if that copy fails.
fn backup_bad(path: &Path) -> Result<(), String> { std::fs::copy(path, path.with_extension("bad")).map(|_| ()).map_err(|e| { eprintln!("Comps backup failed: {e}"); "Couldn't back up your comps. Try again.".to_string() }) }
fn update_store(path: &Path, puuid: &str, change: impl FnOnce(&mut Stored)) -> Result<(), String> {
    let _g = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut all = load_all_for_write(path)?;
    change(all.entry(puuid.to_string()).or_default());
    let bytes = serde_json::to_vec(&all).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}
fn valid(puuid: &str) -> bool { !puuid.is_empty() && puuid.len() <= 128 && puuid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') }

const ROLE_ORDER: [&str; 5] = ["top", "jungle", "mid", "bot", "support"];
fn roster_path() -> Option<std::path::PathBuf> { Some(crate::vault::data_root().ok()?.join("comps-roster.json")) }
#[derive(Default, Serialize, Deserialize)]
struct RosterFile { #[serde(default)] members: Vec<RosterMember> }
/// Boundary validation for roster writes: known PUUIDs only, deduped, max 20, roles are known/deduped/max 5.
pub fn sanitize_roster(members: Vec<RosterMember>) -> Vec<RosterMember> {
    let mut out: Vec<RosterMember> = Vec::new();
    for m in members {
        if !valid(&m.puuid) || out.iter().any(|x| x.puuid == m.puuid) { continue; }
        let mut roles: Vec<String> = Vec::new();
        for r in &m.roles { let r = r.to_lowercase(); if ROLE_ORDER.contains(&r.as_str()) && !roles.contains(&r) { roles.push(r); } }
        out.push(RosterMember { puuid: m.puuid, riot_id: m.riot_id.trim().chars().take(80).collect(),
            nickname: m.nickname.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()), roles: roles.into_iter().take(5).collect() });
        if out.len() >= 20 { break; }
    }
    out
}
fn load_roster(path: &Path) -> Vec<RosterMember> {
    if std::fs::metadata(path).map_or(true, |m| m.len() > 1024 * 1024) { return Vec::new(); }
    let file: RosterFile = std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    sanitize_roster(file.members)
}
fn save_roster(path: &Path, members: Vec<RosterMember>) -> Result<Vec<RosterMember>, String> {
    let members = sanitize_roster(members);
    let _g = FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Ok(m) = std::fs::metadata(path) { if m.len() > 1024 * 1024 { backup_bad(path)?; } }
    let bytes = serde_json::to_vec(&RosterFile { members: members.clone() }).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(members)
}
/// Ordered role preferences from a player's recent Summoner's Rift games, most-played first.
pub fn roles_from_history(history: &Value, puuid: &str) -> Vec<String> {
    let mut counted: Vec<(String, u64)> = Vec::new();
    for game in history["games"]["games"].as_array().into_iter().flatten().take(20) {
        if let Some(q) = game["queueId"].as_u64() { if !matches!(q, 400 | 420 | 430 | 440 | 490 | 700) { continue; } }
        if let Some(m) = game["mapId"].as_u64() { if m != 11 { continue; } }
        let pid = game["participantIdentities"].as_array().into_iter().flatten()
            .find(|i| i["player"]["puuid"].as_str() == Some(puuid)).and_then(|i| i["participantId"].as_u64());
        let Some(part) = pid.and_then(|id| game["participants"].as_array().into_iter().flatten().find(|p| p["participantId"].as_u64() == Some(id))) else { continue };
        let raw = super::model::role(part);
        let role = if raw == "adc" { "bot" } else { raw };
        match counted.iter_mut().find(|(r, _)| r == role) { Some((_, n)) => *n += 1, None => counted.push((role.into(), 1)) }
    }
    counted.sort_by(|a, b| b.1.cmp(&a.1).then(ROLE_ORDER.iter().position(|r| *r == a.0).cmp(&ROLE_ORDER.iter().position(|r| *r == b.0))));
    counted.into_iter().map(|(r, _)| r).collect()
}

async fn current_member(client: &crate::runes::Lcu) -> Option<Member> {
    let v: Value = crate::runes::lcu_get(client, "/lol-summoner/v1/current-summoner").await.ok()?;
    let puuid = v["puuid"].as_str().filter(|s| valid(s))?.to_string();
    let name = v["gameName"].as_str().unwrap_or_default();
    let tag = v["tagLine"].as_str().unwrap_or_default();
    Some(Member { puuid, riot_id: if tag.is_empty() { name.into() } else { format!("{name}#{tag}") } })
}

/// A champion is pooled if played 2+ times in the last 20 games, or in the
/// mastery top 10 / >=100k points and played within the last ~18 months.
pub fn build_pool(history: &Value, mastery: &Value, puuid: &str, now_ms: u64) -> Vec<PoolEntry> {
    let mut games: HashMap<i64, u64> = HashMap::new();
    let mut roles: HashMap<i64, HashMap<String, u64>> = HashMap::new();
    for game in history["games"]["games"].as_array().into_iter().flatten().take(20) {
        // Only Summoner's Rift queues count toward pools (not ARAM/Arena/etc.).
        if let Some(q) = game["queueId"].as_u64() { if !matches!(q, 400 | 420 | 430 | 440 | 490 | 700) { continue; } }
        if let Some(m) = game["mapId"].as_u64() { if m != 11 { continue; } }
        let pid = game["participantIdentities"].as_array().into_iter().flatten()
            .find(|i| i["player"]["puuid"].as_str() == Some(puuid))
            .and_then(|i| i["participantId"].as_u64());
        let Some(part) = pid.and_then(|id| game["participants"].as_array().into_iter().flatten().find(|p| p["participantId"].as_u64() == Some(id))) else { continue };
        let Some(champ) = part["championId"].as_i64() else { continue };
        *games.entry(champ).or_default() += 1;
        let raw = super::model::role(part);
        let role = if raw == "adc" { "bot" } else { raw }.to_string();
        *roles.entry(champ).or_default().entry(role).or_default() += 1;
    }
    let mut masteries: Vec<(i64, u64, u64, Option<u64>)> = mastery.as_array().into_iter().flatten()
        .filter_map(|m| Some((m["championId"].as_i64()?, m["championPoints"].as_u64()?, m["lastPlayTime"].as_u64().unwrap_or(0), m["championLevel"].as_u64()))).collect();
    masteries.sort_by(|a, b| b.1.cmp(&a.1));
    let top10: Vec<i64> = masteries.iter().take(10).map(|m| m.0).collect();
    let mut out: Vec<PoolEntry> = Vec::new();
    let champs: Vec<i64> = { let mut v: Vec<i64> = games.keys().copied().chain(masteries.iter().map(|m| m.0)).collect(); v.sort_unstable(); v.dedup(); v };
    for champ in champs {
        let g = games.get(&champ).copied().unwrap_or(0);
        let m = masteries.iter().find(|m| m.0 == champ);
        let mastery_ok = m.is_some_and(|(_, pts, last, _)| (top10.contains(&champ) || *pts >= 100_000) && now_ms.saturating_sub(*last) <= RECENT_MS);
        if g < 2 && !mastery_ok { continue; }
        let role = roles.get(&champ).and_then(|r| r.iter().max_by(|a, b| a.1.cmp(b.1).then(a.0.cmp(b.0)))).map(|(r, _)| r.clone()).unwrap_or_default();
        out.push(PoolEntry { champion_id: champ, role,
            reason: if g >= 2 { format!("Played {g} of last 20") } else { format!("Mastery {}k", m.map_or(0, |x| x.1) / 1000) },
            games: (g > 0).then_some(g), mastery_points: m.map(|x| x.1), mastery_level: m.and_then(|x| x.3) });
    }
    out.sort_by(|a, b| b.games.unwrap_or(0).cmp(&a.games.unwrap_or(0)).then(b.mastery_points.unwrap_or(0).cmp(&a.mastery_points.unwrap_or(0))).then(a.champion_id.cmp(&b.champion_id)));
    out
}


/// Allies in 3+ deduped Flex/Clash games, most-shared first (max 4 before PUUID resolution).
pub fn count_allies(games: &[super::model::Game]) -> Vec<(String, u64)> {
    let mut seen: Vec<u64> = Vec::new();
    let mut counts: Vec<(String, u64)> = Vec::new();
    for game in games {
        if !matches!(game.queue.as_str(), "flex" | "clash") || seen.contains(&game.id) { continue; }
        seen.push(game.id);
        for p in &game.players {
            if p.me || !p.ally || p.riot_id.is_empty() { continue; }
            match counts.iter_mut().find(|(id, _)| *id == p.riot_id) {
                Some((_, n)) => *n += 1,
                None => counts.push((p.riot_id.clone(), 1)),
            }
        }
    }
    counts.retain(|(_, n)| *n >= 3);
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    counts.truncate(6);
    counts
}

async fn resolve_puuid(client: &crate::runes::Lcu, riot_id: &str) -> Option<String> {
    let (name, tag) = riot_id.split_once('#')?;
    let mut url = reqwest::Url::parse("https://localhost/lol-summoner/v1/alias/lookup").ok()?;
    url.query_pairs_mut().append_pair("gameName", name).append_pair("tagLine", tag);
    let alias: Value = tokio::time::timeout(TIMEOUT, crate::runes::lcu_get(client, &format!("{}?{}", url.path(), url.query().unwrap_or_default()))).await.ok()?.ok()?;
    alias["puuid"].as_str().filter(|s| valid(s)).map(str::to_string)
}

/// Frequent teammates from this season's u.gg Flex games plus recent client
/// Flex/Clash history. Only u.gg is time-boxed: on timeout the client games and
/// any u.gg pages already fetched are used, and the result is not cached.
async fn season_suggestions(client: &crate::runes::Lcu, me: &Member) -> Vec<Suggested> {
    if let Some((at, v)) = SUGGEST_CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|c| c.get(&me.puuid).cloned()) {
        if at.elapsed() < SUGGEST_CACHE_TTL { return v; }
    }
    // Client history first, outside the u.gg time-box, so a u.gg timeout keeps these allies.
    let mut games: Vec<super::model::Game> = Vec::new();
    let hist_path = format!("/lol-match-history/v1/products/lol/{}/matches?begIndex=0&endIndex=19", me.puuid);
    if let Ok(Ok(history)) = tokio::time::timeout(TIMEOUT, crate::runes::lcu_get::<Value>(client, &hist_path)).await {
        let owner = super::model::Summoner { puuid: me.puuid.clone(), summoner_id: 0, riot_id: me.riot_id.clone(), level: 0, icon: None };
        for raw in history["games"]["games"].as_array().into_iter().flatten() {
            if let Some(g) = super::model::game(raw, &owner) { games.push(g); }
        }
    }
    // u.gg games go straight into `games`, so a timeout keeps the pages already fetched.
    let ugg = async {
        let g = &mut games;
        if let (Some((name, tag)), Ok(region_v)) = (me.riot_id.split_once('#'), crate::runes::lcu_get::<Value>(client, "/riotclient/region-locale").await) {
            if let Some(region) = region_v["region"].as_str() {
                let mut flex = 0usize;
                for page in 1..=5u32 {
                    let Ok(part) = super::search::season::page(name, tag, region, page).await else { break };
                    let done = part.done;
                    for game in part.games { if game.queue == "flex" { flex += 1; } g.push(game); }
                    if done || flex >= 100 { break; }
                }
            }
        }
    };
    let timed_out = tokio::time::timeout(Duration::from_secs(5), ugg).await.is_err();
    let mut out: Vec<Suggested> = Vec::new();
    for (riot_id, n) in count_allies(&games) {
        if out.len() >= 4 { break; }
        if let Some(puuid) = resolve_puuid(client, &riot_id).await {
            if puuid != me.puuid { out.push(Suggested { puuid, riot_id, games: n }); }
        }
    }
    if should_cache_suggestions(timed_out) {
        SUGGEST_CACHE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(me.puuid.clone(), (std::time::Instant::now(), out.clone()));
    }
    out
}

#[tauri::command]
pub async fn comps_team() -> Context {
    let mut ctx = Context::default();
    if let Some(path) = roster_path() { ctx.roster = load_roster(&path); }
    let Ok(client) = crate::runes::lcu().await else { return ctx };
    let Some(me) = current_member(&client).await else { return ctx };
    if let Some(path) = store_path() { let stored = load_all(&path).remove(&me.puuid).unwrap_or_default(); ctx.overrides = stored.overrides; }
    if let Ok(Ok(lobby)) = tokio::time::timeout(TIMEOUT, crate::runes::lcu_get::<Value>(&client, "/lol-lobby/v2/lobby")).await {
        ctx.lobby = lobby["members"].as_array().into_iter().flatten().filter_map(|m| {
            let puuid = m["puuid"].as_str().filter(|s| valid(s))?.to_string();
            let name = m["gameName"].as_str().or_else(|| m["summonerName"].as_str()).unwrap_or_default();
            let tag = m["gameTag"].as_str().or_else(|| m["tagLine"].as_str()).unwrap_or_default();
            Some(Member { puuid: puuid.clone(), riot_id: if name.is_empty() { puuid } else if tag.is_empty() { name.into() } else { format!("{name}#{tag}") } })
        }).collect();
    }
    ctx.suggested = season_suggestions(&client, &me).await;
    // The lobby often gives only a PUUID; resolve missing Riot IDs for display.
    ctx.lobby = named(&client, std::mem::take(&mut ctx.lobby)).await;
    // Refresh stored Riot IDs from the lobby when it knows them.
    for m in &ctx.lobby { if let Some(r) = ctx.roster.iter_mut().find(|r| r.puuid == m.puuid) { if !m.riot_id.is_empty() && m.riot_id != m.puuid { r.riot_id = m.riot_id.clone(); } } }
    // Roster Riot IDs above are display-only refreshes; the file is rewritten on the next roster save.
    ctx.me = Some(me);
    ctx
}

/// Members whose Riot ID is missing (empty or just their PUUID) get it from the summoner endpoint, four at a time.
async fn named(client: &crate::runes::Lcu, members: Vec<Member>) -> Vec<Member> {
    use futures_util::{stream, StreamExt};
    stream::iter(members.into_iter().map(|m| async move {
        if !m.riot_id.is_empty() && m.riot_id != m.puuid { return m; }
        let path = format!("/lol-summoner/v2/summoners/puuid/{}", m.puuid);
        let found = match tokio::time::timeout(TIMEOUT, crate::runes::lcu_get::<Value>(client, &path)).await { Ok(Ok(v)) => Some(v), _ => None };
        let name = found.as_ref().and_then(|v| v["gameName"].as_str()).filter(|n| !n.is_empty());
        let tag = found.as_ref().and_then(|v| v["tagLine"].as_str()).filter(|t| !t.is_empty());
        match (name, tag) {
            (Some(n), Some(t)) => Member { riot_id: format!("{n}#{t}"), ..m },
            (Some(n), None) => Member { riot_id: n.to_string(), ..m },
            _ => Member { riot_id: String::new(), ..m },
        }
    })).buffered(4).collect().await
}

/// Resolve a typed Riot ID (`Name#TAG`) to a team member via the LCU.
#[tauri::command]
pub async fn comps_resolve(riot_id: String) -> Result<Member, String> {
    let client = crate::runes::lcu().await.map_err(|e| e.to_string())?;
    let puuid = resolve_puuid(&client, riot_id.trim()).await.ok_or("No player found for that Riot ID.")?;
    Ok(Member { puuid, riot_id: riot_id.trim().to_string() })
}

#[tauri::command]
pub async fn comps_pools(puuids: Vec<String>) -> Vec<Pool> {
    use futures_util::{stream, StreamExt};
    stream::iter(puuids.into_iter().map(|puuid| async move {
        if !valid(&puuid) { return Pool { puuid, entries: vec![], error: Some("That player couldn't load. Try again.".into()), profile_icon_id: None }; }
        if let Some((at, c)) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|c| c.get(&puuid).cloned()) { if at.elapsed() < POOL_CACHE_TTL { return c; } }
        let (pool, mastery_ok) = fetch_pool(&puuid).await;
        if pool.error.is_none() && mastery_ok { CACHE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(puuid.clone(), (std::time::Instant::now(), pool.clone())); }
        pool
    })).buffer_unordered(4).collect().await
}

async fn fetch_pool(puuid: &str) -> (Pool, bool) {
    let fail = |e: String| (Pool { puuid: puuid.into(), entries: vec![], error: Some(e), profile_icon_id: None }, false);
    let Ok(client) = crate::runes::lcu().await else { return fail("League Client is not running.".into()) };
    let hist_path = format!("/lol-match-history/v1/products/lol/{puuid}/matches?begIndex=0&endIndex=19");
    let mastery_path = format!("/lol-champion-mastery/v1/{puuid}/champion-mastery");
    let summoner_path = format!("/lol-summoner/v2/summoners/puuid/{puuid}");
    let (history, mastery, summoner) = tokio::join!(
        tokio::time::timeout(TIMEOUT, crate::runes::lcu_get::<Value>(&client, &hist_path)),
        tokio::time::timeout(TIMEOUT, crate::runes::lcu_get::<Value>(&client, &mastery_path)),
        tokio::time::timeout(TIMEOUT, crate::runes::lcu_get::<Value>(&client, &summoner_path)));
    let profile_icon_id = match summoner { Ok(Ok(v)) => v["profileIconId"].as_i64(), _ => None };
    let history = match history { Ok(Ok(v)) => v, _ => return fail("Could not load this player's recent games.".into()) };
    let (mastery, mastery_ok) = match mastery { Ok(Ok(v)) => (v, true), _ => (Value::Null, false) };
    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    (Pool { puuid: puuid.into(), entries: build_pool(&history, &mastery, puuid, now_ms), error: None, profile_icon_id }, mastery_ok)
}

fn check_account(me: &Member, expected_puuid: &str) -> Result<(), String> {
    if expected_puuid != me.puuid { return Err("Your signed-in account changed. Reload, then save again.".into()); }
    Ok(())
}

#[tauri::command]
pub async fn comps_roster_load() -> Vec<RosterMember> {
    roster_path().map(|p| load_roster(&p)).unwrap_or_default()
}

#[tauri::command]
pub async fn comps_roster_save(members: Vec<RosterMember>) -> Result<Vec<RosterMember>, String> {
    let path = roster_path().ok_or("Couldn't open storage. Restart Swapper and retry.")?;
    tokio::task::spawn_blocking(move || save_roster(&path, members)).await.map_err(|e| e.to_string())?
}

/// Ordered role preferences for a new roster member, from their recent games. Empty when unknown.
#[tauri::command]
pub async fn comps_prefill_roles(puuid: String) -> Vec<String> {
    if !valid(&puuid) { return Vec::new(); }
    let Ok(client) = crate::runes::lcu().await else { return Vec::new() };
    let hist_path = format!("/lol-match-history/v1/products/lol/{puuid}/matches?begIndex=0&endIndex=19");
    match tokio::time::timeout(TIMEOUT, crate::runes::lcu_get::<Value>(&client, &hist_path)).await {
        Ok(Ok(history)) => roles_from_history(&history, &puuid),
        _ => Vec::new(),
    }
}


#[tauri::command]
pub async fn comps_save_overrides(added: Vec<Added>, hidden: Vec<i64>, expected_puuid: String) -> Result<(), String> {
    let client = crate::runes::lcu().await.map_err(|e| e.to_string())?;
    let me = current_member(&client).await.ok_or("No one is signed in. Open League Client first.")?;
    check_account(&me, &expected_puuid)?;
    let path = store_path().ok_or("Couldn't open storage. Restart Swapper and retry.")?;
    let added: Vec<Added> = added.into_iter().take(200).collect();
    let hidden: Vec<i64> = hidden.into_iter().take(200).collect();
    tokio::task::spawn_blocking(move || update_store(&path, &me.puuid, |s| s.overrides = Overrides { added, hidden })).await.map_err(|e| e.to_string())??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn game(champ: i64, lane: &str) -> Value { serde_json::json!({"participantIdentities":[{"participantId":1,"player":{"puuid":"me"}}],"participants":[{"participantId":1,"championId":champ,"spell1Id":4,"spell2Id":12,"stats":{"item0":0},"timeline":{"lane":lane,"role":"SOLO"}}]}) }
    #[test]
    fn pool_needs_two_games_or_recent_high_mastery() {
        let now = 1_800_000_000_000u64;
        let history = serde_json::json!({"games":{"games":[game(103,"MIDDLE"), game(103,"MIDDLE"), game(64,"JUNGLE")]}});
        let mastery = serde_json::json!([{"championId":61,"championPoints":150000,"lastPlayTime":now},{"championId":157,"championPoints":410000,"lastPlayTime":1}]);
        let pool = build_pool(&history, &mastery, "me", now);
        let ids: Vec<i64> = pool.iter().map(|e| e.champion_id).collect();
        assert!(ids.contains(&103), "two games pools Ahri: {ids:?}");
        assert!(ids.contains(&61), "recent 150k mastery pools Orianna");
        assert!(!ids.contains(&64), "one game does not pool Lee Sin");
        assert!(!ids.contains(&157), "stale mastery does not pool Yasuo");
        assert_eq!(pool.iter().find(|e| e.champion_id == 103).unwrap().role, "mid");
    }

    fn ally_game(id: u64, queue: &str, allies: &[&str]) -> super::super::model::Game {
        let player = |riot_id: &str, me: bool, ally: bool| super::super::model::Player {
            champion_id: 1, riot_id: riot_id.into(), puuid: String::new(), me, ally, role: "mid".into(),
            kills: 0, deaths: 0, assists: 0, cs: 0, damage: 0, gold: 0, vision: 0, items: vec![],
        };
        super::super::model::Game {
            id, queue: queue.into(), champion_id: 1, created_at: 0, duration: 1800, win: true,
            kills: 0, deaths: 0, assists: 0, metrics: [None; 6], items: vec![], spells: vec![], keystone: 0,
            cs: 0, level: 1, role: "mid".into(), placement: 0, lp_change: None,
            players: std::iter::once(player("Me#1", true, true)).chain(allies.iter().map(|a| player(a, false, true))).collect(),
            opponent: None,
        }
    }
    #[test]
    fn allies_need_three_shared_flex_or_clash_games_deduped() {
        let games = vec![
            ally_game(1, "flex", &["A#1", "B#1"]), ally_game(1, "flex", &["A#1", "B#1"]),
            ally_game(2, "clash", &["A#1"]), ally_game(3, "flex", &["A#1"]),
            ally_game(4, "solo", &["B#1"]), ally_game(5, "solo", &["B#1"]), ally_game(6, "solo", &["B#1"]),
        ];
        assert_eq!(count_allies(&games), vec![("A#1".to_string(), 3)]);
    }
    #[test]
    fn overrides_round_trip_per_account() {
        let path = std::env::temp_dir().join(format!("comps-{}.json", uuid::Uuid::new_v4()));
        update_store(&path, "a", |s| s.overrides.hidden = vec![157]).unwrap();
        update_store(&path, "b", |s| s.overrides.added = vec![Added { champion_id: 61, role: "mid".into() }]).unwrap();
        let all = load_all(&path);
        assert_eq!(all["a"].overrides.hidden, vec![157]);
        assert_eq!(all["b"].overrides.added.len(), 1);
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn pool_ignores_aram_games() {
        let now = 1_800_000_000_000u64;
        let aram = |champ: i64| serde_json::json!({"queueId":450,"mapId":12,"participantIdentities":[{"participantId":1,"player":{"puuid":"me"}}],"participants":[{"participantId":1,"championId":champ,"spell1Id":4,"spell2Id":12,"stats":{"item0":0},"timeline":{"lane":"MIDDLE","role":"SOLO"}}]});
        let history = serde_json::json!({"games":{"games":[aram(103), aram(103), game(103,"MIDDLE")]}});
        let pool = build_pool(&history, &serde_json::json!([]), "me", now);
        assert!(pool.is_empty(), "ARAM games must not pool: {pool:?}");
    }
    #[test]
    fn suggestion_timeout_keeps_client_allies_and_skips_cache() {
        assert!(!should_cache_suggestions(true));
        assert!(should_cache_suggestions(false));
        let games = vec![ally_game(1, "flex", &["A#1"]), ally_game(2, "flex", &["A#1"]), ally_game(3, "clash", &["A#1"])];
        // Client games alone (u.gg timed out -> no u.gg games) still yield the ally.
        assert_eq!(count_allies(&games), vec![("A#1".to_string(), 3)]);
    }
    #[test]
    fn corrupt_store_backup_preserves_bad_file() {
        let path = std::env::temp_dir().join(format!("comps-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"not json").unwrap();
        update_store(&path, "a", |s| s.overrides.hidden = vec![157]).unwrap();
        let bad = std::fs::read(path.with_extension("bad")).unwrap();
        assert_eq!(bad, b"not json");
        assert_eq!(load_all(&path)["a"].overrides.hidden, vec![157]);
        let _ = std::fs::remove_file(&path); let _ = std::fs::remove_file(path.with_extension("bad"));
    }
    #[test]
    fn roster_round_trip_and_sanitize() {
        let path = std::env::temp_dir().join(format!("comps-roster-{}.json", uuid::Uuid::new_v4()));
        let saved = save_roster(&path, vec![
            RosterMember { puuid: "p1".into(), riot_id: " A#1 ".into(), nickname: Some(" Al ".into()), roles: vec!["MID".into(), "mid".into(), "top".into(), "bogus".into()] },
            RosterMember { puuid: "p1".into(), riot_id: "Dup#1".into(), nickname: None, roles: vec![] },
            RosterMember { puuid: "bad puuid!".into(), riot_id: "Bad#1".into(), nickname: None, roles: vec![] },
        ]).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].riot_id, "A#1");
        assert_eq!(saved[0].nickname.as_deref(), Some("Al"));
        assert_eq!(saved[0].roles, vec!["mid".to_string(), "top".to_string()]);
        assert_eq!(load_roster(&path), saved);
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn prefill_roles_orders_by_recent_games() {
        let jng = serde_json::json!({"participantIdentities":[{"participantId":1,"player":{"puuid":"me"}}],"participants":[{"participantId":1,"championId":64,"spell1Id":11,"spell2Id":4,"stats":{"item0":0},"timeline":{"lane":"JUNGLE","role":"NONE"}}]});
        let history = serde_json::json!({"games":{"games":[game(103,"MIDDLE"), game(103,"MIDDLE"), jng, game(103,"MIDDLE")]}});
        assert_eq!(roles_from_history(&history, "me"), vec!["mid".to_string(), "jungle".to_string()]);
        assert!(roles_from_history(&history, "someone-else").is_empty());
    }
}
