//! Parsing of the League client's rune game-data assets.
//!
//! Names, icons and tree structure come from `/lol-game-data/assets/v1/perks.json`
//! and `/lol-game-data/assets/v1/perkstyles.json`, so Swapper never depends on a
//! CDN. These functions are pure so the structure can be tested from saved JSON.

use std::collections::HashMap;

use serde::Deserialize;

use super::RuneError;

const ASSET_ROOT: &str = "/lol-game-data/assets";

#[derive(Debug, Clone, Deserialize)]
pub struct PerkAsset {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "iconPath")]
    pub icon_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PerkStyleSlot {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, rename = "slotLabel")]
    pub label: String,
    #[serde(default)]
    pub perks: Vec<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PerkStyleAsset {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "iconPath")]
    pub icon_path: String,
    #[serde(default)]
    pub slots: Vec<PerkStyleSlot>,
}

#[derive(Deserialize)]
struct PerkStylesFile {
    #[serde(default)]
    styles: Vec<PerkStyleAsset>,
}

/// Normalizes an asset path found in game data to the form the LCU serves.
pub fn normalize_asset_path(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    if let Some(rest) = path.strip_prefix(ASSET_ROOT) {
        return Some(format!("{ASSET_ROOT}{rest}"));
    }
    let relative = path.trim_start_matches('/');
    if relative.is_empty() || relative.contains("..") {
        return None;
    }
    Some(format!("{ASSET_ROOT}/v1/{relative}"))
}

pub fn parse_perks(body: &str) -> Result<HashMap<i64, PerkAsset>, RuneError> {
    let perks: Vec<PerkAsset> = serde_json::from_str(body)
        .map_err(|e| RuneError::unavailable(format!("Could not read perk data: {e}")))?;
    Ok(perks.into_iter().map(|perk| (perk.id, perk)).collect())
}

pub fn parse_styles(body: &str) -> Result<Vec<PerkStyleAsset>, RuneError> {
    let file: PerkStylesFile = serde_json::from_str(body)
        .map_err(|e| RuneError::unavailable(format!("Could not read rune tree data: {e}")))?;
    Ok(file.styles)
}

pub fn parse_recommended_pages(body: &str) -> Result<Vec<RecommendedPage>, RuneError> {
    serde_json::from_str(body)
        .map_err(|e| RuneError::unavailable(format!("Could not read recommended pages: {e}")))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedPage {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub primary_perk_style_id: i64,
    #[serde(default)]
    pub secondary_perk_style_id: i64,
    #[serde(default)]
    pub selected_perk_ids: Vec<i64>,
    #[serde(default)]
    pub win_rate: Option<f64>,
}

impl RecommendedPage {
    /// Splits a 9-id recommended page into the rune selection the editor uses.
    pub fn selection(&self) -> Option<super::page::RuneSelection> {
        if self.primary_perk_style_id == 0
            || self.secondary_perk_style_id == 0
            || self.selected_perk_ids.len() < 9
        {
            return None;
        }
        let ids = &self.selected_perk_ids;
        Some(super::page::RuneSelection {
            primary_page_id: self.primary_perk_style_id,
            secondary_page_id: self.secondary_perk_style_id,
            keystone: ids[0],
            primary_runes: vec![ids[1], ids[2], ids[3]],
            secondary_runes: vec![ids[4], ids[5]],
            shards: vec![ids[6], ids[7], ids[8]],
        })
    }
}

/// The rune assets needed to render and validate pages.
#[derive(Clone)]
pub struct PerkCatalog {
    pub perks: HashMap<i64, PerkAsset>,
    pub styles: Vec<PerkStyleAsset>,
    style_index: HashMap<i64, usize>,
}

impl PerkCatalog {
    pub fn new(perks: HashMap<i64, PerkAsset>, styles: Vec<PerkStyleAsset>) -> Self {
        let style_index = styles
            .iter()
            .enumerate()
            .map(|(index, style)| (style.id, index))
            .collect();
        Self {
            perks,
            styles,
            style_index,
        }
    }

    pub fn perk(&self, id: i64) -> Option<&PerkAsset> {
        self.perks.get(&id)
    }

    pub fn name(&self, id: i64) -> Option<&str> {
        self.perk(id).map(|perk| perk.name.as_str())
    }

    pub fn style(&self, id: i64) -> Option<&PerkStyleAsset> {
        self.style_index.get(&id).map(|index| &self.styles[*index])
    }

    /// The LCU asset path for a perk id or a tree (style) id.
    pub fn asset_path(&self, id: i64) -> Option<String> {
        if let Some(style) = self.style(id) {
            return normalize_asset_path(&style.icon_path);
        }
        self.perk(id)
            .and_then(|perk| normalize_asset_path(&perk.icon_path))
    }

    /// The playable rune trees (the five styles), in client order.
    pub fn trees(&self) -> impl Iterator<Item = &PerkStyleAsset> {
        self.styles.iter()
    }

    /// The keystone options for a tree.
    pub fn keystones(&self, style_id: i64) -> Vec<i64> {
        self.style(style_id)
            .map(|style| {
                style
                    .slots
                    .iter()
                    .filter(|slot| slot.kind == "kKeyStone")
                    .flat_map(|slot| slot.perks.iter().copied())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The three minor rune rows of a tree (keystone and shard slots removed).
    pub fn rows(&self, style_id: i64) -> Vec<Vec<i64>> {
        self.style(style_id)
            .map(|style| {
                style
                    .slots
                    .iter()
                    .filter(|slot| slot.kind != "kKeyStone" && slot.kind != "kStatMod")
                    .map(|slot| slot.perks.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The three stat shard rows (Offense, Flex, Defense) with their labels.
    pub fn shard_rows(&self) -> Vec<(String, Vec<i64>)> {
        for style in &self.styles {
            let shards: Vec<(String, Vec<i64>)> = style
                .slots
                .iter()
                .filter(|slot| slot.kind == "kStatMod")
                .map(|slot| (slot.label.clone(), slot.perks.clone()))
                .collect();
            if !shards.is_empty() {
                return shards;
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERKS: &str = r#"[
        {"id":8005,"name":"Press the Attack","iconPath":"perk-images/Styles/Precision/PressTheAttack/PressTheAttack.png"},
        {"id":8112,"name":"Electrocute","iconPath":"/lol-game-data/assets/v1/perk-images/Styles/Domination/Electrocute/Electrocute.png"},
        {"id":5008,"name":"Adaptive Force","iconPath":"perk-images/StatMods/StatModsAdaptiveForceIcon.png"}
    ]"#;

    const STYLES: &str = r#"{"schemaVersion":2,"styles":[
        {"id":8000,"name":"Precision","iconPath":"/lol-game-data/assets/v1/perk-images/Styles/7201_Precision.png",
         "slots":[
            {"type":"kKeyStone","slotLabel":"","perks":[8005,8008]},
            {"type":"kMixedRegularSplashable","slotLabel":"Heroism","perks":[9101,9111]},
            {"type":"kMixedRegularSplashable","slotLabel":"Legend","perks":[9104,9105]},
            {"type":"kMixedRegularSplashable","slotLabel":"Combat","perks":[8014,8017]},
            {"type":"kStatMod","slotLabel":"Offense","perks":[5008,5005,5007]},
            {"type":"kStatMod","slotLabel":"Flex","perks":[5008,5010,5001]},
            {"type":"kStatMod","slotLabel":"Defense","perks":[5011,5013,5001]}
         ]},
        {"id":8100,"name":"Domination","iconPath":"perk-images/Styles/7200_Domination.png",
         "slots":[
            {"type":"kKeyStone","slotLabel":"","perks":[8112]},
            {"type":"kMixedRegularSplashable","slotLabel":"Malice","perks":[8124,8139]},
            {"type":"kMixedRegularSplashable","slotLabel":"Prestige","perks":[8137,8141]},
            {"type":"kMixedRegularSplashable","slotLabel":"Hunter","perks":[8106,8135]}
         ]}
    ]}"#;

    fn catalog() -> PerkCatalog {
        PerkCatalog::new(parse_perks(PERKS).unwrap(), parse_styles(STYLES).unwrap())
    }

    #[test]
    fn normalizes_relative_and_absolute_asset_paths() {
        assert_eq!(
            normalize_asset_path("perk-images/StatMods/x.png").as_deref(),
            Some("/lol-game-data/assets/v1/perk-images/StatMods/x.png")
        );
        assert_eq!(
            normalize_asset_path("/lol-game-data/assets/v1/perk-images/x.png").as_deref(),
            Some("/lol-game-data/assets/v1/perk-images/x.png")
        );
        assert_eq!(normalize_asset_path("  "), None);
        assert_eq!(normalize_asset_path("../secret"), None);
    }

    #[test]
    fn exposes_keystones_rows_and_shard_rows() {
        let catalog = catalog();
        assert_eq!(catalog.keystones(8000), vec![8005, 8008]);
        assert_eq!(
            catalog.rows(8000),
            vec![vec![9101, 9111], vec![9104, 9105], vec![8014, 8017]]
        );
        let shards = catalog.shard_rows();
        assert_eq!(shards.len(), 3);
        assert_eq!(shards[0], ("Offense".to_string(), vec![5008, 5005, 5007]));
        assert_eq!(shards[1], ("Flex".to_string(), vec![5008, 5010, 5001]));
    }

    #[test]
    fn resolves_icons_for_perks_trees_and_stat_shards() {
        let catalog = catalog();
        // A tree (style) id, as the pro cards' primary/secondary style ids.
        assert_eq!(
            catalog.asset_path(8000).as_deref(),
            Some("/lol-game-data/assets/v1/perk-images/Styles/7201_Precision.png")
        );
        // The second tree is a style id too, not a perk id.
        assert!(catalog
            .asset_path(8100)
            .unwrap()
            .ends_with("/7200_Domination.png"));
        assert!(catalog
            .asset_path(8005)
            .unwrap()
            .ends_with("/PressTheAttack/PressTheAttack.png"));
        // Stat shards are perks, so their icons resolve for the pro-card rows.
        assert!(catalog
            .asset_path(5008)
            .unwrap()
            .ends_with("/StatModsAdaptiveForceIcon.png"));
        assert!(catalog.asset_path(999999).is_none());
    }

    #[test]
    fn splits_a_recommended_page_into_a_selection() {
        let page: RecommendedPage = serde_json::from_str(
            r#"{"name":"Ahri","primaryPerkStyleId":8100,"secondaryPerkStyleId":8200,"selectedPerkIds":[8112,8139,8140,8106,8210,8226,5005,5008,5001]}"#,
        )
        .unwrap();
        let selection = page.selection().unwrap();
        assert_eq!(selection.keystone, 8112);
        assert_eq!(selection.primary_runes, vec![8139, 8140, 8106]);
        assert_eq!(selection.secondary_runes, vec![8210, 8226]);
        assert_eq!(selection.shards, vec![5005, 5008, 5001]);
    }

    #[test]
    fn rejects_a_truncated_recommended_page() {
        let page: RecommendedPage = serde_json::from_str(
            r#"{"primaryPerkStyleId":8100,"secondaryPerkStyleId":8200,"selectedPerkIds":[1,2]}"#,
        )
        .unwrap();
        assert!(page.selection().is_none());
    }
}
