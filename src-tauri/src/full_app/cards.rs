//! Per-account rank and recent record for the Dashboard's account cards. The
//! League client only answers for the signed-in account, so each dashboard
//! refresh saves that account's card; the others show their last saved card.
//! Keyed by Swapper's account id so PUUIDs never reach the UI.

use super::model::{Rank, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub rank: Option<Rank>,
    /// Wins and losses over the recent Rift games in the snapshot (up to 20).
    pub wins: u64,
    pub losses: u64,
    /// When the newest of those games ended (ms), 0 if none.
    pub last_played: u64,
    pub updated_at: i64,
}

pub type Cards = BTreeMap<Uuid, Card>;

fn path() -> Option<PathBuf> {
    Some(crate::vault::data_root().ok()?.join("account-cards.json"))
}

fn load(path: &Path) -> Cards {
    if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_FILE_BYTES) {
        return Cards::new();
    }
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn card(snapshot: &Snapshot) -> Card {
    let wins = snapshot.games.iter().filter(|g| g.win).count() as u64;
    Card {
        rank: snapshot.rank.clone(),
        wins,
        losses: snapshot.games.len() as u64 - wins,
        last_played: snapshot
            .games
            .iter()
            .map(|g| g.created_at + g.duration * 1000)
            .max()
            .unwrap_or(0),
        updated_at: snapshot.fetched_at,
    }
}

/// Saves `snapshot` as the card of the saved account with its PUUID (if any)
/// and returns the cards of every saved account.
fn update_at(
    path: &Path,
    accounts: &[crate::vault::Account],
    snapshot: Option<&Snapshot>,
) -> Cards {
    let mut cards = load(path);
    let owner = snapshot.and_then(|s| {
        accounts
            .iter()
            .find(|a| a.puuid.as_deref() == Some(s.summoner.puuid.as_str()))
            .map(|a| (a.id, card(s)))
    });
    if let Some((id, card)) = owner {
        if cards.get(&id) != Some(&card) {
            cards.insert(id, card);
            cards.retain(|id, _| accounts.iter().any(|a| a.id == *id));
            if let Ok(bytes) = serde_json::to_vec(&cards) {
                let pending = path.with_extension("tmp");
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::write(&pending, bytes).is_ok() {
                    let _ = std::fs::rename(&pending, path);
                }
            }
        }
    }
    cards.retain(|id, _| accounts.iter().any(|a| a.id == *id));
    cards
}

pub fn update(snapshot: Option<&Snapshot>) -> Cards {
    let (Some(path), Ok(config)) = (path(), crate::vault::load()) else {
        return Cards::new();
    };
    update_at(&path, &config.accounts, snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(puuid: Option<&str>) -> crate::vault::Account {
        serde_json::from_value(serde_json::json!({
            "id": Uuid::new_v4(), "name": "a", "vault_id": Uuid::new_v4(), "puuid": puuid
        }))
        .unwrap()
    }

    fn snapshot(puuid: &str, results: &[bool]) -> Snapshot {
        let games: Vec<_> = results
            .iter()
            .enumerate()
            .map(|(i, win)| {
                serde_json::json!({
                    "id": i, "queue": "solo", "championId": 1, "createdAt": 1000 * i as u64,
                    "duration": 60, "win": win, "kills": 0, "deaths": 0, "assists": 0,
                    "metrics": [null, null, null, null, null, null]
                })
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "summoner": { "puuid": puuid, "summonerId": 0, "riotId": "x", "level": 1, "icon": null },
            "rank": { "tier": "GOLD", "division": "II", "lp": 40, "wins": 9, "losses": 8 },
            "fetchedAt": 7, "games": games, "summaries": {}
        }))
        .unwrap()
    }

    #[test]
    fn saves_the_signed_in_account_and_keeps_the_others() {
        let path = std::env::temp_dir().join(format!("cards-test-{}.json", Uuid::new_v4()));
        let (main, smurf, unsaved) = (account(Some("p1")), account(Some("p2")), account(None));
        let accounts = vec![main.clone(), smurf.clone(), unsaved];
        update_at(
            &path,
            &accounts,
            Some(&snapshot("p1", &[true, false, true])),
        );
        let cards = update_at(&path, &accounts, Some(&snapshot("p2", &[false])));
        assert_eq!(cards.len(), 2);
        let first = &cards[&main.id];
        assert_eq!(
            (first.wins, first.losses, first.last_played),
            (2, 1, 62_000)
        );
        assert_eq!(first.rank.as_ref().unwrap().lp, 40);
        // A League account Swapper has not saved changes nothing; removed accounts drop out.
        update_at(&path, &accounts, Some(&snapshot("other", &[true])));
        let cards = update_at(&path, std::slice::from_ref(&smurf), None);
        assert_eq!(cards.keys().collect::<Vec<_>>(), vec![&smurf.id]);
        std::fs::write(&path, b"{broken").unwrap();
        assert!(update_at(&path, &accounts, None).is_empty());
        let _ = std::fs::remove_file(path);
    }
}
