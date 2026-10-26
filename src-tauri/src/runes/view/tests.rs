use super::builds::*;
use super::pro::*;
use super::*;

fn win_catalog() -> perks::PerkCatalog {
    let styles = perks::parse_styles(
        r#"{"styles":[
        {"id":8100,"slots":[
            {"type":"kKeyStone","perks":[8112]},
            {"type":"minor","perks":[8139]},
            {"type":"minor","perks":[8137,8140]},
            {"type":"minor","perks":[8106]},
            {"type":"kStatMod","perks":[5005,5008]},
            {"type":"kStatMod","perks":[5008]},
            {"type":"kStatMod","perks":[5001,5011]}]},
        {"id":8400,"slots":[
            {"type":"minor","perks":[]},
            {"type":"minor","perks":[8473]},
            {"type":"minor","perks":[8242]}]}
    ]}"#,
    )
    .unwrap();
    perks::PerkCatalog::new(HashMap::new(), styles)
}

#[test]
fn win_preset_maps_tree_ids_and_carries_its_own_apply_payload() {
    let builds = lolalytics::parse_builds(include_str!(
        "../../../tests/fixtures/lolalytics-ahri-mid-8112.html"
    ))
    .unwrap();
    let catalog = win_catalog();
    let win = highest_win_view(
        &builds,
        None,
        &catalog,
        true,
        Some(123),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(win.preset.primary_page_id, 8100);
    assert_eq!(win.preset.secondary_page_id, 8400);
    assert_eq!(win.preset.primary_runes, vec![8139, 8137, 8106]);
    assert_eq!(win.preset.secondary_runes, vec![8473, 8242]);
    assert_eq!(win.preset.shards, vec![5008, 5008, 5011]);
    assert_eq!(win.preset.spells, vec![4, 14]);
    assert_eq!(win.preset.play, 1835);
    assert_eq!(win.preset.win_pct, Some(56.19));
    assert!(win.build.stale);
    assert_eq!(win.build.updated_at, Some(123));
    let view = PresetBuildView {
        pick: keystone_build_view(builds.pick, false, None, &HashMap::new()),
        highest_win: Some(win),
        highest_win_matches_pick: false,
    };
    assert_eq!(view.clone().into_items(true).unwrap().items[0].id, 2503);
    assert_eq!(view.clone().into_items(false).unwrap().items[0].id, 3118);
    let without_win = PresetBuildView {
        highest_win: None,
        ..view
    };
    assert!(
        without_win.into_items(true).is_none(),
        "never import pick as a win build"
    );
}

#[test]
fn win_view_dedupes_against_the_displayed_preset_and_omits_invalid_runes() {
    let mut builds = lolalytics::parse_builds(include_str!(
        "../../../tests/fixtures/lolalytics-ahri-mid-8112.html"
    ))
    .unwrap();
    let catalog = win_catalog();
    let mut most_played = builds
        .win
        .as_ref()
        .unwrap()
        .runes
        .selection(&catalog)
        .unwrap();
    builds.pick.items = builds.win.as_ref().unwrap().build.items.clone();
    let project = |builds: &lolalytics::KeystoneBuilds,
                   selection: &super::super::RuneSelection| {
        highest_win_view(
            builds,
            Some(selection),
            &catalog,
            false,
            None,
            &HashMap::new(),
        )
    };
    assert!(matches!(
        project(&builds, &most_played),
        Err(HighestWinOmission::Duplicate)
    ));
    most_played.primary_runes[1] = 8140;
    assert!(project(&builds, &most_played).is_ok());
    builds.win.as_mut().unwrap().runes.secondary_runes[1] = 99999;
    assert!(matches!(
        project(&builds, &most_played),
        Err(HighestWinOmission::Unavailable)
    ));
    builds.win = None;
    assert!(matches!(
        project(&builds, &most_played),
        Err(HighestWinOmission::Unavailable)
    ));
}

#[test]
fn formats_patches_from_the_api_version_string() {
    assert_eq!(patch_label("16_19"), "16.19");
    assert_eq!(patch_label(""), "–");
}

#[test]
fn formats_how_long_ago_a_match_was_played() {
    let now = 1_000_000_000_000;
    assert_eq!(played_ago(now, now - 30_000), "just now");
    assert_eq!(played_ago(now, now - 12 * 60_000), "12m ago");
    assert_eq!(played_ago(now, now - 3 * 3_600_000), "3h ago");
    assert_eq!(played_ago(now, now - 2 * 86_400_000), "2d ago");
    // A clock skew in the future must not produce a negative duration.
    assert_eq!(played_ago(now, now + 5_000), "just now");
}

/// A catalog of named items; `finished` and `boots` mark their kind.
fn item_catalog(finished: &[i64], boots: &[i64]) -> items::Catalog {
    let item = |id: i64, finished: bool, boots: bool| items::Item {
        id,
        name: format!("Item {id}"),
        icon_path: "/lol-game-data/assets/x.png".into(),
        from: if finished { vec![1] } else { vec![] },
        categories: if boots { vec!["Boots".into()] } else { vec![] },
        ..Default::default()
    };
    items::Catalog::build(finished.iter().map(|id| item(*id, true, boots.contains(id))).chain(boots.iter().filter(|id| !finished.contains(id)).map(|id| item(*id, false, true))).collect())
}

#[test]
fn pro_games_ended_before_two_finished_items_besides_boots_are_left_out() {
    let mut matched = probuilds::parse(include_str!("../../../tests/fixtures/probuilds-ahri-mid.json")).unwrap().remove(0);
    let catalog = item_catalog(&[2503, 4629], &[3158, 3020]);
    matched.completed_items = vec![3158, 2503, 4629, 3340]; matched.final_build = vec![3158, 2503, 4629, 3340];
    assert!(pro_build_view(&matched, 0, &catalog).is_some(), "two finished items at 20 minutes is a build");
    matched.completed_items = vec![3158, 2503, 3340];
    matched.final_build = vec![3158, 2503, 3020, 3340];
    assert!(pro_build_view(&matched, 0, &catalog).is_none(), "boots, one item and a trinket is a remake, not a build");
}

#[test]
fn pro_builds_add_upgraded_and_quest_items_the_api_leaves_out() {
    // Live Seraphine games: Archangel's became Seraph's, Circlet became Diadem,
    // and the support quest item never appears in `completedItems`.
    let mut matched = probuilds::parse(include_str!("../../../tests/fixtures/probuilds-ahri-mid.json")).unwrap().remove(0);
    matched.completed_items = vec![3158, 3003, 2526, 4005];
    matched.final_build = vec![3158, 3040, 2530, 4005, 3870, 1082, 3364];
    let catalog = recipe_catalog(&[
        (3003, &[], &[3040], false), (3040, &[], &[], false),
        (2526, &[], &[2530], false), (2530, &[], &[], false),
        (3158, &[], &[], true), (4005, &[], &[], false), (3870, &[], &[], false),
    ]);
    assert_eq!(matched.built_items(&catalog), vec![3158, 3040, 2530, 4005, 3870]);
    matched.completed_items = vec![6620, 3158];
    matched.final_build = vec![6620, 3158, 3870, 3070, 2031, 3114, 3364];
    let view = pro_build_view(&matched, 0, &item_catalog(&[6620, 3870], &[3158])).expect("two finished items is a build");
    assert_eq!(view.completed_items.iter().map(|item| item.id).collect::<Vec<_>>(), vec![6620, 3158, 3870]);
}

/// A catalog of `(id, from, to, is_boots)` items, all priced as full items.
fn recipe_catalog(items: &[(i64, &[i64], &[i64], bool)]) -> items::Catalog {
    items::Catalog::build(
        items
            .iter()
            .map(|(id, from, to, boots)| items::Item {
                id: *id,
                name: format!("Item {id}"),
                icon_path: "/lol-game-data/assets/x.png".into(),
                from: from.to_vec(),
                to: to.to_vec(),
                price_total: 3000,
                categories: if *boots { vec!["Boots".into()] } else { vec![] },
            })
            .collect(),
    )
}

#[test]
fn a_sold_item_is_dropped_and_an_upgrade_does_not_take_an_unrelated_slot() {
    // Tristana: Guardian Angel is new, Mercurial was sold; the boots stay.
    let mut matched = probuilds::parse(include_str!("../../../tests/fixtures/probuilds-ahri-mid.json")).unwrap().remove(0);
    let catalog = recipe_catalog(&[
        (3006, &[], &[], true), (3139, &[], &[], false), (3031, &[], &[], false),
        (3094, &[], &[], false), (3026, &[], &[], false),
    ]);
    matched.completed_items = vec![3006, 3139, 3031, 3094];
    matched.final_build = vec![3006, 3031, 3094, 3026, 3364];
    assert_eq!(matched.built_items(&catalog), vec![3006, 3031, 3094, 3026]);
    // Ahri: Sorcerer's Shoes became Spellslinger's; sold Mercury's must not show as second boots.
    let catalog = recipe_catalog(&[
        (3111, &[], &[], true), (3020, &[], &[3175], true), (3175, &[], &[], true), (6655, &[], &[], false),
    ]);
    matched.completed_items = vec![3111, 3020, 6655];
    matched.final_build = vec![3175, 6655, 3364];
    assert_eq!(matched.built_items(&catalog), vec![3175, 6655]);
    // Never more than six items.
    let catalog = recipe_catalog(&(1..=8).map(|id| (id, &[][..], &[][..], false)).collect::<Vec<_>>());
    matched.completed_items = vec![1, 2, 3, 4, 5];
    matched.final_build = (1..=8).collect();
    assert_eq!(matched.built_items(&catalog).len(), 6);
}

