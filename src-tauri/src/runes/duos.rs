//! Bot lane duos (ADC + Support) from u.gg's duo tier list.
//!
//! `stats2.u.gg/lol/1.5/duos/world/{16_20}/ranked_solo_5x5/{rank}/1.5.0.json`
//! is `[{"adc_supp": [row, ...], ...}, updated_at, ...]`, each row
//! `[adc_id, role, adc_wins, adc_games, support_id, role, support_wins,
//! support_games, duo_wins, duo_games, ...]`. u.gg already drops small pairs
//! (a few thousand games at Emerald+). Pairs are blended with the previous
//! patch and scored like the lane tier list (shrunk win rate + popularity).
//! If u.gg is unavailable, lolalytics' support synergy tables supply the pairs.

use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::cache::{self, LastGoodCache};
use super::health::{self, Source};
use super::patch::{self, blend_rate};
use super::provider::{DataKind, Provider, Sourced};
use super::tierlist::{percentile_tiers, scores};

const TTL: Duration = Duration::from_secs(60 * 60);
const LAST_GOOD_MAX_ENTRIES: usize = 16;
const LAST_GOOD_MAX_BYTES: usize = 512 * 1024;
/// Pick-rate weight in a duo's score. Lower than the lane tier list's: a duo
/// list is read for the pairs that win, and u.gg already drops rare ones.
const POP_WEIGHT: f64 = 0.25;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Duo {
    pub adc_id: i64,
    pub support_id: i64,
    pub win_pct: f64,
    /// The current patch's games together (0 for a previous-patch-only pair).
    pub games: u64,
    /// Our tier index (0 = S+, 5 = D).
    pub tier: u8,
    pub score: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DuoList {
    /// Best first.
    pub duos: Vec<Duo>,
}

/// The unscored pairs of one duo file, `None` when it has no `adc_supp` table.
#[cfg(test)]
fn parse(body: &str) -> Option<DuoList> {
    let root: Value = serde_json::from_str(body).ok()?;
    parse_value(&root)
}

pub(super) fn parse_value(root: &Value) -> Option<DuoList> {
    let rows = root.get(0)?.get("adc_supp")?.as_array()?;
    let duos: Vec<Duo> = rows
        .iter()
        .filter_map(|row| {
            let at = |index: usize| row.get(index).and_then(Value::as_u64);
            let games = at(9).filter(|games| *games > 0)?;
            let wins = at(8).filter(|wins| *wins <= games)?;
            let adc_id = at(0).filter(|id| *id > 0)? as i64;
            let support_id = at(4).filter(|id| *id > 0)? as i64;
            if at(1)? != 3 || at(5)? != 2 {
                return None;
            }
            Some(Duo {
                adc_id,
                support_id,
                win_pct: 100.0 * wins as f64 / games as f64,
                games,
                ..Duo::default()
            })
        })
        .collect();
    (!duos.is_empty()).then_some(DuoList { duos })
}

/// Blends each pair's win rate with the previous patch's (games stay the
/// current patch's), adds pairs only the previous patch has, then scores and
/// sorts best-first. Pick rate is each pair's share of the listed games.
pub(super) fn blend(now: DuoList, prev: Option<&DuoList>) -> DuoList {
    let same = |a: &Duo, b: &Duo| a.adc_id == b.adc_id && a.support_id == b.support_id;
    let mut duos: Vec<Duo> = now
        .duos
        .iter()
        .map(
            |duo| match prev.and_then(|prev| prev.duos.iter().find(|old| same(old, duo))) {
                Some(old) => Duo {
                    win_pct: blend_rate(duo.win_pct, duo.games, old.win_pct),
                    ..duo.clone()
                },
                None => duo.clone(),
            },
        )
        .collect();
    let prev_only: Vec<Duo> = prev
        .map(|prev| {
            prev.duos
                .iter()
                .filter(|old| !now.duos.iter().any(|duo| same(duo, old)))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    // Previous-patch pairs keep their old games for popularity only, scaled
    // to the current list's size so they neither dominate nor vanish.
    let now_total: u64 = now.duos.iter().map(|duo| duo.games).sum();
    let prev_total: u64 = prev
        .map(|prev| prev.duos.iter().map(|duo| duo.games).sum())
        .unwrap_or(0);
    let scale = if now_total == 0 || prev_total == 0 {
        1.0
    } else {
        now_total as f64 / prev_total as f64
    };
    let mut weights: Vec<f64> = duos.iter().map(|duo| duo.games as f64).collect();
    weights.extend(prev_only.iter().map(|old| old.games as f64 * scale));
    duos.extend(prev_only.into_iter().map(|old| Duo { games: 0, ..old }));

    let total: f64 = weights.iter().sum::<f64>().max(1.0);
    let stats: Vec<(f64, f64, u64)> = duos
        .iter()
        .zip(&weights)
        .map(|(duo, weight)| (duo.win_pct, 100.0 * weight / total, *weight as u64))
        .collect();
    for (duo, score) in duos.iter_mut().zip(scores(&stats, POP_WEIGHT)) {
        duo.score = score;
    }
    duos.sort_by(|left, right| right.score.total_cmp(&left.score));
    label_duos(&mut duos);
    DuoList { duos }
}

fn label_duos(duos: &mut [Duo]) {
    let tiers = percentile_tiers(duos.iter().map(|duo| duo.score));
    for (duo, tier) in duos.iter_mut().zip(tiers) {
        duo.tier = tier;
    }
}

static LAST_GOOD: OnceLock<LastGoodCache<DuoList>> = OnceLock::new();

fn last_good() -> &'static LastGoodCache<DuoList> {
    LAST_GOOD.get_or_init(|| {
        LastGoodCache::open(
            cache::cache_path(DataKind::Duos),
            TTL,
            LAST_GOOD_MAX_ENTRIES,
            LAST_GOOD_MAX_BYTES,
        )
    })
}

/// The bot lane duo list for a rank bracket. A fresh cached copy is served as
/// is; a failed fetch falls back to the last good copy, marked stale. On the
/// first day of a patch u.gg may not have the new file yet, so the previous
/// patch alone is shown.
pub async fn load(tier: &str) -> Option<Sourced<DuoList>> {
    let mut found = super::ugg::prefer(super::ugg::duos(tier).await, async {
        Ok(load_lolalytics(tier).await)
    })
    .await
    .ok()
    .flatten()?;
    label_duos(&mut found.value.duos);
    Some(found)
}

async fn load_lolalytics(tier: &str) -> Option<Sourced<DuoList>> {
    let key = format!("lol-duos-v1|{}", super::lolalytics::tier_slug(tier));
    let gate = super::data::keystone_gate(&key);
    let _guard = gate.lock().await;
    let cached = last_good().lookup(&key);
    if let Some(hit) = cached.as_ref().filter(|hit| hit.fresh) {
        return Some(Sourced {
            value: hit.value.clone(),
            provider: Provider::Lolalytics,
            stale: false,
            fetched_at: Some(hit.fetched_at),
        });
    }
    let stale = || {
        cached
            .clone()
            .map(|hit| Sourced::stale(hit.value, Provider::Lolalytics, hit.fetched_at))
    };
    static FAILURES: OnceLock<std::sync::Mutex<super::lolalytics::BuildCache<()>>> =
        OnceLock::new();
    let failures =
        FAILURES.get_or_init(|| std::sync::Mutex::new(super::lolalytics::BuildCache::default()));
    if matches!(
        failures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .lookup(&key),
        super::lolalytics::Cached::Unavailable
    ) {
        return stale();
    }
    let Some(patches) = patch::patches().await else {
        return stale();
    };
    let now = fetch_lolalytics(&patches.current, tier).await;
    let prev = match &patches.previous {
        Some(previous) => {
            patch::prior(last_good(), &patch::prior_key(&key, previous), async {
                Ok(fetch_lolalytics(previous, tier).await)
            })
            .await
        }
        None => None,
    };
    if now.is_none() && prev.is_none() {
        failures.lock().unwrap_or_else(|e| e.into_inner()).fail(key);
        return stale();
    }
    let found = blend(now.unwrap_or_default(), prev.as_ref());
    last_good().store(key, found.clone());
    Some(Sourced::fresh(found, Provider::Lolalytics))
}

/// Lolalytics' Synergy tab loads this table from its build-team dataset.
/// Each support row is [id, win %, delta, normalized delta, pick %, games].
pub(super) fn parse_lolalytics(root: &Value, adc_id: i64) -> Option<Vec<Duo>> {
    let rows = root.get("team")?.get("support")?.as_array()?;
    let rows: Vec<Duo> = rows
        .iter()
        .filter_map(|row| {
            let support_id = row.get(0)?.as_i64().filter(|id| *id > 0)?;
            let win_pct = row
                .get(1)?
                .as_f64()
                .filter(|n| n.is_finite() && (0.0..=100.0).contains(n))?;
            let games = row.get(5)?.as_u64().filter(|n| *n >= 100)?;
            Some(Duo {
                adc_id,
                support_id,
                win_pct,
                games,
                ..Duo::default()
            })
        })
        .collect();
    (!rows.is_empty()).then_some(rows)
}

async fn fetch_lolalytics(patch: &str, tier: &str) -> Option<DuoList> {
    use futures_util::{stream, StreamExt};
    let names = super::data::champion_names().await.ok()?;
    let client = super::lolalytics::LolalyticsClient::new().ok()?;
    let list = client.tierlist("bottom", tier, Some(patch)).await.ok()??;
    let slugs: Vec<_> = list
        .rows
        .iter()
        .filter(|row| row.games >= super::tierlist::MIN_TIER_GAMES)
        .filter_map(|row| {
            Some((
                row.champion_id,
                super::lolalytics::champion_slug(names.get(&row.champion_id)?)?,
            ))
        })
        .collect();
    let duos: Vec<Duo> = stream::iter(slugs).map(|(id, slug)| {
        let client = &client;
        async move {
            let _permit = super::data::keystone_fetches().acquire().await.ok()?;
            let body = client.page(format!("https://a1.lolalytics.com/mega/?ep=build-team&v=1&patch={patch}&c={slug}&lane=bottom&tier={}&queue=ranked&region=all", super::lolalytics::tier_slug(tier))).await.ok()?;
            let rows = serde_json::from_str::<Value>(&body).ok().and_then(|root| parse_lolalytics(&root, id));
            match rows {
                Some(_) => health::ok(Source::Lolalytics),
                None => health::format_changed(Source::Lolalytics, "build-team synergy"),
            }
            rows
        }
    }).buffer_unordered(4).filter_map(|rows| async { rows }).collect::<Vec<_>>().await.into_iter().flatten().collect();
    (!duos.is_empty()).then_some(DuoList { duos })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[{"adc_supp":[
        [222,3,102124,195868,201,2,34536,66177,4226,7539,146968,null,null],
        [51,3,1000,2000,412,2,1000,2000,6855,13314,1,null,null],
        [18,3,1000,2000,555,2,1000,2000,0,0,1,null,null]
    ],"jungle_supp":[]},"2026-10-13T17:47:47Z",1.0,1]"#;

    fn duo(adc_id: i64, support_id: i64, win_pct: f64, games: u64) -> Duo {
        Duo {
            adc_id,
            support_id,
            win_pct,
            games,
            ..Duo::default()
        }
    }

    #[test]
    fn parses_the_adc_support_pairs_and_skips_empty_ones() {
        let list = parse(SAMPLE).unwrap();
        assert_eq!(list.duos.len(), 2);
        assert_eq!(
            (
                list.duos[0].adc_id,
                list.duos[0].support_id,
                list.duos[0].games
            ),
            (222, 201, 7539)
        );
        assert!((list.duos[0].win_pct - 56.055).abs() < 0.01);
        assert!(parse("<html>").is_none());
    }

    #[test]
    fn duos_receive_percentile_tiers_after_sorting_and_cached_labels_can_be_refreshed() {
        let list = blend(DuoList {duos:(1..=45).map(|id|duo(id,200,40.0+id as f64*0.2,1000)).collect()},None);
        assert_eq!(list.duos.iter().map(|duo|duo.adc_id).collect::<Vec<_>>(),(1..=45).rev().collect::<Vec<_>>());
        let mut counts = [0usize;6];
        for duo in &list.duos { counts[duo.tier as usize] += 1; }
        assert_eq!(counts,[2,3,9,13,12,6]);
        let mut cached = list.clone();
        for duo in &mut cached.duos { duo.tier = 5; }
        label_duos(&mut cached.duos);
        assert_eq!(cached,list);
    }

    #[test]
    fn a_strong_popular_pair_tops_the_list_and_a_previous_only_pair_has_zero_games() {
        let now = DuoList {
            duos: vec![
                duo(1, 2, 54.0, 20_000),
                duo(3, 4, 49.0, 5_000),
                duo(5, 6, 50.0, 8_000),
            ],
        };
        let prev = DuoList {
            duos: vec![duo(3, 4, 47.0, 9_000), duo(7, 8, 53.0, 9_000)],
        };
        let list = blend(now, Some(&prev));
        assert_eq!((list.duos[0].adc_id, list.duos[0].support_id), (1, 2));
        assert!(list
            .duos
            .windows(2)
            .all(|pair| pair[0].score >= pair[1].score));
        let old = list.duos.iter().find(|duo| duo.adc_id == 7).unwrap();
        assert_eq!(old.games, 0);
        // 5,000 current games dominate the 400-game prior.
        let blended = list.duos.iter().find(|duo| duo.adc_id == 3).unwrap();
        assert!(blended.win_pct < 49.0 && blended.win_pct > 48.5);
    }

    /// Run with `cargo test --lib -- --ignored --nocapture prints_the_live_duos`.
    #[tokio::test]
    #[ignore = "network: fetches the live emerald_plus duo list"]
    async fn prints_the_live_duos() {
        let list = super::super::ugg::duos("emerald_plus")
            .await
            .expect("a list")
            .value;
        let mut counts = std::collections::BTreeMap::new();
        for duo in &list.duos {
            *counts
                .entry(crate::runes::tierlist::tier_label(duo.tier))
                .or_insert(0usize) += 1;
        }
        for duo in list.duos.iter().take(15) {
            println!(
                "{:>4}+{:<4} {:<2} wr={:.2} games={} score={:.2}",
                duo.adc_id,
                duo.support_id,
                crate::runes::tierlist::tier_label(duo.tier),
                duo.win_pct,
                duo.games,
                duo.score
            );
        }
        println!("{} pairs, tiers {counts:?}", list.duos.len());
    }

    #[test]
    fn lolalytics_fallback_reads_support_synergy_only_and_rejects_bad_samples() {
        let root = serde_json::json!({"team":{"top":[[266,53.26,0,0,3.05,9791]],
            "support":[[117,53.19,0.48,-0.54,14.01,44989],[412,56.27,2.51,0.45,11.6,37261],
                       [902,54.07,0,0,0,20],[0,50,0,0,0,1000],[1,101,0,0,0,1000]]}});
        let pairs = parse_lolalytics(&root, 222).unwrap();
        assert_eq!(pairs.len(), 2);
        assert_eq!(
            (pairs[0].adc_id, pairs[0].support_id, pairs[0].games),
            (222, 117, 44989)
        );
        assert_eq!(pairs[0].win_pct, 53.19);
        assert!(parse_lolalytics(&serde_json::json!({"team":{"top":[]}}), 222).is_none());
        assert!(parse_value(&serde_json::json!([{"adc_supp":[]}])).is_none());
    }

    #[tokio::test]
    #[ignore = "network: verifies lolalytics' support synergy fallback payload"]
    async fn prints_live_lolalytics_duo_sample() {
        let patches = patch::patches().await.expect("patch names");
        let client = super::super::lolalytics::LolalyticsClient::new().unwrap();
        let body = client.page(format!("https://a1.lolalytics.com/mega/?ep=build-team&v=1&patch={}&c=jinx&lane=bottom&tier=emerald_plus&queue=ranked&region=all", patches.current)).await.unwrap();
        let pairs = parse_lolalytics(&serde_json::from_str(&body).unwrap(), 222).unwrap();
        assert!(pairs.len() > 20);
        assert!(pairs
            .iter()
            .all(|duo| duo.adc_id == 222 && duo.games >= 100));
        println!(
            "Lolalytics fallback: {} Jinx + Support pairs, first {:?}",
            pairs.len(),
            pairs.first()
        );
    }
}
