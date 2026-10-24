use super::*;
use serde_json::json;

fn bundle() -> Value {
    serde_json::from_str(include_str!("fixtures/live_game.json")).unwrap()
}

fn names() -> BTreeMap<i64, String> {
    [
        (86, "Garen"),
        (64, "Lee Sin"),
        (103, "Ahri"),
        (222, "Jinx"),
        (412, "Thresh"),
        (122, "Darius"),
        (60, "Elise"),
        (238, "Zed"),
        (145, "Kai'Sa"),
        (117, "Lulu"),
    ]
    .into_iter()
    .map(|(id, name)| (id, name.to_string()))
    .collect()
}

fn ids() -> BTreeMap<String, i64> {
    names()
        .into_iter()
        .map(|(id, n)| (n.to_lowercase(), id))
        .collect()
}

fn session_view() -> View {
    let b = bundle();
    model_view(&b)
}

fn model_view(b: &Value) -> View {
    from_session(&b["session"], b["selfPuuid"].as_str().unwrap(), &names()).unwrap()
}

/// Every player's bundle applied, as the background fetch would.
fn full_view() -> View {
    let b = bundle();
    let mut view = model_view(&b);
    for player in &mut view.players {
        let p = &b["players"][&player.puuid];
        let rows = p["history"]["games"]["games"].as_array().cloned();
        player.apply(Some(&p["summoner"]), Some(&p["ranked"]), rows.as_deref());
        player.damage = b["damage"][player.champion_id.to_string()]
            .as_str()
            .unwrap()
            .into();
    }
    view.summarize_teams();
    view
}

fn by_name<'a>(view: &'a View, name: &str) -> &'a Player {
    view.players
        .iter()
        .find(|p| p.riot_id.starts_with(name))
        .unwrap_or_else(|| panic!("{name} missing"))
}

#[test]
fn session_lists_both_teams_with_spells_and_self() {
    let view = session_view();
    assert_eq!(view.game_id, 7000000001);
    assert_eq!(view.queue_id, 420);
    assert_eq!(view.players.len(), 10);
    assert!(view.players[..5].iter().all(|p| p.side == Side::Blue));
    assert!(view.players[5..].iter().all(|p| p.side == Side::Red));
    let me: Vec<_> = view.players.iter().filter(|p| p.is_self).collect();
    assert_eq!(me.len(), 1);
    assert_eq!(me[0].champion_id, 103);
    assert_eq!(me[0].champion_name, "Ahri");
    assert_eq!(me[0].spells, [4, 14]);
    assert_eq!(me[0].position, "MIDDLE");
    assert!(view
        .players
        .iter()
        .all(|p| p.status == Status::Pending && p.key == p.puuid));
    // Riot IDs come from the summoner lookup, not the session.
    assert!(view.players.iter().all(|p| p.riot_id.is_empty()));
}

#[test]
fn session_outside_a_game_or_hidden_players_degrade() {
    assert!(from_session(&json!({ "gameData": { "gameId": 0 } }), "", &names()).is_none());
    let view = from_session(
        &json!({ "gameData": { "gameId": 5, "teamOne": [{ "championId": 1, "puuid": "" }], "teamTwo": [] } }),
        "",
        &names(),
    )
    .unwrap();
    assert_eq!(view.players[0].key, "teamOne:0");
    assert_eq!(view.players[0].status, Status::Skipped);
    assert!(
        !view.players[0].is_self,
        "an empty puuid never matches an empty self puuid"
    );
}

fn live(riot: &str, champ: &str, side: Option<Side>) -> LivePlayer {
    LivePlayer {
        riot_id: riot.into(),
        champion_name: champ.into(),
        side,
        ..Default::default()
    }
}

fn player(riot: &str, champ: &str, side: Side) -> Player {
    let mut p = Player::new(riot.into(), side);
    p.riot_id = riot.into();
    p.champion_name = champ.into();
    p
}

#[test]
fn matching_prefers_riot_id_then_champion_on_side_then_champion() {
    let players = vec![
        player("A#1", "Ahri", Side::Blue),
        player("B#1", "Zed", Side::Red),
        player("", "Lux", Side::Blue),
        player("", "Jinx", Side::Blue),
    ];
    let lives = vec![
        live("b#1", "Zed", Some(Side::Red)), // case-insensitive Riot ID
        live("Renamed#X", "Ahri", Some(Side::Blue)), // Riot ID changed: champion + side
        live("C#1", "Jinx", Some(Side::Red)), // session side wrong: champion alone
        live("D#1", "Lux", None),
        live("E#1", "Garen", Some(Side::Blue)), // unknown
    ];
    assert_eq!(
        match_live(&players, &lives),
        vec![Some(1), Some(0), Some(3), Some(2), None]
    );
}

#[test]
fn ambiguous_champions_never_match_and_players_are_used_once() {
    // One for All: two blue Ahris, no Riot IDs known yet.
    let players = vec![
        player("", "Ahri", Side::Blue),
        player("", "Ahri", Side::Blue),
    ];
    let lives = vec![live("X#1", "Ahri", Some(Side::Blue))];
    assert_eq!(match_live(&players, &lives), vec![None]);
    // A Riot ID match takes its player out of the champion passes.
    let players = vec![
        player("X#1", "Ahri", Side::Blue),
        player("", "Ahri", Side::Blue),
    ];
    let lives = vec![
        live("X#1", "Ahri", Some(Side::Blue)),
        live("Y#1", "Ahri", Some(Side::Blue)),
    ];
    assert_eq!(match_live(&players, &lives), vec![Some(0), Some(1)]);
}

#[test]
fn live_merge_fills_runes_items_and_live_numbers() {
    let b = bundle();
    let mut view = full_view();
    merge_live(&mut view, &b["allgamedata"], &ids());
    assert_eq!(
        view.players.len(),
        10,
        "every live player matched a session player"
    );
    assert_eq!(view.game_time, Some(1087.4));
    let me = view.players.iter().find(|p| p.is_self).unwrap();
    assert_eq!(
        (me.keystone, me.primary_style, me.secondary_style),
        (8112, 8100, 8300)
    );
    let live = me.live.as_ref().unwrap();
    assert_eq!(live.items.len(), 7);
    assert_eq!(live.items[0], 6655);
    assert_eq!(live.items[6], 3340);
    // Renamed after the session was read: matched by champion, keeps the
    // summoner lookup's Riot ID.
    let kaisa = view.players.iter().find(|p| p.champion_id == 145).unwrap();
    assert!(kaisa.live.is_some());
    assert_eq!(kaisa.riot_id, "Red Bot#FIX");
    let elise = view.players.iter().find(|p| p.champion_id == 60).unwrap();
    assert!(elise.live.as_ref().unwrap().dead);
}

#[test]
fn live_merge_corrects_sides_and_adds_players_the_session_missed() {
    let mut view = from_session(
        &json!({ "gameData": { "gameId": 9, "teamOne": [{ "championId": 103, "puuid": "p1" }], "teamTwo": [] } }),
        "p1",
        &names(),
    )
    .unwrap();
    let data = json!({
        "gameData": { "gameTime": 30.0 },
        "allPlayers": [
            { "riotId": "Bot#1", "championName": "Zed", "team": "ORDER", "isBot": true,
              "summonerSpells": { "summonerSpellOne": { "rawDisplayName": "GeneratedTip_SummonerSpell_SummonerFlash_DisplayName" },
                                  "summonerSpellTwo": { "rawDisplayName": "GeneratedTip_SummonerSpell_SummonerDot_DisplayName" } } },
            { "riotId": "Me#1", "championName": "Ahri", "team": "CHAOS", "position": "" }
        ]
    });
    merge_live(&mut view, &data, &ids());
    assert_eq!(view.players.len(), 2);
    // Blue first: the bot (ORDER), then me, moved to red by the Live Client.
    assert_eq!(view.players[0].key, "live:Bot#1");
    assert!(view.players[0].is_bot);
    assert_eq!(view.players[0].champion_id, 238);
    assert_eq!(view.players[0].spells, [4, 14]);
    assert_eq!(view.players[0].status, Status::Skipped);
    assert_eq!(view.players[1].side, Side::Red);
    assert_eq!(view.players[1].riot_id, "Me#1");
    // An empty read changes nothing.
    let before = view.clone();
    merge_live(&mut view, &json!({}), &ids());
    assert_eq!(view, before);
}

#[test]
fn practice_dummies_and_duplicate_ids_never_grow_the_teams() {
    let mut view = from_session(
        &json!({ "gameData": { "gameId": 7, "teamOne": [], "teamTwo": [] } }),
        "", &BTreeMap::new(),
    ).unwrap();
    let dummy = json!({ "riotId": "", "summonerName": "Unknown", "championName": "", "team": "CHAOS", "isBot": true });
    let twin = json!({ "riotId": "Twin#1", "championName": "Zed", "team": "CHAOS" });
    let data = json!({ "gameData": { "gameTime": 30.0 }, "allPlayers": [
        { "riotId": "Bot#1", "championName": "Zed", "team": "ORDER", "isBot": true },
        dummy.clone(), dummy.clone(), dummy, twin.clone(), twin,
    ] });
    for _ in 0..3 {
        merge_live(&mut view, &data, &ids());
    }
    assert_eq!(view.players.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(), ["live:Bot#1", "live:Twin#1"]);
}

fn row(
    id: u64,
    champion: i64,
    win: bool,
    kda: (u64, u64, u64),
    queue: u64,
    duration: u64,
) -> Value {
    json!({
        "gameId": id, "gameCreation": 1_000_000 + id, "gameDuration": duration, "queueId": queue, "mapId": 11,
        "participantIdentities": [{ "participantId": 1, "player": { "puuid": "p" } }],
        "participants": [{ "participantId": 1, "championId": champion, "teamId": 100, "spell1Id": 4, "spell2Id": 14,
            "stats": { "win": win, "kills": kda.0, "deaths": kda.1, "assists": kda.2, "item0": 0,
                       "totalMinionsKilled": 150, "neutralMinionsKilled": 0 },
            "timeline": { "lane": "MIDDLE" } }]
    })
}

#[test]
fn history_aggregates_win_rate_kda_champion_and_streak() {
    // Newest first by gameCreation (higher id = newer).
    let rows = vec![
        row(1, 103, false, (1, 5, 1), 420, 1800),
        row(2, 238, true, (4, 2, 4), 420, 1800),
        row(3, 103, true, (10, 0, 5), 440, 1800),
        row(4, 103, true, (2, 2, 2), 420, 1800),
        row(5, 103, true, (9, 9, 9), 450, 1800), // ARAM: not Rift, ignored
        row(6, 103, true, (9, 9, 9), 420, 200),  // remake: ignored
    ];
    let h = summarize(&rows, "p", 103);
    assert_eq!(h.form.games, 4);
    assert_eq!(h.form.wins, 3);
    assert_eq!((h.form.kills, h.form.deaths, h.form.assists), (17, 9, 12));
    assert_eq!(h.on_champion.games, 3);
    assert_eq!(h.on_champion.wins, 2);
    assert_eq!(h.streak, 3, "latest three games are wins");
    assert_eq!(h.main_role, Some(("mid".into(), 4)));
    assert_eq!(h.recent.first().unwrap().created_at, 1_000_004);
    assert_eq!(h.recent.len(), 4);
    let losses = summarize(
        &[
            row(1, 1, false, (0, 1, 0), 420, 1800),
            row(2, 1, false, (0, 1, 0), 420, 1800),
        ],
        "p",
        1,
    );
    assert_eq!(losses.streak, -2);
    assert_eq!(summarize(&[], "p", 1), History::default());
}

fn sample(games: usize, on_champion: usize, streak: i32, role: (&str, usize)) -> History {
    History {
        form: Form {
            games,
            ..Default::default()
        },
        on_champion: Form {
            games: on_champion,
            ..Default::default()
        },
        main_role: Some((role.0.into(), role.1)),
        streak,
        recent: vec![],
    }
}

fn kinds(badges: &[Badge]) -> Vec<&str> {
    badges.iter().map(|b| b.kind).collect()
}

#[test]
fn badges_need_enough_games_to_be_honest() {
    assert_eq!(
        kinds(&badges(&sample(20, 0, 3, ("mid", 15)), "Ahri", "MIDDLE")),
        ["streak-win", "new-champ"]
    );
    assert_eq!(
        kinds(&badges(&sample(20, 12, -4, ("mid", 15)), "Ahri", "MIDDLE")),
        ["streak-loss", "one-trick"]
    );
    // Two wins is not a streak; nine games cannot say "new on champion".
    assert!(badges(&sample(9, 0, 2, ("mid", 9)), "Ahri", "MIDDLE").is_empty());
    // Off-role: needs a clear main role (60%+ of 10+ games) that differs.
    assert_eq!(
        kinds(&badges(&sample(20, 5, 1, ("mid", 12)), "Garen", "TOP")),
        ["off-role"]
    );
    assert!(badges(&sample(20, 5, 1, ("mid", 11)), "Garen", "TOP").is_empty());
    assert!(badges(&sample(20, 5, 1, ("top", 18)), "Garen", "TOP").is_empty());
    // No assigned position (blind pick, ARAM): never off-role.
    assert!(badges(&sample(20, 5, 1, ("mid", 18)), "Garen", "").is_empty());
}

#[test]
fn ranks_parse_solo_and_flex_and_average_by_score() {
    let b = bundle();
    let red_mid = &b["players"][b["session"]["gameData"]["teamTwo"][2]["puuid"]
        .as_str()
        .unwrap()]["ranked"];
    let (solo, flex) = ranks(red_mid);
    let solo = solo.unwrap();
    assert_eq!(
        (solo.tier.as_str(), solo.division.as_str(), solo.lp),
        ("DIAMOND", "II", 88)
    );
    assert_eq!(flex.unwrap().tier, "EMERALD");
    let blue_support = &b["players"][b["session"]["gameData"]["teamOne"][4]["puuid"]
        .as_str()
        .unwrap()]["ranked"];
    assert_eq!(ranks(blue_support), (None, None));
    let rank = |tier: &str, division: &str, lp| Rank {
        tier: tier.into(),
        division: division.into(),
        lp,
        wins: 0,
        losses: 0,
    };
    assert_eq!(rank_score(&rank("IRON", "IV", 0)), Some(0));
    assert_eq!(rank_score(&rank("GOLD", "I", 50)), Some(3 * 400 + 300 + 50));
    assert_eq!(rank_score(&rank("MASTER", "", 143)), Some(2800 + 143));
    assert_eq!(rank_score(&rank("CHALLENGER", "", 900)), Some(3700));
    assert_eq!(rank_score(&rank("UNKNOWN", "I", 0)), None);
    assert_eq!(score_label(3 * 400 + 300 + 50), ("GOLD".into(), "I".into()));
    assert_eq!(score_label(5000), ("MASTER".into(), String::new()));
}

#[test]
fn fixture_game_end_to_end() {
    let view = full_view();
    assert!(view.players.iter().all(|p| p.riot_id.ends_with("#FIX")));
    let me = view.players.iter().find(|p| p.is_self).unwrap();
    assert_eq!(me.status, Status::Ready);
    assert_eq!(me.solo.as_ref().unwrap().tier, "EMERALD");
    let h = me.history.as_ref().unwrap();
    assert_eq!(h.form.games, 20);
    assert_eq!(h.streak, 3);
    assert!(kinds(&me.badges).contains(&"streak-win"));
    assert!(kinds(&by_name(&view, "Blue Bot").badges).contains(&"new-champ"));
    assert!(kinds(&by_name(&view, "Blue Top").badges).contains(&"off-role"));
    assert!(kinds(&by_name(&view, "Red Mid").badges).contains(&"one-trick"));
    assert!(kinds(&by_name(&view, "Blue Jungle").badges).contains(&"streak-loss"));
    // History failed and unranked: shown partially, no badges, no error.
    let support = by_name(&view, "Blue Support");
    assert_eq!(support.status, Status::Partial);
    assert!(support.history.is_none() && support.solo.is_none() && support.badges.is_empty());
    // Blue: Gold II, Plat IV, Emerald III, Emerald IV ranked (4 of 5).
    assert_eq!(view.blue.ranked, 4);
    assert_eq!(view.blue.average_tier.as_deref(), Some("PLATINUM"));
    assert_eq!(view.red.ranked, 5);
    // Blue: Garen, Lee Sin, Jinx physical; Ahri, Thresh magic = 60%.
    assert_eq!(view.blue.physical, Some(60));
    // Red: Darius, Zed physical, Kai'Sa mixed, Elise, Lulu magic = 50%.
    assert_eq!(view.red.physical, Some(50));
}

#[test]
fn team_damage_mix_waits_for_every_champion() {
    let mut view = full_view();
    view.players[0].damage.clear();
    view.summarize_teams();
    assert_eq!(view.blue.physical, None);
    assert_eq!(view.red.physical, Some(50));
}

#[test]
fn only_loading_and_running_phases_count_as_in_game() {
    for phase in ["GameStart", "InProgress", "Reconnect"] {
        assert!(in_game(phase));
    }
    for phase in [
        "ChampSelect",
        "Lobby",
        "WaitingForStats",
        "EndOfGame",
        "None",
        "",
    ] {
        assert!(!in_game(phase));
    }
}
