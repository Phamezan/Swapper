//! Seeds an account's LP history so the LP graph has a line before Swapper has
//! recorded much on its own. op.gg gives exact daily rank snapshots; u.gg gives
//! each ranked game's LP change, which places the games between two snapshots.
//! Best-effort: once per account per run, short timeouts, and any change on
//! either site just means fewer points. op.gg's JSON API refuses direct
//! requests, so its points are read from the `lpHistories` list its profile
//! page embeds. Only the account's own Riot ID and region are sent.

use super::lp::{self, rank_at, score, Point};
use super::model::Rank;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(6);
const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
/// op.gg serves its regular page to a browser user agent.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
const APEX: [&str; 3] = ["MASTER", "GRANDMASTER", "CHALLENGER"];
/// u.gg serves 20 games a page; this bounds a season of games.
const MAX_UGG_PAGES: u32 = 10;
const UGG_QUERY: &str = "\
query FetchMatchSummaries($page: Int, $queueType: [Int], $regionId: String!, $riotUserName: String!, $riotTagLine: String!, $seasonIds: [Int]!) {
  fetchPlayerMatchSummaries(page: $page, queueType: $queueType, regionId: $regionId, riotUserName: $riotUserName, riotTagLine: $riotTagLine, seasonIds: $seasonIds) {
    finishedMatchSummaries
    matchSummaries { matchCreationTime lpInfo { lp placement } }
  }
}";

static TRIED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// The League Client's region code (`/riotclient/region-locale`) as op.gg's URL region.
fn region(client_region: &str) -> Option<&'static str> {
    Some(match client_region.trim().to_ascii_uppercase().as_str() {
        "EUW" | "EUW1" => "euw",
        "NA" | "NA1" => "na",
        "EUNE" | "EUN1" => "eune",
        "KR" => "kr",
        "BR" | "BR1" => "br",
        "LAN" | "LA1" => "lan",
        "LAS" | "LA2" => "las",
        "OCE" | "OC1" => "oce",
        "TR" | "TR1" => "tr",
        "RU" => "ru",
        "JP" | "JP1" => "jp",
        "PH" | "PH2" => "ph",
        "SG" | "SG2" => "sg",
        "TH" | "TH2" => "th",
        "TW" | "TW2" => "tw",
        "VN" | "VN2" => "vn",
        "ME" | "ME1" => "me",
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Snapshot {
    created_at: String,
    tier_info: TierInfo,
}

#[derive(Deserialize)]
struct TierInfo {
    tier: Option<String>,
    lp: Option<u64>,
    /// "D 4" for Diamond IV.
    label: Option<String>,
}

/// Unix milliseconds for an ISO time like `2026-10-14T10:50:04+09:00`.
fn parse_time(iso: &str) -> Option<u64> {
    let (date, rest) = iso.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let clock = rest.get(..8)?;
    let mut t = clock.split(':').map(|p| p.parse::<i64>().ok());
    let (hh, mm, ss) = (t.next()??, t.next()??, t.next()??);
    let zone = &rest[8..];
    let offset = match zone.chars().next() {
        Some('Z') | None => 0,
        Some(sign @ ('+' | '-')) => {
            let (zh, zm) = zone[1..].split_once(':')?;
            let minutes = zh.parse::<i64>().ok()? * 60 + zm.parse::<i64>().ok()?;
            if sign == '+' { minutes } else { -minutes }
        }
        _ => return None,
    };
    // Days since 1970-01-01 (Howard Hinnant's days_from_civil).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let seconds = days * 86_400 + hh * 3600 + mm * 60 + ss - offset * 60;
    u64::try_from(seconds).ok().map(|s| s * 1000)
}

fn division(label: &str) -> Option<&'static str> {
    Some(match label.trim().rsplit(' ').next()? {
        "1" => "I",
        "2" => "II",
        "3" => "III",
        "4" => "IV",
        _ => return None,
    })
}

/// One ranked game from u.gg: when it started, and its LP change when u.gg knows it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct GameLp {
    at: u64,
    lp: Option<i64>,
}

/// Points for the games between op.gg snapshots, from exact values only: walked
/// forward from a snapshot through known LP changes, and back from the next
/// snapshot (the score after the last game is that snapshot). A game without a
/// known change ends a walk, so no point is guessed.
fn fill(anchors: &[Point], games: &[GameLp]) -> Vec<Point> {
    let mut games = games.to_vec();
    games.sort_by_key(|g| g.at);
    let mut points = Vec::new();
    for (i, anchor) in anchors.iter().enumerate() {
        let next = anchors.get(i + 1);
        let day: Vec<&GameLp> = games.iter().filter(|g| g.at > anchor.at && g.at < next.map_or(u64::MAX, |n| n.at)).collect();
        let point = |at, walked| Point { at, rank: rank_at(walked, &anchor.rank.tier), source: Some("ugg".into()) };
        let mut walked = score(&anchor.rank);
        let mut forward = 0;
        for game in &day {
            let Some(change) = game.lp else { break };
            walked += change;
            points.push(point(game.at, walked));
            forward += 1;
        }
        let Some(next) = next else { continue };
        let mut walked = score(&next.rank);
        for game in day[forward..].iter().rev() {
            points.push(point(game.at, walked));
            let Some(change) = game.lp else { break };
            walked -= change;
        }
    }
    points.sort_by_key(|p| p.at);
    points
}

/// u.gg's games from one page, and whether it was the last page.
fn parse_games(body: &str) -> Option<(Vec<GameLp>, bool)> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let summaries = value.pointer("/data/fetchPlayerMatchSummaries")?;
    let games = summaries["matchSummaries"]
        .as_array()?
        .iter()
        .filter_map(|m| {
            let at = m["matchCreationTime"].as_u64()?;
            // u.gg writes 0 (or null) when it did not see the change, and placements have none.
            let lp = m["lpInfo"]["lp"].as_i64().filter(|lp| *lp != 0 && m["lpInfo"]["placement"].as_i64().unwrap_or(-1) == -1);
            Some(GameLp { at, lp })
        })
        .collect();
    Some((games, summaries["finishedMatchSummaries"].as_bool().unwrap_or(true)))
}

/// Solo/Duo games this season back to `since`, newest pages first.
async fn ugg_games(name: &str, tag: &str, region_id: &str, since: u64) -> Vec<GameLp> {
    let Ok(client) = reqwest::Client::builder().connect_timeout(REQUEST_TIMEOUT).timeout(REQUEST_TIMEOUT).build() else {
        return vec![];
    };
    let mut games = Vec::new();
    for page in 1..=MAX_UGG_PAGES {
        let body = serde_json::json!({ "query": UGG_QUERY, "variables": {
            "page": page, "queueType": [420], "regionId": region_id, "riotUserName": name, "riotTagLine": tag,
            "seasonIds": [crate::runes::ugg_record::SEASON_ID] } });
        let Ok(response) = client.post(crate::runes::probuilds::ENDPOINT).json(&body).send().await else { break };
        let Some((batch, last)) = response.text().await.ok().as_deref().and_then(parse_games) else { break };
        let reached = batch.iter().any(|g| g.at < since);
        games.extend(batch);
        if last || reached {
            break;
        }
    }
    games
}

/// The `lpHistories` points from an op.gg profile page, oldest first. The list
/// sits JSON-escaped inside the page's script payload.
fn parse(page: &str) -> Vec<Point> {
    const KEY: &str = "lpHistories\\\":";
    let Some(start) = page.find(&format!("{KEY}[")).map(|i| i + KEY.len()) else {
        return vec![];
    };
    let mut depth = 0;
    let Some(end) = page[start..].char_indices().find_map(|(i, c)| {
        depth += match c { '[' => 1, ']' => -1, _ => 0 };
        (depth == 0).then_some(start + i + 1)
    }) else {
        return vec![];
    };
    let json = page[start..end].replace("\\\"", "\"");
    let Ok(snapshots) = serde_json::from_str::<Vec<Snapshot>>(&json) else {
        return vec![];
    };
    let mut points: Vec<Point> = snapshots
        .into_iter()
        .filter_map(|s| {
            let tier = s.tier_info.tier?.to_ascii_uppercase();
            let apex = APEX.contains(&tier.as_str());
            let division = if apex { "" } else { division(s.tier_info.label.as_deref()?)? };
            Some(Point {
                at: parse_time(&s.created_at)?,
                rank: Rank { tier, division: division.into(), lp: s.tier_info.lp?, wins: 0, losses: 0 },
                source: Some("opgg".into()),
            })
        })
        .collect();
    points.sort_by_key(|p| p.at);
    points
}

async fn fetch(region: &str, name: &str, tag: &str) -> Option<String> {
    let mut url = url::Url::parse("https://op.gg/lol/summoners").ok()?;
    url.path_segments_mut().ok()?.push(region).push(&format!("{name}-{tag}"));
    let client = reqwest::Client::builder()
        .connect_timeout(REQUEST_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .ok()?;
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    (bytes.len() <= MAX_PAGE_BYTES).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

/// Once per account per run, until u.gg's games have been placed, fetches
/// op.gg's snapshots and the games between them and stores those older than
/// Swapper's first own point. The graph shows them on the next dashboard refresh.
pub fn spawn(puuid: String, riot_id: String, points: &[Point]) {
    if lp::seeded(points, "ugg") {
        return;
    }
    {
        let mut tried = TRIED.lock().unwrap_or_else(|e| e.into_inner());
        if !tried.get_or_insert_with(HashSet::new).insert(puuid.clone()) {
            return;
        }
    }
    tauri::async_runtime::spawn(async move {
        let Some((name, tag)) = riot_id.split_once('#') else { return };
        let Ok(client) = crate::runes::lcu().await else { return };
        let Ok(locale) = crate::runes::lcu_get::<serde_json::Value>(&client, "/riotclient/region-locale").await else { return };
        let client_region = locale["region"].as_str().unwrap_or_default();
        let Some(page) = (match region(client_region) { Some(region) => fetch(region, name, tag).await, None => None }) else { return };
        let mut seeds = parse(&page);
        if let (Some(first), Some(region_id)) = (seeds.first(), crate::runes::ugg_record::region_id(client_region)) {
            let games = ugg_games(name, tag, region_id, first.at).await;
            let placed = fill(&seeds, &games);
            seeds.extend(placed);
        }
        if !seeds.is_empty() {
            let _ = tokio::task::spawn_blocking(move || lp::seed(&puuid, seeds)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shaped like op.gg's page payload (escaped JSON inside a script string); invented values.
    const PAGE: &str = r#"<script>self.__next_f.push([1,"[\"$\",\"$L69\",null,{\"lpHistories\":[{\"created_at\":\"2026-10-14T10:50:04+09:00\",\"tier_info\":{\"lp\":35,\"tier\":\"DIAMOND\",\"label\":\"D 4\"},\"elo_point\":2288},{\"created_at\":\"2026-10-12T00:00:00Z\",\"tier_info\":{\"lp\":80,\"tier\":\"EMERALD\",\"label\":\"E 1\"}},{\"created_at\":\"2026-10-20T08:00:00+00:00\",\"tier_info\":{\"lp\":250,\"tier\":\"MASTER\",\"label\":\"M\"}},{\"created_at\":\"bad\",\"tier_info\":{\"lp\":1,\"tier\":\"GOLD\",\"label\":\"G 2\"}}],\"other\":[1]}]"])</script>"#;

    #[test]
    fn reads_daily_snapshots_oldest_first_and_skips_unreadable_ones() {
        let points = parse(PAGE);
        let ranks: Vec<_> = points.iter().map(|p| (p.rank.tier.as_str(), p.rank.division.as_str(), p.rank.lp)).collect();
        assert_eq!(ranks, [("EMERALD", "I", 80), ("DIAMOND", "IV", 35), ("MASTER", "", 250)]);
        assert!(points.iter().all(|p| p.source.as_deref() == Some("opgg") && p.rank.wins == 0));
        assert_eq!(points[0].at, 1_791_763_200_000);
        assert!(parse("<html>no list here</html>").is_empty());
        assert!(parse(r#"lpHistories\":[{broken"#).is_empty());
    }

    #[test]
    fn iso_times_respect_their_offset() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2026-10-14T10:50:04+09:00"), parse_time("2026-10-14T01:50:04Z"));
        assert_eq!(parse_time("2024-02-29T12:00:00-05:00"), Some(1_709_226_000_000));
        assert_eq!(parse_time("nonsense"), None);
    }

    fn anchor(at: u64, tier: &str, division: &str, lp: u64) -> Point {
        Point { at, rank: Rank { tier: tier.into(), division: division.into(), lp, wins: 0, losses: 0 }, source: Some("opgg".into()) }
    }

    #[test]
    fn games_are_walked_from_each_snapshot_and_stop_at_an_unknown_change() {
        let anchors = [anchor(100, "EMERALD", "I", 80), anchor(500, "DIAMOND", "IV", 30)];
        let game = |at, lp| GameLp { at, lp };
        let games = [game(150, Some(25)), game(200, Some(-10)), game(250, None), game(300, None), game(350, Some(20)), game(400, Some(10)), game(600, Some(-15)), game(50, Some(99))];
        let points = fill(&anchors, &games);
        let placed: Vec<_> = points.iter().map(|p| (p.at, p.rank.tier.as_str(), p.rank.division.as_str(), p.rank.lp)).collect();
        // Forward: Emerald I 80 +25 promotes to Diamond IV 5, -10 demotes to Emerald I 95, then an unknown change.
        // Back from Diamond IV 30: after 400 it is 30, after 350 it is 20, after 300 it is 0; 300's own change is unknown.
        assert_eq!(placed, [(150, "DIAMOND", "IV", 5), (200, "EMERALD", "I", 95), (300, "DIAMOND", "IV", 0), (350, "DIAMOND", "IV", 20), (400, "DIAMOND", "IV", 30), (600, "DIAMOND", "IV", 15)]);
        assert!(points.iter().all(|p| p.source.as_deref() == Some("ugg")));
        assert_eq!(rank_at(score(&anchor(0, "GRANDMASTER", "", 300).rank) + 50, "GRANDMASTER").lp, 350);
    }

    #[test]
    fn reads_ugg_games_and_treats_zero_or_placement_lp_as_unknown() {
        let body = r#"{"data":{"fetchPlayerMatchSummaries":{"finishedMatchSummaries":false,"matchSummaries":[
            {"matchCreationTime":3,"lpInfo":{"lp":27,"placement":-1}},
            {"matchCreationTime":2,"lpInfo":{"lp":0,"placement":-1}},
            {"matchCreationTime":1,"lpInfo":{"lp":null,"placement":-1}},
            {"matchCreationTime":0,"lpInfo":{"lp":15,"placement":3}}]}}}"#;
        let (games, last) = parse_games(body).unwrap();
        assert!(!last);
        assert_eq!(games.iter().map(|g| g.lp).collect::<Vec<_>>(), [Some(27), None, None, None]);
        assert!(parse_games(r#"{"errors":[{"message":"x"}]}"#).is_none());
    }

    #[test]
    fn maps_client_regions_to_opgg() {
        assert_eq!(region("EUW"), Some("euw"));
        assert_eq!(region("la2"), Some("las"));
        assert_eq!(region("PBE"), None);
    }
}
