use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summoner {
    pub puuid: String,
    pub summoner_id: u64,
    pub riot_id: String,
    pub level: u64,
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rank {
    pub tier: String,
    pub division: String,
    pub lp: u64,
    pub wins: u64,
    pub losses: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: u64,
    pub queue: String,
    pub champion_id: i64,
    pub created_at: u64,
    pub duration: u64,
    pub win: bool,
    pub kills: u64,
    pub deaths: u64,
    pub assists: u64,
    pub metrics: [Option<f64>; 6],
    /// Seven slots, trinket last; 0 is an empty slot. Empty for snapshots
    /// cached before these fields existed.
    #[serde(default)]
    pub items: Vec<i64>,
    #[serde(default)]
    pub spells: Vec<i64>,
    #[serde(default)]
    pub keystone: i64,
    #[serde(default)]
    pub cs: u64,
    #[serde(default)]
    pub level: u64,
    /// Estimated team assignment; empty when the match cannot be placed.
    #[serde(default)]
    pub role: String,
    /// 1-10 among all players by `placement`; 0 when unknown.
    #[serde(default)]
    pub placement: u8,
    /// Solo/Duo LP gained or lost, when Swapper recorded the rank around it.
    #[serde(default)]
    pub lp_change: Option<i64>,
    /// All ten players (own team first), for the expanded match row and the
    /// lane-opponent comparison. Empty for snapshots cached before this field.
    #[serde(default)]
    pub players: Vec<Player>,
    /// Index into `players` of the lane opponent, when it is unambiguous.
    #[serde(default)]
    pub opponent: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub champion_id: i64,
    pub riot_id: String,
    pub me: bool,
    pub ally: bool,
    pub role: String,
    pub kills: u64,
    pub deaths: u64,
    pub assists: u64,
    pub cs: u64,
    pub damage: u64,
    pub gold: u64,
    pub vision: u64,
    pub items: Vec<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metric {
    pub average: Option<f64>,
    pub delta: Option<f64>,
    pub points: Vec<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Champion {
    pub id: i64,
    pub wins: usize,
    pub losses: usize,
    pub kda: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RoleRecord {
    pub role: String,
    pub wins: usize,
    pub games: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub metrics: Vec<Metric>,
    pub champions: Vec<Champion>,
    #[serde(default)]
    pub roles: Vec<RoleRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(default)]
    pub champion_names: BTreeMap<i64, String>,
    pub summoner: Summoner,
    pub rank: Option<Rank>,
    pub fetched_at: i64,
    pub games: Vec<Game>,
    pub summaries: BTreeMap<String, Summary>,
    /// Recorded Solo/Duo rank points, oldest first; empty for older caches.
    #[serde(default)]
    pub lp_history: Vec<super::lp::Point>,
}

pub fn summoner(value: &Value) -> Option<Summoner> {
    let puuid = value["puuid"].as_str()?.to_string();
    if puuid.is_empty() {
        return None;
    }
    let name = value["gameName"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| value["displayName"].as_str())
        .unwrap_or("Summoner");
    let tag = value["tagLine"].as_str().unwrap_or("");
    Some(Summoner {
        puuid,
        summoner_id: value["summonerId"].as_u64().unwrap_or(0),
        riot_id: if tag.is_empty() {
            name.into()
        } else {
            format!("{name}#{tag}")
        },
        level: value["summonerLevel"].as_u64().unwrap_or(0),
        icon: None,
    })
}

pub fn rank(value: &Value) -> Option<Rank> {
    rank_with_record(value).map(|(rank, _)| rank)
}

pub fn rank_with_record(value: &Value) -> Option<(Rank, bool)> {
    let queue = value["queueMap"]["RANKED_SOLO_5x5"]
        .as_object()
        .map(|_| &value["queueMap"]["RANKED_SOLO_5x5"])
        .or_else(|| {
            value["queues"]
                .as_array()?
                .iter()
                .find(|q| q["queueType"] == "RANKED_SOLO_5x5")
        })?;
    let tier = queue["tier"].as_str()?.to_string();
    if tier.is_empty() || tier == "NONE" || tier == "UNRANKED" {
        return None;
    }
    let record_available = queue["wins"].as_u64().is_some() && queue["losses"].as_u64().is_some();
    Some((
        Rank {
            tier,
            division: queue["division"]
                .as_str()
                .filter(|s| *s != "NA")
                .unwrap_or("")
                .into(),
            lp: queue["leaguePoints"].as_u64().unwrap_or(0),
            wins: queue["wins"].as_u64().unwrap_or(0),
            losses: queue["losses"].as_u64().unwrap_or(0),
        },
        record_available,
    ))
}

pub fn eligible(value: &Value) -> bool {
    value["mapId"] == 11
        && matches!(
            value["queueId"].as_u64(),
            Some(400 | 420 | 430 | 440 | 480 | 490)
        )
        && value["gameDuration"].as_u64().unwrap_or(0) >= 300
}

pub fn game(value: &Value, player: &Summoner) -> Option<Game> {
    if !eligible(value) {
        return None;
    }
    let identity = value["participantIdentities"]
        .as_array()?
        .iter()
        .find(|identity| {
            identity["player"]["puuid"].as_str() == Some(&player.puuid)
                || (player.summoner_id != 0
                    && identity["player"]["puuid"]
                        .as_str()
                        .unwrap_or("")
                        .is_empty()
                    && identity["player"]["summonerId"].as_u64() == Some(player.summoner_id))
        })?;
    let participants = value["participants"].as_array()?;
    let own = participants
        .iter()
        .find(|p| p["participantId"] == identity["participantId"])?;
    let stats = &own["stats"];
    if stats["gameEndedInEarlySurrender"] == true {
        return None;
    }
    let kills = stats["kills"].as_u64()?;
    let deaths = stats["deaths"].as_u64()?;
    let assists = stats["assists"].as_u64()?;
    let duration = value["gameDuration"].as_u64()?;
    let minutes = duration as f64 / 60.0;
    let team: Vec<_> = participants
        .iter()
        .filter(|p| p["teamId"] == own["teamId"])
        .collect();
    let team_total = |key: &str| -> Option<f64> {
        if team.len() != 5 {
            return None;
        }
        let total: u64 = team
            .iter()
            .map(|p| p["stats"][key].as_u64())
            .sum::<Option<u64>>()?;
        (total > 0).then_some(total as f64)
    };
    let damage = stats["totalDamageDealtToChampions"]
        .as_u64()
        .map(|n| n as f64);
    let total_cs = stats["totalMinionsKilled"]
        .as_u64()
        .zip(stats["neutralMinionsKilled"].as_u64())
        .map(|(lane, jungle)| lane + jungle);
    let cs = total_cs.map(|n| n as f64 / minutes);
    Some(Game {
        id: value["gameId"].as_u64()?,
        queue: match value["queueId"].as_u64()? {
            420 => "solo",
            440 => "flex",
            _ => "normal",
        }
        .into(),
        champion_id: own["championId"].as_i64()?,
        created_at: value["gameCreation"].as_u64()?,
        duration,
        win: stats["win"].as_bool()?,
        kills,
        deaths,
        assists,
        metrics: [
            Some((kills + assists) as f64 / deaths.max(1) as f64),
            team_total("kills").map(|total| (100.0 * (kills + assists) as f64 / total).min(100.0)),
            cs,
            damage.map(|n| n / minutes),
            damage
                .zip(team_total("totalDamageDealtToChampions"))
                .map(|(n, total)| 100.0 * n / total),
            stats["visionScore"].as_u64().map(|n| n as f64 / minutes),
        ],
        items: (0..7)
            .map(|i| stats[format!("item{i}")].as_i64().unwrap_or(0))
            .collect(),
        spells: ["spell1Id", "spell2Id"]
            .iter()
            .map(|key| own[*key].as_i64().unwrap_or(0))
            .collect(),
        keystone: stats["perk0"].as_i64().unwrap_or(0),
        cs: total_cs.unwrap_or(0),
        level: stats["champLevel"].as_u64().unwrap_or(0),
        role: role(own).into(),
        placement: placement(participants, own),
        lp_change: None,
        opponent: None,
        players: Vec::new(),
    })
    .map(|mut game| {
        (game.players, game.opponent) = players(value, own);
        if let Some(assigned) = game.players.iter().find(|p| p.me && !p.role.is_empty()) {
            game.role = assigned.role.clone();
        } else if !has_smite(own) && !support_item(own)
            && !matches!(own["timeline"]["lane"].as_str(), Some("TOP" | "MIDDLE" | "BOTTOM")) {
            // Partial history rows can carry an explicit role hint, but no opponent.
            game.role.clear();
        }
        game
    })
}

/// Every player's line, own team first, using one assignment for each team.
fn players(value: &Value, own: &Value) -> (Vec<Player>, Option<u8>) {
    let Some(participants) = value["participants"].as_array() else {
        return (Vec::new(), None);
    };
    let identities = value["participantIdentities"].as_array();
    let riot_id = |id: &Value| -> String {
        identities
            .and_then(|all| all.iter().find(|i| i["participantId"] == *id))
            .map(|i| {
                let p = &i["player"];
                let name = p["gameName"].as_str().unwrap_or("");
                match p["tagLine"].as_str().filter(|t| !t.is_empty() && !name.is_empty()) {
                    Some(tag) => format!("{name}#{tag}"),
                    None => name.to_string(),
                }
            })
            .unwrap_or_default()
    };
    let roles = assigned_roles(value);
    let mut ordered: Vec<_> = participants.iter().enumerate().collect();
    ordered.sort_by_key(|(_, p)| p["teamId"] != own["teamId"]);
    let lines: Vec<Player> = ordered
        .iter()
        .map(|(index, p)| {
            let s = &p["stats"];
            let n = |key: &str| s[key].as_u64().unwrap_or(0);
            Player {
                champion_id: p["championId"].as_i64().unwrap_or(0),
                riot_id: riot_id(&p["participantId"]),
                me: p["participantId"] == own["participantId"],
                ally: p["teamId"] == own["teamId"],
                role: roles[*index].into(),
                kills: n("kills"),
                deaths: n("deaths"),
                assists: n("assists"),
                cs: n("totalMinionsKilled") + n("neutralMinionsKilled"),
                damage: n("totalDamageDealtToChampions"),
                gold: n("goldEarned"),
                vision: n("visionScore"),
                items: (0..7).map(|i| s[format!("item{i}")].as_i64().unwrap_or(0)).collect(),
            }
        })
        .collect();
    let opponent = lane_opponent(&lines);
    (lines, opponent)
}

pub fn lane_opponent(players: &[Player]) -> Option<u8> {
    let me = players.iter().find(|p| p.me)?;
    let same = |ally: bool| {
        players
            .iter()
            .enumerate()
            .filter(move |(_, p)| p.ally == ally && p.role == me.role)
    };
    if players.len() != 10 || !me.ally || !ROLES.contains(&me.role.as_str())
        || players.iter().filter(|p| p.ally).count() != 5 || same(true).count() != 1 {
        return None;
    }
    let mut enemies = same(false);
    match (enemies.next(), enemies.next()) {
        (Some((i, _)), None) => Some(i as u8),
        _ => None,
    }
}

const SMITE: i64 = 11;
const ROLES: [&str; 5] = ["top", "jungle", "mid", "adc", "support"];
/// Verified against the client's items.json, including historical quest lines.
const SUPPORT_ITEMS: [i64; 20] = [3850, 3851, 3853, 3854, 3855, 3857, 3858, 3859, 3860, 3862, 3863, 3864,
    3865, 3866, 3867, 3869, 3870, 3871, 3876, 3877];

fn has_smite(p: &Value) -> bool {
    ["spell1Id", "spell2Id"].iter().any(|key| p[*key].as_i64() == Some(SMITE))
}

fn support_item(p: &Value) -> bool {
    (0..7).any(|i| p["stats"][format!("item{i}")].as_i64().is_some_and(|id| SUPPORT_ITEMS.contains(&id)))
}

/// Smite and quest items anchor the assignment; only the three solo/carry slots
/// compete for lane hints. CS/gold break ties when the client reports NONE.
pub fn assigned_roles(value: &Value) -> Vec<&'static str> {
    let Some(all) = value["participants"].as_array() else { return vec![] };
    let mut roles = vec![""; all.len()];
    if !eligible(value) || all.len() != 10 || all.iter().any(|p| p["stats"]["gameEndedInEarlySurrender"] == true) {
        return roles;
    }
    let n = |i: usize, key: &str| all[i]["stats"][key].as_u64().unwrap_or(0) as f64;
    let cs = |i| n(i, "totalMinionsKilled") + n(i, "neutralMinionsKilled");
    for team_id in [100, 200] {
        let team: Vec<_> = (0..all.len()).filter(|&i| all[i]["teamId"] == team_id).collect();
        if team.len() != 5 || team.iter().any(|&i| all[i]["championId"].as_i64().unwrap_or(0) <= 0
            || all[i]["stats"]["totalMinionsKilled"].as_u64().is_none()) {
            return vec![""; all.len()];
        }
        let jungle = *team.iter().max_by_key(|&&i| (
            has_smite(&all[i]),
            all[i]["stats"]["neutralMinionsKilled"].as_u64().unwrap_or(0),
            all[i]["timeline"]["lane"] == "JUNGLE",
        )).expect("five players");
        roles[jungle] = "jungle";
        let support_score = |i: usize| {
            let hint = matches!(all[i]["timeline"]["role"].as_str(), Some("SUPPORT" | "DUO_SUPPORT"));
            // Quest items dominate; low farming plus vision/assists survive sold items.
            if support_item(&all[i]) { -100_000.0 } else { cs(i) * 3.0 - n(i, "visionScore") - n(i, "assists") * 2.0 - if hint { 80.0 } else { 0.0 } }
        };
        let support = *team.iter().filter(|&&i| i != jungle)
            .min_by(|&&a, &&b| support_score(a).total_cmp(&support_score(b))).expect("four non-junglers");
        roles[support] = "support";
        let remaining: Vec<_> = team.into_iter().filter(|&i| i != jungle && i != support).collect();
        let score = |i: usize, slot: usize| {
            let lane = all[i]["timeline"]["lane"].as_str().unwrap_or("");
            let hint = match (slot, lane) { (0, "TOP") | (1, "MIDDLE" | "MID") | (2, "BOTTOM" | "BOT") => 120.0, _ => 0.0 };
            hint + (slot + 1) as f64 * (cs(i) * 0.04 + n(i, "goldEarned") * 0.0004)
        };
        let orders = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
        let best = orders.iter().max_by(|a, b| {
            let total = |order: &[usize; 3]| (0..3).map(|slot| score(remaining[order[slot]], slot)).sum::<f64>();
            total(a).total_cmp(&total(b))
        }).expect("six assignments");
        for (slot, role) in ["top", "mid", "adc"].iter().enumerate() {
            roles[remaining[best[slot]]] = role;
        }
    }
    roles
}

/// The client's `timeline.lane` is unreliable (mid laners often read as
/// JUNGLE), so Smite and the support item decide first, then the lane.
pub fn role(participant: &Value) -> &'static str {
    let has_smite = has_smite(participant);
    let support_item = support_item(participant);
    let timeline = &participant["timeline"];
    match (has_smite, support_item, timeline["lane"].as_str()) {
        (true, _, _) => "jungle",
        (_, true, _) => "support",
        (_, _, Some("TOP")) => "top",
        (_, _, Some("BOTTOM")) if timeline["role"] == "SUPPORT" || timeline["role"] == "DUO_SUPPORT" => "support",
        (_, _, Some("BOTTOM")) => "adc",
        _ => "mid",
    }
}

/// Rank among all ten players by a simple score: KDA, damage to champions,
/// CS and vision, each relative to the game's best, weighted 35/35/15/15.
pub fn placement(participants: &[Value], own: &Value) -> u8 {
    let Some(scores) = performance_scores(participants) else { return 0 };
    let Some(index) = participants.iter().position(|p| p["participantId"] == own["participantId"]) else { return 0 };
    1 + scores.iter().filter(|score| **score > scores[index] + 1e-7).count() as u8
}

/// The same weighted score as placement, exposed on a 0–100 scale.
pub fn performance_scores(participants: &[Value]) -> Option<Vec<f64>> {
    let line = |p: &Value| -> Option<[f64; 4]> {
        let s = &p["stats"];
        let (k, d, a) = (s["kills"].as_u64()?, s["deaths"].as_u64()?, s["assists"].as_u64()?);
        Some([
            (k + a) as f64 / d.max(1) as f64,
            s["totalDamageDealtToChampions"].as_u64()? as f64,
            (s["totalMinionsKilled"].as_u64()? + s["neutralMinionsKilled"].as_u64()?) as f64,
            s["visionScore"].as_u64()? as f64,
        ])
    };
    let lines = participants.iter().map(line).collect::<Option<Vec<_>>>()?;
    if lines.len() != 10 {
        return None;
    }
    let best: Vec<f64> = (0..4)
        .map(|i| lines.iter().map(|l| l[i]).fold(0.0, f64::max))
        .collect();
    let score = |l: &[f64; 4]| -> f64 {
        [0.35, 0.35, 0.15, 0.15]
            .iter()
            .enumerate()
            .map(|(i, w)| if best[i] > 0.0 { w * l[i] / best[i] } else { 0.0 })
            .sum()
    };
    Some(lines.iter().map(|line| (score(line) * 100.0).clamp(0.0, 100.0)).collect())
}

pub fn summarize(games: &[Game]) -> Summary {
    let metrics = (0..6)
        .map(|i| {
            let values: Vec<f64> = games.iter().filter_map(|g| g.metrics[i]).collect();
            let average =
                (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64);
            let recent: Vec<f64> = games.iter().take(5).filter_map(|g| g.metrics[i]).collect();
            let delta = average
                .filter(|_| !recent.is_empty())
                .map(|avg| recent.iter().sum::<f64>() / recent.len() as f64 - avg);
            Metric {
                average,
                delta,
                points: games
                    .iter()
                    .take(10)
                    .rev()
                    .filter_map(|g| g.metrics[i])
                    .collect(),
            }
        })
        .collect();
    let mut grouped: BTreeMap<i64, Vec<&Game>> = BTreeMap::new();
    for game in games {
        grouped.entry(game.champion_id).or_default().push(game);
    }
    let mut champions: Vec<_> = grouped
        .into_iter()
        .map(|(id, games)| {
            let wins = games.iter().filter(|g| g.win).count();
            Champion {
                id,
                wins,
                losses: games.len() - wins,
                kda: games.iter().map(|g| g.kills + g.assists).sum::<u64>() as f64
                    / games.iter().map(|g| g.deaths).sum::<u64>().max(1) as f64,
            }
        })
        .collect();
    champions.sort_by_key(|c| (std::cmp::Reverse(c.wins + c.losses), c.id));
    champions.truncate(5);
    let roles = ["top", "jungle", "mid", "adc", "support"]
        .into_iter()
        .map(|role| {
            let played: Vec<_> = games.iter().filter(|g| g.role == role).collect();
            RoleRecord {
                role: role.into(),
                wins: played.iter().filter(|g| g.win).count(),
                games: played.len(),
            }
        })
        .collect();
    Summary {
        metrics,
        champions,
        roles,
    }
}

pub fn summaries(games: &[Game]) -> BTreeMap<String, Summary> {
    ["all", "solo", "flex", "normal"]
        .into_iter()
        .map(|queue| {
            let filtered: Vec<_> = games
                .iter()
                .filter(|g| queue == "all" || g.queue == queue)
                .cloned()
                .collect();
            (queue.into(), summarize(&filtered))
        })
        .collect()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
