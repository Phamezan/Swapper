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
use std::sync::Mutex;

static HISTORY: Mutex<()> = Mutex::new(());

/// Identity and switch generation captured before reading any account data.
pub struct AccountRead {
    guard: crate::SwitchGuard,
    epoch: u64,
}

impl AccountRead {
    pub fn begin(app: &tauri::AppHandle) -> Result<Self, String> {
        use tauri::Manager;
        let guard = app.state::<crate::AppState>().switch_guard.clone();
        let epoch = guard.epoch();
        if guard.is_switching() { return Err("League account is changing.".into()); }
        Ok(Self { epoch, guard })
    }

    fn verify(&self, puuid: &str, verified: &Value) -> Result<(), String> {
        if puuid.is_empty() || verified["puuid"].as_str() != Some(puuid)
            || self.guard.is_switching() || self.guard.epoch() != self.epoch {
            return Err("League account changed; refresh the dashboard.".into());
        }
        Ok(())
    }
}

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
    /// Where a point came from when Swapper did not record it itself ("opgg").
    /// Such points carry no win/loss counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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

/// The history to modify and save back. `None` means the file exists but cannot
/// be read right now, so saving would destroy other accounts' points. A corrupt
/// or oversized file is moved to `.bad` first and history starts fresh.
fn load_for_write(path: &Path) -> Option<History> {
    let bytes = match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(History::new()),
        Err(_) => return None,
        Ok(m) if m.len() > MAX_FILE_BYTES => None,
        Ok(_) => Some(std::fs::read(path).ok()?),
    };
    if let Some(history) = bytes.and_then(|b| serde_json::from_slice(&b).ok()) {
        return Some(history);
    }
    std::fs::rename(path, path.with_extension("bad")).ok()?;
    Some(History::new())
}

/// Appends `rank` for `puuid` unless it equals the last point. Returns the
/// account's points, oldest first.
#[cfg(test)]
fn record_at(path: &Path, puuid: &str, rank: Rank, at: u64) -> Vec<Point> {
    let _guard = HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    record_unlocked(path, puuid, rank, at)
}

fn save(path: &Path, history: &History) {
    if let Ok(bytes) = serde_json::to_vec(history) {
        let pending = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&pending, bytes).is_ok() {
            let _ = std::fs::rename(&pending, path);
        }
        let _ = std::fs::remove_file(&pending);
    }
}

fn record_unlocked(path: &Path, puuid: &str, rank: Rank, at: u64) -> Vec<Point> {
    let Some(mut history) = load_for_write(path) else { return Vec::new() };
    let points = history.entry(puuid.to_string()).or_default();
    if points.last().map(|p| &p.rank) != Some(&rank) {
        points.push(Point { at, rank, source: None });
        let excess = points.len().saturating_sub(MAX_POINTS);
        points.drain(..excess);
        save(path, &history);
    }
    history.remove(puuid).unwrap_or_default()
}

/// Adds points from other trackers before the first point Swapper recorded
/// itself, so they never interleave with its own record. Earlier seeds stay; a
/// seed at a time already held is skipped. Returns how many were added.
pub fn seed_at(path: &Path, puuid: &str, seeds: Vec<Point>) -> usize {
    let _guard = HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    let Some(mut history) = load_for_write(path) else { return 0 };
    let points = history.entry(puuid.to_string()).or_default();
    let first = points.iter().find(|p| p.source.is_none()).map_or(u64::MAX, |p| p.at);
    let before = points.len();
    points.extend(seeds.into_iter().filter(|p| p.at < first));
    points.sort_by_key(|p| p.at);
    // Stable sort keeps a held point ahead of a new seed at the same time.
    points.dedup_by_key(|p| p.at);
    let excess = points.len().saturating_sub(MAX_POINTS);
    points.drain(..excess);
    let added = points.len() + excess - before;
    if added > 0 {
        save(path, &history);
    }
    added
}

pub fn seed(puuid: &str, seeds: Vec<Point>) -> usize {
    path().map_or(0, |path| seed_at(&path, puuid, seeds))
}

/// True once an account's history holds points seeded from `source`.
pub fn seeded(points: &[Point], source: &str) -> bool {
    points.iter().any(|p| p.source.as_deref() == Some(source))
}

fn record_verified_at(path: &Path, account: &AccountRead, puuid: &str, verified: &Value, rank: Option<Rank>) -> Result<Vec<Point>, String> {
    let _guard = HISTORY.lock().unwrap_or_else(|e| e.into_inner());
    account.verify(puuid, verified)?;
    Ok(match rank {
        Some(rank) => record_unlocked(path, puuid, rank, crate::runes::provider::now_ms().max(0) as u64),
        None => load(path).remove(puuid).unwrap_or_default(),
    })
}

pub async fn record_current(client: &crate::runes::Lcu, account: AccountRead, puuid: String, rank: Option<Rank>) -> Result<Vec<Point>, String> {
    let verified = crate::runes::lcu_get::<Value>(client, "/lol-summoner/v1/current-summoner")
        .await.map_err(|e| e.to_string())?;
    account.verify(&puuid, &verified)?;
    tokio::task::spawn_blocking(move || {
        let Some(path) = path() else { return Ok(Vec::new()); };
        record_verified_at(&path, &account, &puuid, &verified, rank)
    }).await.map_err(|e| e.to_string())?
}

/// LP on one ladder: 100 per division, apex tiers share the top base.
pub(crate) fn score(rank: &Rank) -> i64 {
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

const DIVISIONS: [&str; 4] = ["IV", "III", "II", "I"];
const APEX: [&str; 3] = ["MASTER", "GRANDMASTER", "CHALLENGER"];

/// The rank at a ladder score, inverse of `score`. Apex scores keep `apex_tier`
/// (the score cannot tell Master from Grandmaster), defaulting to Master.
pub(crate) fn rank_at(score: i64, apex_tier: &str) -> Rank {
    let score = score.max(0);
    let top = TIERS.len() as i64 * 400;
    let (tier, division, lp) = if score >= top {
        let tier = if APEX.contains(&apex_tier) { apex_tier } else { "MASTER" };
        (tier.to_string(), String::new(), score - top)
    } else {
        (TIERS[(score / 400) as usize].to_string(), DIVISIONS[(score % 400 / 100) as usize].to_string(), score % 100)
    };
    Rank { tier, division, lp: lp as u64, wins: 0, losses: 0 }
}

/// Sets `lp_change` on Solo/Duo games that sit alone between two points one
/// game apart. Everything else (unrecorded gaps, remakes) stays `None`.
pub fn annotate(games: &mut [Game], points: &[Point]) {
    for pair in points.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        // A seeded point has no game counts, so it never pins a game's LP.
        if a.source.is_some() || b.source.is_some() || b.rank.wins + b.rank.losses != a.rank.wins + a.rank.losses + 1 {
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
pub async fn record_after_game(app: tauri::AppHandle) {
    for delay in [20, 90] {
        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        let Ok(account) = AccountRead::begin(&app) else { continue; };
        let Ok(client) = crate::runes::lcu().await else {
            continue;
        };
        let Ok(summoner) = crate::runes::lcu_get::<Value>(&client, "/lol-summoner/v1/current-summoner").await else {
            continue;
        };
        let Ok(ranked) = crate::runes::lcu_get::<Value>(&client, "/lol-ranked/v1/current-ranked-stats").await else {
            continue;
        };
        if let Some(puuid) = summoner["puuid"].as_str().filter(|p| !p.is_empty()) {
            let (puuid, rank) = (puuid.to_string(), super::model::rank(&ranked));
            let _ = record_current(&client, account, puuid, rank).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_identity_or_switch_generation_never_writes_lp() {
        let path = std::env::temp_dir().join(format!("lp-identity-{}.json", uuid::Uuid::new_v4()));
        let guard = crate::SwitchGuard::new();
        let account = AccountRead { epoch: guard.epoch(), guard: guard.clone() };
        let rank = Some(rank("GOLD", "II", 10, 1, 1));
        let other = serde_json::json!({"puuid": "fixture-b"});
        let rejected = record_verified_at(&path, &account, "fixture-a", &other, rank.clone());
        assert!(!path.exists(), "mixed-account rank was persisted");
        assert!(rejected.is_err());
        let same = serde_json::json!({"puuid": "fixture-a"});
        let lease = guard.acquire().unwrap();
        assert!(record_verified_at(&path, &account, "fixture-a", &same, rank.clone()).is_err());
        drop(lease);
        assert!(record_verified_at(&path, &account, "fixture-a", &same, rank.clone()).is_err());
        assert!(!path.exists(), "read spanning a completed switch was persisted");
        let account = AccountRead { epoch: guard.epoch(), guard };
        assert_eq!(record_verified_at(&path, &account, "fixture-a", &same, rank).unwrap().len(), 1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn seeds_only_go_before_the_first_recorded_point() {
        let path = std::env::temp_dir().join(format!("lp-seed-{}.json", uuid::Uuid::new_v4()));
        record_at(&path, "fixture-a", rank("DIAMOND", "III", 38, 40, 30), 1_000);
        let seed = |at, lp| Point { at, rank: rank("DIAMOND", "IV", lp, 0, 0), source: Some("opgg".into()) };
        assert_eq!(seed_at(&path, "fixture-a", vec![seed(900, 85), seed(500, 35), seed(1_200, 5), seed(500, 35)]), 2);
        let points = load(&path).remove("fixture-a").unwrap();
        assert_eq!(points.iter().map(|p| p.at).collect::<Vec<_>>(), [500, 900, 1_000]);
        assert!(seeded(&points, "opgg") && !seeded(&points, "ugg"));
        // A held time is not added twice; a later seed between seeds still fits in.
        assert_eq!(seed_at(&path, "fixture-a", vec![seed(900, 85)]), 0);
        let game = Point { at: 700, rank: rank("DIAMOND", "IV", 60, 0, 0), source: Some("ugg".into()) };
        assert_eq!(seed_at(&path, "fixture-a", vec![game]), 1);
        let points = load(&path).remove("fixture-a").unwrap();
        assert_eq!(points.iter().map(|p| p.at).collect::<Vec<_>>(), [500, 700, 900, 1_000]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_seeded_point_never_pins_a_games_lp() {
        let mut games = vec![game(1, "solo", 1_500)];
        let seed = Point { at: 1_000, rank: rank("GOLD", "II", 10, 0, 0), source: Some("opgg".into()) };
        annotate(&mut games, &[seed, point(2_000, rank("GOLD", "II", 30, 0, 1))]);
        assert_eq!(games[0].lp_change, None);
    }

    #[test]
    fn concurrent_lp_writers_preserve_every_account() {
        let path = std::env::temp_dir().join(format!("lp-concurrent-{}.json", uuid::Uuid::new_v4()));
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));
        std::thread::scope(|scope| {
            for i in 0..16 {
                let (path, barrier) = (&path, barrier.clone());
                scope.spawn(move || {
                    barrier.wait();
                    let guard = crate::SwitchGuard::new();
                    let account = AccountRead { epoch: guard.epoch(), guard };
                    let puuid = format!("fixture-{i}");
                    record_verified_at(path, &account, &puuid, &serde_json::json!({"puuid": puuid}), Some(rank("GOLD", "II", 10, 1, 1))).unwrap();
                });
            }
        });
        assert_eq!(load(&path).len(), 16);
        std::fs::remove_file(path).unwrap();
    }

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
        Point { at, rank, source: None }
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
    fn corrupt_history_is_set_aside_and_unreadable_history_is_not_overwritten() {
        let path = std::env::temp_dir().join(format!("lp-test-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(record_at(&path, "a", rank("GOLD", "II", 10, 1, 1), 1).len(), 1);
        assert_eq!(std::fs::read(path.with_extension("bad")).unwrap(), b"not json");
        // A directory cannot be read as a file: nothing may be saved over it.
        let blocked = std::env::temp_dir().join(format!("lp-test-{}.json", uuid::Uuid::new_v4()));
        std::fs::create_dir(&blocked).unwrap();
        assert!(record_at(&blocked, "a", rank("GOLD", "II", 10, 1, 1), 1).is_empty());
        assert!(blocked.is_dir());
        let _ = std::fs::remove_file(path.with_extension("bad"));
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir(blocked);
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
