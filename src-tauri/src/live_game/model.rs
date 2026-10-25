//! Pure Live Game data model: who is in the game (LCU gameflow session),
//! what they are doing (Live Client `allgamedata`), and what their recent
//! history says. No Tauri or network here, so a future native overlay widget
//! can build the same [`View`] and every rule is unit-tested.
use crate::full_app::model::{self as history, Game, Rank, Summoner};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// Phases in which a game is loading or running. Champion select is out of
/// scope: names are hidden there.
pub fn in_game(phase: &str) -> bool {
    matches!(phase, "GameStart" | "InProgress" | "Reconnect")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Blue,
    Red,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Form {
    pub games: usize,
    pub wins: usize,
    pub kills: u64,
    pub deaths: u64,
    pub assists: u64,
}

impl Form {
    fn of<'a>(games: impl Iterator<Item = &'a Game>) -> Self {
        games.fold(Self::default(), |mut form, game| {
            form.games += 1;
            form.wins += usize::from(game.win);
            form.kills += game.kills;
            form.deaths += game.deaths;
            form.assists += game.assists;
            form
        })
    }
}

/// One recent Rift game, for the player profile.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recent {
    pub champion_id: i64,
    pub queue: String,
    pub win: bool,
    pub kills: u64,
    pub deaths: u64,
    pub assists: u64,
    pub cs: u64,
    pub duration: u64,
    pub created_at: u64,
    pub role: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Badge {
    /// "streak-win", "streak-loss", "new-champ", "off-role", "one-trick".
    pub kind: &'static str,
    pub label: String,
    pub detail: String,
}

/// What a player's latest Rift games say. Built once per game per player.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub form: Form,
    pub on_champion: Form,
    /// Most played estimated role and its game count.
    pub main_role: Option<(String, usize)>,
    /// Positive: consecutive wins from the latest game; negative: losses.
    pub streak: i32,
    pub recent: Vec<Recent>,
}

/// Recent form from a player's newest-first history rows (any order works;
/// they are sorted). Only eligible Rift games count (see `full_app::model`).
pub fn summarize(rows: &[Value], puuid: &str, champion_id: i64) -> History {
    let player = Summoner {
        puuid: puuid.to_string(),
        summoner_id: 0,
        riot_id: String::new(),
        level: 0,
        icon: None,
    };
    let mut games: Vec<Game> = rows
        .iter()
        .filter_map(|row| history::game(row, &player))
        .collect();
    games.sort_by_key(|g| std::cmp::Reverse(g.created_at));
    games.truncate(20);
    let mut roles: BTreeMap<&str, usize> = BTreeMap::new();
    for game in &games {
        *roles.entry(game.role.as_str()).or_default() += 1;
    }
    let main_role = roles
        .iter()
        .max_by_key(|(role, count)| (**count, std::cmp::Reverse(**role)))
        .map(|(role, count)| (role.to_string(), *count));
    let first = games.first().map(|g| g.win);
    let run = games.iter().take_while(|g| Some(g.win) == first).count() as i32;
    History {
        form: Form::of(games.iter()),
        on_champion: Form::of(games.iter().filter(|g| g.champion_id == champion_id)),
        main_role,
        streak: if first == Some(true) { run } else { -run },
        recent: games
            .iter()
            .map(|g| Recent {
                champion_id: g.champion_id,
                queue: g.queue.clone(),
                win: g.win,
                kills: g.kills,
                deaths: g.deaths,
                assists: g.assists,
                cs: g.cs,
                duration: g.duration,
                created_at: g.created_at,
                role: g.role.clone(),
            })
            .collect(),
    }
}

/// Swapper role names for the LCU/Live Client position strings.
pub fn role_of(position: &str) -> Option<&'static str> {
    match position.to_ascii_uppercase().as_str() {
        "TOP" => Some("top"),
        "JUNGLE" => Some("jungle"),
        "MIDDLE" | "MID" => Some("mid"),
        "BOTTOM" | "BOT" | "ADC" => Some("adc"),
        "UTILITY" | "SUPPORT" => Some("support"),
        _ => None,
    }
}

/// Badges only where the sample supports them. `position` is the assigned
/// role ("" when the queue has none).
pub fn badges(history: &History, champion: &str, position: &str) -> Vec<Badge> {
    let mut out = Vec::new();
    let games = history.form.games;
    if history.streak >= 3 {
        out.push(Badge {
            kind: "streak-win",
            label: format!("{}W streak", history.streak),
            detail: format!("Won the latest {} Rift games", history.streak),
        });
    } else if history.streak <= -3 {
        out.push(Badge {
            kind: "streak-loss",
            label: format!("{}L streak", -history.streak),
            detail: format!("Lost the latest {} Rift games", -history.streak),
        });
    }
    // "First time" cannot be proven from 20 games, so the badge says what is known.
    if games >= 10 && history.on_champion.games == 0 {
        out.push(Badge {
            kind: "new-champ",
            label: "New on champ".into(),
            detail: format!("No {champion} games in the latest {games} Rift games"),
        });
    } else if games >= 10 && history.on_champion.games * 2 >= games {
        out.push(Badge {
            kind: "one-trick",
            label: "Main".into(),
            detail: format!(
                "{} of the latest {games} Rift games on {champion}",
                history.on_champion.games
            ),
        });
    }
    if let (Some(assigned), Some((main, count))) = (role_of(position), history.main_role.as_ref()) {
        // Off-role only when the usual role is clear: 10+ games, 60%+ in one role.
        if games >= 10 && *count * 5 >= games * 3 && main != assigned {
            out.push(Badge {
                kind: "off-role",
                label: "Off-role?".into(),
                detail: format!("Usually plays {main} ({count} of {games} games); estimated"),
            });
        }
    }
    out
}

/// Solo/Duo and Flex from `/lol-ranked/v1/ranked-stats/{puuid}`.
pub fn ranks(value: &Value) -> (Option<Rank>, Option<Rank>) {
    let flex = {
        let mut copy = serde_json::json!({ "queueMap": {} });
        copy["queueMap"]["RANKED_SOLO_5x5"] = value["queueMap"]["RANKED_FLEX_SR"].clone();
        history::rank(&copy)
    };
    (history::rank(value), flex)
}

const TIERS: [&str; 10] = [
    "IRON",
    "BRONZE",
    "SILVER",
    "GOLD",
    "PLATINUM",
    "EMERALD",
    "DIAMOND",
    "MASTER",
    "GRANDMASTER",
    "CHALLENGER",
];
const DIVISIONS: [&str; 4] = ["IV", "III", "II", "I"];

/// A rank on one line: 400 points per tier, 100 per division, plus LP.
/// Master and above share one ladder on top of Diamond I.
pub fn rank_score(rank: &Rank) -> Option<u32> {
    let tier = TIERS
        .iter()
        .position(|t| rank.tier.eq_ignore_ascii_case(t))? as u32;
    if tier >= 7 {
        return Some(7 * 400 + rank.lp as u32);
    }
    let division = DIVISIONS.iter().position(|d| *d == rank.division)? as u32;
    Some(tier * 400 + division * 100 + (rank.lp as u32).min(99))
}

/// The tier and division a score falls in ("EMERALD", "II").
pub fn score_label(score: u32) -> (String, String) {
    if score >= 2800 {
        return ("MASTER".into(), String::new());
    }
    (
        TIERS[(score / 400) as usize].into(),
        DIVISIONS[((score % 400) / 100) as usize].into(),
    )
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSummary {
    /// Average Solo/Duo rank of the ranked players, or `None` if nobody is.
    pub average_tier: Option<String>,
    pub average_division: String,
    pub ranked: usize,
    /// Share of physical damage dealers in percent (mixed counts half), from
    /// each champion's `tacticalInfo.damageType`; `None` until known.
    pub physical: Option<u32>,
}

pub fn team_summary(players: &[&Player]) -> TeamSummary {
    let scores: Vec<u32> = players
        .iter()
        .filter_map(|p| p.solo.as_ref().and_then(rank_score))
        .collect();
    let average = (!scores.is_empty()).then(|| scores.iter().sum::<u32>() / scores.len() as u32);
    let (tier, division) = average.map(score_label).unzip();
    let damage: Vec<f64> = players
        .iter()
        .filter_map(|p| match p.damage.as_str() {
            "kPhysical" => Some(1.0),
            "kMagic" => Some(0.0),
            "kMixed" => Some(0.5),
            _ => None,
        })
        .collect();
    TeamSummary {
        average_tier: tier,
        average_division: division.unwrap_or_default(),
        ranked: scores.len(),
        physical: (damage.len() == players.len() && !damage.is_empty())
            .then(|| (100.0 * damage.iter().sum::<f64>() / damage.len() as f64).round() as u32),
    }
}

/// Per-player in-game numbers from the Live Client, refreshed while the page
/// is visible.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Live {
    pub level: u64,
    pub kills: u64,
    pub deaths: u64,
    pub assists: u64,
    pub cs: u64,
    pub ward_score: f64,
    /// Seven slots by Live Client slot number (6 is the trinket); 0 is empty.
    pub items: Vec<i64>,
    pub dead: bool,
}

/// One `allPlayers` entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LivePlayer {
    pub riot_id: String,
    pub champion_name: String,
    pub side: Option<Side>,
    pub position: String,
    pub spells: [i64; 2],
    pub keystone: i64,
    pub primary_style: i64,
    pub secondary_style: i64,
    pub is_bot: bool,
    pub live: Live,
}

/// Summoner spell ids for the Live Client's `rawDisplayName` keys
/// ("GeneratedTip_SummonerSpell_SummonerFlash_DisplayName"). Used only when
/// the gameflow session has no spell ids for a player.
fn spell_id(raw: &str) -> i64 {
    const KNOWN: [(&str, i64); 11] = [
        ("SummonerBoost", 1),
        ("SummonerExhaust", 3),
        ("SummonerFlash", 4),
        ("SummonerHaste", 6),
        ("SummonerHeal", 7),
        ("SummonerSmite", 11),
        ("SummonerTeleport", 12),
        ("SummonerMana", 13),
        ("SummonerDot", 14),
        ("SummonerBarrier", 21),
        ("SummonerSnowball", 32),
    ];
    KNOWN
        .iter()
        .find(|(key, _)| raw.contains(key))
        .map(|(_, id)| *id)
        .unwrap_or(0)
}

pub fn live_players(data: &Value) -> Vec<LivePlayer> {
    let Some(players) = data["allPlayers"].as_array() else {
        return Vec::new();
    };
    players
        .iter()
        // Practice Tool lists its target dummies as nameless "Unknown" players with no champion.
        .filter(|p| p["championName"].as_str().is_some_and(|name| !name.is_empty()))
        .map(|p| {
            let mut items = vec![0; 7];
            for item in p["items"].as_array().into_iter().flatten() {
                if let (Some(slot), Some(id)) = (item["slot"].as_u64(), item["itemID"].as_i64()) {
                    if let Some(cell) = items.get_mut(slot as usize) {
                        *cell = id;
                    }
                }
            }
            let riot_id = p["riotId"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    let name = p["riotIdGameName"].as_str()?;
                    Some(format!(
                        "{name}#{}",
                        p["riotIdTagLine"].as_str().unwrap_or("")
                    ))
                })
                .or_else(|| p["summonerName"].as_str().map(str::to_string))
                .unwrap_or_default();
            let spell = |key: &str| {
                spell_id(
                    p["summonerSpells"][key]["rawDisplayName"]
                        .as_str()
                        .unwrap_or(""),
                )
            };
            LivePlayer {
                riot_id,
                champion_name: p["championName"].as_str().unwrap_or("").to_string(),
                side: match p["team"].as_str() {
                    Some("ORDER") => Some(Side::Blue),
                    Some("CHAOS") => Some(Side::Red),
                    _ => None,
                },
                position: p["position"].as_str().unwrap_or("").to_string(),
                spells: [spell("summonerSpellOne"), spell("summonerSpellTwo")],
                keystone: p["runes"]["keystone"]["id"].as_i64().unwrap_or(0),
                primary_style: p["runes"]["primaryRuneTree"]["id"].as_i64().unwrap_or(0),
                secondary_style: p["runes"]["secondaryRuneTree"]["id"].as_i64().unwrap_or(0),
                is_bot: p["isBot"] == true,
                live: Live {
                    level: p["level"].as_u64().unwrap_or(0),
                    kills: p["scores"]["kills"].as_u64().unwrap_or(0),
                    deaths: p["scores"]["deaths"].as_u64().unwrap_or(0),
                    assists: p["scores"]["assists"].as_u64().unwrap_or(0),
                    cs: p["scores"]["creepScore"].as_u64().unwrap_or(0),
                    ward_score: p["scores"]["wardScore"].as_f64().unwrap_or(0.0),
                    items,
                    dead: p["isDead"] == true,
                },
            }
        })
        .collect()
}

/// Per-player data fetch state, so the page can show partial results.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Not fetched yet (or a bot / no puuid: never fetched).
    Pending,
    Ready,
    /// Some endpoints failed; shows what arrived.
    Partial,
    /// Nothing to fetch (bot, hidden player).
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    /// Stable key: the puuid, or `live:<riot id>` for a player only the Live
    /// Client knows.
    pub key: String,
    #[serde(skip)]
    pub puuid: String,
    pub side: Side,
    pub champion_id: i64,
    pub champion_name: String,
    pub riot_id: String,
    pub is_self: bool,
    pub is_bot: bool,
    pub position: String,
    pub spells: [i64; 2],
    pub keystone: i64,
    pub primary_style: i64,
    pub secondary_style: i64,
    pub summoner_level: u64,
    pub profile_icon: i64,
    pub solo: Option<Rank>,
    pub flex: Option<Rank>,
    pub history: Option<History>,
    pub badges: Vec<Badge>,
    /// `tacticalInfo.damageType` of the champion ("kPhysical", "kMagic",
    /// "kMixed"), "" until known.
    pub damage: String,
    pub status: Status,
    pub live: Option<Live>,
}

impl Player {
    pub(crate) fn new(key: String, side: Side) -> Self {
        Self {
            key,
            puuid: String::new(),
            side,
            champion_id: 0,
            champion_name: String::new(),
            riot_id: String::new(),
            is_self: false,
            is_bot: false,
            position: String::new(),
            spells: [0, 0],
            keystone: 0,
            primary_style: 0,
            secondary_style: 0,
            summoner_level: 0,
            profile_icon: 0,
            solo: None,
            flex: None,
            history: None,
            badges: Vec::new(),
            damage: String::new(),
            status: Status::Pending,
            live: None,
        }
    }

    /// Applies one fetched player bundle (each part optional).
    pub fn apply(
        &mut self,
        summoner: Option<&Value>,
        ranked: Option<&Value>,
        rows: Option<&[Value]>,
    ) {
        if let Some(summoner) = summoner {
            if let Some(name) = summoner["gameName"].as_str().filter(|s| !s.is_empty()) {
                self.riot_id = format!("{name}#{}", summoner["tagLine"].as_str().unwrap_or(""));
            }
            self.summoner_level = summoner["summonerLevel"].as_u64().unwrap_or(0);
            self.profile_icon = summoner["profileIconId"].as_i64().unwrap_or(0);
        }
        if let Some(ranked) = ranked {
            (self.solo, self.flex) = ranks(ranked);
        }
        if let Some(rows) = rows {
            self.history = Some(summarize(rows, &self.puuid, self.champion_id));
        }
        self.status = if summoner.is_some() && ranked.is_some() && rows.is_some() {
            Status::Ready
        } else {
            Status::Partial
        };
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub game_id: u64,
    pub queue_id: i64,
    pub queue: String,
    pub map_id: i64,
    /// Blue side first, five per side in a normal game.
    pub players: Vec<Player>,
    pub blue: TeamSummary,
    pub red: TeamSummary,
    /// Seconds of game time from the latest Live Client read.
    pub game_time: Option<f64>,
}

impl View {
    /// Recomputes what depends on several sources: badges (history and the
    /// assigned position, which the Live Client may fill in later) and the
    /// team summaries.
    pub fn summarize_teams(&mut self) {
        for player in &mut self.players {
            player.badges = player
                .history
                .as_ref()
                .map(|h| badges(h, &player.champion_name, &player.position))
                .unwrap_or_default();
        }
        let side = |s: Side| {
            self.players
                .iter()
                .filter(|p| p.side == s)
                .collect::<Vec<_>>()
        };
        self.blue = team_summary(&side(Side::Blue));
        self.red = team_summary(&side(Side::Red));
    }
}

fn hidden(puuid: &str) -> bool {
    puuid.is_empty() || puuid.chars().all(|c| c == '0' || c == '-')
}

/// The players from `/lol-gameflow/v1/session`. `teamOne` is blue (100),
/// `teamTwo` red; the Live Client's `team` corrects that in [`merge_live`].
pub fn from_session(
    session: &Value,
    self_puuid: &str,
    champion_names: &BTreeMap<i64, String>,
) -> Option<View> {
    let data = &session["gameData"];
    let game_id = data["gameId"].as_u64().filter(|id| *id > 0)?;
    let selections = data["playerChampionSelections"].as_array();
    let mut players = Vec::new();
    for (team, side) in [("teamOne", Side::Blue), ("teamTwo", Side::Red)] {
        for (index, entry) in data[team].as_array().into_iter().flatten().enumerate() {
            let puuid = entry["puuid"].as_str().unwrap_or("").to_string();
            let key = if hidden(&puuid) {
                format!("{team}:{index}")
            } else {
                puuid.clone()
            };
            let mut player = Player::new(key, side);
            player.champion_id = entry["championId"].as_i64().unwrap_or(0);
            player.champion_name = champion_names
                .get(&player.champion_id)
                .cloned()
                .unwrap_or_default();
            let name = entry["gameName"]
                .as_str()
                .or_else(|| entry["riotIdGameName"].as_str())
                .filter(|s| !s.is_empty());
            let tag = entry["tagLine"]
                .as_str()
                .or_else(|| entry["riotIdTagLine"].as_str())
                .unwrap_or("");
            player.riot_id = match name {
                Some(name) => format!("{name}#{tag}"),
                None => entry["summonerName"].as_str().unwrap_or("").to_string(),
            };
            player.position = entry["selectedPosition"].as_str().unwrap_or("").to_string();
            player.profile_icon = entry["profileIconId"].as_i64().unwrap_or(0);
            player.is_self = !hidden(&puuid) && puuid == self_puuid;
            if let Some(selection) = selections.and_then(|all| {
                all.iter().find(|s| {
                    (!hidden(&puuid) && s["puuid"].as_str() == Some(&puuid))
                        || (s["summonerInternalName"]
                            .as_str()
                            .is_some_and(|n| !n.is_empty())
                            && s["summonerInternalName"] == entry["summonerInternalName"])
                })
            }) {
                player.spells = [
                    selection["spell1Id"].as_i64().unwrap_or(0),
                    selection["spell2Id"].as_i64().unwrap_or(0),
                ];
            }
            player.status = if hidden(&puuid) {
                Status::Skipped
            } else {
                Status::Pending
            };
            player.puuid = puuid;
            players.push(player);
        }
    }
    let queue = &data["queue"];
    let mut view = View {
        game_id,
        queue_id: queue["id"].as_i64().unwrap_or(0),
        queue: queue["description"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| queue["shortName"].as_str())
            .unwrap_or("")
            .to_string(),
        map_id: session["map"]["id"]
            .as_i64()
            .or_else(|| queue["mapId"].as_i64())
            .unwrap_or(0),
        players,
        blue: TeamSummary::default(),
        red: TeamSummary::default(),
        game_time: None,
    };
    view.summarize_teams();
    Some(view)
}

fn same_id(a: &str, b: &str) -> bool {
    !a.is_empty() && a.eq_ignore_ascii_case(b)
}

/// For each live player, the index of the session player it is. Riot ID
/// first (exact, case-insensitive), then champion name on the same side,
/// then champion name alone; every session player is used at most once.
pub fn match_live(players: &[Player], live: &[LivePlayer]) -> Vec<Option<usize>> {
    let mut taken = vec![false; players.len()];
    let mut result = vec![None; live.len()];
    type Rule = fn(&Player, &LivePlayer) -> bool;
    let passes: [Rule; 3] = [
        |p, l| same_id(&p.riot_id, &l.riot_id),
        |p, l| same_id(&p.champion_name, &l.champion_name) && Some(p.side) == l.side,
        |p, l| same_id(&p.champion_name, &l.champion_name),
    ];
    for pass in passes {
        for (li, l) in live.iter().enumerate() {
            if result[li].is_some() {
                continue;
            }
            // Ambiguous candidates (e.g. duplicate champions) never match.
            let candidates: Vec<usize> = players
                .iter()
                .enumerate()
                .filter(|(pi, p)| !taken[*pi] && pass(p, l))
                .map(|(pi, _)| pi)
                .collect();
            if let [only] = candidates[..] {
                taken[only] = true;
                result[li] = Some(only);
            }
        }
    }
    result
}

/// Merges a Live Client read into the view: live numbers, runes, sides and
/// any players the session did not list (custom games, bots).
pub fn merge_live(view: &mut View, data: &Value, champion_ids: &BTreeMap<String, i64>) {
    let live = live_players(data);
    if live.is_empty() {
        return;
    }
    view.game_time = data["gameData"]["gameTime"].as_f64();
    let matches = match_live(&view.players, &live);
    for (l, index) in live.into_iter().zip(matches) {
        let player = match index {
            Some(i) => &mut view.players[i],
            None => {
                let key = format!("live:{}", l.riot_id);
                // An unmatched entry already added by an earlier read (e.g. duplicate IDs) is not added again,
                // otherwise every poll would grow the team.
                if view.players.iter().any(|p| p.key == key) {
                    continue;
                }
                let mut player = Player::new(key, l.side.unwrap_or(Side::Blue));
                player.riot_id = l.riot_id.clone();
                player.champion_name = l.champion_name.clone();
                player.champion_id = champion_ids
                    .get(&l.champion_name.to_lowercase())
                    .copied()
                    .unwrap_or(0);
                player.status = Status::Skipped;
                view.players.push(player);
                view.players.last_mut().unwrap()
            }
        };
        if let Some(side) = l.side {
            player.side = side;
        }
        if player.riot_id.is_empty() {
            player.riot_id = l.riot_id;
        }
        if player.position.is_empty() {
            player.position = l.position;
        }
        if player.spells == [0, 0] {
            player.spells = l.spells;
        }
        player.is_bot = l.is_bot;
        player.keystone = l.keystone;
        player.primary_style = l.primary_style;
        player.secondary_style = l.secondary_style;
        player.live = Some(l.live);
    }
    view.players.sort_by_key(|p| p.side == Side::Red);
    view.summarize_teams();
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
