//! Player search only reads the local client and Swapper's bounded saved data.
use super::{model, LABEL};
use base64::Engine;
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock, time::Duration};

const LIMIT: usize = 8;
const CLOSED: &str = "Open League Client to load a profile. Showing saved players only.";
static GATE: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
static CACHE_GATE: OnceLock<std::sync::Mutex<()>> = OnceLock::new();

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    riot_id: String,
    icon: Option<String>,
    level: Option<u64>,
    ally: Option<bool>,
    last_played: Option<u64>,
    #[serde(default)]
    live: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Search {
    connected: bool,
    players: Vec<Player>,
    message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    player: Player,
    solo: Option<model::Rank>,
    flex: Option<model::Rank>,
    games: Vec<model::Game>,
    live: bool,
    fetched_at: i64,
    message: Option<String>,
}

fn riot_id(value: &str) -> Result<(&str, &str), String> {
    let (name, tag) = value
        .trim()
        .split_once('#')
        .ok_or("Enter a full Riot ID: Name#TAG.")?;
    if name.trim().is_empty()
        || tag.trim().is_empty()
        || name.len() > 64
        || tag.len() > 32
        || tag.contains('#')
        || value.chars().any(char::is_control)
    {
        return Err("Enter a full Riot ID: Name#TAG.".into());
    }
    Ok((name.trim(), tag.trim()))
}

fn valid_puuid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn prefix(name: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    !query.is_empty() && name.to_lowercase().starts_with(&query)
}

fn path() -> Option<PathBuf> {
    Some(crate::vault::data_root().ok()?.join("search-profiles.json"))
}

fn read_at(path: &std::path::Path) -> Vec<Profile> {
    if std::fs::metadata(path).map_or(true, |m| m.len() > 2 * 1024 * 1024) {
        return vec![];
    }
    serde_json::from_slice::<Vec<Profile>>(&std::fs::read(path).unwrap_or_default())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.games.len() <= 20 && riot_id(&p.player.riot_id).is_ok())
        .take(LIMIT)
        .collect()
}

fn read() -> Vec<Profile> {
    path().map(|p| read_at(&p)).unwrap_or_default()
}

fn saved_snapshot() -> Option<model::Snapshot> {
    let mut snapshot = super::read_cache()?;
    for game in crate::match_detail::cached_games() {
        if !snapshot.games.iter().any(|g| g.id == game.id) {
            if let Some(game) = perspective(&game, &snapshot.summoner.riot_id) {
                snapshot.games.push(game);
            }
        }
    }
    Some(snapshot)
}

fn remember(profiles: &mut Vec<Profile>, profile: Profile) {
    profiles.retain(|p| {
        !p.player
            .riot_id
            .eq_ignore_ascii_case(&profile.player.riot_id)
    });
    profiles.insert(0, profile);
    profiles.truncate(LIMIT);
}

fn save(profile: Profile) {
    let Ok(_guard) = CACHE_GATE.get_or_init(Default::default).lock() else {
        return;
    };
    let Some(path) = path() else {
        return;
    };
    let mut profiles = read_at(&path);
    remember(&mut profiles, profile);
    let Ok(bytes) = serde_json::to_vec(&profiles) else {
        return;
    };
    if bytes.len() > 2 * 1024 * 1024 {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let pending = path.with_extension("tmp");
    if std::fs::write(&pending, bytes).is_ok() {
        let _ = std::fs::rename(pending, path);
    }
}

fn saved_players(
    snapshot: Option<&model::Snapshot>,
    profiles: &[Profile],
    query: &str,
) -> Vec<Player> {
    if query.trim().is_empty() {
        return profiles
            .iter()
            .take(LIMIT)
            .map(|profile| Player {
                live: false,
                ..profile.player.clone()
            })
            .collect();
    }
    let mut players = BTreeMap::new();
    if let Some(snapshot) = snapshot {
        for game in &snapshot.games {
            for p in &game.players {
                if p.me || !prefix(&p.riot_id, query) {
                    continue;
                }
                let player = Player {
                    riot_id: p.riot_id.clone(),
                    icon: None,
                    level: None,
                    ally: Some(p.ally),
                    last_played: Some(game.created_at),
                    live: false,
                };
                let previous: Option<&Player> = players.get(&p.riot_id.to_lowercase());
                if previous.is_none_or(|p| p.last_played < player.last_played) {
                    players.insert(p.riot_id.to_lowercase(), player);
                }
            }
        }
    }
    for profile in profiles {
        if prefix(&profile.player.riot_id, query) {
            let key = profile.player.riot_id.to_lowercase();
            let previous = players.get(&key);
            let mut player = profile.player.clone();
            player.live = false;
            if let Some(previous) = previous {
                player.ally = previous.ally;
                player.last_played = previous.last_played;
            }
            players.insert(key, player);
        }
        // Cached details also contain participants whose own profile was never opened.
        for game in &profile.games {
            for p in &game.players {
                if prefix(&p.riot_id, query) {
                    players
                        .entry(p.riot_id.to_lowercase())
                        .or_insert_with(|| Player {
                            riot_id: p.riot_id.clone(),
                            icon: None,
                            level: None,
                            ally: None,
                            last_played: None,
                            live: false,
                        });
                }
            }
        }
    }
    let mut players: Vec<_> = players.into_values().collect();
    players.sort_by_key(|p| std::cmp::Reverse(p.last_played));
    players.truncate(LIMIT);
    players
}

async fn get(client: &crate::runes::Lcu, path: &str) -> Result<Value, String> {
    tokio::time::timeout(Duration::from_secs(4), crate::runes::lcu_get(client, path))
        .await
        .map_err(|_| "League Client timed out. Try again.".to_string())?
        .map_err(|_| "League Client could not provide this data.".to_string())
}

async fn lookup(
    client: &crate::runes::Lcu,
    value: &str,
) -> Result<(model::Summoner, Value), String> {
    let (name, tag) = riot_id(value)?;
    let mut url = reqwest::Url::parse("https://localhost/lol-summoner/v1/alias/lookup").unwrap();
    url.query_pairs_mut()
        .append_pair("gameName", name)
        .append_pair("tagLine", tag);
    let alias = get(
        client,
        &format!("{}?{}", url.path(), url.query().unwrap_or_default()),
    )
    .await?;
    let puuid = alias["puuid"]
        .as_str()
        .filter(|p| valid_puuid(p))
        .ok_or("No player found for that Riot ID.")?;
    let value = get(client, &format!("/lol-summoner/v2/summoners/puuid/{puuid}")).await?;
    let summoner = model::summoner(&value)
        .filter(|s| s.puuid == puuid)
        .ok_or("Player profile is unavailable.")?;
    Ok((summoner, value))
}

async fn icon(client: &crate::runes::Lcu, value: &Value) -> Option<String> {
    let id = value["profileIconId"].as_u64()?;
    let bytes = tokio::time::timeout(
        Duration::from_secs(4),
        crate::runes::lcu_get_bytes(
            client,
            &format!("/lol-game-data/assets/v1/profile-icons/{id}.jpg"),
        ),
    )
    .await
    .ok()?
    .ok()?;
    (bytes.len() <= 128 * 1024).then(|| {
        format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    })
}

fn player(summoner: &model::Summoner, icon: Option<String>) -> Player {
    Player {
        riot_id: summoner.riot_id.clone(),
        icon,
        level: Some(summoner.level),
        ally: None,
        last_played: None,
        live: true,
    }
}

fn flex_rank(ranked: &Value) -> Option<model::Rank> {
    let queue = ranked["queueMap"]["RANKED_FLEX_SR"]
        .as_object()
        .map(|_| ranked["queueMap"]["RANKED_FLEX_SR"].clone())
        .or_else(|| {
            ranked["queues"]
                .as_array()?
                .iter()
                .find(|q| q["queueType"] == "RANKED_FLEX_SR")
                .cloned()
        })?;
    model::rank(&serde_json::json!({"queueMap": {"RANKED_SOLO_5x5": queue}}))
}

fn check(app: &tauri::AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() != LABEL || !super::enabled(app) {
        return Err("Fullsized App is closed or disabled.".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn full_app_search_players(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    query: String,
) -> Result<Search, String> {
    check(&app, &window)?;
    if query.len() > 128 || query.chars().any(char::is_control) {
        return Err("Search is too long or contains invalid characters.".into());
    }
    let saved_query = query.clone();
    let mut players = tokio::task::spawn_blocking(move || {
        saved_players(saved_snapshot().as_ref(), &read(), &saved_query)
    })
    .await
    .unwrap_or_default();
    if query.trim().is_empty() {
        return Ok(Search {
            connected: false,
            players,
            message: None,
        });
    }
    let Ok(client) = crate::runes::lcu().await else {
        return Ok(Search {
            connected: false,
            players,
            message: Some(CLOSED.into()),
        });
    };
    if !query.contains('#') {
        return Ok(Search {
            connected: true,
            players,
            message: Some(
                "Type a full Riot ID: Name#TAG. Saved players match by name prefix.".into(),
            ),
        });
    }
    let _permit = GATE
        .get_or_init(|| tokio::sync::Semaphore::new(2))
        .try_acquire()
        .map_err(|_| "Player search is busy. Try again.".to_string())?;
    match lookup(&client, &query).await {
        Ok((summoner, value)) => {
            let mut hit = player(&summoner, icon(&client, &value).await);
            if let Some(saved) = players
                .iter()
                .find(|p| p.riot_id.eq_ignore_ascii_case(&hit.riot_id))
            {
                hit.ally = saved.ally;
                hit.last_played = saved.last_played;
            }
            players.retain(|p| !p.riot_id.eq_ignore_ascii_case(&hit.riot_id));
            players.insert(0, hit);
            players.truncate(LIMIT);
            Ok(Search {
                connected: true,
                players,
                message: None,
            })
        }
        Err(message) => Ok(Search {
            connected: true,
            players,
            message: Some(message),
        }),
    }
}

fn saved_profile(snapshot: Option<&model::Snapshot>, profiles: Vec<Profile>, id: &str) -> Profile {
    if let Some(mut profile) = profiles
        .iter()
        .find(|p| p.player.riot_id.eq_ignore_ascii_case(id))
        .cloned()
    {
        profile.live = false;
        profile.player.live = false;
        profile.message = None;
        return profile;
    }
    let mut games = BTreeMap::new();
    for game in snapshot
        .into_iter()
        .flat_map(|s| &s.games)
        .chain(profiles.iter().flat_map(|p| &p.games))
    {
        if let Some(game) = perspective(game, id) {
            games.entry(game.id).or_insert(game);
        }
    }
    let mut games: Vec<_> = games.into_values().collect();
    games.sort_by_key(|g| std::cmp::Reverse(g.created_at));
    games.truncate(20);
    Profile {
        player: Player {
            riot_id: id.into(),
            icon: None,
            level: None,
            ally: None,
            last_played: None,
            live: false,
        },
        solo: None,
        flex: None,
        games,
        live: false,
        fetched_at: snapshot.map_or(0, |s| s.fetched_at),
        message: None,
    }
}

fn perspective(game: &model::Game, id: &str) -> Option<model::Game> {
    let p = game
        .players
        .iter()
        .find(|p| p.riot_id.eq_ignore_ascii_case(id))?;
    if p.me {
        return Some(game.clone());
    }
    let mut game = game.clone();
    let p = p.clone();
    game.champion_id = p.champion_id;
    game.win = if p.ally { game.win } else { !game.win };
    game.kills = p.kills;
    game.deaths = p.deaths;
    game.assists = p.assists;
    game.cs = p.cs;
    game.role = p.role.clone();
    game.items = p.items;
    game.lp_change = None;
    game.spells.clear();
    game.keystone = 0;
    game.level = 0;
    game.placement = 0;
    game.opponent = None;
    game.metrics = [
        None,
        None,
        (game.duration > 0).then(|| p.cs as f64 / (game.duration as f64 / 60.0)),
        None,
        None,
        None,
    ];
    for other in &mut game.players {
        other.me = other.riot_id.eq_ignore_ascii_case(id);
        other.ally = other.ally == p.ally;
    }
    Some(game)
}

#[tauri::command]
pub async fn full_app_player_profile(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    riot_id: String,
) -> Result<Profile, String> {
    check(&app, &window)?;
    if riot_id.trim().is_empty() || riot_id.len() > 128 || riot_id.chars().any(char::is_control) {
        return Err("Invalid player name.".into());
    }
    let id = riot_id.clone();
    let saved =
        tokio::task::spawn_blocking(move || saved_profile(saved_snapshot().as_ref(), read(), &id))
            .await
            .map_err(|_| "Saved profile is unavailable.".to_string())?;
    let Ok(client) = crate::runes::lcu().await else {
        return Ok(saved);
    };
    let _permit = GATE
        .get_or_init(|| tokio::sync::Semaphore::new(2))
        .try_acquire()
        .map_err(|_| "Player search is busy. Try again.".to_string())?;
    let (summoner, value) = match lookup(&client, &riot_id).await {
        Ok(value) => value,
        Err(message) => {
            return Ok(Profile {
                message: Some(message),
                ..saved
            })
        }
    };
    let rank_path = format!("/lol-ranked/v1/ranked-stats/{}", summoner.puuid);
    let history_path = format!(
        "/lol-match-history/v1/products/lol/{}/matches?begIndex=0&endIndex=19",
        summoner.puuid
    );
    let (ranked, history, icon) = tokio::join!(
        get(&client, &rank_path),
        get(&client, &history_path),
        icon(&client, &value)
    );
    let history_rows = history
        .as_ref()
        .ok()
        .and_then(|h| h["games"]["games"].as_array());
    let mut incomplete = ranked.as_ref().map_or(true, |r| {
        !r["queueMap"].is_object() && !r["queues"].is_array()
    }) || history_rows.is_none();
    let rows = history_rows.cloned().unwrap_or_default();
    let expected = rows.iter().take(20).filter(|r| model::eligible(r)).count();
    let games = stream::iter(
        rows.into_iter()
            .take(20)
            .filter(model::eligible)
            .map(|row| {
                let client = &client;
                let summoner = &summoner;
                async move {
                    let id = row["gameId"].as_u64()?;
                    let detail = get(client, &format!("/lol-match-history/v1/games/{id}"))
                        .await
                        .ok()
                        .filter(|d| d["gameId"].as_u64() == Some(id));
                    model::game(detail.as_ref().unwrap_or(&row), summoner)
                }
            }),
    )
    .buffer_unordered(3)
    .take_until(tokio::time::sleep(Duration::from_secs(8)))
    .filter_map(|g| async move { g })
    .collect::<Vec<_>>()
    .await;
    let history_incomplete = history_rows.is_none() || games.len() < expected;
    incomplete |= history_incomplete;
    let mut games: BTreeMap<_, _> = games.into_iter().map(|g| (g.id, g)).collect();
    if history_incomplete {
        for game in saved.games {
            games.entry(game.id).or_insert(game);
        }
    }
    let mut games: Vec<_> = games.into_values().collect();
    games.sort_by_key(|g| std::cmp::Reverse(g.created_at));
    games.truncate(20);
    let profile = Profile { player: player(&summoner, icon), solo: ranked.as_ref().ok().and_then(model::rank), flex: ranked.as_ref().ok().and_then(flex_rank),
        games, live: true, fetched_at: crate::runes::provider::now_ms(), message: incomplete.then(|| "Some profile data could not be loaded. Saved games may be shown; try again shortly.".into()) };
    let cached = profile.clone();
    let _ = tokio::task::spawn_blocking(move || save(cached)).await;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    #[ignore = "Requires League Client; only GETs, no persistence or player logs."]
    async fn live_lcu_contract() {
        let client = crate::runes::lcu().await.expect("Client unavailable.");
        let current = get(&client, "/lol-summoner/v1/current-summoner")
            .await
            .expect("Current summoner unavailable.");
        let me = model::summoner(&current).expect("Summoner schema changed.");
        let (found, value) = lookup(&client, &me.riot_id)
            .await
            .expect("Alias or summoner schema changed.");
        assert!(found.puuid == me.puuid, "Lookup identity did not match.");
        assert!(
            value["profileIconId"].as_u64().is_some(),
            "Profile icon schema changed."
        );
        let ranked = get(
            &client,
            &format!("/lol-ranked/v1/ranked-stats/{}", me.puuid),
        )
        .await
        .expect("Ranked GET unavailable.");
        assert!(
            ranked["queueMap"].is_object() || ranked["queues"].is_array(),
            "Ranked schema changed."
        );
        let history = get(
            &client,
            &format!(
                "/lol-match-history/v1/products/lol/{}/matches?begIndex=0&endIndex=19",
                me.puuid
            ),
        )
        .await
        .expect("History GET unavailable.");
        assert!(
            history["games"]["games"].is_array(),
            "History schema changed."
        );
    }
    #[test]
    fn ids_and_prefixes_validate_without_accepting_path_segments() {
        assert_eq!(
            riot_id("  Player One#TEST  ").unwrap(),
            ("Player One", "TEST")
        );
        for value in ["", "Player", "#TAG", "Player#", "P#T#X", "P\n#T"] {
            assert!(riot_id(value).is_err());
        }
        assert!(!valid_puuid("../private"));
        assert!(!valid_puuid(""));
        assert!(valid_puuid("fixture-player_1"));
        assert!(prefix("Player One#TEST", " player"));
        assert!(!prefix("Player One#TEST", "one"));
        assert!(!prefix("Player", ""));
    }
    #[test]
    fn queue_parsing_keeps_solo_and_flex_separate() {
        let value = json!({"queueMap":{"RANKED_SOLO_5x5":{"tier":"GOLD","division":"II","leaguePoints":41},"RANKED_FLEX_SR":{"tier":"SILVER","division":"I","leaguePoints":12}}});
        assert_eq!(model::rank(&value).unwrap().tier, "GOLD");
        assert_eq!(flex_rank(&value).unwrap().lp, 12);
        assert!(flex_rank(&json!({})).is_none());
    }
    #[test]
    fn recent_profiles_are_bounded_and_move_to_front() {
        let mut profiles = vec![];
        for index in 0..20 {
            remember(
                &mut profiles,
                saved_profile(None, vec![], &format!("Player {index}#TEST")),
            );
        }
        assert_eq!(profiles.len(), LIMIT);
        assert_eq!(profiles[0].player.riot_id, "Player 19#TEST");
        remember(&mut profiles, saved_profile(None, vec![], "player 17#test"));
        assert_eq!(profiles.len(), LIMIT);
        assert_eq!(profiles[0].player.riot_id, "player 17#test");
        assert_eq!(saved_players(None, &profiles, "PLAYER 17").len(), 1);
    }
    #[test]
    fn saved_games_keep_the_selected_player_perspective_and_latest_relationship() {
        let line = |name: &str, me: bool, ally: bool| {
            json!({"riotId":name,"championId":103,"me":me,"ally":ally,"role":"mid",
            "kills":5,"deaths":2,"assists":3,"cs":100,"damage":1000,"gold":1000,"vision":10,"items":[1]})
        };
        let game: model::Game = serde_json::from_value(json!({"id":42,"queue":"solo","championId":266,"createdAt":1000,"duration":1200,
            "win":true,"kills":1,"deaths":2,"assists":3,"metrics":[1,null,2,null,null,null],"lpChange":21,"placement":1,
            "spells":[4],"players":[line("You#TEST",true,true),line("Rival#TEST",false,false)]})).unwrap();
        let viewed = perspective(&game, "RIVAL#test").unwrap();
        assert!(!viewed.win);
        assert_eq!(viewed.champion_id, 103);
        assert_eq!(viewed.lp_change, None);
        assert_eq!(viewed.placement, 0);
        assert!(viewed.players[1].me);
        assert!(!viewed.players[0].ally);
        assert_eq!(viewed.metrics[2], Some(5.0));
        assert!(viewed.spells.is_empty());
        assert!(perspective(&game, "Missing#TEST").is_none());
        let mut cached = saved_profile(None, vec![], "Another#TEST");
        cached.games.push(game.clone());
        let selected = saved_profile(None, vec![cached], "Rival#TEST");
        assert_eq!(selected.games.len(), 1);
        assert!(!selected.live);
        assert!(!selected.games[0].win);
        let mut newer = game.clone();
        newer.created_at = 2000;
        newer.players[1].ally = true;
        let snapshot = model::Snapshot {
            summoner: model::summoner(
                &json!({"puuid":"fixture","gameName":"You","tagLine":"TEST"}),
            )
            .unwrap(),
            rank: None,
            fetched_at: 2000,
            games: vec![newer, game],
            summaries: Default::default(),
            champion_names: Default::default(),
            lp_history: vec![],
        };
        let hits = saved_players(Some(&snapshot), &[], "rival");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].ally, Some(true));
        assert_eq!(hits[0].last_played, Some(2000));
    }
    #[test]
    fn corrupt_or_oversized_profile_cache_is_ignored() {
        let path = std::env::temp_dir().join(format!("search-test-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"broken").unwrap();
        assert!(read_at(&path).is_empty());
        std::fs::write(&path, vec![b' '; 2 * 1024 * 1024 + 1]).unwrap();
        assert!(read_at(&path).is_empty());
        std::fs::remove_file(path).unwrap();
    }
}
