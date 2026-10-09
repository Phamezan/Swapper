//! Lane matchup builds from lolalytics' `/vs/` pages.
//!
//! A matchup page has the same Qwik state as the generic build page (see
//! [`super::lolalytics`]) filtered to games against one enemy champion, plus a
//! page object with that matchup's win rate and sample size. The build, rune
//! page, spells and skill order are read from the same "Most Common Build"
//! summary the generic parser uses.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::lolalytics::{self, as_i64, as_u64, number, resolve, KeystoneBuild, Qwik};
use super::perks::PerkCatalog;
use super::provider::ProviderError;
use super::RuneSelection;

/// Below this many games the matchup is too noisy; the generic build is shown.
pub const MIN_MATCHUP_GAMES: u64 = 100;
/// Auto-apply only trusts a matchup build with at least this many games.
pub const AUTO_APPLY_MATCHUP_GAMES: u64 = 500;
/// A skill order id holds one digit per level, 1-15.
const SKILL_ORDER_LEVELS: usize = 15;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MatchupStats {
    pub win_pct: f64,
    pub avg_win_pct: f64,
    pub games: u64,
    pub patch: String,
}

impl MatchupStats {
    /// Matchup win rate minus the champion's average win rate.
    pub fn delta(&self) -> f64 {
        self.win_pct - self.avg_win_pct
    }

    /// Whether the sample is large enough to offer the matchup build.
    pub fn is_reliable(&self) -> bool {
        self.games >= MIN_MATCHUP_GAMES
    }
}

/// The matchup's most common rune page, in the order lolalytics lists it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MatchupRunes {
    pub keystone: i64,
    pub primary_runes: Vec<i64>,
    pub secondary_runes: Vec<i64>,
    pub shards: Vec<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Matchup {
    pub stats: MatchupStats,
    pub build: KeystoneBuild,
    pub runes: Option<MatchupRunes>,
    pub spells: Option<[i64; 2]>,
    /// Ability order for levels 1-15, e.g. "WQEQQRQWQWRWWEE".
    pub skill_order: Option<String>,
}

impl Matchup {
    /// The rune page as a selection, with tree ids looked up in the catalog.
    /// `None` when the page is missing or its runes are not in the catalog.
    pub fn selection(&self, catalog: &PerkCatalog) -> Option<RuneSelection> {
        let runes = self.runes.as_ref()?;
        let primary_page_id = tree_of(catalog, runes.keystone)?;
        let secondary_page_id = tree_of(catalog, *runes.secondary_runes.first()?)?;
        if primary_page_id == secondary_page_id {
            return None;
        }
        Some(RuneSelection {
            primary_page_id,
            secondary_page_id,
            keystone: runes.keystone,
            primary_runes: runes.primary_runes.clone(),
            secondary_runes: runes.secondary_runes.clone(),
            shards: runes.shards.clone(),
        })
    }
}

/// The tree a rune belongs to.
fn tree_of(catalog: &PerkCatalog, rune: i64) -> Option<i64> {
    catalog
        .trees()
        .find(|style| style.slots.iter().any(|slot| slot.perks.contains(&rune)))
        .map(|style| style.id)
}

/// Parses a matchup page. `None` when the page has no build, or its matchup
/// stats are missing or are for a different champion or enemy than asked.
pub fn parse_matchup(html: &str, champion_id: i64, enemy_id: i64) -> Option<Matchup> {
    let build = lolalytics::parse_build(html)?;
    let state: Qwik = serde_json::from_str(lolalytics::qwik_json(html)?).ok()?;
    let objs = &state.objs;
    let stats = page_stats(objs, champion_id, enemy_id)?;
    let details = pick_details(objs);
    Some(Matchup {
        stats,
        build,
        runes: details.runes,
        spells: details.spells,
        skill_order: details.skill_order,
    })
}

/// The rune page, spells and skill order of the "Most Common Build" summary.
#[derive(Debug, Default)]
pub(super) struct PickDetails {
    pub runes: Option<MatchupRunes>,
    pub spells: Option<[i64; 2]>,
    pub skill_order: Option<String>,
}

pub(super) fn pick_details(objs: &[Value]) -> PickDetails {
    let Some(pick) = pick_summary(objs) else {
        return PickDetails::default();
    };
    PickDetails {
        runes: parse_runes(&pick),
        spells: parse_spells(&pick),
        skill_order: parse_skill_order(&pick),
    }
}

/// The page object that carries the matchup win rate, sample and patch, for
/// this champion against this enemy.
fn page_stats(objs: &[Value], champion_id: i64, enemy_id: i64) -> Option<MatchupStats> {
    let required = ["cid", "vs", "wr", "avgWr", "n", "patch"];
    objs.iter().find_map(|entry| {
        let map = entry.as_object()?;
        if !required.iter().all(|key| map.contains_key(*key)) {
            return None;
        }
        let page = resolve(entry, objs, 0);
        if page.get("cid").and_then(as_i64)? != champion_id
            || page.get("vs").and_then(as_i64)? != enemy_id
        {
            return None;
        }
        Some(MatchupStats {
            win_pct: page.get("wr").and_then(number)?,
            avg_win_pct: page.get("avgWr").and_then(number)?,
            games: page.get("n").and_then(as_u64)?,
            patch: page.get("patch")?.as_str()?.to_string(),
        })
    })
}

/// The resolved "Most Common Build" summary.
fn pick_summary(objs: &[Value]) -> Option<Value> {
    objs.iter().find_map(|entry| {
        let map = entry.as_object()?;
        if map.len() > 3 || !map.contains_key("win") {
            return None;
        }
        Some(resolve(map.get("pick")?, objs, 0))
    })
}

fn id_list(value: &Value, len: usize) -> Option<Vec<i64>> {
    let ids: Vec<i64> = value
        .as_array()?
        .iter()
        .filter_map(as_i64)
        .filter(|id| *id > 0)
        .collect();
    (ids.len() == len).then_some(ids)
}

fn parse_runes(pick: &Value) -> Option<MatchupRunes> {
    let set = pick.get("runes")?.get("set")?;
    let primary = id_list(set.get("pri")?, 4)?;
    Some(MatchupRunes {
        keystone: primary[0],
        primary_runes: primary[1..].to_vec(),
        secondary_runes: id_list(set.get("sec")?, 2)?,
        shards: id_list(set.get("mod")?, 3)?,
    })
}

fn parse_spells(pick: &Value) -> Option<[i64; 2]> {
    let ids = id_list(pick.get("sums")?.get("ids")?, 2)?;
    Some([ids[0], ids[1]])
}

/// The level 1-15 order from a digit id (`1`=Q, `2`=W, `3`=E, `4`=R).
fn parse_skill_order(pick: &Value) -> Option<String> {
    let id = pick.get("skillorder")?.get("id")?;
    let digits = id
        .as_u64()
        .map(|number| number.to_string())
        .or_else(|| id.as_str().map(str::to_string))?;
    if digits.len() != SKILL_ORDER_LEVELS {
        return None;
    }
    digits
        .chars()
        .map(|digit| match digit {
            '1' => Some('Q'),
            '2' => Some('W'),
            '3' => Some('E'),
            '4' => Some('R'),
            _ => None,
        })
        .collect()
}

impl lolalytics::LolalyticsClient {
    /// Fetches and parses one matchup page. `Ok(None)` means the page loaded
    /// but carried no matchup for that enemy.
    pub async fn matchup(
        &self,
        champion_slug: &str,
        enemy_slug: &str,
        champion_id: i64,
        enemy_id: i64,
        lane: &str,
        tier: &str,
    ) -> Result<Option<Matchup>, ProviderError> {
        let body = self
            .page(lolalytics::vs_url(champion_slug, enemy_slug, lane, tier))
            .await?;
        Ok(parse_matchup(&body, champion_id, enemy_id))
    }
}

#[cfg(test)]
mod tests {
    use super::super::perks::{PerkStyleAsset, PerkStyleSlot};
    use super::*;
    use std::collections::HashMap;

    /// A trimmed matchup page: Ahri vs Zed (id 238), numbers from a real page.
    /// Index 14 is the page object; 15-19 are its literal values.
    const AHRI_VS_ZED: &str = r#"<script type="qwik/json">{"objs":[
        {"pick":"1","win":"2"},
        {"items":"3","skillpriority":"4","skillorder":"5","sums":"6","runes":"7"},
        {"items":"3"},
        {"core":"8","item4":"9","item5":"a","item6":"b","start":"c"},
        {"id":"QWE","n":239,"wr":53.97},
        {"id":213114121242233,"n":106,"wr":60.38},
        {"ids":[4,14],"n":190,"wr":56.32},
        {"wr":51.69,"n":236,"set":"d"},
        {"set":[3118,3020,4645],"n":200,"wr":54.2},
        [{"id":3157,"n":100,"wr":55}],
        [{"id":3089,"n":90,"wr":57}],
        [{"id":3135,"n":45,"wr":53}],
        {"set":[1056,2003],"setUnique":[1056,2003],"count":[1,1],"n":1000},
        {"pri":[8112,8139,8140,8106],"sec":[8226,8210],"mod":[5005,5008,5001]},
        {"cid":103,"patch":"f","vs":"g","wr":"h","avgWr":"i","n":"j"},
        "16.20",
        238,
        52.59,
        51.88,
        270
    ]}</script>"#;

    #[test]
    fn parses_stats_build_runes_spells_and_skill_order() {
        let matchup = parse_matchup(AHRI_VS_ZED, 103, 238).expect("the matchup page should parse");
        assert_eq!(
            matchup.stats,
            MatchupStats {
                win_pct: 52.59,
                avg_win_pct: 51.88,
                games: 270,
                patch: "16.20".into(),
            }
        );
        assert!((matchup.stats.delta() - 0.71).abs() < 1e-9);
        assert_eq!(matchup.build.items, vec![3118, 3020, 4645, 3157, 3089, 3135]);
        assert_eq!(matchup.build.skill_priority.as_deref(), Some("QWE"));
        assert_eq!(matchup.spells, Some([4, 14]));
        assert_eq!(
            matchup.runes,
            Some(MatchupRunes {
                keystone: 8112,
                primary_runes: vec![8139, 8140, 8106],
                secondary_runes: vec![8226, 8210],
                shards: vec![5005, 5008, 5001],
            })
        );
        // R at levels 6 and 11, as in the game.
        assert_eq!(matchup.skill_order.as_deref(), Some("WQEQQRQWQWRWWEE"));
    }

    #[test]
    fn a_page_for_another_champion_is_rejected() {
        assert!(parse_matchup(AHRI_VS_ZED, 266, 238).is_none());
    }

    #[test]
    fn a_page_for_another_enemy_is_rejected() {
        assert!(parse_matchup(AHRI_VS_ZED, 103, 266).is_none());
    }

    #[test]
    fn a_page_without_matchup_stats_gives_no_matchup() {
        let html = AHRI_VS_ZED.replace(r#""vs":"g","#, "");
        assert!(parse_matchup(&html, 103, 238).is_none());
        assert!(parse_matchup("<html>Just a moment…</html>", 103, 238).is_none());
    }

    #[test]
    fn missing_extras_do_not_drop_the_build() {
        let html = AHRI_VS_ZED
            .replace(r#","sums":"6","runes":"7""#, "")
            .replace("213114121242233", "2131");
        let matchup = parse_matchup(&html, 103, 238).expect("the build is still there");
        assert_eq!(matchup.runes, None);
        assert_eq!(matchup.spells, None);
        assert_eq!(matchup.skill_order, None);
    }

    #[test]
    fn sample_thresholds_gate_the_matchup() {
        let stats = |games| MatchupStats {
            games,
            ..MatchupStats::default()
        };
        assert!(!stats(MIN_MATCHUP_GAMES - 1).is_reliable());
        assert!(stats(MIN_MATCHUP_GAMES).is_reliable());
    }

    fn style(id: i64, perks: &[i64]) -> PerkStyleAsset {
        PerkStyleAsset {
            id,
            name: String::new(),
            icon_path: String::new(),
            slots: vec![PerkStyleSlot {
                kind: "kMixedRegularSplashable".into(),
                label: String::new(),
                perks: perks.to_vec(),
            }],
        }
    }

    #[test]
    fn the_rune_page_becomes_a_selection_with_tree_ids() {
        let catalog = PerkCatalog::new(
            HashMap::new(),
            vec![style(8100, &[8112, 8139, 8140, 8106]), style(8200, &[8226, 8210])],
        );
        let matchup = parse_matchup(AHRI_VS_ZED, 103, 238).unwrap();
        let selection = matchup.selection(&catalog).expect("both trees are known");
        assert_eq!((selection.primary_page_id, selection.secondary_page_id), (8100, 8200));
        assert_eq!(selection.keystone, 8112);
        let unknown = PerkCatalog::new(HashMap::new(), Vec::new());
        assert!(matchup.selection(&unknown).is_none());
    }

    #[test]
    fn matchup_urls_and_keys_carry_the_enemy_and_lane() {
        assert_eq!(
            lolalytics::vs_url("ahri", "zed", "middle", "emerald_plus"),
            "https://lolalytics.com/lol/ahri/vs/zed/build/?lane=middle&vslane=middle&tier=emerald_plus"
        );
        let key = lolalytics::vs_cache_key("ahri", "zed", "middle", "emerald_plus");
        assert_ne!(key, lolalytics::vs_cache_key("ahri", "yasuo", "middle", "emerald_plus"));
        assert_ne!(key, lolalytics::cache_key("ahri", "middle", "emerald_plus", 8112));
    }
}
