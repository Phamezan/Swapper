use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayData {
    pub gold: Option<Gold>,
    pub objectives: Vec<Objective>,
    pub skill: Option<char>,
    /// The game's own level-up arrow for `skill` (client x, y, width, height).
    pub arrow: Option<[i32; 4]>,
    /// Animation step of the diamonds above the arrow (0..PULSE_STEPS).
    pub pulse: u8,
}

/// Held-item value per team, rounded to whole gold so redraws only happen on change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gold {
    pub ours: i64,
    pub theirs: i64,
    pub lane: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Objective {
    pub label: &'static str,
    /// Seconds until spawn; 0 means it is up.
    pub seconds: u64,
    /// Length of the current wait, for the countdown ring.
    pub period: u64,
}

pub fn active_player(data: &Value) -> Option<&Value> {
    let active = &data["activePlayer"];
    let key = if active["riotId"].as_str().is_some_and(|id| !id.is_empty()) {
        "riotId"
    } else {
        "summonerName"
    };
    let id = active[key].as_str().filter(|id| !id.is_empty())?;
    let mut matches = data["allPlayers"]
        .as_array()?
        .iter()
        .filter(|player| player[key].as_str() == Some(id));
    let player = matches.next()?;
    matches.next().is_none().then_some(player)
}

pub fn gold(data: &Value) -> Option<Gold> {
    let players = data["allPlayers"].as_array()?;
    let active = active_player(data)?;
    let team = active["team"].as_str()?;
    let enemy_team = match team {
        "ORDER" => "CHAOS",
        "CHAOS" => "ORDER",
        _ => return None,
    };
    let total = |team: &str| {
        players
            .iter()
            .filter(|p| p["team"] == team)
            .map(inventory)
            .sum::<f64>()
    };
    let ours = total(team);
    let theirs = total(enemy_team);
    let mut gold = Gold {
        ours: ours.round() as i64,
        theirs: theirs.round() as i64,
        lane: None,
    };
    if let Some(lane) = active["position"]
        .as_str()
        .and_then(crate::runes::lolalytics::lane)
    {
        let mut opponents = players.iter().filter(|p| {
            p["team"] == enemy_team
                && p["position"]
                    .as_str()
                    .and_then(crate::runes::lolalytics::lane)
                    == Some(lane)
        });
        if let Some(opponent) = opponents.next().filter(|_| opponents.next().is_none()) {
            gold.lane = Some((inventory(active) - inventory(opponent)).round() as i64);
        }
    }
    Some(gold)
}

// Current held-item purchase value, not earned gold: cash, sold/consumed items,
// free upgrades and discounted purchases cannot be recovered from this API.
fn inventory(player: &Value) -> f64 {
    player["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let price = item["price"].as_f64()?;
            let count = item["count"].as_u64()?;
            (price.is_finite() && (0.0..=100_000.0).contains(&price) && count <= 100)
                .then_some(price * count as f64)
        })
        .sum()
}

// Standard Summoner's Rift, checked against patch 26.1 (2026 season).
// https://www.leagueoflegends.com/en-us/news/game-updates/patch-26-1-notes/
// Elemental first spawn / respawn; Baron first spawn / respawn; first Elder
// after soul uses the dragon's 5 min spawn, subsequent Elders use 6 min.
const TIMERS: [(f64, f64); 3] = [(300.0, 300.0), (1200.0, 360.0), (300.0, 360.0)];

pub fn objectives(data: &Value) -> Vec<Objective> {
    let Some(now) = data["gameData"]["gameTime"]
        .as_f64()
        .filter(|t| t.is_finite() && *t >= 0.0)
    else {
        return Vec::new();
    };
    let events: Vec<_> = data["events"]["Events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| {
            e["EventTime"]
                .as_f64()
                .is_some_and(|t| t.is_finite() && t >= 0.0 && t <= now)
        })
        .collect();
    let latest = |name: &str| {
        events
            .iter()
            .filter(|e| e["EventName"] == name)
            .max_by(|a, b| {
                a["EventTime"]
                    .as_f64()
                    .unwrap()
                    .total_cmp(&b["EventTime"].as_f64().unwrap())
            })
    };
    let baron = latest("BaronKill").map_or((0.0, TIMERS[1].0), |e| {
        (e["EventTime"].as_f64().unwrap(), TIMERS[1].1)
    });
    let mut kills = [0; 2];
    let mut soul = None;
    let mut dragons: Vec<_> = events
        .iter()
        .filter(|e| e["EventName"] == "DragonKill")
        .collect();
    dragons.sort_by(|a, b| {
        a["EventTime"]
            .as_f64()
            .unwrap()
            .total_cmp(&b["EventTime"].as_f64().unwrap())
    });
    let mut last_dragon = None;
    let mut last_elder = None;
    for event in dragons {
        let time = event["EventTime"].as_f64().unwrap();
        if event["DragonType"] == "Elder" {
            last_elder = Some(time);
            continue;
        }
        last_dragon = Some(time);
        let killer = event["KillerName"].as_str().unwrap_or("");
        let team = data["allPlayers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| {
                !killer.is_empty()
                    && ["summonerName", "riotId"]
                        .iter()
                        .any(|key| p[*key].as_str() == Some(killer))
            })
            .and_then(|p| p["team"].as_str());
        if let Some(index) = match team {
            Some("ORDER") => Some(0),
            Some("CHAOS") => Some(1),
            _ => None,
        } {
            kills[index] += 1;
            if kills[index] == 4 {
                soul = Some(time);
            }
        }
    }
    // (label, wait started at, wait length)
    let dragon = if let Some(time) = last_elder {
        ("Elder", time, TIMERS[2].1)
    } else if let Some(time) = soul {
        ("Elder", time, TIMERS[2].0)
    } else {
        last_dragon.map_or(("Dragon", 0.0, TIMERS[0].0), |time| {
            ("Dragon", time, TIMERS[0].1)
        })
    };
    [dragon, ("Baron", baron.0, baron.1)]
        .into_iter()
        .map(|(label, start, period)| Objective {
            label,
            seconds: (start + period - now).max(0.0).ceil() as u64,
            period: period as u64,
        })
        .collect()
}

/// Which of Q, W, E, R the game shows a level-up arrow for: a point to spend,
/// basic abilities up to rank 5 and half the level (rounded up), R at 6/11/16.
pub fn levelable(level: u64, ranks: [u64; 4]) -> [bool; 4] {
    if ranks.iter().sum::<u64>() >= level {
        return [false; 4];
    }
    let ultimate = [6, 11, 16].into_iter().filter(|gate| level >= *gate).count() as u64;
    [0, 1, 2, 3].map(|i| if i == 3 { ranks[3] < ultimate } else { ranks[i] < 5 && ranks[i] < level.div_ceil(2) })
}

pub fn next_skill(level: u64, ranks: [u64; 4], priority: &str) -> Option<char> {
    let priority: Vec<_> = priority.to_ascii_uppercase().chars().collect();
    if !(1..=18).contains(&level)
        || priority.len() != 3
        || !['Q', 'W', 'E'].iter().all(|key| priority.contains(key))
        || ranks[..3].iter().any(|rank| *rank > 5)
        || ranks[3] > 3
        || ranks.iter().sum::<u64>() >= level
    {
        return None;
    }
    let ultimate = [6, 11, 16]
        .into_iter()
        .filter(|gate| level >= *gate)
        .count() as u64;
    if ranks[3] < ultimate {
        return Some('R');
    }
    let index = |key| match key {
        'Q' => 0,
        'W' => 1,
        _ => 2,
    };
    // Learn each basic ability before maxing, then follow the recommended order.
    if let Some(key) = priority.iter().find(|key| ranks[index(**key)] == 0) {
        return Some(*key);
    }
    let cap = level.div_ceil(2).min(5);
    priority.into_iter().find(|key| ranks[index(*key)] < cap)
}

pub fn supported(data: &Value) -> bool {
    data["gameData"]["mapNumber"] == 11
        && matches!(data["gameData"]["gameMode"].as_str(), Some("CLASSIC" | "PRACTICETOOL"))
        && data["activePlayer"]["level"]
            .as_u64()
            .is_some_and(|level| level > 0)
        && active_player(data).is_some()
        && !data["events"]["Events"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|e| e["EventName"] == "GameEnd")
}

pub fn cached_priority(cache: &Value, champion: &str, lane: &str, tier: &str) -> Option<String> {
    let champion = crate::runes::lolalytics::champion_slug(champion)?;
    let lane = crate::runes::lolalytics::lane(lane)?;
    let prefix = format!(
        "{champion}|{lane}|{}|",
        crate::runes::lolalytics::tier_slug(tier)
    );
    cache["entries"]
        .as_array()?
        .iter()
        .filter(|e| {
            e["key"]
                .as_str()
                .is_some_and(|key| key.starts_with(&prefix))
        })
        .max_by_key(|e| e["fetched_at"].as_i64().unwrap_or(0))?["value"]["pick"]["skill_priority"]
        .as_str()
        .map(str::to_owned)
}

/// lolalytics `build-full`'s most picked skill priority ("QWE").
pub fn fetched_priority(root: &Value) -> Option<String> {
    let id = root["summary"]["pick"]["skillpriority"]["id"].as_str()?.to_ascii_uppercase();
    let mut keys: Vec<char> = id.chars().collect();
    keys.sort_unstable();
    (keys == ['E', 'Q', 'W']).then_some(id)
}

/// Summoner's Rift normal, ranked and quickplay games, plus practice tool so the
/// overlay can be tried without a real game.
pub fn supported_session(session: &Value) -> bool {
    let practice = session["gameData"]["queue"]["gameMode"] == "PRACTICETOOL"
        || session["map"]["gameMode"] == "PRACTICETOOL";
    session["map"]["id"] == 11
        && (practice
            || matches!(
                session["gameData"]["queue"]["id"].as_i64(),
                Some(400 | 420 | 430 | 440 | 490)
            ))
        && session["gameClient"]["isSpectating"] != true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub fn fixture() -> Value {
        serde_json::from_str(include_str!("fixtures/allgamedata.json")).unwrap()
    }

    #[test]
    fn full_riot_id_takes_precedence_over_a_duplicate_legacy_name() {
        let mut data = fixture();
        data["allPlayers"][1]["summonerName"] = json!("Player");
        data["allPlayers"].as_array_mut().unwrap().swap(0, 1);
        assert_eq!(active_player(&data).unwrap()["team"], "ORDER");
        data["activePlayer"]["riotId"] = Value::Null;
        assert!(
            active_player(&data).is_none(),
            "ambiguous names must not select the wrong team"
        );
    }

    #[test]
    fn malformed_items_and_future_objective_events_do_not_affect_estimates() {
        assert_eq!(
            inventory(
                &json!({"items":[{"price":-1,"count":1},{"price":10,"count":-2},{"price":10},{"price":999999999,"count":1}]})
            ),
            0.0
        );
        assert_eq!(inventory(&json!({})), 0.0);
        let mut data = fixture();
        data["events"]["Events"].as_array_mut().unwrap().extend([
            json!({"EventName":"BaronKill","EventTime":99999}),
            json!({"EventName":"DragonKill","EventTime":-1}),
            json!({"EventName":"DragonKill","EventTime":"bad"}),
        ]);
        assert_eq!(objectives(&data), objectives(&fixture()));
        assert!(objectives(&json!({})).is_empty());
    }

    #[test]
    fn inventory_estimate_counts_stacks_and_matches_opponent_by_role() {
        assert_eq!(
            gold(&fixture()),
            Some(Gold {
                ours: 1100,
                theirs: 700,
                lane: Some(400)
            })
        );
        let mut data = fixture();
        data["allPlayers"][0]["position"] = json!("");
        assert_eq!(gold(&data).unwrap().lane, None);
        data["activePlayer"]["summonerName"] = json!("missing");
        data["activePlayer"]["riotId"] = json!("missing#TEST");
        assert_eq!(gold(&data), None);
    }

    #[test]
    fn objectives_use_latest_event_and_game_clock() {
        let timers = |data: &Value| {
            objectives(data)
                .into_iter()
                .map(|o| (o.label, o.seconds, o.period))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            timers(&fixture()),
            [("Dragon", 200, 300), ("Baron", 600, 1200)]
        );
        let mut data = fixture();
        data["events"]["Events"]
            .as_array_mut()
            .unwrap()
            .push(json!({"EventName":"BaronKill","EventTime":550.0}));
        assert_eq!(timers(&data)[1], ("Baron", 310, 360));
        data["gameData"]["gameTime"] = json!(1300.0);
        assert!(objectives(&data).iter().all(|o| o.seconds == 0));
    }

    #[test]
    fn soul_transitions_to_elder_and_elder_respawns_in_six_minutes() {
        let mut data = fixture();
        data["events"]["Events"] = json!((0..4).map(|i| json!({"EventID":i,"EventName":"DragonKill","EventTime":100.0+i as f64*100.0,"KillerName":"Player","DragonType":"Fire"})).collect::<Vec<_>>());
        assert_eq!(
            (objectives(&data)[0].label, objectives(&data)[0].seconds),
            ("Elder", 100)
        );
        data["events"]["Events"].as_array_mut().unwrap().push(json!({"EventName":"DragonKill","EventTime":590.0,"DragonType":"Elder","KillerName":"Enemy"}));
        assert_eq!(
            (objectives(&data)[0].label, objectives(&data)[0].seconds),
            ("Elder", 350)
        );
    }

    #[test]
    fn recommendation_respects_unspent_points_rank_caps_and_ultimate_gates() {
        assert_eq!(next_skill(6, [3, 1, 1, 0], "QWE"), Some('R'));
        assert_eq!(next_skill(11, [5, 3, 1, 1], "QWE"), Some('R'));
        assert_eq!(next_skill(16, [5, 5, 3, 2], "QWE"), Some('R'));
        assert_eq!(next_skill(4, [2, 1, 0, 0], "QWE"), Some('E'));
        assert_eq!(next_skill(5, [2, 1, 1, 0], "QWE"), Some('Q'));
        assert_eq!(next_skill(5, [3, 1, 1, 0], "QWE"), None);
        assert_eq!(next_skill(2, [1, 0, 0, 0], "QQE"), None);
        assert_eq!(next_skill(0, [0; 4], "QWE"), None);
        assert_eq!(next_skill(19, [0; 4], "QWE"), None);
        assert_eq!(next_skill(6, [6, 0, 0, 0], "QWE"), None);
    }

    #[test]
    fn only_normal_ranked_sr_with_an_active_player_is_supported() {
        let mut data = fixture();
        assert!(supported(&data));
        data["gameData"]["mapNumber"] = json!(12);
        assert!(!supported(&data));
        data["gameData"]["mapNumber"] = json!(11);
        data["events"]["Events"] = json!([{"EventName":"GameEnd","EventTime":600}]);
        assert!(!supported(&data));
        assert!(!supported(&json!({})));
        for queue in [400, 420, 430, 440, 490] {
            assert!(supported_session(
                &json!({"map":{"id":11},"gameData":{"queue":{"id":queue}},"gameClient":{"isSpectating":false}})
            ));
        }
        for queue in [0, 450, 480, 1700] {
            assert!(!supported_session(
                &json!({"map":{"id":11},"gameData":{"queue":{"id":queue}}})
            ));
        }
        assert!(!supported_session(
            &json!({"map":{"id":11},"gameData":{"queue":{"id":420}},"gameClient":{"isSpectating":true}})
        ));
        // Practice tool is a custom game (queue id -1) in its own game mode.
        let practice = json!({"map":{"id":11,"gameMode":"PRACTICETOOL"},"gameData":{"queue":{"id":-1,"gameMode":"PRACTICETOOL"}}});
        assert!(supported_session(&practice));
        data["events"]["Events"] = json!([]);
        data["gameData"]["gameMode"] = json!("PRACTICETOOL");
        assert!(supported(&data));
        data["gameData"]["gameMode"] = json!("ARAM");
        assert!(!supported(&data));
    }

    #[test]
    fn level_up_arrows_follow_the_game_rules() {
        assert_eq!(levelable(1, [0, 0, 0, 0]), [true, true, true, false]);
        // Level 3 with Q at 2: Q is capped at half the level (2), W and E stay open.
        assert_eq!(levelable(3, [2, 0, 0, 0]), [false, true, true, false]);
        assert_eq!(levelable(6, [3, 1, 1, 0]), [false, true, true, true]);
        // No point to spend, no arrows.
        assert_eq!(levelable(2, [1, 1, 0, 0]), [false; 4]);
    }

    #[test]
    fn fetched_priority_accepts_only_a_full_qwe_order() {
        let root = |id: &str| json!({"summary":{"pick":{"skillpriority":{"id":id}}}});
        assert_eq!(fetched_priority(&root("QWE")).as_deref(), Some("QWE"));
        assert_eq!(fetched_priority(&root("ewq")).as_deref(), Some("EWQ"));
        assert_eq!(fetched_priority(&root("QQE")), None);
        assert_eq!(fetched_priority(&json!({})), None);
    }

    #[test]
    fn cached_recommendation_matches_champion_role_and_bracket() {
        let cache = json!({"entries":[
            {"key":"ahri|middle|emerald_plus|8112","fetched_at":1,"value":{"pick":{"skill_priority":"QWE"}}},
            {"key":"ahri|middle|emerald_plus|8229","fetched_at":2,"value":{"pick":{"skill_priority":"QEW"}}},
            {"key":"ahri|top|emerald_plus|8112","fetched_at":3,"value":{"pick":{"skill_priority":"WQE"}}}
        ]});
        assert_eq!(
            cached_priority(&cache, "Ahri", "MIDDLE", "emerald_plus").as_deref(),
            Some("QEW")
        );
        assert_eq!(cached_priority(&cache, "Ahri", "MIDDLE", "all"), None);
        assert_eq!(cached_priority(&cache, "Ahri", "", "emerald_plus"), None);
    }
}
