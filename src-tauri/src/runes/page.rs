//! Rune page model, League rule validation, and the create/replace decision.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::RuneError;

/// The runes the user (or a preset) selected, before they become a page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneSelection {
    pub primary_page_id: i64,
    pub secondary_page_id: i64,
    pub keystone: i64,
    pub primary_runes: Vec<i64>,
    pub secondary_runes: Vec<i64>,
    pub shards: Vec<i64>,
}

impl RuneSelection {
    /// The nine ids the League client stores in `selectedPerkIds`, in order:
    /// keystone, three primary runes, two secondary runes, three shards.
    pub fn perk_ids(&self) -> Vec<i64> {
        let mut ids = Vec::with_capacity(9);
        ids.push(self.keystone);
        ids.extend(self.primary_runes.iter().copied());
        ids.extend(self.secondary_runes.iter().copied());
        ids.extend(self.shards.iter().copied());
        ids
    }
}

/// A page as reported by `/lol-perks/v1/pages`.
///
/// The list contains League's built-in, non-editable pages as well as the
/// user's own, so the flags matter: only editable pages are counted against
/// the custom-page capacity or replaced.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LcuPage {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub is_editable: bool,
    #[serde(default)]
    pub is_deletable: bool,
    /// Temporary pages (for example a Swiftplay recommendation) do not consume
    /// a custom-page slot.
    #[serde(default)]
    pub is_temporary: bool,
}

impl LcuPage {
    /// A user-owned custom page: editable and deletable, unlike League's
    /// built-ins, and not a temporary page that takes no slot.
    pub fn is_custom(&self) -> bool {
        self.is_editable && self.is_deletable && !self.is_temporary
    }
}

/// What to do with the Swapper page for the current champion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PagePlan {
    /// No Swapper page exists and there is room to add one.
    Create,
    /// Replace this existing Swapper page.
    Replace(i64),
    /// The page limit is reached and there is no Swapper page to reuse.
    LimitReached,
}

/// Decides create vs replace from the page Swapper created last time.
///
/// Ownership is the stored page id, never the name: a user page that merely
/// happens to be called `Swapper: ...` is left alone. A stored id that is no
/// longer in the list, or that points at a non-editable page, is not ours to
/// replace. `can_add_custom_page` is the capacity answer from the inventory
/// (see `apply::Inventory`), which already excludes League's built-in pages.
/// Never deletes anything: when the limit is hit and there is no owned page to
/// reuse the caller must show a clear message.
pub fn plan(existing: &[LcuPage], can_add_custom_page: bool, owned_page_id: Option<i64>) -> PagePlan {
    if let Some(id) = owned_page_id {
        if existing
            .iter()
            .any(|page| page.id == id && page.is_editable)
        {
            return PagePlan::Replace(id);
        }
    }
    if can_add_custom_page {
        PagePlan::Create
    } else {
        PagePlan::LimitReached
    }
}

/// Structural index of the rune catalog used to validate a page offline.
#[derive(Debug, Default)]
pub struct CatalogIndex {
    pub keystones: HashMap<i64, i64>,
    /// Tree id -> its three minor rows, each a list of rune ids.
    pub rows: HashMap<i64, Vec<Vec<i64>>>,
    pub shard_rows: Vec<Vec<i64>>,
}

impl CatalogIndex {
    pub fn tree_ids(&self) -> HashSet<i64> {
        self.rows.keys().copied().collect()
    }

    fn row_of(&self, tree: i64, rune: i64) -> Option<usize> {
        self.rows
            .get(&tree)?
            .iter()
            .position(|row| row.contains(&rune))
    }
}

fn distinct_rows(rows: &[Option<usize>]) -> bool {
    let mut seen = HashSet::new();
    rows.iter()
        .all(|row| row.map(|index| seen.insert(index)).unwrap_or(false))
}

/// Enforces League's page rules: a keystone plus three primary runes, two
/// secondary runes from different rows, and three shards.
pub fn validate(selection: &RuneSelection, catalog: &CatalogIndex) -> Result<(), RuneError> {
    if selection.primary_page_id == selection.secondary_page_id {
        return Err(RuneError::conflict(
            "The primary and secondary trees must be different.",
        ));
    }
    let trees = catalog.tree_ids();
    if !trees.contains(&selection.primary_page_id) || !trees.contains(&selection.secondary_page_id)
    {
        return Err(RuneError::conflict("That rune tree is not available."));
    }
    match catalog.keystones.get(&selection.keystone) {
        Some(tree) if *tree == selection.primary_page_id => {}
        _ => {
            return Err(RuneError::conflict(
                "Choose a keystone from the primary tree.",
            ))
        }
    }
    if selection.primary_runes.len() != 3 {
        return Err(RuneError::conflict(
            "Pick exactly three primary runes.",
        ));
    }
    let primary_rows: Vec<Option<usize>> = selection
        .primary_runes
        .iter()
        .map(|rune| catalog.row_of(selection.primary_page_id, *rune))
        .collect();
    if primary_rows.iter().any(Option::is_none) {
        return Err(RuneError::conflict(
            "A primary rune does not belong to the primary tree.",
        ));
    }
    if !distinct_rows(&primary_rows) {
        return Err(RuneError::conflict(
            "Primary runes must come from three different rows.",
        ));
    }
    if selection.secondary_runes.len() != 2 {
        return Err(RuneError::conflict("Pick exactly two secondary runes."));
    }
    let secondary_rows: Vec<Option<usize>> = selection
        .secondary_runes
        .iter()
        .map(|rune| catalog.row_of(selection.secondary_page_id, *rune))
        .collect();
    if secondary_rows.iter().any(Option::is_none) {
        return Err(RuneError::conflict(
            "A secondary rune does not belong to the secondary tree.",
        ));
    }
    if !distinct_rows(&secondary_rows) {
        return Err(RuneError::conflict(
            "Secondary runes must come from two different rows.",
        ));
    }
    if selection.shards.len() != 3 {
        return Err(RuneError::conflict("Pick exactly three stat shards."));
    }
    for (index, shard) in selection.shards.iter().enumerate() {
        let Some(row) = catalog.shard_rows.get(index) else {
            return Err(RuneError::conflict("That stat shard row is not available."));
        };
        if !row.contains(shard) {
            return Err(RuneError::conflict(
                "A stat shard does not belong to its row.",
            ));
        }
    }
    Ok(())
}

/// Body accepted by `POST /lol-perks/v1/pages`.
///
/// The `/lol-perks/v1/pages` resource uses `primaryStyleId` and `subStyleId`
/// for its trees. `primaryPerkStyleId`/`secondaryPerkStyleId` only exist on the
/// recommended-pages schema (`perks::RecommendedPage`).
pub fn page_body(selection: &RuneSelection, name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "isEditable": true,
        "primaryStyleId": selection.primary_page_id,
        "subStyleId": selection.secondary_page_id,
        "selectedPerkIds": selection.perk_ids(),
        "current": true,
    })
}

/// Body accepted by `PUT /lol-perks/v1/pages/{id}`.
///
/// League expects the full page object for a replace, `id` included, so the
/// update cannot land on the wrong page.
pub fn page_put_body(selection: &RuneSelection, name: &str, id: i64) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "primaryStyleId": selection.primary_page_id,
        "subStyleId": selection.secondary_page_id,
        "selectedPerkIds": selection.perk_ids(),
    })
}

/// Page name Swapper uses for a champion.
pub fn page_name(champion: &str) -> String {
    let champion = champion.trim();
    if champion.is_empty() {
        "Swapper".to_string()
    } else {
        format!("Swapper: {champion}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> CatalogIndex {
        let mut keystones = HashMap::new();
        for keystone in [8112, 8992, 8214] {
            keystones.insert(keystone, 8100);
        }
        keystones.insert(8021, 8000);
        let mut rows = HashMap::new();
        rows.insert(
            8100,
            vec![vec![8124, 8139, 8140], vec![8137, 8141], vec![8106, 8135]],
        );
        rows.insert(8200, vec![vec![8226, 8210], vec![8237, 8233], vec![8236]]);
        rows.insert(8000, vec![vec![9111], vec![8009], vec![9103]]);
        CatalogIndex {
            keystones,
            rows,
            shard_rows: vec![
                vec![5008, 5005, 5007],
                vec![5008, 5010, 5001],
                vec![5011, 5013, 5001],
            ],
        }
    }

    fn valid_selection() -> RuneSelection {
        RuneSelection {
            primary_page_id: 8100,
            secondary_page_id: 8200,
            keystone: 8112,
            primary_runes: vec![8139, 8137, 8106],
            secondary_runes: vec![8210, 8237],
            shards: vec![5005, 5008, 5011],
        }
    }

    #[test]
    fn accepts_a_legal_page() {
        assert!(validate(&valid_selection(), &catalog()).is_ok());
    }

    #[test]
    fn rejects_same_tree_for_primary_and_secondary() {
        let mut selection = valid_selection();
        selection.secondary_page_id = 8100;
        assert!(validate(&selection, &catalog()).is_err());
    }

    #[test]
    fn rejects_a_keystone_outside_the_primary_tree() {
        let mut selection = valid_selection();
        selection.keystone = 8021;
        assert!(validate(&selection, &catalog()).is_err());
    }

    #[test]
    fn rejects_two_primary_runes_from_the_same_row() {
        let mut selection = valid_selection();
        selection.primary_runes = vec![8139, 8140, 8124];
        assert!(validate(&selection, &catalog()).is_err());
    }

    #[test]
    fn rejects_two_secondary_runes_from_the_same_row() {
        let mut selection = valid_selection();
        selection.secondary_runes = vec![8210, 8226];
        assert!(validate(&selection, &catalog()).is_err());
    }

    #[test]
    fn rejects_a_shard_outside_its_row() {
        let mut selection = valid_selection();
        selection.shards = vec![5005, 5008, 5002];
        assert!(validate(&selection, &catalog()).is_err());
    }

    #[test]
    fn perk_ids_are_in_league_order() {
        assert_eq!(
            valid_selection().perk_ids(),
            vec![8112, 8139, 8137, 8106, 8210, 8237, 5005, 5008, 5011]
        );
    }

    fn custom_page(id: i64, name: &str) -> LcuPage {
        LcuPage {
            id,
            name: name.into(),
            is_editable: true,
            is_deletable: true,
            is_temporary: false,
        }
    }

    fn built_in_page(id: i64, name: &str) -> LcuPage {
        LcuPage {
            id,
            name: name.into(),
            is_editable: false,
            is_deletable: false,
            is_temporary: false,
        }
    }

    #[test]
    fn creates_a_page_when_nothing_is_owned_yet() {
        let pages = vec![custom_page(1, "Ahri mid")];
        assert_eq!(plan(&pages, true, None), PagePlan::Create);
    }

    #[test]
    fn reuses_the_stored_swapper_page() {
        let pages = vec![custom_page(7, "Ahri mid"), custom_page(9, "Swapper: Ahri")];
        assert_eq!(plan(&pages, true, Some(9)), PagePlan::Replace(9));
    }

    #[test]
    fn does_not_replace_a_user_page_that_merely_uses_the_swapper_name() {
        let pages = vec![custom_page(9, "Swapper: Mid")];
        // No stored id, so the user's lookalike page is not ours to touch.
        assert_eq!(plan(&pages, true, None), PagePlan::Create);
        // A different stored id is not a match either.
        assert_eq!(plan(&pages, true, Some(4)), PagePlan::Create);
    }

    #[test]
    fn does_not_replace_a_page_the_client_marks_non_editable() {
        // A built-in page whose id happens to match the stored id is not ours.
        let pages = vec![built_in_page(9, "Swapper: Ahri")];
        assert_eq!(plan(&pages, false, Some(9)), PagePlan::LimitReached);
        assert_eq!(plan(&pages, true, Some(9)), PagePlan::Create);
    }

    #[test]
    fn creates_when_the_stored_page_id_is_stale() {
        let pages = vec![custom_page(2, "Swapper: Ahri")];
        assert_eq!(plan(&pages, true, Some(9)), PagePlan::Create);
    }

    #[test]
    fn refuses_to_delete_when_the_page_limit_is_reached() {
        let pages = vec![custom_page(1, "One"), custom_page(2, "Two")];
        assert_eq!(plan(&pages, false, None), PagePlan::LimitReached);
        // A stale stored id does not earn a delete either.
        assert_eq!(plan(&pages, false, Some(9)), PagePlan::LimitReached);
    }

    #[test]
    fn builds_a_page_body_with_league_field_names() {
        let body = page_body(&valid_selection(), &page_name("Ahri"));
        assert_eq!(body["name"], "Swapper: Ahri");
        assert_eq!(body["isEditable"], true);
        // `/lol-perks/v1/pages` names the trees `primaryStyleId`/`subStyleId`.
        // `primaryPerkStyleId`/`secondaryPerkStyleId` belong to the
        // recommended-pages schema and must never be sent here.
        assert_eq!(body["primaryStyleId"], 8100);
        assert_eq!(body["subStyleId"], 8200);
        assert_eq!(body.get("primaryPerkStyleId"), None);
        assert_eq!(body.get("secondaryPerkStyleId"), None);
        assert_eq!(
            body["selectedPerkIds"],
            serde_json::json!([8112, 8139, 8137, 8106, 8210, 8237, 5005, 5008, 5011])
        );
        assert_eq!(body["current"], true);
    }

    #[test]
    fn builds_a_full_replace_body_that_carries_the_page_id() {
        let body = page_put_body(&valid_selection(), &page_name("Ahri"), 9);
        assert_eq!(body["id"], 9);
        assert_eq!(body["name"], "Swapper: Ahri");
        assert_eq!(body["primaryStyleId"], 8100);
        assert_eq!(body["subStyleId"], 8200);
        assert_eq!(
            body["selectedPerkIds"],
            serde_json::json!([8112, 8139, 8137, 8106, 8210, 8237, 5005, 5008, 5011])
        );
        assert_eq!(body.get("primaryPerkStyleId"), None);
        assert_eq!(body.get("secondaryPerkStyleId"), None);
    }
}
