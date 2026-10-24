use crate::full_app::{self, model};
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock, time::Duration};
use tauri::Emitter;

#[derive(Clone, Serialize, Deserialize)]
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
    rank: Option<MatchRank>,
    rank_loaded: bool,
    stats: BTreeMap<String, Value>,
}

#[derive(Clone, Serialize, Deserialize)]
struct MatchRank {
    tier: String,
    division: String,
    lp: Option<u64>,
    queue: String,
}

fn rank_for_match(value: &Value, game_queue: &str) -> Option<MatchRank> {
    let solo = ("RANKED_SOLO_5x5", "Solo/Duo");
    let flex = ("RANKED_FLEX_SR", "Flex");
    let queues = if game_queue == "flex" {
        [flex, solo]
    } else {
        [solo, flex]
    };
    queues.into_iter().find_map(|(key, queue)| {
        let ranked = value["queueMap"][key]
            .as_object()
            .map(|_| &value["queueMap"][key])
            .or_else(|| {
                value["queues"]
                    .as_array()?
                    .iter()
                    .find(|q| q["queueType"] == key)
            })?;
        let tier = ranked["tier"].as_str()?;
        if !matches!(
            tier,
            "IRON"
                | "BRONZE"
                | "SILVER"
                | "GOLD"
                | "PLATINUM"
                | "EMERALD"
                | "DIAMOND"
                | "MASTER"
                | "GRANDMASTER"
                | "CHALLENGER"
        ) {
            return None;
        }
        let division = if matches!(tier, "MASTER" | "GRANDMASTER" | "CHALLENGER") {
            ""
        } else {
            ranked["division"]
                .as_str()
                .filter(|d| matches!(*d, "I" | "II" | "III" | "IV"))
                .unwrap_or("")
        };
        Some(MatchRank {
            tier: tier.into(),
            division: division.into(),
            lp: ranked["leaguePoints"].as_u64(),
            queue: queue.into(),
        })
    })
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Objective {
    label: String,
    kills: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    team_id: u64,
    win: Option<bool>,
    objectives: Vec<Objective>,
    /// Banned champion ids in pick order; empty for blind modes and older saves.
    #[serde(default)]
    bans: Vec<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Detail {
    game: model::Game,
    players: Vec<Player>,
    teams: Vec<Team>,
    timeline: Option<Timeline>,
}

fn extract_stats(stats: &Value) -> BTreeMap<String, Value> {
    stats
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, value)| value.is_number() || value.is_boolean())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Timeline {
    frames: Vec<TimelineFrame>,
    events: Vec<TimelineEvent>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineFrame {
    timestamp: u64,
    gold_diff: i64,
    blue_gold: i64,
    red_gold: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEvent {
    timestamp: u64,
    team_id: u64,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    killer_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    victim_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<TimelinePosition>,
}

#[derive(Clone, Serialize, Deserialize)]
struct TimelinePosition {
    x: u64,
    y: u64,
}

fn timeline_position(value: &Value) -> Option<TimelinePosition> {
    let x = value["x"].as_u64().filter(|x| *x <= 15000)?;
    let y = value["y"].as_u64().filter(|y| *y <= 15000)?;
    Some(TimelinePosition { x, y })
}

fn timeline_from_participants(
    value: &Value,
    participants: &[Value],
    map_id: u64,
) -> Option<Timeline> {
    let sides: BTreeMap<_, _> = participants
        .iter()
        .filter_map(|p| Some((p["participantId"].as_u64()?, p["teamId"].as_u64()?)))
        .collect();
    if sides.len() != 10
        || sides.values().filter(|&&side| side == 100).count() != 5
        || sides.values().filter(|&&side| side == 200).count() != 5
    {
        return None;
    }
    let source = value["frames"].as_array()?;
    let frames: Vec<_> = source
        .iter()
        .map(|frame| {
            let mut totals = [0i64; 2];
            for (&id, &side) in &sides {
                let p = &frame["participantFrames"][id.to_string()];
                if p["participantId"].as_u64() != Some(id) {
                    return None;
                }
                let gold = p["totalGold"].as_i64().filter(|gold| *gold >= 0)?;
                let index = usize::from(side == 200);
                totals[index] = totals[index].checked_add(gold)?;
            }
            Some(TimelineFrame {
                timestamp: frame["timestamp"].as_u64()?,
                gold_diff: totals[0].checked_sub(totals[1])?,
                blue_gold: totals[0],
                red_gold: totals[1],
            })
        })
        .collect::<Option<Vec<_>>>()?;
    if frames.len() < 2
        || frames
            .windows(2)
            .any(|pair| pair[0].timestamp >= pair[1].timestamp)
    {
        return None;
    }
    let start = frames.first()?.timestamp;
    let end = frames.last()?.timestamp;
    let mut events: Vec<_> = source
        .iter()
        .filter_map(|f| f["events"].as_array())
        .flatten()
        .filter_map(|event| {
            let timestamp = event["timestamp"]
                .as_u64()
                .filter(|time| *time >= start && *time <= end)?;
            if event["type"].as_str()? == "CHAMPION_KILL" {
                let killer_id = event["killerId"].as_u64()?;
                let victim_id = event["victimId"].as_u64()?;
                let team_id = *sides.get(&killer_id)?;
                if team_id == *sides.get(&victim_id)? {
                    return None;
                }
                return Some(TimelineEvent {
                    timestamp,
                    team_id,
                    label: "Champion kill".into(),
                    killer_id: Some(killer_id),
                    victim_id: Some(victim_id),
                    position: (map_id == 11)
                        .then(|| timeline_position(&event["position"]))
                        .flatten(),
                });
            }
            let (label, team_id) = match event["type"].as_str()? {
                "BUILDING_KILL" => {
                    let label = match event["buildingType"].as_str()? {
                        "TOWER_BUILDING" => "Tower",
                        "INHIBITOR_BUILDING" => "Inhibitor",
                        _ => return None,
                    };
                    // The building's teamId is the side that LOST it, even for minion kills.
                    let team = match event["teamId"].as_u64()? {
                        100 => 200,
                        200 => 100,
                        _ => return None,
                    };
                    (label, team)
                }
                "ELITE_MONSTER_KILL" => {
                    let label = match event["monsterType"].as_str()? {
                        "DRAGON" if event["monsterSubType"] == "ELDER_DRAGON" => "Elder",
                        "DRAGON" => "Dragon",
                        "BARON_NASHOR" => "Baron",
                        "RIFTHERALD" => "Herald",
                        "HORDE" => "Grub",
                        _ => return None,
                    };
                    let team = event["killerTeamId"]
                        .as_u64()
                        .filter(|team| *team == 100 || *team == 200)
                        .or_else(|| {
                            event["killerId"]
                                .as_u64()
                                .and_then(|id| sides.get(&id).copied())
                        })?;
                    (label, team)
                }
                _ => return None,
            };
            Some(TimelineEvent {
                timestamp,
                team_id,
                label: label.into(),
                killer_id: None,
                victim_id: None,
                position: None,
            })
        })
        .collect();
    events.sort_by_key(|event| event.timestamp);
    Some(Timeline { frames, events })
}

fn objectives(team: &Value) -> Vec<Objective> {
    [
        ("Dragons", "dragonKills", "dragon"),
        ("Heralds", "riftHeraldKills", "riftHerald"),
        ("Grubs", "hordeKills", "horde"),
        ("Barons", "baronKills", "baron"),
        ("Towers", "towerKills", "tower"),
        ("Inhibitors", "inhibitorKills", "inhibitor"),
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

/// A team's bans in pick order; a skipped ban (-1) is left out.
fn bans(team: &Value) -> Vec<i64> {
    let mut bans: Vec<(i64, i64)> = team["bans"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|ban| Some((ban["pickTurn"].as_i64().unwrap_or(0), ban["championId"].as_i64().filter(|id| *id > 0)?)))
        .collect();
    bans.sort_by_key(|(turn, _)| *turn);
    bans.into_iter().map(|(_, id)| id).collect()
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
    // Same per-team assignment as the history rows, so labels and lane opponents agree.
    let roles = model::assigned_roles(value);
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
                role: roles
                    .get(index)
                    .copied()
                    .filter(|role| !role.is_empty())
                    .unwrap_or_else(|| model::role(p))
                    .into(),
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
                stats: extract_stats(s),
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
                bans: team.map(bans).unwrap_or_default(),
            }
        })
        .collect();
    Ok(Detail {
        game,
        players,
        teams,
        timeline: None,
    })
}

#[derive(Default)]
struct Cache {
    owner: String,
    matches: BTreeMap<(u64, String), Detail>,
    // Solo/Duo and Flex choices, including their fallbacks; keep only display data.
    ranks: BTreeMap<String, [Option<MatchRank>; 2]>,
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

/// Opened matches are also kept on disk, so they load after a restart and with
/// League Client closed. Ranks and timeline stay as they were when first opened.
const SAVED_LIMIT: usize = 50;
const SAVED_MAX_BYTES: u64 = 8 * 1024 * 1024;
static SAVED_GATE: OnceLock<std::sync::Mutex<()>> = OnceLock::new();

#[derive(Serialize, Deserialize)]
struct Saved {
    game_id: u64,
    /// Lowercased Riot ID whose view of the match this is.
    perspective: String,
    detail: Detail,
}

fn saved_path() -> Option<std::path::PathBuf> {
    Some(crate::vault::data_root().ok()?.join("match-details.json"))
}

fn read_saved_at(path: &std::path::Path) -> Vec<Saved> {
    if std::fs::metadata(path).map_or(true, |m| m.len() > SAVED_MAX_BYTES) {
        return vec![];
    }
    let mut saved: Vec<Saved> =
        serde_json::from_slice(&std::fs::read(path).unwrap_or_default()).unwrap_or_default();
    saved.truncate(SAVED_LIMIT);
    saved
}

fn saved_detail_at(path: &std::path::Path, key: &(u64, String)) -> Option<Detail> {
    read_saved_at(path)
        .into_iter()
        .find(|s| s.game_id == key.0 && s.perspective == key.1)
        .map(|s| s.detail)
}

/// Newest first; a failed write only costs the offline copy, never the shown match.
fn save_detail_at(path: &std::path::Path, key: &(u64, String), detail: &Detail) {
    let Ok(_guard) = SAVED_GATE.get_or_init(Default::default).lock() else {
        return;
    };
    let mut saved = read_saved_at(path);
    saved.retain(|s| !(s.game_id == key.0 && s.perspective == key.1));
    saved.insert(
        0,
        Saved {
            game_id: key.0,
            perspective: key.1.clone(),
            detail: detail.clone(),
        },
    );
    saved.truncate(SAVED_LIMIT);
    let Ok(bytes) = serde_json::to_vec(&saved) else {
        return;
    };
    if bytes.len() as u64 > SAVED_MAX_BYTES {
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

fn valid_perspective(perspective: &str) -> Result<(), String> {
    if perspective.len() > 128 || perspective.chars().any(char::is_control) {
        return Err("Invalid player name.".into());
    }
    Ok(())
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
    // A match opened before loads without League Client. No Riot ID means the
    // dashboard owner, whose Riot ID the saved dashboard knows.
    let known = riot_id
        .clone()
        .or_else(|| full_app::read_cache().map(|snapshot| snapshot.summoner.riot_id));
    if let Some(perspective) = known {
        valid_perspective(&perspective)?;
        let key = (game_id, perspective.to_lowercase());
        let remembered = CACHE.get_or_init(Default::default).lock().await.matches.get(&key).cloned();
        if let Some(detail) = remembered.or_else(|| saved_detail_at(&saved_path()?, &key)) {
            return Ok(detail);
        }
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
    valid_perspective(&perspective)?;
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
    let rank_requests = stream::iter(pending.into_iter().map(|puuid| {
        let client = &client;
        async move {
            let rank = get(client, &format!("/lol-ranked/v1/ranked-stats/{puuid}")).await;
            (puuid, rank)
        }
    }))
    .buffer_unordered(3)
    .collect::<Vec<_>>();
    let timeline_path = format!("/lol-match-history/v1/game-timelines/{game_id}");
    let (ranks, timeline) = tokio::join!(rank_requests, get(&client, &timeline_path));
    detail.timeline = timeline.ok().and_then(|timeline| {
        value["participants"].as_array().and_then(|participants| {
            timeline_from_participants(
                &timeline,
                participants,
                value["mapId"].as_u64().unwrap_or(0),
            )
        })
    });
    for (puuid, result) in ranks {
        if let Ok(value) = result {
            cache.ranks.insert(
                puuid,
                [
                    rank_for_match(&value, "solo"),
                    rank_for_match(&value, "flex"),
                ],
            );
        }
    }
    for player in &mut detail.players {
        let puuid = identities
            .iter()
            .find(|i| i["participantId"] == player.participant_id)
            .and_then(|i| i["player"]["puuid"].as_str());
        if let Some(rank) = puuid.and_then(|id| cache.ranks.get(id)) {
            player.rank = rank[usize::from(detail.game.queue == "flex")].clone();
            player.rank_loaded = true;
        }
    }
    if cache.matches.len() >= 20 {
        cache.matches.clear();
    }
    if cache.ranks.len() > 200 {
        cache.ranks.clear();
    }
    if let Some(path) = saved_path() {
        save_detail_at(&path, &key, &detail);
    }
    cache.matches.insert(key, detail.clone());
    Ok(detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn current_rank_uses_match_queue_and_preserves_lp() {
        let value = json!({"queueMap": {
            "RANKED_SOLO_5x5": {"tier": "MASTER", "division": "I", "leaguePoints": 200},
            "RANKED_FLEX_SR": {"tier": "DIAMOND", "division": "II", "leaguePoints": 45}
        }});
        let solo = serde_json::to_value(rank_for_match(&value, "solo")).unwrap();
        assert_eq!(solo["tier"], "MASTER");
        assert_eq!(solo["division"], "");
        assert_eq!(solo["lp"], 200);
        assert_eq!(solo["queue"], "Solo/Duo");
        let flex = serde_json::to_value(rank_for_match(&value, "flex")).unwrap();
        assert_eq!(flex["tier"], "DIAMOND");
        assert_eq!(flex["division"], "II");
        assert_eq!(flex["lp"], 45);
        assert_eq!(flex["queue"], "Flex");
        assert_eq!(
            serde_json::to_value(rank_for_match(&value, "normal")).unwrap(),
            solo
        );
    }

    #[test]
    fn current_rank_falls_back_across_queue_shapes_without_inventing_lp() {
        let queue = json!({"queueType": "RANKED_FLEX_SR", "tier": "GOLD", "division": "IV", "leaguePoints": 0});
        let mut value =
            json!({"queues": [queue], "queueMap": {"RANKED_SOLO_5x5": {"tier": "UNRANKED"}}});
        let fallback = serde_json::to_value(rank_for_match(&value, "solo")).unwrap();
        assert_eq!(fallback["queue"], "Flex");
        assert_eq!(fallback["lp"], 0);
        for missing in [json!(null), json!(-1), json!("45")] {
            value["queues"][0]["leaguePoints"] = missing;
            assert!(serde_json::to_value(rank_for_match(&value, "flex")).unwrap()["lp"].is_null());
        }
        for tier in ["NONE", "UNRANKED", "", "INVALID"] {
            value["queues"][0]["tier"] = json!(tier);
            assert!(rank_for_match(&value, "solo").is_none());
        }
        assert!(rank_for_match(&json!({}), "solo").is_none());
    }

    #[test]
    fn stat_extraction_preserves_zero_false_and_unknown_numeric_fields_only() {
        let stats = extract_stats(
            &json!({"kills": 0, "firstBloodKill": false, "newStat": 12.5,
            "missing": null, "text": "private", "object": {"x": 1}}),
        );
        assert_eq!(stats.len(), 3);
        assert_eq!(stats["kills"], 0);
        assert_eq!(stats["firstBloodKill"], false);
        assert_eq!(stats["newStat"], 12.5);
    }

    #[test]
    fn bans_follow_pick_order_and_skip_empty_ones() {
        let team = json!({"bans": [{"championId": 64, "pickTurn": 3}, {"championId": -1, "pickTurn": 2}, {"championId": 157, "pickTurn": 1}]});
        assert_eq!(bans(&team), vec![157, 64]);
        assert!(bans(&json!({})).is_empty());
    }

    #[test]
    fn timeline_uses_explicit_team_membership_and_damaged_building_side() {
        let participants: Vec<_> = (1..=10)
            .map(|id| {
                json!({"participantId": id,
            "teamId": if id % 2 == 0 {100} else {200}})
            })
            .collect();
        let frame = |timestamp, blue_gold, red_gold| {
            let entries: serde_json::Map<String, Value> = (1..=10).map(|id| (id.to_string(),
                json!({"participantId": id, "totalGold": if id % 2 == 0 {blue_gold} else {red_gold}}))).collect();
            json!({"timestamp": timestamp, "participantFrames": entries, "events": []})
        };
        let mut value = json!({"frames": [frame(0, 500, 500), frame(60000, 1500, 1000)]});
        value["frames"][1]["events"] = json!([
            {"type": "BUILDING_KILL", "timestamp": 30000, "teamId": 100, "killerId": 0, "buildingType": "TOWER_BUILDING"},
            {"type": "ELITE_MONSTER_KILL", "timestamp": 40000, "killerId": 2, "teamId": 0, "monsterType": "DRAGON", "monsterSubType": "ELDER_DRAGON"},
            {"type": "ELITE_MONSTER_KILL", "timestamp": 45000, "killerId": 0, "teamId": 0, "monsterType": "DRAGON"},
            {"type": "CHAMPION_KILL", "timestamp": 50000, "killerId": 2, "victimId": 1, "position": {"x": 2443, "y": 12747}},
            {"type": "CHAMPION_KILL", "timestamp": 51000, "killerId": 0, "victimId": 1},
            {"type": "CHAMPION_KILL", "timestamp": 52000, "killerId": 2, "victimId": 99},
            {"type": "CHAMPION_KILL", "timestamp": 53000, "killerId": 2, "victimId": 4}
        ]);
        let timeline = timeline_from_participants(&value, &participants, 11).unwrap();
        assert_eq!(timeline.frames[0].gold_diff, 0);
        assert_eq!(timeline.frames[1].gold_diff, 2500);
        assert_eq!(timeline.events.len(), 3);
        assert_eq!(timeline.events[0].team_id, 200);
        assert_eq!(timeline.events[1].team_id, 100);
        assert_eq!(timeline.events[1].label, "Elder");
        let serialized = serde_json::to_value(&timeline).unwrap();
        assert_eq!(serialized["frames"][1]["blueGold"], 7500);
        assert_eq!(serialized["frames"][1]["redGold"], 5000);
        assert_eq!(serialized["events"][2]["killerId"], 2);
        assert_eq!(serialized["events"][2]["victimId"], 1);
        assert_eq!(serialized["events"][2]["teamId"], 100);
        assert_eq!(
            serialized["events"][2]["position"],
            json!({"x": 2443, "y": 12747})
        );
        let other_map = timeline_from_participants(&value, &participants, 12).unwrap();
        assert_eq!(other_map.events.len(), 3);
        assert!(other_map
            .events
            .iter()
            .all(|event| event.position.is_none()));
        for position in [
            json!(null),
            json!({"x": -1, "y": 2}),
            json!({"x": 1}),
            json!({"x": 15001, "y": 1}),
        ] {
            let mut missing = value.clone();
            missing["frames"][1]["events"][3]["position"] = position;
            let parsed = timeline_from_participants(&missing, &participants, 11).unwrap();
            let serialized = serde_json::to_value(parsed).unwrap();
            assert!(serialized["events"][2].get("position").is_none());
        }
        assert!(serialized["events"][0].get("killerId").is_none());
        let mut invalid = value.clone();
        invalid["frames"][1]["timestamp"] = json!(0);
        assert!(timeline_from_participants(&invalid, &participants, 11).is_none());
        invalid = value.clone();
        invalid["frames"][1]["participantFrames"]["2"]["totalGold"] = json!(-1);
        assert!(timeline_from_participants(&invalid, &participants, 11).is_none());
        invalid = value.clone();
        invalid["frames"][1]["participantFrames"]["2"]["participantId"] = json!(3);
        assert!(timeline_from_participants(&invalid, &participants, 11).is_none());
        value["frames"][1]["participantFrames"]
            .as_object_mut()
            .unwrap()
            .remove("2");
        assert!(timeline_from_participants(&value, &participants, 11).is_none());
        assert!(timeline_from_participants(&value, &participants[..9], 11).is_none());
        assert!(timeline_from_participants(&json!({"frames": []}), &participants, 11).is_none());
    }

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
    fn opened_matches_load_again_from_disk_newest_first() {
        let owner = model::Summoner {
            puuid: "fixture-self".into(),
            summoner_id: 1,
            riot_id: "You#TEST".into(),
            level: 1,
            icon: None,
        };
        let participants: Vec<_> = (1..=10).map(|id| json!({
            "participantId": id, "teamId": if id <= 5 { 100 } else { 200 }, "championId": 103,
            "stats": { "win": id <= 5, "kills": 2, "deaths": 1, "assists": 3 }
        })).collect();
        let value = json!({ "gameId": 42, "mapId": 11, "queueId": 420, "gameDuration": 1200, "gameCreation": 1234,
            "participants": participants, "participantIdentities": [{"participantId": 1, "player": {"puuid": "fixture-self", "gameName": "You", "tagLine": "TEST"}}],
            "teams": [{"teamId": 100, "win": "Win", "dragonKills": 2}] });
        let detail = parse(&value, &owner).unwrap();
        let path = std::env::temp_dir().join(format!("match-details-test-{}.json", uuid::Uuid::new_v4()));
        let key = (42, "you#test".to_string());

        assert!(saved_detail_at(&path, &key).is_none());
        save_detail_at(&path, &key, &detail);
        for game_id in 100..100 + SAVED_LIMIT as u64 {
            save_detail_at(&path, &(game_id, key.1.clone()), &detail);
        }
        assert!(saved_detail_at(&path, &key).is_none(), "the oldest match drops past the limit");
        save_detail_at(&path, &key, &detail);

        let loaded = saved_detail_at(&path, &key).expect("saved match loads again");
        assert_eq!(loaded.players.len(), 10);
        assert!(loaded.players[0].me);
        assert!(saved_detail_at(&path, &(42, "someone#else".into())).is_none());
        assert_eq!(read_saved_at(&path).len(), SAVED_LIMIT);
        let _ = std::fs::remove_file(path);
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
        assert_eq!(detail.players[0].stats["kills"], 2);
        assert!(!detail.players[0].stats.contains_key("goldEarned"));
        assert!(detail.timeline.is_none());
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
