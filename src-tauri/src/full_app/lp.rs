//! Solo/Duo LP history per account. The League client only reports the current
//! LP, so Swapper keeps its own record: one point whenever the ranked stats it
//! reads differ from the last point (dashboard refresh and shortly after each
//! game). A game's LP change is the difference between the two points around
//! it when exactly one ranked game separates them.

use super::model::{Game, Rank};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MAX_POINTS: usize = 500;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const TIERS: [&str; 7] = [
    "IRON", "BRONZE", "SILVER", "GOLD", "PLATINUM", "EMERALD", "DIAMOND",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    pub at: u64,
    #[serde(flatten)]
    pub rank: Rank,
}

type History = BTreeMap<String, Vec<Point>>;

fn path() -> Option<PathBuf> {
    Some(crate::vault::data_root().ok()?.join("lp-history.json"))
}

fn load(path: &Path) -> History {
    if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_FILE_BYTES) {
        return History::new();
    }
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Appends `rank` for `puuid` unless it equals the last point. Returns the
/// account's points, oldest first.
fn record_at(path: &Path, puuid: &str, rank: Rank, at: u64) -> Vec<Point> {
    let mut history = load(path);
    let points = history.entry(puuid.to_string()).or_default();
    if points.last().map(|p| &p.rank) != Some(&rank) {
        points.push(Point { at, rank });
        let excess = points.len().saturating_sub(MAX_POINTS);
        points.drain(..excess);
        if let Ok(bytes) = serde_json::to_vec(&history) {
            let pending = path.with_extension("tmp");
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::write(&pending, bytes).is_ok() {
                let _ = std::fs::rename(&pending, path);
            }
        }
    }
    history.remove(puuid).unwrap_or_default()
}

pub fn record(puuid: &str, rank: Option<Rank>) -> Vec<Point> {
    let Some(path) = path() else {
        return Vec::new();
    };
    match rank {
        Some(rank) => record_at(
            &path,
            puuid,
            rank,
            crate::runes::provider::now_ms().max(0) as u64,
        ),
        None => load(&path).remove(puuid).unwrap_or_default(),
    }
}

/// LP on one ladder: 100 per division, apex tiers share the top base.
fn score(rank: &Rank) -> i64 {
    let tier = TIERS
        .iter()
        .position(|t| *t == rank.tier)
        .unwrap_or(TIERS.len()) as i64;
    let division = match rank.division.as_str() {
        "III" => 1,
        "II" => 2,
        "I" => 3,
        _ => 0,
    };
    let base = if tier < TIERS.len() as i64 {
        tier * 400 + division * 100
    } else {
        TIERS.len() as i64 * 400
    };
    base + rank.lp as i64
}

/// Sets `lp_change` on Solo/Duo games that sit alone between two points one
/// game apart. Everything else (unrecorded gaps, remakes) stays `None`.
pub fn annotate(games: &mut [Game], points: &[Point]) {
    for pair in points.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if b.rank.wins + b.rank.losses != a.rank.wins + a.rank.losses + 1 {
            continue;
        }
        let ended = |g: &Game| g.created_at + g.duration * 1000;
        let mut inside = games
            .iter_mut()
            .filter(|g| g.queue == "solo" && ended(g) > a.at && ended(g) <= b.at);
        if let (Some(game), None) = (inside.next(), inside.next()) {
            game.lp_change = Some(score(&b.rank) - score(&a.rank));
        }
    }
}

/// Records the signed-in account's rank shortly after a game ends. Ranked
/// stats update a little after the end screen, so it reads twice; unchanged
/// reads add nothing.
pub async fn record_after_game() {
    for delay in [20, 90] {
        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        let Ok(client) = crate::runes::lcu().await else {
            continue;
        };
        let (Ok(summoner), Ok(ranked)) = tokio::join!(
            crate::runes::lcu_get::<Value>(&client, "/lol-summoner/v1/current-summoner"),
            crate::runes::lcu_get::<Value>(&client, "/lol-ranked/v1/current-ranked-stats"),
        ) else {
            continue;
        };
        if let Some(puuid) = summoner["puuid"].as_str().filter(|p| !p.is_empty()) {
            let (puuid, rank) = (puuid.to_string(), super::model::rank(&ranked));
            let _ = tokio::task::spawn_blocking(move || record(&puuid, rank)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rank(tier: &str, division: &str, lp: u64, wins: u64, losses: u64) -> Rank {
        Rank {
            tier: tier.into(),
            division: division.into(),
            lp,
            wins,
            losses,
        }
    }

    fn point(at: u64, rank: Rank) -> Point {
        Point { at, rank }
    }

    fn game(id: u64, queue: &str, ended_at: u64) -> Game {
        serde_json::from_value(serde_json::json!({
            "id": id, "queue": queue, "championId": 1, "createdAt": ended_at,
            "duration": 0, "win": true, "kills": 0, "deaths": 0, "assists": 0,
            "metrics": [null, null, null, null, null, null]
        }))
        .unwrap()
    }

    #[test]
    fn records_only_changes_and_keeps_a_bounded_history() {
        let path = std::env::temp_dir().join(format!("lp-test-{}.json", uuid::Uuid::new_v4()));
        record_at(&path, "a", rank("GOLD", "II", 10, 1, 1), 1);
        let points = record_at(&path, "a", rank("GOLD", "II", 10, 1, 1), 2);
        assert_eq!(points.len(), 1);
        for i in 0..(MAX_POINTS as u64 + 5) {
            record_at(&path, "a", rank("GOLD", "II", i, 10 + i, 1), 10 + i);
        }
        let points = record_at(&path, "b", rank("IRON", "IV", 0, 0, 1), 1);
        assert_eq!(points.len(), 1);
        assert_eq!(load(&path)["a"].len(), MAX_POINTS);
        std::fs::write(&path, b"not json").unwrap();
        assert!(load(&path).is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn lp_change_crosses_divisions_and_tiers() {
        let points = [
            point(100, rank("GOLD", "I", 90, 10, 10)),
            point(200, rank("PLATINUM", "IV", 8, 11, 10)),
            point(300, rank("GOLD", "I", 75, 11, 11)),
            point(400, rank("DIAMOND", "I", 95, 12, 11)),
            point(500, rank("MASTER", "", 12, 13, 11)),
        ];
        let mut games = vec![
            game(1, "solo", 150),
            game(2, "solo", 250),
            game(3, "solo", 450),
        ];
        annotate(&mut games, &points);
        assert_eq!(games[0].lp_change, Some(18));
        assert_eq!(games[1].lp_change, Some(-33));
        assert_eq!(games[2].lp_change, Some(17));
    }

    #[test]
    fn ambiguous_or_unrecorded_games_get_no_lp_change() {
        let points = [
            point(100, rank("GOLD", "II", 50, 10, 10)),
            // Two games played while Swapper was closed.
            point(200, rank("GOLD", "II", 90, 12, 10)),
            point(300, rank("GOLD", "II", 70, 12, 11)),
        ];
        let mut games = vec![
            game(1, "solo", 150),
            game(2, "solo", 160),
            // Flex and a second solo game in the same window make it ambiguous.
            game(3, "flex", 250),
        ];
        annotate(&mut games, &points);
        assert!(games.iter().all(|g| g.lp_change.is_none()));
        let mut two = vec![game(4, "solo", 250), game(5, "solo", 260)];
        annotate(&mut two, &points);
        assert!(two.iter().all(|g| g.lp_change.is_none()));
        let mut one = vec![game(6, "solo", 250), game(7, "flex", 260)];
        annotate(&mut one, &points);
        assert_eq!(one[0].lp_change, Some(-20));
    }
}
