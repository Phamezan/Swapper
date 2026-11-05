//! Current-patch, single-population evidence. Never use the blended counters DTO.
use super::*;
use serde::Serialize;

pub(crate) const MIN_GAMES: u64 = 500;
const MAX_AGE_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CardStats {
    pub wins: u64,
    pub games: u64,
    pub patch: String,
    pub data_version: String,
    pub region: &'static str,
    pub queue: &'static str,
    pub tier: String,
    pub provider: &'static str,
    pub endpoint_url: String,
    pub schema_id: &'static str,
    pub fetched_at: i64,
    pub stale: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Gold15 {
    pub mean: f64,
    pub games_at15: u64,
    pub signed_total: f64,
    pub units: &'static str,
    pub perspective: &'static str,
    pub schema_id: &'static str,
}

fn gold15(_row: &Value) -> Option<Gold15> {
    // Disabled: column 4's sign and metric-eligible denominator are not verified.
    None
}

fn row_counts(root: &Value, lane: &str, enemy: i64) -> Option<(u64, u64)> {
    let rows = root.get(role(lane)?.1)?.get(0)?.as_array()?;
    let mut matches = rows.iter().filter(|r| r.get(0).and_then(Value::as_i64) == Some(enemy));
    let row = matches.next()?;
    if matches.next().is_some() { return None; }
    let (opponent_wins, games) = (row.get(1)?.as_u64()?, row.get(2)?.as_u64()?);
    let _disabled_gold = gold15(row);
    (opponent_wins <= games).then(|| (games - opponent_wins, games))
}

fn union(a: Option<(u64, u64)>, b: Option<(u64, u64)>) -> Option<(u64, u64)> {
    let (a, b) = (a?, b?);
    Some((a.0.checked_add(b.0)?, a.1.checked_add(b.1)?))
}

fn current_patch(versions: &Value, live: &str) -> Option<String> {
    let patch = patch_names(versions)?.current;
    (super::super::patch::patch_name(live)? == patch).then_some(patch)
}

fn usable_age(at: Option<i64>, now: i64) -> Option<i64> {
    at.filter(|at| *at > 0 && now >= *at && now - *at <= MAX_AGE_MS)
}

pub(crate) fn supported_tier(tier: &str) -> bool {
    matches!(tier, "all" | "gold_plus" | "platinum_plus" | "emerald_plus" | "diamond_plus" | "master_plus" | "challenger")
}

pub(crate) async fn stats(champion: i64, enemy: i64, lane: &str, tier: &str) -> Option<CardStats> {
    if !supported_tier(tier) { return None; }
    role(lane)?;
    let (versions, live) = tokio::join!(versions(), super::super::patch::patches());
    let (versions, live) = (versions?, live?);
    let patch = current_patch(&versions.value, &live.current)?;
    let now = super::super::provider::now_ms();
    usable_age(versions.fetched_at, now)?;
    let (counts, fetched_at, stale, bucket) = if tier == "gold_plus" {
        // Gold and Platinum+ are disjoint. Never relabel a partial population Gold+.
        let (gold, platinum) = tokio::join!(
            dataset_bucket(&versions, "matchups", &patch, "gold", Some(champion), false),
            dataset_bucket(&versions, "matchups", &patch, "platinum_plus", Some(champion), false),
        );
        let (gold, platinum) = (gold?, platinum?);
        let now = super::super::provider::now_ms();
        let at = usable_age(gold.fetched_at, now)?.min(usable_age(platinum.fetched_at, now)?);
        (union(row_counts(&gold.value, lane, enemy), row_counts(&platinum.value, lane, enemy))?,
            at, gold.stale || platinum.stale, "gold")
    } else {
        let found = dataset(&versions, "matchups", &patch, tier, Some(champion), false).await?;
        (row_counts(&found.value, lane, enemy)?, usable_age(found.fetched_at, super::super::provider::now_ms())?, found.stale, tier)
    };
    let (wins, games) = counts;
    if games < MIN_GAMES { return None; }
    Some(CardStats {
        wins, games, data_version: versions.value.get(patch.replace('.', "_"))?.get("matchups")?.as_str()?.into(),
        endpoint_url: dataset_url(&versions.value, "matchups", &patch, bucket, Some(champion))?,
        patch, tier: tier.into(), region: "world", queue: "ranked_solo_5x5", provider: "ugg",
        schema_id: if tier == "gold_plus" { "ugg-matchups-v1:opponent-wins:gold+platinum-plus" } else { "ugg-matchups-v1:opponent-wins" },
        fetched_at, stale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn complements_opponent_wins_once_and_selects_only_the_requested_role() {
        let raw = json!({"3":[[[51,540,1000]]],"2":[[[51,100,800]]]});
        assert_eq!(row_counts(&raw, "adc", 51), Some((460,1000)));
        assert_eq!(row_counts(&raw, "support", 51), Some((700,800)));
        assert_eq!(row_counts(&raw, "mid", 51), None);
        assert_eq!(row_counts(&raw, "adc", 89), None);
        let reciprocal = json!({"3":[[[222,460,1000]]]});
        assert_eq!(row_counts(&reciprocal, "adc", 222), Some((540,1000)));
        assert_eq!(460 + 540, 1000);
        for row in [json!([51,501,500]), json!([51,-1,500]), json!([51,1.5,500]), json!([51,null,500]), json!([51,10])] {
            assert_eq!(row_counts(&json!({"3":[[row]]}), "adc", 51), None);
        }
    }

    #[test]
    fn gold_plus_unions_counts_without_averaging_and_rejects_overflow() {
        assert_eq!(union(Some((200,500)), Some((800,1000))), Some((1000,1500)));
        assert_eq!(union(Some((1,u64::MAX)), Some((1,1))), None);
        assert_eq!(union(Some((u64::MAX,u64::MAX)), Some((1,1))), None);
        assert_eq!(union(None, Some((800,1000))), None);
        assert_eq!(union(Some((200,500)), None), None);
    }

    #[test]
    fn only_the_live_patch_is_eligible_and_a_display_alias_is_not_a_data_version() {
        let versions = json!({"16_22":{},"16_21":{}});
        assert_eq!(current_patch(&versions, "16.22.1"), Some("16.22".into()));
        assert_eq!(current_patch(&versions, "16.23"), None);
        assert_eq!(current_patch(&versions, "16.21"), None);
        assert_eq!(current_patch(&versions, "26.22"), None);
        assert_eq!(current_patch(&versions, "unknown"), None);
    }

    #[test]
    fn evidence_age_is_not_renewed_by_a_read_or_clock_rollback() {
        assert_eq!(usable_age(Some(10), 10 + MAX_AGE_MS), Some(10));
        assert_eq!(usable_age(Some(10), 11 + MAX_AGE_MS), None);
        assert_eq!(usable_age(Some(10), 9), None);
        assert_eq!(usable_age(None, 100), None);
    }

    #[test]
    fn unverified_gold_is_always_hidden_including_truncated_and_reciprocal_rows() {
        for raw in [json!([122,540,1000,0,-180000]), json!([86,460,1000,0,180000]), json!([122,540,1000]), json!([122,1,500,0,125000])] {
            assert!(gold15(&raw).is_none());
        }
    }
}
