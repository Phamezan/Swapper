use serde::Serialize;
use super::abilities::{self, Abilities};
use crate::runes::ugg::data_card::{self, CardStats, Gold15, MIN_GAMES};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CardKit {
    sheet: Abilities,
    version: String,
    asset_url: String,
    fetched_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MatchupDataCard {
    kind: &'static str,
    champion_id: i64,
    enemy_champion_id: i64,
    lane: String,
    tier: String,
    stats: CardStats,
    #[serde(skip_serializing_if = "Option::is_none")] gold15: Option<Gold15>,
    #[serde(skip_serializing_if = "Option::is_none")] enemy_kit: Option<CardKit>,
}

fn eligible(stats: &CardStats, gold: Option<&Gold15>, kit: Option<&CardKit>) -> bool {
    stats.games >= MIN_GAMES && stats.wins <= stats.games
        && (gold.is_some_and(|g| g.games_at15 >= MIN_GAMES && g.games_at15 <= stats.games
            && g.mean.is_finite() && g.signed_total.is_finite() && g.units == "gold" && g.perspective == "champion"
            && (g.mean - g.signed_total / g.games_at15 as f64).abs() < 0.001)
            || kit.is_some_and(|k| k.version == stats.patch && k.sheet.spells.len() >= 2))
}

#[tauri::command]
pub(crate) async fn get_matchup_data_card(champion_id: i64, enemy_champion_id: i64, position: String, tier: String) -> Result<Option<MatchupDataCard>, String> {
    if champion_id <= 0 || enemy_champion_id <= 0 || champion_id > i32::MAX as i64 || enemy_champion_id > i32::MAX as i64
        || !matches!(position.as_str(), "top" | "jungle" | "mid" | "adc" | "support") || !data_card::supported_tier(&tier) {
        return Err("Unknown champion, position or tier.".into());
    }
    if champion_id == enemy_champion_id { return Ok(None); }
    let champions = crate::runes::champion_list().await;
    if ![champion_id, enemy_champion_id].iter().all(|id| champions.iter().any(|c| c.id == *id)) { return Ok(None); }
    let Some(stats) = data_card::stats(champion_id, enemy_champion_id, &position, &tier).await else { return Ok(None) };
    let kit = abilities::for_patch(enemy_champion_id, &stats.patch).await.map(|(sheet, asset_url, fetched_at)| CardKit {
        sheet, version: stats.patch.clone(), asset_url, fetched_at,
    });
    // Gold remains disabled in the provider adapter until its evidence contract is verified.
    if !eligible(&stats, None, kit.as_ref()) { return Ok(None); }
    Ok(Some(MatchupDataCard { kind: "data-based", champion_id, enemy_champion_id, lane: position, tier, stats, gold15: None, enemy_kit: kit }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_card_requires_500_games_and_two_numeric_spells_or_500_verified_gold_games() {
        let mut stats = CardStats { wins: 240, games: 499, patch: "16.22".into(), data_version: "1.5.0".into(),
            region: "world", queue: "ranked_solo_5x5", tier: "emerald_plus".into(), provider: "ugg", endpoint_url: "test".into(),
            schema_id: "test", fetched_at: 1, stale: false };
        let raw: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/champion-abilities.json")).unwrap();
        let mut sheet = abilities::parse(238, &raw["238"]);
        abilities::numeric_facts(&mut sheet);
        sheet.spells.truncate(2);
        let mut kit = CardKit { sheet, version: stats.patch.clone(), asset_url: "test".into(), fetched_at: 1 };
        assert!(!eligible(&stats, None, Some(&kit)));
        stats.games = 500;
        assert!(!eligible(&stats, None, None));
        assert!(eligible(&stats, None, Some(&kit)));
        kit.sheet.spells.truncate(1);
        assert!(!eligible(&stats, None, Some(&kit)));
        let mut gold = Gold15 { mean: -180.0, games_at15: 499, signed_total: -180.0 * 499.0, units: "gold", perspective: "champion", schema_id: "synthetic-verified" };
        assert!(!eligible(&stats, Some(&gold), None));
        gold.games_at15 = 500; gold.signed_total = -90_000.0;
        assert!(eligible(&stats, Some(&gold), None));
        gold.perspective = "opponent";
        assert!(!eligible(&stats, Some(&gold), None));
        kit.version = "16.21".into();
        assert!(!eligible(&stats, None, Some(&kit)));
        let mut descriptions = abilities::parse(238, &serde_json::json!({"spells":[{"name":"Q","description":"Effect"},{"name":"W","description":"Effect"}]}));
        abilities::numeric_facts(&mut descriptions);
        assert!(descriptions.spells.is_empty());
    }
}
