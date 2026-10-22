use crate::full_app::{self, model};
use futures_util::{stream, StreamExt};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock, time::Duration};
use tauri::Emitter;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    participant_id: u64,
    team_id: u64,
    champion_id: i64,
    riot_id: String,
    me: bool,
    role: String,
    level: Option<u64>,
    spells: Vec<i64>,
    perks: Vec<i64>,
    keystone: Option<i64>,
    secondary_style: Option<i64>,
    kills: Option<u64>,
    deaths: Option<u64>,
    assists: Option<u64>,
    cs: Option<u64>,
    damage: Option<u64>,
    gold: Option<u64>,
    vision: Option<u64>,
    damage_taken: Option<u64>,
    wards_placed: Option<u64>,
    wards_killed: Option<u64>,
    items: Vec<i64>,
    placement: Option<u8>,
    score: Option<f64>,
    rank: Option<model::Rank>,
    rank_loaded: bool,
}

#[derive(Clone, Serialize)]
pub struct Objective {
    label: String,
    kills: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    team_id: u64,
    win: Option<bool>,
    objectives: Vec<Objective>,
}

#[derive(Clone, Serialize)]
pub struct Detail {
    game: model::Game,
    players: Vec<Player>,
    teams: Vec<Team>,
}

fn objectives(team: &Value) -> Vec<Objective> {
    [
        ("Dragons", "dragonKills", "dragon"),
        ("Heralds", "riftHeraldKills", "riftHerald"),
        ("Grubs", "hordeKills", "horde"),
        ("Barons", "baronKills", "baron"),
        ("Towers", "towerKills", "tower"),
    ]
    .into_iter()
    .filter_map(|(label, legacy, nested)| {
        team[legacy]
            .as_u64()
            .or_else(|| team["objectives"][nested]["kills"].as_u64())
            .map(|kills| Objective {
                label: label.into(),
                kills,
            })
    })
    .collect()
}

fn parse(value: &Value, summoner: &model::Summoner) -> Result<Detail, String> {
    let game = model::game(value, summoner)
        .ok_or("This Rift match is unavailable for the signed-in player.")?;
    let participants = value["participants"]
        .as_array()
        .ok_or("Match participants unavailable.")?;
    let identities = value["participantIdentities"]
        .as_array()
        .ok_or("Match identities unavailable.")?;
    let scores = model::performance_scores(participants);
    let players: Vec<_> = participants
        .iter()
        .enumerate()
        .filter_map(|(index, p)| {
            let id = p["participantId"].as_u64()?;
            let identity = identities.iter().find(|i| i["participantId"] == id);
            let who = identity.map(|i| &i["player"]);
            let riot_id = who
                .map(|who| {
                    let name = who["gameName"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .or_else(|| who["summonerName"].as_str())
                        .unwrap_or("");
                    match who["tagLine"]
                        .as_str()
                        .filter(|s| !s.is_empty() && !name.is_empty())
                    {
                        Some(tag) => format!("{name}#{tag}"),
                        None => name.into(),
                    }
                })
                .unwrap_or_default();
            let me = who.is_some_and(|who| {
                who["puuid"] == summoner.puuid
                    || (who["puuid"].as_str().unwrap_or("").is_empty()
                        && summoner.summoner_id != 0
                        && who["summonerId"] == summoner.summoner_id)
            });
            let s = &p["stats"];
            let n = |key: &str| s[key].as_u64();
            let score = scores.as_ref().map(|scores| scores[index]);
            Some(Player {
                participant_id: id,
                team_id: p["teamId"].as_u64()?,
                champion_id: p["championId"].as_i64()?,
                riot_id,
                me,
                role: model::role(p).into(),
                level: n("champLevel"),
                spells: ["spell1Id", "spell2Id"]
                    .iter()
                    .filter_map(|key| p[*key].as_i64().filter(|id| *id > 0))
                    .collect(),
                perks: (0..9)
                    .filter_map(|i| s[format!("perk{i}")].as_i64().filter(|id| *id > 0))
                    .collect(),
                keystone: s["perk0"].as_i64().filter(|id| *id > 0),
                secondary_style: s["perkSubStyle"].as_i64().filter(|id| *id > 0),
                kills: n("kills"),
                deaths: n("deaths"),
                assists: n("assists"),
                cs: n("totalMinionsKilled")
                    .zip(n("neutralMinionsKilled"))
                    .map(|(a, b)| a + b),
                damage: n("totalDamageDealtToChampions"),
                gold: n("goldEarned"),
                vision: n("visionScore"),
                damage_taken: n("totalDamageTaken"),
                wards_placed: n("wardsPlaced"),
                wards_killed: n("wardsKilled"),
                items: (0..7)
                    .map(|i| s[format!("item{i}")].as_i64().unwrap_or(0))
                    .collect(),
                placement: score.map(|own| {
                    1 + scores
                        .as_ref()
                        .unwrap()
                        .iter()
                        .filter(|s| **s > own + 1e-7)
                        .count() as u8
                }),
                score,
                rank: None,
                rank_loaded: false,
            })
        })
        .collect();
    let teams = [100, 200]
        .into_iter()
        .map(|team_id| {
            let team = value["teams"]
                .as_array()
                .and_then(|teams| teams.iter().find(|t| t["teamId"] == team_id));
            let participant = participants.iter().find(|p| p["teamId"] == team_id);
            Team {
                team_id,
                win: team
                    .and_then(|t| {
                        t["win"].as_bool().or_else(|| match t["win"].as_str() {
                            Some("Win") => Some(true),
                            Some("Fail") => Some(false),
                            _ => None,
                        })
                    })
                    .or_else(|| participant.and_then(|p| p["stats"]["win"].as_bool())),
                objectives: if participants.len() == 10 {
                    team.map(objectives).unwrap_or_default()
                } else {
                    Vec::new()
                },
            }
        })
        .collect();
    Ok(Detail {
        game,
        players,
        teams,
    })
}

#[derive(Default)]
struct Cache {
    owner: String,
    matches: BTreeMap<(u64, String), Detail>,
    ranks: BTreeMap<String, Option<model::Rank>>,
}

static CACHE: OnceLock<tokio::sync::Mutex<Cache>> = OnceLock::new();

pub(crate) fn cached_games() -> Vec<model::Game> {
    CACHE
        .get()
        .and_then(|cache| cache.try_lock().ok())
        .map(|cache| {
            cache
                .matches
                .values()
                .map(|detail| detail.game.clone())
                .collect()
        })
        .unwrap_or_default()
}

async fn get(client: &crate::runes::Lcu, path: &str) -> Result<Value, String> {
    tokio::time::timeout(Duration::from_secs(4), crate::runes::lcu_get(client, path))
        .await
        .map_err(|_| "League Client timed out.".to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn full_app_match_detail(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    game_id: u64,
    riot_id: Option<String>,
) -> Result<Detail, String> {
    if window.label() != full_app::LABEL || !full_app::enabled(&app) {
        return Err("Fullsized App is closed or disabled.".into());
    }
    let client = crate::runes::lcu().await.map_err(|e| e.to_string())?;
    let current = get(&client, "/lol-summoner/v1/current-summoner").await?;
    let owner =
        model::summoner(&current).ok_or("Sign in to League Client to load match details.")?;
    let mut cache = CACHE.get_or_init(Default::default).lock().await;
    if cache.owner != owner.puuid {
        *cache = Cache {
            owner: owner.puuid.clone(),
            ..Default::default()
        };
    }
    let perspective = riot_id.unwrap_or_else(|| owner.riot_id.clone());
    if perspective.len() > 128 || perspective.chars().any(char::is_control) {
        return Err("Invalid player name.".into());
    }
    let key = (game_id, perspective.to_lowercase());
    if let Some(detail) = cache.matches.get(&key) {
        return Ok(detail.clone());
    }
    let value = get(&client, &format!("/lol-match-history/v1/games/{game_id}")).await?;
    if value["gameId"].as_u64() != Some(game_id) {
        return Err("League Client returned another match.".into());
    }
    let selected = value["participantIdentities"]
        .as_array()
        .and_then(|identities| {
            identities.iter().find_map(|i| {
                let player = &i["player"];
                let name = player["gameName"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| player["summonerName"].as_str())?;
                let tag = player["tagLine"].as_str().unwrap_or("");
                let id = if tag.is_empty() {
                    name.to_string()
                } else {
                    format!("{name}#{tag}")
                };
                id.eq_ignore_ascii_case(&perspective)
                    .then(|| model::Summoner {
                        puuid: player["puuid"].as_str().unwrap_or_default().into(),
                        summoner_id: player["summonerId"].as_u64().unwrap_or(0),
                        riot_id: id,
                        level: 0,
                        icon: None,
                    })
            })
        });
    let summoner = if perspective.eq_ignore_ascii_case(&owner.riot_id) {
        &owner
    } else {
        selected
            .as_ref()
            .ok_or("This player is not in the match details.")?
    };
    let mut detail = parse(&value, summoner)?;
    // Paint the scoreboard before remote rank lookups finish.
    let _ = app.emit_to(full_app::LABEL, "match_detail", &detail);
    let identities = value["participantIdentities"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let pending: std::collections::BTreeSet<_> = identities
        .iter()
        .filter_map(|identity| identity["player"]["puuid"].as_str())
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .filter(|id| !cache.ranks.contains_key(*id))
        .map(str::to_string)
        .collect();
    let ranks: Vec<_> = stream::iter(pending.into_iter().map(|puuid| {
        let client = &client;
        async move {
            let rank = get(client, &format!("/lol-ranked/v1/ranked-stats/{puuid}")).await;
            (puuid, rank)
        }
    }))
    .buffer_unordered(3)
    .collect()
    .await;
    for (puuid, result) in ranks {
        if let Ok(value) = result {
            cache.ranks.insert(puuid, model::rank(&value));
        }
    }
    for player in &mut detail.players {
        let puuid = identities
            .iter()
            .find(|i| i["participantId"] == player.participant_id)
            .and_then(|i| i["player"]["puuid"].as_str());
        if let Some(rank) = puuid.and_then(|id| cache.ranks.get(id)) {
            player.rank = rank.clone();
            player.rank_loaded = true;
        }
    }
    if cache.matches.len() >= 20 {
        cache.matches.clear();
    }
    if cache.ranks.len() > 200 {
        cache.ranks.clear();
    }
    cache.matches.insert(key, detail.clone());
    Ok(detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn objectives_keep_observed_zeroes_but_skip_unknowns() {
        let values = objectives(
            &json!({"dragonKills": 0, "baronKills": 2, "objectives": {"horde": {"kills": 3}}}),
        );
        assert_eq!(
            values
                .iter()
                .map(|o| (o.label.as_str(), o.kills))
                .collect::<Vec<_>>(),
            vec![("Dragons", 0), ("Grubs", 3), ("Barons", 2)]
        );
        assert!(objectives(&json!({})).is_empty());
    }

    #[test]
    fn performance_scores_share_placement_weights_and_require_all_ten() {
        let mut players: Vec<_> = (1..=10).map(|id| json!({"participantId": id, "stats": {"kills": 2, "deaths": 1, "assists": 3,
            "totalDamageDealtToChampions": 1000, "totalMinionsKilled": 100, "neutralMinionsKilled": 0, "visionScore": 20}})).collect();
        players[0]["stats"]["kills"] = json!(0);
        players[0]["stats"]["assists"] = json!(0);
        let scores = model::performance_scores(&players).unwrap();
        assert!((scores[0] - 65.0).abs() < 1e-7);
        assert!((scores[1] - 100.0).abs() < 1e-7);
        assert_eq!(model::placement(&players, &players[0]), 10);
        assert!(model::performance_scores(&players[..9]).is_none());
        players[3]["stats"]
            .as_object_mut()
            .unwrap()
            .remove("visionScore");
        assert!(model::performance_scores(&players).is_none());
    }

    #[test]
    fn detail_maps_identity_loadout_and_missing_stats_without_defaults() {
        let owner = model::Summoner {
            puuid: "fixture-self".into(),
            summoner_id: 1,
            riot_id: "You#TEST".into(),
            level: 1,
            icon: None,
        };
        let participants: Vec<_> = (1..=10).map(|id| json!({
            "participantId": id, "teamId": if id <= 5 { 100 } else { 200 }, "championId": 103,
            "spell1Id": 4, "spell2Id": 12,
            "stats": { "win": id <= 5, "kills": 2, "deaths": 1, "assists": 3,
                "totalDamageDealtToChampions": 1000, "totalMinionsKilled": 100, "neutralMinionsKilled": 0,
                "visionScore": 20, "champLevel": 16, "perk0": 8112, "perk1": 8139, "perkSubStyle": 8200, "item0": 6653 }
        })).collect();
        let mut value = json!({ "gameId": 42, "mapId": 11, "queueId": 420, "gameDuration": 1200, "gameCreation": 1234,
            "participants": participants, "participantIdentities": [{"participantId": 1, "player": {"puuid": "fixture-self", "gameName": "You", "tagLine": "TEST"}}],
            "teams": [{"teamId": 100, "win": "Win", "dragonKills": 2}] });
        let detail = parse(&value, &owner).unwrap();
        assert_eq!(detail.players.len(), 10);
        assert!(detail.players[0].me);
        assert!(!detail.players[1].me);
        assert_eq!(detail.players[0].riot_id, "You#TEST");
        assert_eq!(detail.players[0].perks, vec![8112, 8139]);
        assert_eq!(detail.players[0].keystone, Some(8112));
        assert_eq!(detail.players[0].spells, vec![4, 12]);
        assert_eq!(detail.players[0].secondary_style, Some(8200));
        assert_eq!(detail.players[0].gold, None);
        assert_eq!(detail.players[0].wards_placed, None);
        assert_eq!(detail.teams[0].win, Some(true));
        assert_eq!(detail.teams[1].win, Some(false));
        assert!(detail.teams[1].objectives.is_empty());
        value["participants"][2]["stats"]
            .as_object_mut()
            .unwrap()
            .remove("visionScore");
        let partial = parse(&value, &owner).unwrap();
        assert!(partial
            .players
            .iter()
            .all(|p| p.score.is_none() && p.placement.is_none()));
        assert_eq!(partial.players[2].vision, None);
        value["participants"][2]["stats"]
            .as_object_mut()
            .unwrap()
            .remove("perk0");
        let missing_keystone = parse(&value, &owner).unwrap();
        assert_eq!(missing_keystone.players[2].keystone, None);
        assert_eq!(missing_keystone.players[2].perks, vec![8139]);
        value["participants"].as_array_mut().unwrap().truncate(1);
        let incomplete = parse(&value, &owner).unwrap();
        assert_eq!(incomplete.players.len(), 1);
        assert!(incomplete.players[0].score.is_none());
        assert!(incomplete
            .teams
            .iter()
            .all(|team| team.objectives.is_empty()));
    }
}
