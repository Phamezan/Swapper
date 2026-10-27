use super::{online, Player};
use crate::live_game::model::{Player as GamePlayer, Side, Status, TeamSummary, View};
use futures_util::{stream, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Mutex, time::{Duration, Instant}};

const FRESH: Duration = Duration::from_secs(30);
const ENRICH_CONCURRENCY: usize = 10;
const ENRICH_BUDGET: Duration = Duration::from_secs(20);
const EXISTS: &str = "query($n:String!,$t:String!,$r:String!){liveGameExists(riotUserName:$n,riotTagLine:$t,regionId:$r)}";
const GAME: &str = "query($n:String!,$t:String!,$r:String!){getLiveGame(riotUserName:$n,riotTagLine:$t,regionId:$r){gameLengthSeconds queueId gameType teamA{isAnonEnabled riotUserName riotTagLine championId summonerSpellA summonerSpellB summonerRuneA summonerRuneB} teamB{isAnonEnabled riotUserName riotTagLine championId summonerSpellA summonerSpellB summonerRuneA summonerRuneB}}}";
static FRIENDS: Mutex<Option<(Instant, Vec<Player>)>> = Mutex::new(None);
static FRIEND_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static PRESENCE: Mutex<Option<online::Cache<Option<bool>>>> = Mutex::new(None);
static GAMES: Mutex<Option<online::Cache<Option<View>>>> = Mutex::new(None);
static KNOWN: Mutex<BTreeMap<String, (Instant, bool)>> = Mutex::new(BTreeMap::new());
static GATE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveStatus {
    live: bool,
    view: Option<View>,
    message: Option<&'static str>,
}

fn remember(id: &str, live: bool) {
    if let Ok(mut cache) = KNOWN.lock() {
        cache.retain(|_, (at, _)| at.elapsed() < FRESH);
        if cache.len() >= 64 { cache.clear(); }
        cache.insert(id.to_lowercase(), (Instant::now(), live));
    }
}

pub(super) fn cached_friends() -> Vec<Player> {
    FRIENDS.lock().ok().and_then(|g| g.as_ref().filter(|(at, _)| at.elapsed() < Duration::from_secs(15)).map(|(_, players)| players.clone())).unwrap_or_default()
}

pub(super) fn known(id: &str) -> bool {
    if let Some((_, live)) = KNOWN.lock().ok().and_then(|cache| cache.get(&id.to_lowercase()).copied()).filter(|(at, _)| at.elapsed() < FRESH) { return live; }
    cached_friends().iter().any(|p| p.live && p.riot_id.to_lowercase() == id.to_lowercase())
}

fn parse_friends(value: &Value) -> Vec<Player> {
    let Some(rows) = value.as_array().filter(|rows| rows.len() <= 2000) else { return vec![] };
    rows.iter().filter_map(|row| {
        let id = online::identity(row["gameName"].as_str()?, row["gameTag"].as_str()?)?;
        let league = row["product"].as_str().is_none_or(|p| matches!(p, "league_of_legends" | ""));
        Some(Player { riot_id: id, icon: None, level: None, ally: None, last_played: None,
            live: league && row["lol"]["gameStatus"] == "inGame" })
    }).collect()
}

pub(super) async fn friends(client: &crate::runes::Lcu) -> Vec<Player> {
    let _guard = FRIEND_GATE.lock().await;
    if let Some(players) = FRIENDS.lock().ok().and_then(|g| g.as_ref().filter(|(at, _)| at.elapsed() < Duration::from_secs(15)).map(|(_, players)| players.clone())) { return players; }
    let players = super::get(client, "/lol-chat/v1/friends").await.ok().map(|v| parse_friends(&v)).unwrap_or_default();
    if let Ok(mut cache) = FRIENDS.lock() { *cache = Some((Instant::now(), players.clone())); }
    players
}

fn no_errors(value: &Value) -> bool {
    value.get("errors").is_none_or(|e| e.as_array().is_some_and(Vec::is_empty))
}

fn parse_exists(value: &Value) -> Option<bool> {
    no_errors(value).then(|| value.pointer("/data/liveGameExists")?.as_bool()).flatten()
}

/// For a real game that is not two teams of five (Arena, Swarm…), the reason the roster can't be shown.
fn unsupported_mode(value: &Value) -> Option<&'static str> {
    if !no_errors(value) { return None; }
    let game = value.pointer("/data/getLiveGame")?;
    let five_v_five = ["teamA", "teamB"].iter().all(|key| game[*key].as_array().is_some_and(|team| team.len() == 5));
    if five_v_five { return None; }
    Some(match game["queueId"].as_i64()? {
        1700 | 1710 => "Live players for Arena games aren't supported yet. Swapper shows 5v5 games.",
        1810..=1840 => "Live players for Swarm games aren't supported yet. Swapper shows 5v5 games.",
        _ => "Live players for this game mode aren't supported yet. Swapper shows 5v5 games.",
    })
}

fn parse_game(value: &Value, id: &str) -> Option<View> {
    if !no_errors(value) { return None; }
    let game = value.pointer("/data/getLiveGame")?;
    let queue_id = game["queueId"].as_i64().filter(|id| (0..=10000).contains(id))?;
    let mut players = Vec::new();
    for (key, side) in [("teamA", Side::Blue), ("teamB", Side::Red)] {
        let team = game[key].as_array().filter(|p| p.len() == 5)?;
        for (index, entry) in team.iter().enumerate() {
            let anonymous = entry["isAnonEnabled"].as_bool()?;
            let riot_id = if anonymous { String::new() } else {
                online::identity(entry["riotUserName"].as_str()?, entry["riotTagLine"].as_str()?)?
            };
            if !riot_id.is_empty() && players.iter().any(|p: &GamePlayer| p.riot_id.to_lowercase() == riot_id.to_lowercase()) { return None; }
            let mut player = GamePlayer::new(format!("{key}:{index}"), side);
            player.riot_id = riot_id;
            player.champion_id = entry["championId"].as_i64().filter(|id| (1..=10000).contains(id))?;
            let spell = |key: &str| entry[key].as_i64().filter(|id| (0..=10000).contains(id));
            player.spells = [spell("summonerSpellA")?, spell("summonerSpellB")?];
            player.primary_style = spell("summonerRuneA").unwrap_or(0);
            player.secondary_style = spell("summonerRuneB").unwrap_or(0);
            // A roster has no PUUID or game ID; enrich by exact Riot ID only when opened.
            player.status = if anonymous { Status::Skipped } else { Status::Partial };
            players.push(player);
        }
    }
    if !players.iter().any(|p| p.riot_id.to_lowercase() == id.to_lowercase()) { return None; }
    let queue = match queue_id { 420 => "Ranked Solo/Duo", 440 => "Ranked Flex", 450 => "ARAM", 400 | 430 | 490 => "Normal", _ => "Live game" };
    Some(View { game_id: 0, queue_id, queue: queue.into(), map_id: 0, players,
        blue: TeamSummary::default(), red: TeamSummary::default(), game_time: None })
}

async fn request(query: &str, region: &str, id: &str) -> Option<Value> {
    let (name, tag) = super::riot_id(id).ok()?;
    let body = json!({"query": query, "variables":{"n":name,"t":tag,"r":region}});
    serde_json::from_str(&online::post("https://u.gg/api", &body, None).await?).ok()
}

async fn exists(region: &str, id: &str) -> Option<bool> {
    let key = format!("{region}:{}", id.to_lowercase());
    {
        let mut guard = PRESENCE.lock().ok()?;
        let cache = guard.get_or_insert_with(online::Cache::default);
        if let Some(live) = cache.get(&key, FRESH) { return live; }
        if !cache.reserve(12) { return None; }
        // Reserve a failed/unknown check too, so repeated opens never hammer the source.
        cache.put(key.clone(), None);
    }
    let live = parse_exists(&request(EXISTS, region, id).await?)?;
    if let Ok(mut cache) = PRESENCE.lock() { cache.get_or_insert_with(online::Cache::default).put(key, Some(live)); }
    remember(id, live);
    Some(live)
}

async fn enrich(client: &crate::runes::Lcu, view: &mut View) {
    let names = super::super::read_cache().map(|s| s.champion_names).unwrap_or_default();
    let players = view.players.clone();
    let updates = stream::iter(players.into_iter().map(|mut p| async move {
        if p.riot_id.is_empty() { return p; }
        if let Ok((summoner, value)) = super::lookup(client, &p.riot_id).await {
            p.puuid = summoner.puuid;
            // u.gg answers for all ten at once; the client's history, served one player at a time, only fills gaps.
            let ((_, rank), quick) = tokio::join!(crate::live_game::profile(&p.puuid), crate::live_game::quick_history(&p.riot_id));
            p.apply(Some(&value), rank.as_ref(), None);
            match quick.filter(|rows| !rows.is_empty()) {
                Some(rows) => p.apply_quick(&rows),
                None => if let Some(rows) = crate::live_game::history(&p.puuid).await { p.apply(None, None, Some(&rows)) },
            }
        }
        p
    // All ten at once. Players unfinished after the budget
    // keep their slot without rank/form rather than holding the page.
    })).buffer_unordered(ENRICH_CONCURRENCY).take_until(tokio::time::sleep(ENRICH_BUDGET)).collect::<Vec<_>>().await;
    for update in updates {
        if let Some(player) = view.players.iter_mut().find(|p| p.key == update.key) { *player = update; }
    }
    for player in &mut view.players {
        player.champion_name = names.get(&player.champion_id).cloned().unwrap_or_else(|| format!("Champion {}", player.champion_id));
        player.is_self = false;
        player.live = None;
    }
    view.summarize_teams();
}

pub(super) async fn status(client: &crate::runes::Lcu, id: &str, details: bool) -> LiveStatus {
    let Ok(_permit) = GATE.try_acquire() else {
        return LiveStatus { live: known(id), view: None, message: Some("Live players are unavailable. Try again shortly.") };
    };
    let (region, friends) = tokio::join!(super::get(client, "/riotclient/region-locale"), friends(client));
    let friend_live = friends.iter().any(|p| p.live && p.riot_id.to_lowercase() == id.to_lowercase());
    let region = region.ok().and_then(|v| v["region"].as_str().and_then(crate::runes::ugg_record::region_id));
    let mut result = LiveStatus { live: friend_live || known(id), view: None, message: None };
    let Some(region) = region else { return result; };
    if !details {
        if let Some(live) = exists(region, id).await { result.live = live || friend_live; }
        remember(id, result.live);
        return result;
    }
    let key = format!("{region}:{}", id.to_lowercase());
    {
        let Ok(mut guard) = GAMES.lock() else { return result };
        let cache = guard.get_or_insert_with(online::Cache::default);
        if let Some(view) = cache.get(&key, FRESH) {
            result.live |= view.is_some(); result.view = view;
            result.message = result.view.is_none().then_some("Live players are unavailable. Try again shortly.");
            return result;
        }
        if !cache.reserve(12) { result.message = Some("Live players are unavailable. Try again shortly."); return result; }
        cache.put(key.clone(), None);
    }
    let response = request(GAME, region, id).await;
    if let Some(mut view) = response.as_ref().and_then(|v| parse_game(v, id)) {
        result.live = true;
        remember(id, true);
        // Keep the roster even if the optional rank/history reads exceed the total budget.
        enrich(client, &mut view).await;
        result.view = Some(view);
    } else if let Some(reason) = response.as_ref().and_then(unsupported_mode) {
        // They are in a game, just not one with a 5v5 roster to show.
        result.live = true;
        result.message = Some(reason);
    } else { result.message = Some("Live players are unavailable. The game may have ended; try again shortly."); }
    if let Ok(mut cache) = GAMES.lock() { cache.get_or_insert_with(online::Cache::default).put(key, result.view.clone()); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[tokio::test]
    #[ignore = "Requires League Client; reads friend presence, never logs player identifiers."]
    async fn live_lcu_presence_contract() {
        let client = crate::runes::lcu().await.expect("Client unavailable.");
        let value = super::super::get(&client, "/lol-chat/v1/friends").await.expect("Friends unavailable.");
        let rows = value.as_array().expect("Friend-list contract changed.");
        assert!(rows.len() <= 2000);
        assert!(rows.iter().all(|r| r["lol"].get("gameStatus").is_none_or(Value::is_string)));
        assert!(rows.is_empty() || !parse_friends(&value).is_empty());
    }
    #[tokio::test]
    #[ignore = "Requires League Client and public u.gg; read-only checks, no player logs."]
    async fn live_ugg_game_contract() {
        let client = crate::runes::lcu().await.expect("Client unavailable.");
        let region = super::super::get(&client, "/riotclient/region-locale").await.expect("Region unavailable.");
        let region = region["region"].as_str().and_then(crate::runes::ugg_record::region_id).expect("Unsupported region.");
        let friends = friends(&client).await;
        let id = if let Some(friend) = friends.iter().find(|p| p.live) { friend.riot_id.clone() } else {
            let me = super::super::get(&client, "/lol-summoner/v1/current-summoner").await.expect("Current player unavailable.");
            crate::full_app::model::summoner(&me).expect("Summoner schema changed.").riot_id
        };
        let data = request(EXISTS, region, &id).await.expect("u.gg check unavailable.");
        if parse_exists(&data).expect("u.gg presence contract changed.") {
            let data = request(GAME, region, &id).await.expect("u.gg roster unavailable.");
            let mut view = parse_game(&data, &id).expect("u.gg roster contract changed.");
            enrich(&client, &mut view).await;
            assert_eq!(view.players.len(), 10);
            println!("Live roster: 10 players, {} with ranks, {} with recent form.",
                view.players.iter().filter(|p| p.solo.is_some()).count(), view.players.iter().filter(|p| p.history.is_some()).count());
        }
    }
    #[test]
    fn friend_presence_requires_lol_in_game_and_valid_names() {
        let friends = parse_friends(&serde_json::from_str(include_str!("fixtures/friends.json")).unwrap());
        assert_eq!(friends.len(), 2);
        assert!(friends[0].live);
        assert!(!friends[1].live);
        assert!(parse_friends(&json!({})).is_empty());
    }
    #[test]
    fn live_exists_requires_a_boolean_and_no_graphql_errors() {
        assert_eq!(parse_exists(&serde_json::from_str(include_str!("fixtures/ugg-presence.json")).unwrap()), Some(true));
        assert_eq!(parse_exists(&json!({"data":{"liveGameExists":false}})), Some(false));
        assert_eq!(parse_exists(&json!({"data":{"liveGameExists":true}})), Some(true));
        assert_eq!(parse_exists(&json!({"data":{"liveGameExists":"true"}})), None);
        assert_eq!(parse_exists(&json!({"data":{"liveGameExists":true},"errors":[{}]})), None);
    }
    #[test]
    fn live_roster_is_validated_as_two_teams_containing_the_requested_player() {
        let mut data: serde_json::Value = serde_json::from_str(include_str!("fixtures/ugg-live.json")).unwrap();
        let view = parse_game(&data, "Example 0#TAG").unwrap();
        assert_eq!(view.players.len(), 10);
        assert!(view.players.iter().all(|p| !p.is_self && p.live.is_none()));
        assert_eq!(view.game_time, None);
        assert!(parse_game(&data, "Missing#TAG").is_none());
        data["data"]["getLiveGame"]["teamA"][0]["championId"] = json!(-1);
        assert!(parse_game(&data, "Example 0#TAG").is_none());
        data["data"]["getLiveGame"]["teamA"][0]["championId"] = json!(103);
        data["data"]["getLiveGame"]["teamB"][0] = data["data"]["getLiveGame"]["teamA"][0].clone();
        assert!(parse_game(&data, "Example 0#TAG").is_none());
    }
    #[test]
    fn non_five_v_five_modes_get_a_named_reason_instead_of_a_generic_error() {
        let team = |n: usize| serde_json::Value::Array(vec![serde_json::json!({}); n]);
        let game = |queue: i64, a: usize, b: usize| serde_json::json!({ "data": { "getLiveGame": { "queueId": queue, "teamA": team(a), "teamB": team(b) } } });
        assert!(unsupported_mode(&game(1700, 8, 8)).unwrap().contains("Arena"));
        assert!(unsupported_mode(&game(1820, 4, 0)).unwrap().contains("Swarm"));
        assert!(unsupported_mode(&game(9999, 3, 3)).unwrap().contains("this game mode"));
        assert_eq!(unsupported_mode(&game(420, 5, 5)), None, "a 5v5 game is supported");
        assert_eq!(unsupported_mode(&serde_json::json!({ "errors": [{}], "data": null })), None);
    }

    #[test]
    fn malformed_rosters_never_reach_the_player_view() {
        let original: Value = serde_json::from_str(include_str!("fixtures/ugg-live.json")).unwrap();
        for (key, invalid) in [("riotUserName", json!("Example#Injected")), ("riotTagLine", json!("")),
            ("championId", json!(10001)), ("summonerSpellA", json!(-1)), ("summonerSpellB", json!("4"))] {
            let mut data = original.clone();
            data["data"]["getLiveGame"]["teamA"][0][key] = invalid;
            assert!(parse_game(&data, "Example 0#TAG").is_none());
        }
        let mut data = original.clone();
        data["data"]["getLiveGame"]["teamB"].as_array_mut().unwrap().pop();
        assert!(parse_game(&data, "Example 0#TAG").is_none());
        assert!(parse_game(&json!({"data": original["data"], "errors":[{}]}), "Example 0#TAG").is_none());
    }
    #[test]
    fn anonymous_slots_keep_the_champion_without_exposing_or_looking_up_an_identity() {
        let mut data: Value = serde_json::from_str(include_str!("fixtures/ugg-live.json")).unwrap();
        for i in 0..2 {
            let row = &mut data["data"]["getLiveGame"]["teamB"][i];
            row["isAnonEnabled"] = json!(true);
            row["riotUserName"] = json!(""); row["riotTagLine"] = json!("");
        }
        let view = parse_game(&data, "Example 0#TAG").expect("Hidden players must not discard a valid game.");
        assert_eq!(view.players.len(), 10);
        assert!(view.players[5].riot_id.is_empty());
        assert_eq!(view.players[5].status, Status::Skipped);
    }
}
