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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub metrics: Vec<Metric>,
    pub champions: Vec<Champion>,
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
    Some(Rank {
        tier,
        division: queue["division"]
            .as_str()
            .filter(|s| *s != "NA")
            .unwrap_or("")
            .into(),
        lp: queue["leaguePoints"].as_u64().unwrap_or(0),
        wins: queue["wins"].as_u64().unwrap_or(0),
        losses: queue["losses"].as_u64().unwrap_or(0),
    })
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
    let cs = stats["totalMinionsKilled"]
        .as_u64()
        .zip(stats["neutralMinionsKilled"].as_u64())
        .map(|(lane, jungle)| (lane + jungle) as f64 / minutes);
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
    })
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
    Summary { metrics, champions }
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
