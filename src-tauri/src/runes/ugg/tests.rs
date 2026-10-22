use super::*;
use serde_json::json;

fn bulk() -> Value {
    json!([{"top":[["266",[[27,709,1262]],53358,106575,0,0,0,0,0,0]],
            "mid":[["103",[],510,1000,0,0,0,0,0,0]]},
           {"266":114076,"103":100,"total_matches":1534032},"timestamp",1662713.0])
}

#[test]
fn tier_lists_filter_role_share_on_each_patch_without_filtering_overviews() {
    let root = json!([{
        "top":[["40",[],300,600],["266",[],500,1000],["24",[],500,1000]],
        "mid":[["40",[],0,0],["266",[],5000,10000],["24",[],4500,9000]],
        "jungle":[["40",[],0,0],["266",[],5000,10000],["24",[],0,0]],
        "adc":[["40",[],0,0],["266",[],0,0],["24",[],0,0]],
        "supp":[["40",[],10000,20000],["266",[],0,0],["24",[],0,0]]},
        {"40":100,"266":100,"24":100,"total_matches":100000},"t",100000]);
    let raw = parse_tierlist(&root, "top").unwrap();
    assert_eq!(
        raw.rows.len(),
        3,
        "off-role stats remain available to overviews"
    );
    let filtered = parse_lane_tierlist(&root, "top").unwrap();
    assert_eq!(
        filtered
            .rows
            .iter()
            .map(|row| row.champion_id)
            .collect::<Vec<_>>(),
        vec![24],
        "Janna and Aatrox are below 5%; Jax is above the boundary"
    );
    let mut previous = root.clone();
    previous[0]["mid"][1][3] = json!(9000);
    let previous = parse_lane_tierlist(&previous, "top").unwrap();
    assert_eq!(
        previous
            .rows
            .iter()
            .map(|row| row.champion_id)
            .collect::<Vec<_>>(),
        vec![266, 24],
        "exactly 5% qualifies on the previous patch independently"
    );
    let info = PatchInfo {
        patch: "16.20".into(),
        previous: Some("16.19".into()),
    };
    let blended = tierlist::blend_tierlist(filtered, Some(&previous), "emerald_plus", info);
    assert!(blended.rows.iter().all(|row| row.champion_id != 40));
    assert_eq!(
        blended
            .rows
            .iter()
            .find(|row| row.champion_id == 266)
            .unwrap()
            .games,
        0
    );
    let overview = overview_from_lists(
        40,
        "top",
        Some(Sourced::fresh(raw, Provider::Ugg)),
        None,
        Patches {
            current: "16.20".into(),
            previous: None,
        },
    )
    .unwrap();
    assert_eq!(
        (overview.value.stats.games, overview.value.stats.win_pct),
        (600, 50.0)
    );
}

#[test]
fn gold_plus_sums_counts_and_denominators_before_calculating_rates() {
    let gold = json!([{"top":[["266",[],60,100],["24",[],30,50]]},
        {"266":20,"24":10,"total_matches":200},"t",250]);
    let platinum = json!([{"top":[["266",[],90,300],["54",[],40,50]]},
        {"266":50,"54":20,"total_matches":500},"t",750]);
    let merged = merge_datasets("champion_ranking", &gold, &platinum).unwrap();
    assert_eq!(merged[1]["266"], 70);
    assert_eq!(merged[1]["total_matches"], 700);
    assert_eq!(merged[3], 1000.0);
    let list = parse_tierlist(&merged, "top").unwrap();
    let row = list.rows.iter().find(|row| row.champion_id == 266).unwrap();
    assert_eq!(
        (row.games, row.win_pct, row.pick_pct, row.ban_pct),
        (400, 37.5, 40.0, 10.0)
    );
    assert_eq!(list.rows.len(), 3, "one-bucket-only champions are retained");

    let gold = json!({"4":[[[27,6,10],[24,1,2]],"t"]});
    let platinum = json!({"4":[[[27,9,30],[54,2,3]],"t"]});
    let merged = merge_datasets("matchups", &gold, &platinum).unwrap();
    assert_eq!(merged["4"][0][1], json!([27, 15, 40]));
    assert_eq!(merged["4"][0].as_array().unwrap().len(), 3);

    let gold = json!([{"adc_supp":[[222,3,60,100,117,2,10,20,6,10],
        [51,3,10,20,412,2,10,20,1,2]]}]);
    let platinum = json!([{"adc_supp":[[222,3,90,300,117,2,20,80,9,30],
        [81,3,10,20,201,2,10,20,2,3]]}]);
    let merged = merge_datasets("duos", &gold, &platinum).unwrap();
    let pair = merged[0]["adc_supp"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == 222)
        .unwrap();
    assert_eq!(pair, &json!([222, 3, 150, 400, 117, 2, 30, 100, 15, 40]));
    let pairs = super::super::duos::parse_value(&merged).unwrap();
    assert_eq!(pairs.duos.len(), 3);
    let pair = pairs.duos.iter().find(|duo| duo.adc_id == 222).unwrap();
    assert_eq!((pair.games, pair.win_pct), (40, 37.5));
    assert!(merge_datasets("duos", &gold, &json!(null)).is_none());
}

#[test]
fn bulk_fields_use_full_role_counts_and_distinct_pick_and_ban_denominators() {
    let list = parse_tierlist(&bulk(), "top").unwrap();
    let row = &list.rows[0];
    assert_eq!((row.champion_id, row.games), (266, 106575));
    assert!((row.win_pct - 50.06615).abs() < 0.0001);
    assert!((row.pick_pct - 6.4097).abs() < 0.001);
    assert!((row.ban_pct - 7.4363).abs() < 0.001);
    assert_eq!(
        parse_tierlist(&bulk(), "middle").unwrap().rows[0].champion_id,
        103
    );
    assert!(parse_tierlist(&bulk(), "support").is_none());
}

#[test]
fn exact_brackets_and_roles_do_not_silently_substitute_another_population() {
    for (slug, id) in [
        ("all", "8"),
        ("platinum_plus", "10"),
        ("emerald_plus", "17"),
        ("diamond_plus", "11"),
        ("master_plus", "14"),
        ("challenger", "1"),
    ] {
        assert_eq!(rank(slug).unwrap().1, id);
    }
    assert!(rank("gold_plus").is_none());
    assert_eq!(rank("gold"), Some(("gold", "5")));
    for (lane, key, id) in [
        ("top", "top", "4"),
        ("jungle", "jungle", "1"),
        ("middle", "mid", "5"),
        ("bottom", "adc", "3"),
        ("support", "supp", "2"),
    ] {
        assert_eq!(role(lane), Some((key, id)));
    }
    assert!(role("none").is_none());
}

#[test]
fn full_matchups_are_from_the_requested_champions_perspective() {
    let body = json!({"4":[[[27,709,1262,-879834],[103,0,0,0]],"timestamp"]});
    let mut list = parse_tierlist(&bulk(), "top").unwrap();
    list.rows.push(TierRow {
        champion_id: 27,
        win_pct: 52.0,
        ..Default::default()
    });
    let found = parse_counters(&body, "top", &list).unwrap();
    assert_eq!(found.rows.len(), 1);
    assert_eq!(found.rows[0].games, 1262);
    // 709 of 1262 are the opponent's wins.
    assert!((found.rows[0].vs_win_pct - 43.8193).abs() < 0.001);
    assert_eq!(found.rows[0].all_win_pct, 52.0);
    assert!(parse_counters(&body, "middle", &list).is_none());
}

#[test]
fn malformed_or_empty_data_is_missing_instead_of_zero_percent() {
    for body in [
        json!(null),
        json!([]),
        json!([{}, {}, "t", 0]),
        json!([{"top":[["266",[],11,10]]},{"266":1,"total_matches":10},"t",10]),
    ] {
        assert!(parse_tierlist(&body, "top").is_none());
    }
}

#[tokio::test]
async fn missing_failed_and_stale_primary_data_use_lolalytics() {
    for primary in [None, Some(Sourced::stale(1, Provider::Ugg, 0))] {
        let found = prefer(primary, async {
            Ok(Some(Sourced::fresh(2, Provider::Lolalytics)))
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!((found.value, found.provider), (2, Provider::Lolalytics));
    }
    let found = prefer(Some(Sourced::fresh(1, Provider::Ugg)), async {
        panic!("fallback must be lazy")
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(found.value, 1);
    let found = prefer(Some(Sourced::stale(1, Provider::Ugg, 0)), async {
        Ok(None)
    })
    .await
    .unwrap()
    .unwrap();
    assert!(found.stale);
}

#[test]
fn discovers_patch_order_and_dataset_versions_without_hardcoding_or_path_injection() {
    let versions = json!({"15_24":{"champion_ranking":"1.5.1"},"16_1":{"champion_ranking":"1.6.0"},"garbage":{}});
    assert_eq!(
        patch_names(&versions),
        Some(Patches {
            current: "16.1".into(),
            previous: Some("15.24".into())
        })
    );
    assert!(
        dataset_url(&versions, "champion_ranking", "16.1", "all", None)
            .unwrap()
            .ends_with("/overall/1.6.0.json")
    );
    assert!(dataset_url(&versions, "duos", "16.1", "all", None).is_none());
    assert!(dataset_url(
        &json!({"16_1":{"duos":"../../bad"}}),
        "duos",
        "16.1",
        "all",
        None
    )
    .is_none());
    assert!(dataset_url(&versions, "champion_ranking", "16.1", "gold_plus", None).is_none());
}

#[test]
fn overviews_keep_small_samples_and_previous_only_champions_have_zero_games() {
    let mut list = parse_tierlist(&bulk(), "middle").unwrap();
    list.rows[0].games = 10;
    let patches = Patches {
        current: "16.20".into(),
        previous: Some("16.19".into()),
    };
    let now = Some(Sourced::fresh(list.clone(), Provider::Ugg));
    let mut prev = list;
    prev.rows[0].win_pct = 55.0;
    let prev = Some(Sourced::fresh(prev, Provider::Ugg));
    let found = overview_from_lists(103, "middle", now, prev.clone(), patches.clone()).unwrap();
    assert_eq!(found.value.stats.games, 10);
    assert!(
        (found.value.stats.win_pct - super::super::patch::blend_rate(51.0, 10, 55.0)).abs()
            < 1e-9
    );
    let found = overview_from_lists(103, "middle", None, prev, patches).unwrap();
    assert_eq!(
        (found.value.stats.games, found.value.stats.win_pct),
        (0, 55.0)
    );
    assert!(overview_from_lists(
        999,
        "middle",
        None,
        None,
        Patches {
            current: "16.20".into(),
            previous: None
        }
    )
    .is_none());
}

#[tokio::test]
#[ignore = "network: audits live u.gg fields, scores, overview, counters and duos"]
async fn prints_live_ugg_audit() {
    let versions = versions().await.expect("version map");
    assert!(!versions.stale);
    let patches = patch_names(&versions.value).unwrap();
    println!("patches {patches:?}");
    for (lane, ids) in [
        ("top", vec![266, 86, 122, 24, 54, 114]),
        ("middle", vec![103, 134, 777, 238, 55]),
        ("jungle", vec![]),
        ("bottom", vec![]),
        ("support", vec![]),
    ] {
        let raw = raw_list(&versions, &patches.current, lane, "emerald_plus", false)
            .await
            .unwrap();
        let current =
            raw_lane_tierlist(&versions, &patches.current, lane, "emerald_plus", false)
                .await
                .unwrap();
        let current_kept =
            tierlist::blend_tierlist(current.value, None, "emerald_plus", PatchInfo::default())
                .rows
                .len();
        let found = tier_list(lane, "emerald_plus").await.unwrap();
        assert!(!found.stale);
        let games: u64 = found.value.rows.iter().map(|row| row.games).sum();
        let weighted = found
            .value
            .rows
            .iter()
            .map(|row| row.win_pct * row.games as f64)
            .sum::<f64>()
            / games as f64;
        let mut tiers = std::collections::BTreeMap::new();
        for row in &found.value.rows {
            *tiers.entry(row.label()).or_insert(0usize) += 1;
        }
        println!(
            "{lane}: {} current-only rows, {} blended rows ({} previous-only), weighted {weighted:.6}%, tiers {tiers:?}",
            current_kept,found.value.rows.len(),found.value.rows.iter().filter(|row|row.games==0).count()
        );
        if lane == "top" {
            assert!(!found.value.rows.iter().any(|row| row.champion_id == 40));
            let off_role = overview(40, Some("top"), "emerald_plus").await.unwrap();
            assert!(off_role.value.stats.games > 0);
            println!(
                "Janna top: excluded from placement; overview {:.4}% / {} games retained",
                off_role.value.stats.win_pct, off_role.value.stats.games
            );
        }
        for id in ids {
            let row = found
                .value
                .rows
                .iter()
                .find(|r| r.champion_id == id)
                .unwrap();
            let raw = raw.value.rows.iter().find(|r| r.champion_id == id).unwrap();
            let overview = overview(id, Some(lane), "emerald_plus").await.unwrap();
            assert!((overview.value.stats.win_pct - row.win_pct).abs() < 1e-9);
            println!(
                "{lane} {id}: shown {:.4}/{:.4}/{:.4}/{}; raw {:.4}/{:.4}/{:.4}/{}",
                row.win_pct,
                row.pick_pct,
                row.ban_pct,
                row.games,
                raw.win_pct,
                raw.pick_pct,
                raw.ban_pct,
                raw.games
            );
        }
    }
    let counters = counters(266, Some("top"), "emerald_plus").await.unwrap();
    assert!(!counters.stale && counters.value.rows.len() > 20);
    println!(
        "Aatrox counters: {} rows; vs Singed {:?}",
        counters.value.rows.len(),
        counters.value.rows.iter().find(|r| r.champion_id == 27)
    );
    let duos = duos("emerald_plus").await.unwrap();
    assert!(!duos.stale && !duos.value.duos.is_empty());
    println!("duos: {} pairs", duos.value.duos.len());
    probe().await.expect("Doctor live data probe");
}

#[tokio::test]
#[ignore = "network: verifies Gold+ uses merged u.gg counts across all stats screens"]
async fn prints_live_gold_plus_audit() {
    let list = tierlist::load("top", "gold_plus").await.unwrap().unwrap();
    assert_eq!(list.provider, Provider::Ugg);
    assert!(!list.stale && !list.value.rows.is_empty());
    let overview = super::super::overview::load("aatrox", 266, Some("top"), "gold_plus")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(overview.provider, Provider::Ugg);
    assert!(!overview.stale);
    assert!(overview.value.build.is_some());
    let counters = super::super::counters::load("aatrox", 266, Some("top"), "gold_plus")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(counters.provider, Provider::Ugg);
    assert!(!counters.stale && !counters.value.rows.is_empty());
    let duos = super::super::duos::load("gold_plus").await.unwrap();
    assert_eq!(duos.provider, Provider::Ugg);
    assert!(!duos.stale && !duos.value.duos.is_empty());
    let matchup = matchup_stats(266, 27, "top", "gold_plus").await.unwrap();
    assert!(!matchup.stale);
    println!(
        "Gold+ u.gg: {} tier rows, overview {:.4}% / {} games with build, {} counters, {} duo pairs; vs Singed {:.4}% / {} games",
        list.value.rows.len(),
        overview.value.stats.win_pct,
        overview.value.stats.games,
        counters.value.rows.len(),duos.value.duos.len(),matchup.value.win_pct,matchup.value.games
    );
}
