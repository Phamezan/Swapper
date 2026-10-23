//! An MMR estimate: the average Solo/Duo rank of the other players in the
//! account's recent Solo/Duo games. Riot does not expose MMR, so this is
//! Swapper's own approximation, labelled as such in the UI. Lobbies are
//! recorded when a game starts (ranks as they were then); games from before
//! that fall back to the players' current ranks.

use super::lp;
use super::model::{Game, Rank};
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// Recent Solo/Duo games averaged; the trend compares the newer half with the older.
const GAMES: usize = 10;
/// A lobby counts once this many of its other players have a known rank.
const MIN_RANKED: usize = 4;
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(4);
/// Recorded lobbies kept, newest game ids first.
const RECORDED_GAMES: usize = 50;
const SOLO_QUEUE_ID: i64 = 420;

/// Other players' ladder scores per game id, as they were at game start.
type Lobbies = BTreeMap<u64, Vec<i64>>;
static LOBBY_FILE: Mutex<()> = Mutex::new(());

fn lobby_path() -> Option<std::path::PathBuf> {
    Some(crate::vault::data_root().ok()?.join("lobby-ranks.json"))
}

fn load_lobbies(path: &Path) -> Lobbies {
    if std::fs::metadata(path).map_or(true, |m| m.len() > 512 * 1024) {
        return Lobbies::new();
    }
    std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn save_lobby_at(path: &Path, game_id: u64, scores: Vec<i64>) {
    let _guard = LOBBY_FILE.lock().unwrap_or_else(|e| e.into_inner());
    let mut lobbies = load_lobbies(path);
    lobbies.insert(game_id, scores);
    while lobbies.len() > RECORDED_GAMES {
        lobbies.pop_first();
    }
    if let Ok(bytes) = serde_json::to_vec(&lobbies) {
        let pending = path.with_extension("tmp");
        if std::fs::write(&pending, bytes).is_ok() {
            let _ = std::fs::rename(pending, path);
        }
    }
}

// shortcut: ranks are cached for the app run, so a player's later climb is missed; add a TTL if that matters.
static RANKS: Mutex<Option<HashMap<String, Option<i64>>>> = Mutex::new(None);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mmr {
    #[serde(flatten)]
    pub rank: Rank,
    /// Lobbies the average covers.
    pub games: usize,
    /// Ladder LP the newer half of those lobbies sits above (negative: below) the older half.
    pub trend: Option<i64>,
}

/// The estimate from each lobby's known player scores, newest game first.
fn estimate(lobbies: &[Vec<i64>]) -> Option<Mmr> {
    let averages: Vec<i64> = lobbies
        .iter()
        .filter(|scores| scores.len() >= MIN_RANKED)
        .take(GAMES)
        .map(|scores| scores.iter().sum::<i64>() / scores.len() as i64)
        .collect();
    if averages.is_empty() {
        return None;
    }
    let mean = |scores: &[i64]| scores.iter().sum::<i64>() / scores.len() as i64;
    let half = averages.len() / 2;
    let trend = (half >= 2).then(|| mean(&averages[..half]) - mean(&averages[half..]));
    let mut rank = lp::rank_at(mean(&averages), "MASTER");
    rank.wins = 0;
    rank.losses = 0;
    Some(Mmr { rank, games: averages.len(), trend })
}

/// Solo/Duo ladder scores for `puuids` (None: unranked), four at a time;
/// failed lookups are left out.
async fn ranks(client: &crate::runes::Lcu, puuids: Vec<String>) -> Vec<(String, Option<i64>)> {
    stream::iter(puuids.into_iter().map(|puuid| async move {
        let path = format!("/lol-ranked/v1/ranked-stats/{puuid}");
        let value = tokio::time::timeout(LOOKUP_TIMEOUT, crate::runes::lcu_get::<Value>(client, &path)).await;
        match value {
            Ok(Ok(value)) => Some((puuid, super::model::rank(&value).map(|rank| lp::score(&rank)))),
            // A failed lookup is retried on a later refresh instead of cached as unranked.
            _ => None,
        }
    }))
    .buffer_unordered(4)
    .filter_map(|found| async move { found })
    .collect()
    .await
}

/// Ranks for `puuids` not looked up yet this run, cached for the run.
async fn look_up(client: &crate::runes::Lcu, puuids: Vec<String>) {
    let found = ranks(client, puuids).await;
    let mut cache = RANKS.lock().unwrap_or_else(|e| e.into_inner());
    cache.get_or_insert_with(HashMap::new).extend(found);
}

/// Records the ranks of the other players in a Solo/Duo game that just
/// started, so the estimate later uses ranks as they were in that game.
pub async fn record_lobby() {
    let Ok(client) = crate::runes::lcu().await else { return };
    let (Ok(session), Ok(me)) = tokio::join!(
        crate::runes::lcu_get::<Value>(&client, "/lol-gameflow/v1/session"),
        crate::runes::lcu_get::<Value>(&client, "/lol-summoner/v1/current-summoner"),
    ) else {
        return;
    };
    let data = &session["gameData"];
    let (Some(game_id), Some(me)) = (data["gameId"].as_u64().filter(|id| *id > 0), me["puuid"].as_str()) else { return };
    if data["queue"]["id"].as_i64() != Some(SOLO_QUEUE_ID) {
        return;
    }
    let puuids: Vec<String> = ["teamOne", "teamTwo"]
        .iter()
        .flat_map(|team| data[*team].as_array().into_iter().flatten())
        .filter_map(|p| p["puuid"].as_str())
        .filter(|puuid| *puuid != me && valid_puuid(puuid))
        .map(str::to_string)
        .collect();
    let scores: Vec<i64> = ranks(&client, puuids).await.into_iter().filter_map(|(_, score)| score).collect();
    if scores.len() >= MIN_RANKED {
        if let Some(path) = lobby_path() {
            let _ = tokio::task::spawn_blocking(move || save_lobby_at(&path, game_id, scores)).await;
        }
    }
}

/// The signed-in account's estimate for its `games` (newest first): recorded
/// lobbies where Swapper saw the game start, current ranks otherwise.
pub async fn current(client: &crate::runes::Lcu, games: &[Game]) -> Option<Mmr> {
    let recorded = lobby_path().map(|path| load_lobbies(&path)).unwrap_or_default();
    estimate_from(client, games, &recorded).await
}

/// Another player's estimate from their `games` (seen from their side, `me` is
/// them), always from current ranks: recorded lobbies belong to the signed-in account.
pub async fn for_player(client: &crate::runes::Lcu, games: &[Game]) -> Option<Mmr> {
    estimate_from(client, games, &Lobbies::new()).await
}

async fn estimate_from(client: &crate::runes::Lcu, games: &[Game], recorded: &Lobbies) -> Option<Mmr> {
    let recent: Vec<&Game> = games.iter().filter(|g| g.queue == "solo").take(GAMES).collect();
    let others = |game: &&Game| -> Vec<String> {
        game.players.iter().filter(|p| !p.me && valid_puuid(&p.puuid)).map(|p| p.puuid.clone()).collect()
    };
    let missing: Vec<String> = {
        let cache = RANKS.lock().unwrap_or_else(|e| e.into_inner());
        let mut missing: Vec<String> = recent
            .iter()
            .filter(|game| !recorded.contains_key(&game.id))
            .flat_map(others)
            .filter(|puuid| !cache.as_ref().is_some_and(|c| c.contains_key(puuid)))
            .collect();
        missing.sort();
        missing.dedup();
        missing
    };
    look_up(client, missing).await;
    let cache = RANKS.lock().unwrap_or_else(|e| e.into_inner());
    let known = |puuid: &String| cache.as_ref().and_then(|c| c.get(puuid).copied().flatten());
    let lobbies: Vec<Vec<i64>> = recent
        .iter()
        .map(|game| match recorded.get(&game.id) {
            Some(scores) => scores.clone(),
            None => others(game).iter().filter_map(known).collect(),
        })
        .collect();
    estimate(&lobbies)
}

/// Only plain PUUIDs reach the client's API path.
fn valid_puuid(puuid: &str) -> bool {
    !puuid.is_empty() && puuid.len() <= 128 && puuid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_lobbies_and_compares_the_newer_half() {
        // Diamond II 0 = 2600, Diamond III 50 = 2550.
        let newer = vec![2600, 2600, 2600, 2600];
        let older = vec![2550, 2550, 2550, 2550, 2550];
        let mmr = estimate(&[newer.clone(), newer, older.clone(), older]).unwrap();
        assert_eq!((mmr.rank.tier.as_str(), mmr.rank.division.as_str(), mmr.rank.lp), ("DIAMOND", "III", 75));
        assert_eq!((mmr.games, mmr.trend), (4, Some(50)));
    }

    #[test]
    fn needs_enough_ranked_players_and_reads_apex_as_master() {
        assert!(estimate(&[vec![2600, 2600, 2600]]).is_none(), "three ranked players is not a lobby");
        let apex = estimate(&[vec![2813; 9]]).unwrap();
        assert_eq!((apex.rank.tier.as_str(), apex.rank.lp, apex.trend), ("MASTER", 13, None));
        assert!(estimate(&[]).is_none());
    }

    #[test]
    fn recorded_lobbies_keep_the_newest_games() {
        let path = std::env::temp_dir().join(format!("lobby-ranks-{}.json", uuid::Uuid::new_v4()));
        for id in 0..RECORDED_GAMES as u64 + 3 {
            save_lobby_at(&path, id, vec![2600; 9]);
        }
        let lobbies = load_lobbies(&path);
        assert_eq!(lobbies.len(), RECORDED_GAMES);
        assert_eq!(lobbies.keys().next(), Some(&3), "the oldest game ids drop first");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn only_plain_puuids_are_looked_up() {
        assert!(valid_puuid("fixture-Puuid_0123"));
        assert!(!valid_puuid("../../lol-summoner"));
        assert!(!valid_puuid(""));
    }
}
