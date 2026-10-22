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
        8112,
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
            8112,
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
