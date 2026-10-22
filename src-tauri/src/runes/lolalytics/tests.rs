use super::*;

const AHRI: &str = include_str!("../../../tests/fixtures/lolalytics-ahri-mid-8112.html");
const JINX: &str = include_str!("../../../tests/fixtures/lolalytics-jinx-adc-8008.html");

#[test]
fn win_later_slots_prefer_support_over_a_small_high_win_sample() {
    let html = r#"<script type="qwik/json">{"objs":[
        {"pick":"1","win":"1"},
        {"items":"2","runes":"3"},
        {"core":{"set":[1,2,3]},
         "item4":[{"id":4,"n":50,"wr":90},{"id":5,"n":1500,"wr":55}],
         "item5":[{"id":6,"n":50,"wr":90},{"id":7,"n":1500,"wr":55}],
         "item6":[{"id":8,"n":50,"wr":90},{"id":9,"n":1500,"wr":55}]},
        {"set":{"pri":[8112,8139,8137,8106],"sec":[8473,8242],"mod":[5008,5008,5011]}}
    ]}</script>"#;
    let builds = parse_builds(html).unwrap();
    let win = &builds.win.as_ref().unwrap().build;
    assert_eq!(win.items, vec![1, 2, 3, 5, 7, 9]);
    assert!(
        win.options.iter().any(|item| item.id == 4),
        "the small sample exceeds the 2% floor"
    );

    let blended = blend_builds(builds.clone(), Some(&builds), blend_info());
    assert_eq!(blended.win.unwrap().build.items, vec![1, 2, 3, 5, 7, 9]);
}

#[test]
fn qwik_requires_both_summary_keys_but_accepts_a_null_win() {
    let html = r#"<script type="qwik/json">{"objs":[
        {"pick":{"items":{"core":{"set":[1,2,3]}}},"win":null}
    ]}</script>"#;
    assert!(parse_qwik(html).is_some());
    assert!(parse_qwik(&html.replace(",\"win\":null", "")).is_none());
}

#[test]
fn both_summaries_keep_their_own_items_runes_spells_and_skills() {
    let ahri = parse_builds(AHRI).unwrap();
    assert_eq!(ahri.pick, parse_build(AHRI).unwrap());
    let win = ahri.win.unwrap();
    assert_eq!(win.build.items, vec![2503, 3020, 3100, 3089, 3157, 3041]);
    assert_eq!(win.build.core_games, 1835);
    assert_eq!(win.build.core_win_pct, Some(56.19));
    assert_eq!(win.runes.keystone, 8112);
    assert_eq!(win.runes.primary_runes, vec![8139, 8137, 8106]);
    assert_eq!(win.runes.secondary_runes, vec![8473, 8242]);
    assert_eq!(win.runes.shards, vec![5008, 5008, 5011]);
    assert_eq!(win.spells, Some([4, 14]));
    assert_eq!(win.build.skill_order.as_deref(), Some("WEQQQRQWQWRWWEE"));
    assert_ne!(win.build.skill_order, ahri.pick.skill_order);

    let jinx = parse_builds(JINX).unwrap();
    assert_eq!(jinx.pick.core_games, 44247);
    let win = jinx.win.unwrap();
    assert_eq!(win.build.items, vec![2523, 3085, 3006, 3031, 3036, 3026]);
    assert_eq!(win.build.core_games, 4790);
    assert_eq!(win.runes.primary_runes, vec![8009, 9103, 8014]);
    assert_eq!(win.runes.shards, vec![5008, 5008, 5001]);
}

#[test]
fn a_missing_or_unusable_win_summary_keeps_the_pick() {
    for win in ["null", r#"{"items":{"core":{"set":[1,2,3]}}}"#] {
        let html = format!(
            r#"<script type="qwik/json">{{"objs":[{{"pick":{{"items":{{"core":{{"set":[4,5,6]}}}}}},"win":{win}}}]}}</script>"#
        );
        let builds = parse_builds(&html).unwrap();
        assert_eq!(builds.pick.items, vec![4, 5, 6]);
        assert!(builds.win.is_none());
    }
}

#[test]
fn win_dedupe_requires_the_same_runes_and_core_set() {
    let mut builds = parse_builds(JINX).unwrap();
    let win = builds.win.as_ref().unwrap();
    let selection = super::super::RuneSelection {
        primary_page_id: 8000,
        secondary_page_id: 8300,
        keystone: win.runes.keystone,
        primary_runes: win.runes.primary_runes.clone(),
        secondary_runes: win.runes.secondary_runes.clone(),
        shards: win.runes.shards.clone(),
    };
    // The fixture cores have the same three items in different orders.
    assert!(win.matches_most_played(&selection, &builds.pick));
    for field in ["keystone", "primary", "secondary", "shards"] {
        let mut other = selection.clone();
        match field {
            "keystone" => other.keystone += 1,
            "primary" => other.primary_runes[0] += 1,
            "secondary" => other.secondary_runes[0] += 1,
            _ => other.shards[0] += 1,
        }
        assert!(!win.matches_most_played(&other, &builds.pick), "{field}");
    }
    builds.pick.items[0] = 9999;
    assert!(!builds
        .win
        .as_ref()
        .unwrap()
        .matches_most_played(&selection, &builds.pick));
}

#[test]
fn both_builds_blend_against_their_own_previous_summary() {
    let now = parse_builds(AHRI).unwrap();
    let mut prev = now.clone();
    prev.pick.core_win_pct = Some(40.0);
    prev.win.as_mut().unwrap().build.core_win_pct = Some(70.0);
    let original_win = now.win.as_ref().unwrap().clone();
    let blended = blend_builds(now.clone(), Some(&prev), blend_info());
    assert_eq!(
        blended.pick,
        blend_build(now.pick, Some(&prev.pick), blend_info())
    );
    let win = blended.win.unwrap();
    assert!(win.build.core_win_pct.unwrap() > original_win.build.core_win_pct.unwrap());
    assert_eq!(win.runes, original_win.runes);
    assert_eq!(win.spells, original_win.spells);
    assert_eq!(win.build.patch, Some(blend_info()));
}

#[test]
fn old_pick_only_cache_entries_remain_readable() {
    let pick = parse_build(AHRI).unwrap();
    let cached: KeystoneBuilds =
        serde_json::from_value(serde_json::to_value(&pick).unwrap()).unwrap();
    assert_eq!(cached.pick, pick);
    assert!(cached.win.is_none());
}

fn no_duplicates(ids: &[i64]) -> bool {
    let mut sorted = ids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.len() == ids.len()
}

#[test]
fn parses_two_real_keystone_pages_into_their_builds() {
    let ahri = parse_build(AHRI).expect("the Ahri page should give a build");
    // Item 5 and Item 6 both have Rabadon's (3089) at the top; the second
    // slot falls through to its next option (Void Staff, 3135).
    assert_eq!(ahri.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
    assert_eq!(ahri.games, 70627);
    assert!(
        no_duplicates(&ahri.items),
        "Ahri build has duplicates: {:?}",
        ahri.items
    );

    let jinx = parse_build(JINX).expect("the Jinx page should give a build");
    assert_eq!(jinx.items, vec![2523, 3006, 3085, 3031, 3036, 3026]);
    assert_eq!(jinx.games, 133854);
    assert!(jinx.games > 0);
    assert!(no_duplicates(&jinx.items));
    assert_ne!(ahri.items, jinx.items);
}

#[test]
fn a_slot_whose_top_option_is_used_falls_through_to_its_next_option() {
    // Item 5 and Item 6 share the same top item (3089); Item 6's next
    // option (3135) must be chosen so the build has no duplicate legendaries.
    let html = r#"<script type="qwik/json">{"objs":[
        {"pick":"1","win":"2"},
        {"items":"3"},
        {"items":"4"},
        {"core":"5","item4":"6","item5":"7","item6":"8","start":"9"},
        {"core":"5"},
        {"set":[3118,3020,4645]},
        [{"id":3157}],
        [{"id":3089}],
        [{"id":3089},{"id":3135}],
        {"n":10}
    ]}</script>"#;
    let build = parse_build(html).expect("the hand-built state should parse");
    assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
    assert!(no_duplicates(&build.items));

    // When a slot has no unused option at all, it is simply left out.
    let html = r#"<script type="qwik/json">{"objs":[
        {"pick":"1","win":"2"},
        {"items":"3"},
        {"items":"4"},
        {"core":"5","item4":"6","item5":"7","item6":"8","start":"9"},
        {"core":"5"},
        {"set":[3118,3020,4645]},
        [{"id":3157}],
        [{"id":3089}],
        [{"id":3089}],
        {"n":10}
    ]}</script>"#;
    let build = parse_build(html).expect("the hand-built state should parse");
    assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089]);
}

#[test]
fn parses_starter_stacks_core_evidence_and_slot_options() {
    let html = r#"<script type="qwik/json">{"objs":[
        {"pick":"1","win":"2"},
        {"items":"3"},
        {"items":"3"},
        {"core":"4","item4":"5","item5":"6","item6":"7","start":"8"},
        {"set":[3118,3020,4645],"n":200,"wr":54.2},
        [{"id":3157,"n":100,"wr":55},{"id":3161,"n":20,"wr":52}],
        [{"id":3089,"n":90,"wr":57},{"id":3124,"n":30,"wr":51}],
        [{"id":3089,"n":80,"wr":58},{"id":3135,"n":45,"wr":53}],
        {"set":[1054,2003,2003],"setUnique":[1054,2003],"count":[1,2],"n":1000}
    ]}</script>"#;
    let build = parse_build(html).expect("the hand-built state should parse");
    assert_eq!(
        build.starters,
        vec![
            ItemStack { id: 1054, count: 1 },
            ItemStack { id: 2003, count: 2 }
        ]
    );
    assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
    assert_eq!(build.games, 1000);
    assert_eq!(build.core_games, 200);
    assert_eq!(build.core_win_pct, Some(54.2));
    assert_eq!(build.options.len(), 2);
    assert!(build
        .options
        .iter()
        .any(|option| option.id == 3161 && option.slot == 4));
    assert!(build
        .options
        .iter()
        .any(|option| option.id == 3124 && option.slot == 5));
}

/// A page with the build summary plus the page-wide per-slot tables and
/// skill data, shaped like lolalytics' Qwik state.
const PAGE_WITH_TABLES: &str = r#"<script type="qwik/json">{"objs":[
        {"pick":"1","win":"2"},
        {"items":"3","skillpriority":"9"},
        {"items":"3"},
        {"core":"4","item4":"5","item5":"6","item6":"7","start":"8"},
        {"set":[3118,3020,4645],"n":200,"wr":54.2},
        [{"id":3157,"n":100,"wr":55}],
        [{"id":3089,"n":90,"wr":57}],
        [{"id":3135,"n":45,"wr":53}],
        {"set":[1056,2003],"setUnique":[1056,2003],"count":[1,1],"n":1000},
        {"id":"QWE","n":800,"wr":52.4},
        {"item3":"b","item4":"c","item5":"d","skillOrder":"e"},
        [[3157,55.7,33.7,1000,31],[3041,79.5,7.2,300,27],[3100,67.9,3.3,150,30],[3102,56.6,1.9,5,29]],
        [[3089,60.8,31.7,600,31],[3135,54.4,8.3,200,35],[3137,58.4,3.8,80,34]],
        [[3165,63.1,3.3,100,40],[3089,62.8,20.8,400,34]],
        [["QWE",52.4,91.6,800],["QEW",52.5,7.8,70]]
    ]}</script>"#;

#[test]
fn reads_the_level_by_level_skill_order_from_a_real_build_page() {
    let order = parse_build(AHRI).unwrap().skill_order.expect("the Ahri page has a skill order");
    assert_eq!(order.len(), 15);
    assert!(order.chars().all(|c| "QWER".contains(c)), "{order}");
}

#[test]
fn reads_the_skill_priority_from_the_build_summary() {
    let build = parse_build(PAGE_WITH_TABLES).expect("the page should parse");
    assert_eq!(build.skill_priority.as_deref(), Some("QWE"));
}

#[test]
fn an_unreadable_skill_priority_is_left_out() {
    let html = PAGE_WITH_TABLES.replace(r#""id":"QWE""#, r#""id":"QXZ""#);
    let build = parse_build(&html).expect("the page should parse");
    // Falls back to the page-wide skill order.
    assert_eq!(build.skill_priority.as_deref(), Some("QWE"));
    let html = html.replace(r#"["QWE",52.4"#, r#"["Q",52.4"#);
    assert_eq!(parse_build(&html).unwrap().skill_priority, None);
}

#[test]
fn the_full_slot_tables_widen_the_options() {
    let build = parse_build(PAGE_WITH_TABLES).expect("the page should parse");
    assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
    let ids: Vec<i64> = build.options.iter().map(|option| option.id).collect();
    // Mejai's, Lich Bane, Morellonomicon, Void Staff… but never an item
    // already in the build, and not Banshee's with 5 of 1455 games.
    for expected in [3041, 3100, 3137, 3165] {
        assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
    }
    assert!(!ids.contains(&3102), "rare pick kept: {ids:?}");
    for built in &build.items {
        assert!(!ids.contains(built), "build item {built} in options");
    }
    let void_staff = build
        .options
        .iter()
        .find(|option| option.id == 3165)
        .unwrap();
    assert_eq!((void_staff.slot, void_staff.games), (6, 100));
}

#[test]
fn only_the_core_is_a_joint_sample_and_a_tiny_slot_pick_is_never_recommended() {
    // Slot 6's only unused item has 5 of 905 games (0.6%) but a 90% win
    // rate: it must neither join the build nor the options.
    let html = r#"<script type="qwik/json">{"objs":[
        {"pick":"1","win":"2"},
        {"items":"3"},
        {"items":"3"},
        {"core":"4","item4":"5","item5":"6","item6":"7","start":"8"},
        {"set":[3118,3020,4645],"n":200,"wr":54.2},
        [{"id":3157,"n":100,"wr":55}],
        [{"id":3089,"n":90,"wr":57}],
        [{"id":3089,"n":900,"wr":58},{"id":3135,"n":5,"wr":90}],
        {"set":[1056],"setUnique":[1056],"count":[1],"n":1000}
    ]}</script>"#;
    let build = parse_build(html).expect("the hand-built state should parse");
    assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089]);
    assert_eq!(build.core_len, 3);
    assert!(build.options.iter().all(|option| option.id != 3135));
}

#[test]
fn the_markup_fallback_reports_no_joint_core() {
    assert_eq!(parse_build(AHRI).unwrap().core_len, 3);
    let anchors = r#"Core Build <img src="x/item64/3118.webp"> Item 4 <img src="x/item64/3157.webp"> Item 5 <img src="x/item64/3089.webp"> Item 6 <img src="x/item64/3135.webp">"#;
    assert_eq!(parse_build(anchors).unwrap().core_len, 0);
}

#[test]
fn a_page_without_the_anchors_or_state_gives_no_build() {
    assert!(parse_build("<html><body>Just a moment…</body></html>").is_none());
    assert!(parse_build("").is_none());
    // A Qwik payload that is not valid JSON must not panic.
    assert!(parse_build(r#"<script type="qwik/json">{not json</script>"#).is_none());
}

#[test]
fn resolves_base36_references_and_reads_the_most_common_build() {
    // A hand-built but well-formed Qwik state: objs[0] is the pick/win
    // summary, and every index is base-36 (a = 10). The `pick` branch is
    // the most common build.
    let html = r#"<script type="qwik/json">{"objs":[
        {"pick":"1","win":"2"},
        {"items":"3"},
        {"items":"4"},
        {"core":"5","item4":"6","item5":"7","item6":"8","start":"9"},
        {"core":"5","item4":"a"},
        {"set":[3118,3020,4645]},
        [{"id":3157}],
        [{"id":3089}],
        [{"id":3089},{"id":3135}],
        {"n":70627},
        [{"id":100}]
    ]}</script>"#;
    let build = parse_build(html).expect("the hand-built state should parse");
    assert_eq!(build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
    assert_eq!(build.games, 70627);
}

#[test]
fn maps_champion_names_to_lolalytics_slugs() {
    assert_eq!(champion_slug("Ahri").as_deref(), Some("ahri"));
    assert_eq!(champion_slug("Lee Sin").as_deref(), Some("leesin"));
    assert_eq!(champion_slug("Kai'Sa").as_deref(), Some("kaisa"));
    assert_eq!(champion_slug("Dr. Mundo").as_deref(), Some("drmundo"));
    assert_eq!(champion_slug("Cho'Gath").as_deref(), Some("chogath"));
    assert_eq!(champion_slug("Jarvan IV").as_deref(), Some("jarvaniv"));
    assert_eq!(champion_slug("Kog'Maw").as_deref(), Some("kogmaw"));
    assert_eq!(champion_slug("LeBlanc").as_deref(), Some("leblanc"));
    assert_eq!(champion_slug("Rek'Sai").as_deref(), Some("reksai"));
    assert_eq!(champion_slug("Tahm Kench").as_deref(), Some("tahmkench"));
    assert_eq!(
        champion_slug("Twisted Fate").as_deref(),
        Some("twistedfate")
    );
    assert_eq!(champion_slug("Xin Zhao").as_deref(), Some("xinzhao"));
    assert_eq!(
        champion_slug("Aurelion Sol").as_deref(),
        Some("aurelionsol")
    );
    assert_eq!(champion_slug("Master Yi").as_deref(), Some("masteryi"));
    assert_eq!(
        champion_slug("Miss Fortune").as_deref(),
        Some("missfortune")
    );
    assert_eq!(champion_slug("Bel'Veth").as_deref(), Some("belveth"));
    assert_eq!(champion_slug("K'Sante").as_deref(), Some("ksante"));
    // Site-specific shorter slugs.
    assert_eq!(champion_slug("Wukong").as_deref(), Some("wukong"));
    assert_eq!(champion_slug("MonkeyKing").as_deref(), Some("wukong"));
    assert_eq!(champion_slug("Nunu & Willump").as_deref(), Some("nunu"));
    assert_eq!(champion_slug("Renata Glasc").as_deref(), Some("renata"));
    assert_eq!(champion_slug(""), None);
    assert_eq!(champion_slug("  "), None);
}

#[test]
fn maps_positions_to_lolalytics_lanes() {
    assert_eq!(lane("top"), Some("top"));
    assert_eq!(lane("JUNGLE"), Some("jungle"));
    assert_eq!(lane("mid"), Some("middle"));
    assert_eq!(lane("middle"), Some("middle"));
    assert_eq!(lane("adc"), Some("bottom"));
    assert_eq!(lane("support"), Some("support"));
    assert_eq!(lane("none"), None);
    assert_eq!(lane("aram"), None);
    assert_eq!(lane(""), None);
}

#[test]
fn maps_rank_brackets_and_defaults_unknown_ones() {
    assert_eq!(tier_slug("all"), "all");
    assert_eq!(tier_slug("EMERALD_PLUS"), "emerald_plus");
    assert_eq!(tier_slug("diamond_plus"), "diamond_plus");
    assert_eq!(tier_slug("grandmaster_plus"), DEFAULT_TIER);
    assert_eq!(tier_slug(""), DEFAULT_TIER);
}

#[test]
fn the_cache_key_carries_the_keystone_and_bracket() {
    let base = cache_key("ahri", "middle", "emerald_plus", 8112);
    assert_eq!(base, "ahri|middle|emerald_plus|8112");
    assert_ne!(base, cache_key("ahri", "middle", "emerald_plus", 8229));
    assert_ne!(base, cache_key("ahri", "middle", "diamond_plus", 8112));
    assert_ne!(base, cache_key("ahri", "top", "emerald_plus", 8112));
    // An unknown bracket is normalized into the key.
    assert_eq!(
        cache_key("ahri", "middle", "nonsense", 8112),
        cache_key("ahri", "middle", DEFAULT_TIER, 8112)
    );
}

#[test]
fn resolving_shared_references_stays_within_the_node_budget() {
    // Each level references the previous one twice: 2^30 leaves unbudgeted.
    let mut objs = vec![serde_json::json!(1)];
    for level in 1..=30usize {
        objs.push(serde_json::json!([base36(level - 1), base36(level - 1)]));
    }
    let resolved = resolve(&Value::String(base36(30)), &objs, 0);
    // Terminates, and the budget cut leaves Nulls rather than a huge tree.
    assert!(resolved.is_array());
}

fn base36(mut number: usize) -> String {
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = vec![digits[number % 36]];
    while number >= 36 {
        number /= 36;
        out.push(digits[number % 36]);
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

#[test]
fn builds_the_endpoint_url_with_the_keystone() {
    assert_eq!(
        url("ahri", "middle", "emerald_plus", 8112, None),
        "https://lolalytics.com/lol/ahri/build/?lane=middle&tier=emerald_plus&keystone=8112"
    );
    // An unknown bracket is normalized in the URL too.
    assert_eq!(
        url("leesin", "jungle", "bogus", 8010, None),
        "https://lolalytics.com/lol/leesin/build/?lane=jungle&tier=emerald_plus&keystone=8010"
    );
}

fn blend_info() -> PatchInfo {
    PatchInfo { patch: "16.20".into(), previous: Some("16.19".into()) }
}

fn with_games(build: &KeystoneBuild, games: u64) -> KeystoneBuild {
    KeystoneBuild {
        // Reversed: the current patch has not settled on an order yet.
        slots: build.slots.iter().rev().map(|c| ItemCandidate { games, ..c.clone() }).collect(),
        ..build.clone()
    }
}

#[test]
fn the_real_page_keeps_every_slot_candidate() {
    let build = parse_build(AHRI).unwrap();
    assert!(build.slots.len() >= build.options.len(), "slots hold options and chosen items");
    for slot in [4u8, 5, 6] {
        assert!(build.slots.iter().any(|c| c.slot == slot), "slot {slot} is empty");
    }
}

#[test]
fn a_day_zero_build_is_the_previous_patchs_path_with_current_game_counts() {
    let prev = parse_build(AHRI).unwrap();
    let now = with_games(&prev, 0);
    let blended = blend_build(KeystoneBuild { items: now.items[..now.core_len].to_vec(), ..now }, Some(&prev), blend_info());
    assert_eq!(blended.items, prev.items);
    assert!(blended.options.iter().all(|option| option.games == 0));
    assert!(blended.patch.unwrap().previous.is_some());
}

#[test]
fn a_current_only_item_can_take_a_slot_and_a_big_sample_keeps_its_rate() {
    let prev = parse_build(AHRI).unwrap();
    let mut now = with_games(&prev, 4000);
    now.slots.push(ItemCandidate { id: 99_999, slot: 4, games: 9000, win_pct: Some(61.0) });
    let blended = blend_build(now, Some(&prev), blend_info());
    assert_eq!(blended.items[prev.core_len], 99_999, "the new item is the strongest slot-4 pick");
    let shown = blended.options.iter().find(|option| option.games == 4000);
    assert!(shown.is_some(), "options show current games, never the weighted ones");
}

#[test]
fn the_build_url_pins_the_previous_patch() {
    assert!(url("ahri", "middle", "emerald_plus", 8112, Some("16.19")).ends_with("&keystone=8112&patch=16.19"));
    assert!(!url("ahri", "middle", "emerald_plus", 8112, None).contains("patch"));
}
