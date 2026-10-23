//! Contract tests for the scraped sources, on recorded responses.
//!
//! Each fixture under `tests/fixtures/` is a real response, trimmed to the
//! entries the parser reads (player names replaced). The tests pin the values
//! the UI shows, so a parser change that drops a field fails here.
//!
//! The `canary_*` tests hit the live endpoints once and assert the same
//! contract, so a site that changed its format fails loudly:
//! `cargo test --lib -- --ignored canary`. They use the parsers directly and
//! never touch Swapper's on-disk caches.

use serde_json::Value;

use super::lolalytics::LolalyticsClient;
use super::*;

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("../../tests/fixtures/", $name))
    };
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("fixture is JSON")
}

fn close(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 1e-6
}

// ---------------------------------------------------------------------------
// lolalytics
// ---------------------------------------------------------------------------

#[test]
fn lolalytics_counters_page_has_matchup_rows() {
    let found =
        counters::parse_counters(fixture!("lolalytics-graves-jungle-counters.html"), 104).unwrap();
    assert_eq!(found.lane, "jungle");
    assert_eq!(found.rows.len(), 12);
    let first = &found.rows[0];
    assert_eq!((first.champion_id, first.games), (238, 242));
    assert!(close(first.vs_win_pct, 58.68) && close(first.all_win_pct, 50.99));
    assert!(
        counters::parse_counters(fixture!("lolalytics-graves-jungle-counters.html"), 1).is_none(),
        "another champion's page is not used"
    );
}

#[test]
fn lolalytics_tier_list_page_has_ranked_rows() {
    let rows = tierlist::parse_rows(fixture!("lolalytics-middle-tierlist.html"), 0).unwrap();
    assert_eq!(rows.len(), 14);
    let annie = rows.iter().find(|row| row.champion_id == 1).unwrap();
    assert_eq!(annie.games, 3114);
    assert!(
        close(annie.win_pct, 52.63) && close(annie.pick_pct, 1.81) && close(annie.ban_pct, 0.56)
    );
}

#[test]
fn lolalytics_champion_page_has_stats_build_runes_and_skills() {
    let found =
        overview::parse_overview(fixture!("lolalytics-ahri-mid-overview.html"), 103).unwrap();
    assert_eq!(found.lane, "middle");
    let stats = &found.stats;
    assert_eq!(
        (
            stats.tier.as_str(),
            stats.games,
            stats.rank,
            stats.rank_total
        ),
        ("S", 17210, 6, 94)
    );
    assert_eq!(stats.patch, "16.21");
    assert!(close(stats.win_pct, 52.27) && close(stats.avg_win_pct, 50.9));
    assert!(close(stats.pick_pct, 9.78) && close(stats.ban_pct, 3.06));
    assert!(close(found.damage.as_ref().unwrap().magic, 18290.45));
    let build = found.build.as_ref().unwrap();
    assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
    assert_eq!((build.core_len, build.core_games), (3, 3119));
    let runes = found.runes.as_ref().unwrap();
    assert_eq!(
        (runes.keystone, runes.secondary_runes.clone()),
        (8112, vec![8226, 8210])
    );
    assert_eq!(found.spells, Some([4, 14]));
    assert_eq!(found.skill_order.as_deref(), Some("WQEQQRQWQWRWWEE"));
}

#[test]
fn lolalytics_keystone_page_has_both_builds() {
    let builds = lolalytics::parse_builds(fixture!("lolalytics-ahri-mid-build-8112.html")).unwrap();
    let pick = &builds.pick;
    assert_eq!(pick.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
    assert_eq!(
        pick.starters
            .iter()
            .map(|s| (s.id, s.count))
            .collect::<Vec<_>>(),
        vec![(1056, 1), (2003, 2)]
    );
    assert_eq!((pick.games, pick.core_games), (13720, 2682));
    assert_eq!(pick.skill_priority.as_deref(), Some("QWE"));
    assert_eq!(pick.skill_order.as_deref(), Some("WQEQQRQWQWRWWEE"));
    assert_eq!(pick.slots.len(), 8);
    let win = builds.win.as_ref().unwrap();
    assert_eq!(win.build.items, vec![2503, 3158, 3100, 4645, 3157, 3089]);
    assert_eq!(win.runes.keystone, 8112);
    assert_eq!(win.spells, Some([4, 14]));
}

#[test]
fn lolalytics_matchup_page_reads_its_own_first_three_items() {
    // Since 16.21 the matchup summary lists `item1`..`item3` instead of a
    // joint `core` set; the build must still come from the matchup's data.
    let found =
        matchup::parse_matchup(fixture!("lolalytics-ahri-vs-zed-mid.html"), 103, 238).unwrap();
    assert_eq!(
        (found.stats.games, found.stats.patch.as_str()),
        (642, "16.21")
    );
    assert!(close(found.stats.win_pct, 51.71) && close(found.stats.avg_win_pct, 50.9));
    assert_eq!(found.build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
    assert_eq!(found.build.core_len, 0, "single slots are not a joint core");
    assert_eq!(found.runes.as_ref().unwrap().keystone, 8112);
    assert_eq!(found.spells, Some([4, 14]));
    assert_eq!(found.skill_order.as_deref(), Some("WQEQQRQWQWRWWEE"));
}

#[test]
fn lolalytics_build_team_has_support_synergy_rows() {
    let rows =
        duos::parse_lolalytics(&json(fixture!("lolalytics-build-team-jinx.json")), 222).unwrap();
    assert_eq!(rows.len(), 8);
    assert_eq!(
        (rows[0].adc_id, rows[0].support_id, rows[0].games),
        (222, 117, 3600)
    );
    assert!(close(rows[0].win_pct, 53.89));
}

// ---------------------------------------------------------------------------
// u.gg
// ---------------------------------------------------------------------------

fn ugg_ranking() -> Value {
    json(fixture!("ugg-champion-ranking-emerald.json"))
}

#[test]
fn ugg_versions_name_the_two_newest_patches_and_dataset_urls() {
    let versions = json(fixture!("ugg-versions.json"));
    let patches = ugg::patch_names(&versions).unwrap();
    assert_eq!(
        (patches.current.as_str(), patches.previous.as_deref()),
        ("16.21", Some("16.20"))
    );
    assert_eq!(
        ugg::dataset_url(&versions, "champion_ranking", "16.21", "emerald_plus", None).unwrap(),
        "https://stats2.u.gg/lol/1.5/champion_ranking/world/16_21/ranked_solo_5x5/emerald_plus/1.5.0.json"
    );
    assert_eq!(
        ugg::dataset_url(&versions, "matchups", "16.21", "emerald_plus", Some(266)).unwrap(),
        "https://stats2.u.gg/lol/1.5/matchups/16_21/ranked_solo_5x5/266/1.5.0.json"
    );
}

#[test]
fn ugg_champion_ranking_has_rates_and_games_per_role() {
    let ranking = ugg_ranking();
    let top = ugg::parse_tierlist(&ranking, "top").unwrap();
    assert_eq!(top.rows.len(), 33);
    let aatrox = top.rows.iter().find(|row| row.champion_id == 266).unwrap();
    assert_eq!(aatrox.games, 10328);
    assert!((aatrox.win_pct - 50.3776).abs() < 1e-3);
    assert!((aatrox.pick_pct - 6.6403).abs() < 1e-3);
    assert!((aatrox.ban_pct - 7.8473).abs() < 1e-3);
    assert_eq!(
        ugg::parse_lane_tierlist(&ranking, "top")
            .unwrap()
            .rows
            .len(),
        12
    );
    for lane in ["jungle", "middle", "bottom", "support"] {
        assert!(ugg::parse_tierlist(&ranking, lane).is_some(), "{lane}");
    }
}

#[test]
fn ugg_matchups_are_full_counter_rows() {
    let top = ugg::parse_tierlist(&ugg_ranking(), "top").unwrap();
    let matchups = json(fixture!("ugg-matchups-aatrox.json"));
    let found = ugg::parse_counters(&matchups["12"]["17"], "top", &top).unwrap();
    assert_eq!(found.lane, "top");
    assert_eq!(found.rows.len(), 6);
    let first = &found.rows[0];
    assert_eq!((first.champion_id, first.games), (777, 556));
    assert!((first.vs_win_pct - 51.6187).abs() < 1e-3);
    assert!((first.all_win_pct - 49.1063).abs() < 1e-3);
}

#[test]
fn ugg_duos_have_adc_support_pairs() {
    let found = duos::parse_value(&json(fixture!("ugg-duos-emerald.json"))).unwrap();
    assert_eq!(found.duos.len(), 8);
    let first = &found.duos[0];
    assert_eq!(
        (first.adc_id, first.support_id, first.games),
        (360, 412, 521)
    );
    assert!((first.win_pct - 57.5816).abs() < 1e-3);
}

// ---------------------------------------------------------------------------
// probuildstats / u.gg GraphQL and op.gg
// ---------------------------------------------------------------------------

#[test]
fn probuilds_graphql_has_pro_matches_with_builds() {
    let matches = probuilds::parse(fixture!("probuilds-graphql-ahri-mid.json")).unwrap();
    assert_eq!(matches.len(), 4);
    let first = &matches[0];
    assert_eq!((first.match_id, first.win), (8019342423, false));
    assert_eq!(first.pro_info.league, "LEC");
    assert_eq!(
        first.final_items(),
        vec![2503, 3116, 4629, 3171, 1082, 1052, 3364]
    );
    assert_eq!(
        first.skill_order().iter().collect::<String>(),
        "WQEQQRQWQWRWWEE"
    );
    assert_eq!(first.summoner_spells, vec![4, 14]);
    assert_eq!(first.runes.perk0, Some(8112));
    assert!(matches.iter().all(|found| found.selection().is_some()));
}

#[test]
fn opgg_champion_data_has_rune_pages_and_spells() {
    let data = opgg::parse_data(fixture!("opgg-ahri-mid-ranked.json")).unwrap();
    assert_eq!(data.rune_pages.len(), 3);
    assert_eq!(data.rune_pages[0].primary_page_id, 8100);
    assert_eq!(
        data.rune_pages[0].builds[0].stat_mod_ids,
        vec![5005, 5008, 5001]
    );
}

// ---------------------------------------------------------------------------
// Live canaries: `cargo test --lib -- --ignored canary`
// ---------------------------------------------------------------------------

async fn get_text(url: &str) -> String {
    let response = reqwest::Client::new()
        .get(url)
        .header(reqwest::header::USER_AGENT, "Mozilla/5.0")
        .send()
        .await
        .unwrap_or_else(|error| panic!("{url} unreachable: {error}"));
    assert!(
        response.status().is_success(),
        "{url} answered {}",
        response.status()
    );
    response.text().await.unwrap()
}

#[tokio::test]
#[ignore = "network: live lolalytics pages and mega endpoints"]
async fn canary_lolalytics() {
    let client = LolalyticsClient::new().unwrap();
    let list = client
        .tierlist("middle", "emerald_plus", None)
        .await
        .unwrap()
        .expect("tier list page");
    assert!(list.rows.len() > 20);
    let found = client
        .counters("graves", 104, Some("jungle"), "emerald_plus", None)
        .await
        .unwrap()
        .expect("counters page");
    assert!(found.rows.len() > 10);
    let found = client
        .overview("ahri", 103, Some("middle"), "emerald_plus", None)
        .await
        .unwrap()
        .expect("champion page");
    assert!(found.stats.games > 0 && found.runes.is_some() && found.skill_order.is_some());
    let build = found.build.expect("champion page build");
    assert!(
        build.core_len == 3 && build.items.len() >= 5,
        "joint core from the Qwik state"
    );
    let builds = client
        .build("ahri", "middle", "emerald_plus", 8112, None)
        .await
        .unwrap()
        .expect("keystone page");
    assert!(
        builds.pick.skill_order.is_some(),
        "parsed from the Qwik state, not the markup"
    );
    assert!(builds.win.is_some());
    let found = client
        .matchup("ahri", "zed", 103, 238, "middle", "emerald_plus", None)
        .await
        .unwrap()
        .expect("matchup page");
    assert!(found.stats.games > 0 && found.runes.is_some());
    assert!(
        found.build.skill_order.is_some(),
        "parsed from the Qwik state, not the markup"
    );
    let patch = patch::patches().await.expect("ddragon versions").current;
    let body = get_text(&format!(
        "https://a1.lolalytics.com/mega/?ep=build-team&v=1&patch={patch}&c=jinx&lane=bottom&tier=emerald_plus&queue=ranked&region=all"
    ))
    .await;
    assert!(!duos::parse_lolalytics(&json(&body), 222)
        .unwrap()
        .is_empty());
}

#[tokio::test]
#[ignore = "network: live u.gg versions and datasets"]
async fn canary_ugg() {
    let versions = json(&get_text(ugg::VERSIONS_URL).await);
    let patches = ugg::patch_names(&versions).expect("versions map");
    let url = |kind, champion| {
        ugg::dataset_url(&versions, kind, &patches.current, "emerald_plus", champion)
            .unwrap_or_else(|| panic!("{kind} dataset url"))
    };
    let ranking = json(&get_text(&url("champion_ranking", None)).await);
    let top = ugg::parse_tierlist(&ranking, "top").expect("champion ranking");
    assert!(top.rows.len() > 30);
    for lane in ["jungle", "middle", "bottom", "support"] {
        assert!(ugg::parse_lane_tierlist(&ranking, lane).is_some_and(|list| list.rows.len() > 10));
    }
    let matchups = json(&get_text(&url("matchups", Some(266))).await);
    let found = ugg::parse_counters(&matchups["12"]["17"], "top", &top).expect("matchups");
    assert!(found.rows.len() > 20);
    let pairs = json(&get_text(&url("duos", None)).await);
    assert!(duos::parse_value(&pairs).is_some_and(|list| list.duos.len() > 20));
}

#[tokio::test]
#[ignore = "network: live probuildstats / u.gg GraphQL"]
async fn canary_probuilds() {
    let matches = probuilds::ProBuildsClient::new()
        .unwrap()
        .matches(103, Some("mid"), 1, false)
        .await
        .expect("pro match list");
    assert!(!matches.is_empty());
    assert!(matches
        .iter()
        .all(|found| found.selection().is_some() && !found.final_items().is_empty()));
}

#[tokio::test]
#[ignore = "network: live op.gg champion data"]
async fn canary_opgg() {
    let data = opgg::OpggClient::new()
        .unwrap()
        .champion_data(
            opgg::DEFAULT_REGION,
            opgg::MODE_RANKED,
            103,
            "mid",
            opgg::DEFAULT_TIER,
            None,
        )
        .await
        .expect("op.gg champion data");
    assert!(!data.rune_pages.is_empty() && !data.summoner_spells.is_empty());
}

#[tokio::test]
#[ignore = "network: live u.gg fetchProfileRanks (unknown player, no real Riot ID sent)"]
async fn canary_ugg_profile_ranks() {
    // An unknown player is a valid empty answer; that proves the query name,
    // variables and field set are still accepted by the live schema.
    let text = reqwest::Client::new()
        .post(probuilds::ENDPOINT)
        .json(&ugg_record::body("zzqx", "0000", "euw1"))
        .send()
        .await
        .expect("u.gg reachable")
        .text()
        .await
        .unwrap();
    assert_eq!(ugg_record::parse(&text).expect("profile ranks shape"), vec![]);
}
