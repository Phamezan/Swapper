use super::*;
use serde_json::json;

fn player() -> Summoner {
    summoner(&json!({"puuid":"own", "summonerId":42, "gameName":"Test", "tagLine":"EUW", "summonerLevel":100})).unwrap()
}
fn fixture() -> Value {
    let mut participants = vec![];
    for id in 1..=10 {
        participants.push(json!({"participantId": id, "teamId": if id <= 5 {100} else {200}, "championId": 103,
            "stats": {"kills":4,"deaths":2,"assists":6,"win":true,"totalMinionsKilled":150,"neutralMinionsKilled":30,
                "totalDamageDealtToChampions":12000,"visionScore":24}}));
    }
    json!({"mapId":11,"queueId":420,"gameDuration":1200,"gameId":7,"gameCreation":123456,
        "participantIdentities":[{"participantId":1,"player":{"puuid":"own","summonerId":42}}], "participants":participants})
}

#[test]
fn identifies_signed_in_player_and_computes_team_metrics() {
    let g = game(&fixture(), &player()).unwrap();
    assert_eq!(
        g.metrics,
        [
            Some(5.0),
            Some(50.0),
            Some(9.0),
            Some(600.0),
            Some(20.0),
            Some(1.2)
        ]
    );
    let mut other = player();
    other.puuid = "other".into();
    other.summoner_id = 99;
    assert!(game(&fixture(), &other).is_none());
}

#[test]
fn a_conflicting_puuid_cannot_match_by_summoner_id() {
    let mut value = fixture();
    value["participantIdentities"][0]["player"]["puuid"] = json!("different-account");
    assert!(game(&value, &player()).is_none());
    value["participantIdentities"][0]["player"]["puuid"] = Value::Null;
    assert!(game(&value, &player()).is_some());
}

#[test]
fn incomplete_teams_and_zero_totals_do_not_invent_percentages() {
    let mut value = fixture();
    value["participants"].as_array_mut().unwrap().truncate(1);
    let g = game(&value, &player()).unwrap();
    assert_eq!(g.metrics[1], None);
    assert_eq!(g.metrics[4], None);
    value = fixture();
    for p in value["participants"].as_array_mut().unwrap() {
        p["stats"]["kills"] = json!(0);
        p["stats"]["totalDamageDealtToChampions"] = json!(0);
    }
    let g = game(&value, &player()).unwrap();
    assert_eq!(g.metrics[1], None);
    assert_eq!(g.metrics[4], None);
}

#[test]
fn filters_modes_remakes_and_malformed_games() {
    for queue in [400, 420, 430, 440, 480, 490] {
        let mut v = fixture();
        v["queueId"] = json!(queue);
        assert!(eligible(&v));
    }
    for queue in [450, 1700, 3140, 1750] {
        let mut v = fixture();
        v["queueId"] = json!(queue);
        assert!(!eligible(&v));
    }
    let mut v = fixture();
    v["mapId"] = json!(12);
    assert!(!eligible(&v));
    v = fixture();
    v["gameDuration"] = json!(0);
    assert!(game(&v, &player()).is_none());
    v = fixture();
    v["participants"][0]["stats"]["gameEndedInEarlySurrender"] = json!(true);
    assert!(game(&v, &player()).is_none());
    v = fixture();
    v["participants"][0]["stats"]["kills"] = Value::Null;
    assert!(game(&v, &player()).is_none());
}

#[test]
fn deathless_and_missing_stats_are_finite_or_unavailable() {
    let mut v = fixture();
    v["participants"][0]["stats"]["deaths"] = json!(0);
    v["participants"][0]["stats"]["visionScore"] = Value::Null;
    let g = game(&v, &player()).unwrap();
    assert_eq!(g.metrics[0], Some(10.0));
    assert_eq!(g.metrics[5], None);
}

#[test]
fn aggregates_queues_champions_and_recent_delta_without_empty_nan() {
    let mut games = vec![game(&fixture(), &player()).unwrap(); 6];
    games[0].metrics[0] = Some(11.0);
    games[0].queue = "flex".into();
    games[0].win = false;
    let summary = summaries(&games);
    assert_eq!(summary["all"].metrics[0].average, Some(6.0));
    assert!((summary["all"].metrics[0].delta.unwrap() - 0.2).abs() < 1e-10);
    assert_eq!(
        summary["all"].metrics[0].points,
        vec![5.0, 5.0, 5.0, 5.0, 5.0, 11.0]
    );
    assert_eq!(summary["solo"].champions[0].wins, 5);
    assert_eq!(summary["flex"].champions[0].losses, 1);
    assert_eq!(summary["normal"].metrics[0].average, None);
    assert!(summary["normal"].champions.is_empty());
}

#[test]
fn parses_solo_rank_and_summoner_variants() {
    assert!(summoner(&json!({})).is_none());
    assert_eq!(player().riot_id, "Test#EUW");
    let entry = json!({"queueType":"RANKED_SOLO_5x5","tier":"DIAMOND","division":"II","leaguePoints":35,"wins":30,"losses":17});
    assert_eq!(
        rank(&json!({"queues":[entry.clone()]})),
        rank(&json!({"queueMap":{"RANKED_SOLO_5x5":entry}}))
    );
    assert_eq!(rank(&json!({"queues":[]})), None);
    assert_eq!(
        rank(&json!({"queueMap":{"RANKED_SOLO_5x5":{"tier":"NONE"}}})),
        None
    );
}

#[test]
fn reads_build_and_estimates_role_from_smite_and_support_item() {
    let mut value = fixture();
    let own = &mut value["participants"][0];
    own["spell1Id"] = json!(4);
    own["spell2Id"] = json!(11);
    own["stats"]["perk0"] = json!(8010);
    own["stats"]["champLevel"] = json!(16);
    own["stats"]["item0"] = json!(3071);
    own["stats"]["item6"] = json!(3364);
    own["timeline"] = json!({"lane": "MIDDLE", "role": "SOLO"});
    let g = game(&value, &player()).unwrap();
    assert_eq!(g.items, vec![3071, 0, 0, 0, 0, 0, 3364]);
    assert_eq!(g.spells, vec![4, 11]);
    assert_eq!((g.keystone, g.level, g.cs), (8010, 16, 180));
    assert_eq!(g.role, "jungle");

    let lane = |lane: &str, role: &str, item: i64| {
        role_of(&json!({"spell1Id": 4, "spell2Id": 14, "stats": {"item0": item},
            "timeline": {"lane": lane, "role": role}}))
    };
    assert_eq!(lane("JUNGLE", "NONE", 0), "mid", "mislabelled mid laner without Smite");
    assert_eq!(lane("BOTTOM", "CARRY", 0), "adc");
    assert_eq!(lane("BOTTOM", "SUPPORT", 0), "support");
    assert_eq!(lane("JUNGLE", "NONE", 3877), "support", "support item wins over lane");
    assert_eq!(lane("TOP", "SOLO", 0), "top");
}

fn role_of(value: &Value) -> &'static str {
    role(value)
}

fn role_fixture() -> Value {
    let mut value = fixture();
    for (index, p) in value["participants"].as_array_mut().unwrap().iter_mut().enumerate() {
        let slot = index % 5;
        p["timeline"] = json!({"lane": (["TOP", "JUNGLE", "MIDDLE", "MIDDLE", "MIDDLE"][slot]), "role":"NONE"});
        p["spell1Id"] = json!(4);
        p["spell2Id"] = json!(if slot == 1 { 11 } else { 14 });
        p["stats"]["totalMinionsKilled"] = json!(([190, 20, 210, 230, 18][slot]));
        p["stats"]["neutralMinionsKilled"] = json!(if slot == 1 { 140 } else { 0 });
        p["stats"]["goldEarned"] = json!(([11000, 10000, 12000, 14000, 7500][slot]));
        p["stats"]["visionScore"] = json!(if slot == 4 { 65 } else { 15 });
        p["stats"]["item0"] = json!(if slot == 4 { 3870 } else { 0 });
    }
    value["participants"][3]["championId"] = json!(145);
    value["participants"][8]["championId"] = json!(236);
    value["participants"][9]["championId"] = json!(267);
    value["participantIdentities"].as_array_mut().unwrap().push(json!({"participantId":4,"player":{"puuid":"adc","summonerId":44}}));
    value
}

#[test]
fn kaisa_faces_lucian_instead_of_nami_with_mislabelled_bot_lanes() {
    let value = role_fixture();
    let mut adc = player();
    adc.puuid = "adc".into();
    let g = game(&value, &adc).unwrap();
    assert_eq!(g.role, "adc");
    assert_eq!(g.players[g.opponent.unwrap() as usize].champion_id, 236);
    assert_eq!(g.players[9].role, "support");
}

#[test]
fn support_upgrades_and_absent_lane_hints_still_assign_each_role_once() {
    let catalog: Value = serde_json::from_str(include_str!("profile-support-items.json")).unwrap();
    for item in catalog.as_array().unwrap() {
        let mut value = role_fixture();
        for p in value["participants"].as_array_mut().unwrap() {
            p["timeline"] = json!({"lane":"NONE","role":"NONE"});
        }
        value["participants"][4]["stats"]["item0"] = item["id"].clone();
        let roles = assigned_roles(&value);
        assert_eq!(roles[4], "support", "catalog item {}", item["id"]);
        assert_eq!(roles[1], "jungle");
        for team in roles.chunks(5) {
            let mut actual = team.to_vec();
            actual.sort_unstable();
            assert_eq!(actual, ["adc", "jungle", "mid", "support", "top"]);
        }
    }
    let mut value = role_fixture();
    value["participants"][4]["stats"]["item0"] = json!(0);
    value["participants"][4]["timeline"]["lane"] = json!("NONE");
    assert_eq!(assigned_roles(&value)[4], "support", "low-CS support without a quest item");
}

#[test]
fn lane_swaps_use_observed_lanes_and_non_rift_games_have_no_assignment() {
    let mut value = role_fixture();
    value["participants"][0]["timeline"]["lane"] = json!("BOTTOM");
    value["participants"][3]["timeline"]["lane"] = json!("TOP");
    let roles = assigned_roles(&value);
    assert_eq!((roles[0], roles[3]), ("adc", "top"));
    value["queueId"] = json!(450);
    value["mapId"] = json!(12);
    assert!(assigned_roles(&value).iter().all(|r| r.is_empty()));
    assert!(game(&value, &player()).is_none());
    value = role_fixture();
    value["gameDuration"] = json!(100);
    assert!(assigned_roles(&value).iter().all(|r| r.is_empty()));
}

#[test]
fn partial_history_keeps_only_explicit_role_hints_and_never_an_opponent() {
    let mut value = role_fixture();
    value["participants"].as_array_mut().unwrap().truncate(1);
    assert_eq!(game(&value, &player()).unwrap().role, "top");
    assert_eq!(game(&value, &player()).unwrap().opponent, None);
    value["participants"][0]["timeline"]["lane"] = json!("NONE");
    assert_eq!(game(&value, &player()).unwrap().role, "");
}

#[test]
fn placement_ranks_all_ten_and_needs_complete_stats() {
    let mut value = fixture();
    // Everyone is equal: all share first place.
    let g = game(&value, &player()).unwrap();
    assert_eq!(g.placement, 1);
    value["participants"][3]["stats"]["kills"] = json!(20);
    value["participants"][3]["stats"]["totalDamageDealtToChampions"] = json!(40000);
    assert_eq!(game(&value, &player()).unwrap().placement, 2);
    value["participants"][0]["stats"]["deaths"] = json!(12);
    value["participants"][0]["stats"]["totalDamageDealtToChampions"] = json!(1000);
    assert_eq!(game(&value, &player()).unwrap().placement, 10);
    value["participants"][7]["stats"]
        .as_object_mut()
        .unwrap()
        .remove("visionScore");
    assert_eq!(game(&value, &player()).unwrap().placement, 0);
}

#[test]
fn snapshots_cached_before_build_fields_still_load() {
    let old = json!({"id": 1, "queue": "solo", "championId": 103, "createdAt": 5, "duration": 1200,
        "win": true, "kills": 1, "deaths": 1, "assists": 1, "metrics": [null, null, null, null, null, null]});
    let g: Game = serde_json::from_value(old).unwrap();
    assert!(g.items.is_empty() && g.role.is_empty() && g.placement == 0);
}

#[test]
fn role_records_count_wins_per_estimated_role() {
    let mut games = vec![];
    for (role, win) in [("mid", true), ("mid", false), ("top", true)] {
        let mut g = game(&fixture(), &player()).unwrap();
        g.role = role.into();
        g.win = win;
        games.push(g);
    }
    let roles = summarize(&games).roles;
    assert_eq!(roles.len(), 5);
    assert_eq!(roles[2], RoleRecord { role: "mid".into(), wins: 1, games: 2 });
    assert_eq!(roles[0], RoleRecord { role: "top".into(), wins: 1, games: 1 });
    assert_eq!(roles[4].games, 0);
}

fn line(ally: bool, role: &str, me: bool) -> Player {
    Player {
        champion_id: 1,
        riot_id: String::new(),
        me,
        ally,
        role: role.into(),
        kills: 0,
        deaths: 0,
        assists: 0,
        cs: 0,
        damage: 0,
        gold: 0,
        vision: 0,
        items: vec![],
    }
}

fn lobby(own: &str, enemies: [&str; 5]) -> Vec<Player> {
    let mut players = vec![line(true, own, true)];
    players.extend(["top", "jungle", "adc", "support", "mid"].iter().filter(|r| **r != own).take(4).map(|r| line(true, r, false)));
    players.extend(enemies.iter().map(|r| line(false, r, false)));
    players
}

#[test]
fn lane_opponent_needs_exactly_one_player_in_the_role_on_each_side() {
    let roles = ["top", "jungle", "mid", "adc", "support"];
    assert_eq!(lane_opponent(&lobby("mid", roles)), Some(7));
    assert_eq!(lane_opponent(&lobby("support", roles)), Some(9));
    // Two enemy mids (a lane swap or an odd pick) and no enemy mid at all.
    assert_eq!(lane_opponent(&lobby("mid", ["top", "mid", "mid", "adc", "support"])), None);
    assert_eq!(lane_opponent(&lobby("mid", ["top", "jungle", "top", "adc", "support"])), None);
    // A teammate in the same role makes the user's own role uncertain.
    let mut players = lobby("mid", roles);
    players[1].role = "mid".into();
    assert_eq!(lane_opponent(&players), None);
    // Remakes or missing participants.
    assert_eq!(lane_opponent(&lobby("mid", roles)[..9]), None);
}

#[test]
fn players_list_own_team_first_with_riot_ids() {
    let mut value = fixture();
    value["participantIdentities"][0]["player"]["gameName"] = json!("Own");
    value["participantIdentities"][0]["player"]["tagLine"] = json!("EUW");
    value["participants"][0]["teamId"] = json!(200);
    let g = game(&value, &player()).unwrap();
    assert_eq!(g.players.len(), 10);
    assert_eq!(g.players[0].riot_id, "Own#EUW");
    assert!(g.players[0].me && g.players.iter().take(6).all(|p| p.ally));
    assert_eq!(g.players[0].cs, 180);
    // Every fixture player reads as mid, so the opponent is ambiguous.
    assert_eq!(g.opponent, None);
}
