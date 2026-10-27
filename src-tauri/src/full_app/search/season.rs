//! A player's whole ranked season from u.gg: the League Client lists only the
//! last 20 games, and nothing without it. The profile asks for one page at a
//! time; pages are kept per player so a revisit only fetches what is new.
//! Only the viewed player's Riot ID and region are sent.

use super::super::model::{self, Game, Player};
use crate::runes::ugg_record::{self, SEASON_ID};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
/// u.gg serves 20 games a page; 100 pages bounds the busiest season.
const PAGE_SIZE: usize = 20;
pub const MAX_PAGES: u32 = 100;
const KEEP_PLAYERS: usize = 16;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const QUEUES: [u32; 2] = [420, 440];
const MATCHES: &str = "\
query FetchMatchSummaries($page: Int, $queueType: [Int], $regionId: String!, $riotUserName: String!, $riotTagLine: String!, $seasonIds: [Int]!) {
  fetchPlayerMatchSummaries(page: $page, queueType: $queueType, regionId: $regionId, riotUserName: $riotUserName, riotTagLine: $riotTagLine, seasonIds: $seasonIds) {
    matchSummaries { matchId queueType championId win kills deaths assists cs jungleCs damage visionScore killParticipation
      level matchDuration matchCreationTime role items summonerSpells runes
      teamA { championId riotUserName riotTagLine role } teamB { championId riotUserName riotTagLine role } }
  }
}";
const INFO_FIELDS: &str = "iconId summonerLevel riotUserName riotTagLine regionId";
static FILES: Mutex<()> = Mutex::new(());

/// Who u.gg knows by this Riot ID; `region` is the League Client's code ("EUW").
pub struct Identity {
    pub riot_id: String,
    pub region: &'static str,
    pub icon_id: Option<u64>,
    pub level: Option<u64>,
}

#[derive(Serialize)]
pub struct SeasonPage {
    pub games: Vec<Game>,
    pub done: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    season: i64,
    complete: bool,
    games: Vec<Game>,
}

async fn post(body: Value) -> Result<Value, String> {
    let unavailable = |_| "u.gg is unavailable right now. Try again shortly.".to_string();
    let response = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(unavailable)?
        .post(crate::runes::probuilds::ENDPOINT)
        .json(&body)
        .send()
        .await
        .map_err(unavailable)?;
    if !response.status().is_success() {
        return Err("u.gg is unavailable right now. Try again shortly.".into());
    }
    response.json().await.map_err(|_| "u.gg sent an unreadable answer.".to_string())
}

/// A player u.gg has never seen is an answer, not an outage.
fn unknown_player(body: &Value) -> bool {
    body["errors"][0]["message"] == "player_info_not_found"
}

/// The player in `region`, or wherever u.gg finds the Riot ID when no region is known.
pub async fn identity(name: &str, tag: &str, region: Option<&str>) -> Option<Identity> {
    let body = match region.and_then(ugg_record::region_id) {
        Some(id) => post(json!({
            "query": format!("query I($regionId: String!, $riotUserName: String!, $riotTagLine: String!) {{ profilePlayerInfo(regionId: $regionId, riotUserName: $riotUserName, riotTagLine: $riotTagLine) {{ {INFO_FIELDS} }} }}"),
            "variables": { "regionId": id, "riotUserName": name, "riotTagLine": tag } })).await.ok()?["data"]["profilePlayerInfo"].take(),
        None => post(json!({
            "query": format!("query S($riotUserName: String!, $riotTagLine: String!) {{ crossRegionProfileSearch(riotUserName: $riotUserName, riotTagLine: $riotTagLine) {{ playerInfo {{ {INFO_FIELDS} }} }} }}"),
            "variables": { "riotUserName": name, "riotTagLine": tag } })).await.ok()?["data"]["crossRegionProfileSearch"][0]["playerInfo"].take(),
    };
    parse_identity(&body)
}

fn parse_identity(info: &Value) -> Option<Identity> {
    let name = info["riotUserName"].as_str().filter(|n| !n.is_empty())?;
    let tag = info["riotTagLine"].as_str().filter(|t| !t.is_empty())?;
    Some(Identity {
        riot_id: format!("{name}#{tag}"),
        region: ugg_record::client_region(info["regionId"].as_str()?)?,
        icon_id: info["iconId"].as_u64(),
        level: info["summonerLevel"].as_u64().filter(|level| *level > 0),
    })
}

/// One page (from 1) of the season's Solo/Duo and Flex games, newest first. Once
/// the season is covered, `done` is set and `games` holds every saved game.
pub async fn page(name: &str, tag: &str, client_region: &str, page: u32) -> Result<SeasonPage, String> {
    let region = ugg_record::region_id(client_region).ok_or("u.gg doesn't cover this region.")?;
    let page = page.clamp(1, MAX_PAGES);
    let body = post(json!({ "query": MATCHES, "variables": {
        "page": page, "queueType": QUEUES, "regionId": region, "riotUserName": name, "riotTagLine": tag, "seasonIds": [SEASON_ID] } }))
    .await?;
    let rows = match body["data"]["fetchPlayerMatchSummaries"]["matchSummaries"].as_array() {
        Some(rows) => rows.clone(),
        None if unknown_player(&body) => Vec::new(),
        None => return Err("u.gg sent an unreadable answer.".into()),
    };
    let finished = rows.len() < PAGE_SIZE || page >= MAX_PAGES;
    let games: Vec<Game> = rows.iter().filter_map(|row| game(row, name, tag)).collect();
    let Some(path) = path(region, name, tag) else {
        return Ok(SeasonPage { games, done: finished });
    };
    tokio::task::spawn_blocking(move || merge(&path, games, finished))
        .await
        .map_err(|_| "Saved season games are unavailable.".to_string())
}

fn path(region: &str, name: &str, tag: &str) -> Option<PathBuf> {
    let key = format!("{region}|{name}#{tag}").to_lowercase();
    let file = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key);
    Some(crate::vault::data_root().ok()?.join("profile-seasons").join(format!("{file}.json")))
}

/// Adds a fetched page to the saved season. A page of only known games, once a
/// previous run reached the season's start, means everything older is saved.
fn merge(path: &Path, games: Vec<Game>, finished: bool) -> SeasonPage {
    let _guard = FILES.lock().unwrap_or_else(|e| e.into_inner());
    let mut saved = std::fs::metadata(path)
        .ok()
        .filter(|m| m.len() <= MAX_FILE_BYTES)
        .and_then(|_| serde_json::from_slice::<Saved>(&std::fs::read(path).ok()?).ok())
        .filter(|s| s.season == SEASON_ID)
        .unwrap_or(Saved { season: SEASON_ID, ..Saved::default() });
    let known = !games.is_empty() && games.iter().all(|g| saved.games.iter().any(|s| s.id == g.id));
    let done = finished || (saved.complete && known);
    saved.games.retain(|s| !games.iter().any(|g| g.id == s.id));
    saved.games.extend(games.iter().cloned());
    saved.games.sort_by_key(|g| std::cmp::Reverse(g.created_at));
    saved.complete |= done;
    write(path, &saved);
    SeasonPage { games: if done { saved.games } else { games }, done }
}

fn write(path: &Path, saved: &Saved) {
    let (Some(dir), Ok(bytes)) = (path.parent(), serde_json::to_vec(saved)) else { return };
    if bytes.len() as u64 > MAX_FILE_BYTES || std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let pending = path.with_extension("tmp");
    if std::fs::write(&pending, bytes).is_ok() {
        let _ = std::fs::rename(pending, path);
    }
    // Keep only the most recently viewed players.
    let mut files: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, old) in files.into_iter().skip(KEEP_PLAYERS) {
        let _ = std::fs::remove_file(old);
    }
}

fn role(value: &Value) -> &'static str {
    match value.as_u64() {
        Some(1) => "jungle",
        Some(2) => "support",
        Some(3) => "adc",
        Some(4) => "top",
        Some(5) => "mid",
        _ => "",
    }
}

/// u.gg's summary of the player's game. Other players carry only who and what
/// they played: their stats stay zero, as u.gg doesn't send them.
fn game(row: &Value, name: &str, tag: &str) -> Option<Game> {
    let duration = row["matchDuration"].as_u64().filter(|d| *d >= 300)?;
    let queue = match row["queueType"].as_str()? {
        "ranked_solo_5x5" => "solo",
        "ranked_flex_sr" => "flex",
        _ => return None,
    };
    let n = |key: &str| row[key].as_u64();
    let (kills, deaths, assists) = (n("kills")?, n("deaths")?, n("assists")?);
    let minutes = duration as f64 / 60.0;
    let cs = n("cs").unwrap_or(0) + n("jungleCs").unwrap_or(0);
    let ids = |key: &str| -> Vec<i64> { row[key].as_array().into_iter().flatten().map(|id| id.as_i64().unwrap_or(0)).collect() };
    let is_me = |p: &Value| {
        p["riotUserName"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name))
            && p["riotTagLine"].as_str().is_some_and(|t| t.eq_ignore_ascii_case(tag))
    };
    let teams = [&row["teamA"], &row["teamB"]].map(|team| team.as_array().cloned().unwrap_or_default());
    let players: Vec<Player> = match teams.iter().position(|team| team.iter().any(is_me)) {
        Some(own) => teams[own].iter().map(|p| (p, true)).chain(teams[1 - own].iter().map(|p| (p, false)))
            .map(|(p, ally)| {
                let name = p["riotUserName"].as_str().unwrap_or("");
                Player {
                    champion_id: p["championId"].as_i64().unwrap_or(0),
                    riot_id: match p["riotTagLine"].as_str().filter(|t| !t.is_empty() && !name.is_empty()) {
                        Some(tag) => format!("{name}#{tag}"),
                        None => name.to_string(),
                    },
                    puuid: String::new(),
                    me: is_me(p),
                    ally,
                    role: role(&p["role"]).into(),
                    kills: 0, deaths: 0, assists: 0, cs: 0, damage: 0, gold: 0, vision: 0,
                    items: Vec::new(),
                }
            })
            .collect(),
        None => Vec::new(),
    };
    let mut items = ids("items");
    items.resize(7, 0);
    Some(Game {
        id: n("matchId")?,
        queue: queue.into(),
        champion_id: row["championId"].as_i64()?,
        created_at: n("matchCreationTime")?,
        duration,
        win: row["win"].as_bool()?,
        kills,
        deaths,
        assists,
        metrics: [
            Some((kills + assists) as f64 / deaths.max(1) as f64),
            n("killParticipation").map(|kp| kp.min(100) as f64),
            Some(cs as f64 / minutes),
            n("damage").map(|d| d as f64 / minutes),
            None,
            n("visionScore").map(|v| v as f64 / minutes),
        ],
        items,
        spells: ids("summonerSpells"),
        keystone: ids("runes").first().copied().unwrap_or(0),
        cs,
        level: n("level").unwrap_or(0),
        role: role(&row["role"]).into(),
        placement: 0,
        lp_change: None,
        opponent: model::lane_opponent(&players),
        players,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team(names: &[(&str, i64, u64)]) -> Value {
        json!(names.iter().map(|(n, c, r)| json!({"riotUserName": n, "riotTagLine": "EUW", "championId": c, "role": r})).collect::<Vec<_>>())
    }

    fn row() -> Value {
        json!({"matchId": 1234567890u64, "queueType": "ranked_solo_5x5", "championId": 236, "win": true, "kills": 8, "deaths": 3,
            "assists": 12, "cs": 248, "jungleCs": 8, "damage": 23692, "visionScore": 24, "killParticipation": 56, "level": 17,
            "matchDuration": 1955, "matchCreationTime": 1793109546025u64, "role": 3, "items": [1055, 3508, 3031, 6675, 3036, 1053, 3363],
            "summonerSpells": [4, 21], "runes": [8005, 8009],
            "teamA": team(&[("a", 43, 2), ("b", 91, 5), ("c", 120, 1), ("d", 22, 3), ("e", 13, 4)]),
            "teamB": team(&[("f", 902, 2), ("g", 64, 1), ("h", 112, 5), ("i", 777, 4), ("Me", 236, 3)])})
    }

    #[test]
    fn maps_a_u_gg_summary_from_the_players_side() {
        let game = game(&row(), "me", "euw").unwrap();
        assert_eq!((game.id, game.queue.as_str(), game.cs, game.role.as_str()), (1234567890, "solo", 256, "adc"));
        assert_eq!(game.metrics[1], Some(56.0));
        assert_eq!(game.keystone, 8005);
        assert_eq!(game.players.len(), 10);
        assert!(game.players[..5].iter().all(|p| p.ally) && game.players[0].riot_id == "f#EUW");
        assert!(game.players.iter().filter(|p| p.me).count() == 1);
        assert_eq!(game.opponent.map(|i| game.players[i as usize].champion_id), Some(22));
    }

    #[test]
    fn skips_remakes_and_other_queues_but_keeps_games_without_the_roster() {
        let mut remake = row();
        remake["matchDuration"] = json!(200);
        assert!(game(&remake, "me", "euw").is_none());
        let mut aram = row();
        aram["queueType"] = json!("normal_aram");
        assert!(game(&aram, "me", "euw").is_none());
        let renamed = game(&row(), "someone else", "euw").unwrap();
        assert!(renamed.players.is_empty() && renamed.opponent.is_none());
    }

    #[test]
    fn reads_a_player_identity_with_its_client_region() {
        let found = parse_identity(&json!({"riotUserName": "Example", "riotTagLine": "ABC", "regionId": "euw1", "iconId": 7145, "summonerLevel": 1238})).unwrap();
        assert_eq!((found.riot_id.as_str(), found.region, found.icon_id, found.level), ("Example#ABC", "EUW", Some(7145), Some(1238)));
        assert!(parse_identity(&json!({"riotUserName": "A", "riotTagLine": "B", "regionId": "mars"})).is_none());
        assert!(parse_identity(&Value::Null).is_none());
    }

    fn stub(id: u64, at: u64) -> Game {
        let mut game = game(&row(), "me", "euw").unwrap();
        (game.id, game.created_at) = (id, at);
        game
    }

    #[test]
    fn a_revisit_stops_at_known_games_only_after_a_full_season_was_saved() {
        let dir = std::env::temp_dir().join(format!("swapper-season-{}", std::process::id()));
        let path = dir.join("p.json");
        let _ = std::fs::remove_dir_all(&dir);
        let first = merge(&path, vec![stub(3, 30), stub(2, 20)], false);
        assert!(!first.done && first.games.len() == 2);
        // Interrupted before the season's start: known games don't end the walk.
        assert!(!merge(&path, vec![stub(3, 30), stub(2, 20)], false).done);
        let last = merge(&path, vec![stub(1, 10)], true);
        assert!(last.done);
        assert_eq!(last.games.iter().map(|g| g.id).collect::<Vec<_>>(), vec![3, 2, 1]);
        let revisit = merge(&path, vec![stub(4, 40), stub(3, 30)], false);
        assert!(!revisit.done && revisit.games.len() == 2);
        let rest = merge(&path, vec![stub(2, 20), stub(1, 10)], false);
        assert!(rest.done);
        assert_eq!(rest.games.iter().map(|g| g.id).collect::<Vec<_>>(), vec![4, 3, 2, 1]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Set `SWAPPER_LIVE_RIOT_ID` to a ranked player's `Name#TAG` to run it.
    #[test]
    #[ignore = "live u.gg request"]
    fn live_u_gg_season_page() {
        let Ok(id) = std::env::var("SWAPPER_LIVE_RIOT_ID") else { return };
        let (name, tag) = id.split_once('#').expect("Name#TAG");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let found = rt.block_on(identity(name, tag, None)).expect("identity");
        let first = rt.block_on(page(name, tag, found.region, 1)).expect("page");
        assert!(!first.games.is_empty());
    }
}
