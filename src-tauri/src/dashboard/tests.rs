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
