//! Writing the single rune page Swapper owns: create it once, then replace it
//! in place. The stored page id is the ownership proof, so a user page that
//! merely shares the `Swapper:` name is never touched.

use reqwest::Method;
use serde::Deserialize;
use serde_json::Value;

use super::matchup::AUTO_APPLY_MATCHUP_GAMES;
use super::matchup_view::matchup_view;
use super::page::{self, PagePlan};
use super::session;
use super::spells;
use super::{data, view, AppliedView, LcuPage, RuneError, RuneSelection};

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Inventory {
    /// League's own answer for whether another custom page will be accepted.
    /// It already accounts for slot-consuming pages that are not editable.
    #[serde(default)]
    can_add_custom_page: Option<bool>,
    /// Custom pages currently in use.
    #[serde(default)]
    custom_page_count: Option<u32>,
    /// The account's custom-page capacity.
    #[serde(default)]
    owned_page_count: Option<u32>,
    #[serde(default)]
    is_custom_page_creation_unlocked: Option<bool>,
}

impl Inventory {
    /// Whether a new custom page may be created.
    ///
    /// `canAddCustomPage` is authoritative and is what LeagueAkari's auto-rune
    /// code checks. Older responses can omit it; then fall back to counting the
    /// user's custom pages against the capacity, which never counts League's
    /// built-in non-editable pages.
    fn allows_new_page(&self, existing: &[LcuPage]) -> bool {
        if let Some(allowed) = self.can_add_custom_page {
            return allowed;
        }
        if self.is_custom_page_creation_unlocked == Some(false) {
            return false;
        }
        let Some(capacity) = self.owned_page_count else {
            // No capacity information: let League reject the create if full.
            return true;
        };
        let counted = existing.iter().filter(|page| page.is_custom()).count() as u32;
        let used = self.custom_page_count.unwrap_or(counted);
        used < capacity
    }
}

/// Applies a rune selection by creating or replacing the single Swapper page.
///
/// `owned_page_id` is the page id stored in Swapper's settings; only that id
/// may be replaced. When `apply_spells` is on and `spells` is present, the
/// recommended summoner-spell pair is written too; a spell failure never fails
/// the rune apply.
pub async fn apply(
    selection: RuneSelection,
    champion_id: i64,
    champion_name: &str,
    preset_index: Option<usize>,
    owned_page_id: Option<i64>,
    spells: Option<[i64; 2]>,
    apply_spells: bool,
) -> Result<AppliedView, RuneError> {
    let catalog = data::catalog().await?;
    page::validate(&selection, &view::catalog_index(&catalog))?;
    let lcu = super::lcu().await?;
    let existing: Vec<LcuPage> = super::lcu_get(&lcu, super::PAGES_PATH).await?;
    let inventory: Inventory = super::lcu_get(&lcu, super::INVENTORY_PATH)
        .await
        .unwrap_or_default();
    let name = page::page_name(champion_name);
    let page_id = match page::plan(
        &existing,
        inventory.allows_new_page(&existing),
        owned_page_id,
    ) {
        PagePlan::Create => {
            let created = super::lcu_send(
                &lcu,
                Method::POST,
                super::PAGES_PATH,
                Some(page::page_body(&selection, &name)),
            )
            .await?;
            match created
                .as_ref()
                .and_then(|value| value.get("id"))
                .and_then(Value::as_i64)
            {
                Some(id) => Some(id),
                None => {
                    // Some client builds answer without a body. Re-read the
                    // list and take the page that was not there before, instead
                    // of trusting the pre-create list.
                    let after: Vec<LcuPage> = super::lcu_get(&lcu, super::PAGES_PATH)
                        .await
                        .unwrap_or_default();
                    after
                        .iter()
                        .find(|candidate| {
                            candidate.name == name
                                && !existing.iter().any(|seen| seen.id == candidate.id)
                        })
                        .map(|candidate| candidate.id)
                }
            }
        }
        PagePlan::Replace(id) => {
            super::lcu_send(
                &lcu,
                Method::PUT,
                &format!("{}/{id}", super::PAGES_PATH),
                Some(page::page_put_body(&selection, &name, id)),
            )
            .await?;
            Some(id)
        }
        PagePlan::LimitReached => {
            return Err(RuneError::conflict(
                "Your rune pages are full. Swapper never deletes your pages — remove one in the League client, then try again.",
            ));
        }
    };
    if let Some(id) = page_id {
        // Make it current. The page body already asked for `current: true` on
        // create, so a failure here is not fatal.
        let _ = super::lcu_send(
            &lcu,
            Method::PUT,
            super::CURRENT_PAGE_PATH,
            Some(serde_json::json!(id)),
        )
        .await;
    }
    let applied = AppliedView {
        name,
        champion_id,
        primary_page_id: selection.primary_page_id,
        secondary_page_id: selection.secondary_page_id,
        keystone: selection.keystone,
        primary_runes: selection.primary_runes,
        secondary_runes: selection.secondary_runes,
        shards: selection.shards,
        preset_index,
        page_id,
        auto_applied: false,
    };
    super::shared().applied = Some(applied.clone());
    if apply_spells {
        if let Some(pair) = spells {
            // Best-effort: the rune page already applied, so a spell failure
            // must not turn the whole action into an error.
            let _ = super::spells::apply_recommended(pair).await;
        }
    }
    Ok(applied)
}

/// Applies the top recommendation for a champion, used by the auto-apply
/// setting. Returns `None` when nothing could be resolved.
pub async fn apply_top(
    context: &session::ChampSelectContext,
    owned_page_id: Option<i64>,
    tier: &str,
    apply_spells: bool,
    import_items: bool,
) -> Result<Option<AppliedView>, RuneError> {
    let catalog = data::catalog().await?;
    let mut context = context.clone();
    if context.champion_name.trim().is_empty() {
        if let Ok(names) = data::champion_names().await {
            if let Some(name) = names.get(&context.champion_id) {
                context.champion_name = name.clone();
            }
        }
    }
    let lcu = super::lcu().await?;
    let position = super::position_for(&lcu, &context).await;
    if let Some(applied) =
        apply_top_matchup(&context, position, owned_page_id, tier, apply_spells, import_items)
            .await?
    {
        return Ok(Some(applied));
    }
    let loaded = data::load_for(&lcu, &context, &catalog, tier, position).await?;
    let spells = loaded.spell_pair;
    let Some(preset) = loaded.selections.first() else {
        return Ok(None);
    };
    let applied = apply(
        preset.selection.clone(),
        context.champion_id,
        &context.champion_name,
        Some(0),
        owned_page_id,
        spells,
        apply_spells,
    )
    .await?;
    if import_items {
        spawn_preset_items_import(
            Some(position.to_string()),
            tier.to_string(),
            preset.selection.keystone,
            None,
        );
    }
    Ok(Some(applied))
}

/// Applies the lane matchup build instead of the generic top preset, but only
/// when the sample is large enough to trust without the player looking at it.
/// `None` means the generic preset should be used.
async fn apply_top_matchup(
    context: &session::ChampSelectContext,
    position: &str,
    owned_page_id: Option<i64>,
    tier: &str,
    apply_spells: bool,
    import_items: bool,
) -> Result<Option<AppliedView>, RuneError> {
    let Some(enemy_id) = context.enemy_champion_id else {
        return Ok(None);
    };
    let Some(matchup) = matchup_view(context.champion_id, enemy_id, position, tier).await else {
        return Ok(None);
    };
    // Never auto-apply from cached data left over after a failed fetch.
    let trusted = !matchup.stale
        && matchup
            .stats
            .as_ref()
            .is_some_and(|stats| stats.games >= AUTO_APPLY_MATCHUP_GAMES);
    let Some(preset) = matchup.preset.filter(|_| trusted && !matchup.fallback) else {
        return Ok(None);
    };
    let selection = RuneSelection {
        primary_page_id: preset.primary_page_id,
        secondary_page_id: preset.secondary_page_id,
        keystone: preset.keystone,
        primary_runes: preset.primary_runes,
        secondary_runes: preset.secondary_runes,
        shards: preset.shards,
    };
    let applied = apply(
        selection,
        context.champion_id,
        &context.champion_name,
        None,
        owned_page_id,
        spells::pair_from_ids(&preset.spells),
        apply_spells,
    )
    .await?;
    if import_items {
        spawn_preset_items_import(
            Some(position.to_string()),
            tier.to_string(),
            preset.keystone,
            Some(enemy_id),
        );
    }
    Ok(Some(applied))
}

/// Applies a selection to the champion currently in champion select.
pub async fn apply_selection(
    selection: RuneSelection,
    preset_index: Option<usize>,
    owned_page_id: Option<i64>,
    spells: Option<[i64; 2]>,
    apply_spells: bool,
) -> Result<AppliedView, RuneError> {
    let (_, context) = match super::rune_context().await {
        Ok(super::RuneContext { phase, context }) => (phase, context),
        Err(error) => return Err(error),
    };
    let Some(mut context) = context else {
        return Err(RuneError::conflict("Champion select is not active."));
    };
    if context.champion_name.trim().is_empty() {
        if let Ok(names) = data::champion_names().await {
            if let Some(name) = names.get(&context.champion_id) {
                context.champion_name = name.clone();
            }
        }
    }
    apply(
        selection,
        context.champion_id,
        &context.champion_name,
        preset_index,
        owned_page_id,
        spells,
        apply_spells,
    )
    .await
}

/// Adds the item build people play with `keystone` to the League shop for
/// the champion in champion select, replacing Swapper's previous item set.
/// `position` is the role the player picked, if any; otherwise the assigned
/// or detected role is used.
///
/// With an `enemy_champion_id`, the lane matchup's build is imported (and the
/// set is titled "Champion vs Enemy") when it has a large enough sample;
/// otherwise the keystone's generic build is.
pub async fn import_preset_items(
    position: Option<&str>,
    tier: &str,
    keystone: i64,
    enemy_champion_id: Option<i64>,
) -> Result<(), RuneError> {
    let (_, context) = match super::rune_context().await {
        Ok(super::RuneContext { phase, context }) => (phase, context),
        Err(error) => return Err(error),
    };
    let Some(mut context) = context else {
        return Err(RuneError::conflict("Champion select is not active."));
    };
    if context.champion_name.trim().is_empty() {
        if let Ok(names) = data::champion_names().await {
            if let Some(name) = names.get(&context.champion_id) {
                context.champion_name = name.clone();
            }
        }
    }
    let position = match position.and_then(session::position_from_request) {
        Some(position) => position,
        None => super::position_for(&super::lcu().await?, &context).await,
    };
    let matchup = match validated_enemy(&context, enemy_champion_id) {
        Some(enemy_id) => matchup_view(context.champion_id, enemy_id, position, tier).await,
        None => None,
    };
    let (build, matchup_enemy) = match matchup.and_then(|m| m.build.map(|b| (b, m.enemy_name))) {
        Some((build, enemy_name)) => (build, Some(enemy_name)),
        None => {
            let Some(build) =
                view::preset_build_view(context.champion_id, position, tier, keystone).await
            else {
                return Ok(());
            };
            (build, None)
        }
    };
    let label = super::item_sets::build_label(build.skill_priority.as_deref());
    let title_name = match &matchup_enemy {
        Some(enemy) => super::item_sets::matchup_title_name(&context.champion_name, enemy, &label),
        None => context.champion_name.clone(),
    };
    super::item_sets::import_keystone_build(context.champion_id, &title_name, &label, &build)
        .await
        .map(|_| ())
}

/// The requested enemy, only if it is actually revealed on the enemy team in
/// this champion select; the id comes from the client and is not trusted.
fn validated_enemy(context: &session::ChampSelectContext, requested: Option<i64>) -> Option<i64> {
    requested.filter(|id| context.enemy_champion_ids.contains(id))
}

/// Starts [`import_preset_items`] in the background, so importing items never
/// delays the rune page. A failure only means no item set this time.
pub fn spawn_preset_items_import(
    position: Option<String>,
    tier: String,
    keystone: i64,
    enemy_champion_id: Option<i64>,
) {
    tauri::async_runtime::spawn(async move {
        let _ = import_preset_items(position.as_deref(), &tier, keystone, enemy_champion_id).await;
    });
}

#[cfg(test)]
mod tests {
    use super::super::page::PagePlan;
    use super::*;

    #[test]
    fn a_requested_enemy_must_be_on_the_enemy_team() {
        let context = session::parse_champ_select(
            r#"{
                "localPlayerCellId": 0,
                "myTeam": [{"cellId": 0, "championId": 103, "assignedPosition": ""}],
                "theirTeam": [{"cellId": 5, "championId": 238}, {"cellId": 6, "championId": 0}],
                "actions": []
            }"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(validated_enemy(&context, Some(238)), Some(238));
        assert_eq!(validated_enemy(&context, Some(266)), None);
        assert_eq!(validated_enemy(&context, None), None);
    }

    fn custom(id: i64) -> LcuPage {
        LcuPage {
            id,
            name: format!("Page {id}"),
            is_editable: true,
            is_deletable: true,
            is_temporary: false,
        }
    }

    fn built_in(id: i64) -> LcuPage {
        LcuPage {
            id,
            name: format!("Built-in {id}"),
            is_editable: false,
            is_deletable: false,
            is_temporary: false,
        }
    }

    fn inventory(
        can_add: Option<bool>,
        custom_count: Option<u32>,
        owned: Option<u32>,
    ) -> Inventory {
        Inventory {
            can_add_custom_page: can_add,
            custom_page_count: custom_count,
            owned_page_count: owned,
            is_custom_page_creation_unlocked: Some(true),
        }
    }

    fn plan_for(inv: &Inventory, pages: &[LcuPage], owned: Option<i64>) -> PagePlan {
        super::super::page::plan(pages, inv.allows_new_page(pages), owned)
    }

    #[test]
    fn built_in_pages_do_not_count_against_the_custom_capacity() {
        // Three League built-ins plus one user page, capacity three.
        let pages = vec![built_in(1), built_in(2), built_in(3), custom(4)];
        let inv = inventory(None, None, Some(3));
        assert!(inv.allows_new_page(&pages));
        assert_eq!(plan_for(&inv, &pages, None), PagePlan::Create);
    }

    #[test]
    fn a_full_inventory_with_no_owned_page_is_a_limit() {
        let pages = vec![built_in(1), built_in(2), custom(3), custom(4)];
        let inv = inventory(None, Some(2), Some(2));
        assert!(!inv.allows_new_page(&pages));
        assert_eq!(plan_for(&inv, &pages, None), PagePlan::LimitReached);
    }

    #[test]
    fn the_capacity_fallback_counts_only_user_pages() {
        // Capacity two: one user page and one built-in still has room.
        let inv = inventory(None, None, Some(2));
        let pages = vec![built_in(1), custom(2)];
        assert!(inv.allows_new_page(&pages));
        // A second user page fills the capacity.
        let full = vec![built_in(1), custom(2), custom(3)];
        assert!(!inv.allows_new_page(&full));
    }

    #[test]
    fn the_clients_can_add_answer_wins_over_the_counts() {
        let pages = vec![built_in(1), built_in(2)];
        let open = inventory(Some(true), Some(9), Some(0));
        assert!(open.allows_new_page(&pages));
        let shut = inventory(Some(false), Some(0), Some(99));
        assert!(!shut.allows_new_page(&pages));
    }

    #[test]
    fn locked_custom_page_creation_is_a_limit() {
        let inv = Inventory {
            can_add_custom_page: None,
            custom_page_count: None,
            owned_page_count: Some(5),
            is_custom_page_creation_unlocked: Some(false),
        };
        assert!(!inv.allows_new_page(&[custom(1)]));
    }

    #[test]
    fn an_unknown_inventory_lets_league_reject_the_create() {
        let inv = Inventory::default();
        assert!(inv.allows_new_page(&[]));
    }
}
