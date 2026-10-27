use crate::state::{view, AppView};
use crate::{runes, AppState};
use base64::Engine;
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager, State};

#[tauri::command]
pub(crate) fn set_auto_apply_top_preset(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppView, String> {
    state.set_auto_apply_top_preset(enabled)?;
    // Turning the setting on with the champion already locked applies now,
    // instead of waiting for the watcher's next poll.
    if enabled {
        tauri::async_runtime::spawn(async move {
            let _ = runes::auto_apply_current(&app).await;
        });
    }
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(view(
        &config,
        &state.bundled_deceive,
        &state.remote.status(),
        false,
        state.hotkey_active.load(Ordering::SeqCst),
    ))
}

#[tauri::command]
pub(crate) async fn get_runes(
    state: State<'_, AppState>,
    position: Option<String>,
) -> Result<runes::RunesView, String> {
    let mut view = runes::view(
        state.auto_apply_top_preset(),
        state.apply_spells_with_runes(),
        &state.rune_tier(),
        position.as_deref(),
    )
    .await;
    view.import_items = state.import_items_with_runes();
    Ok(view)
}

#[tauri::command]
pub(crate) fn set_apply_spells_with_runes(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppView, String> {
    state.set_apply_spells_with_runes(enabled)?;
    let view = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        view(
            &config,
            &state.bundled_deceive,
            &state.remote.status(),
            false,
            state.hotkey_active.load(Ordering::SeqCst),
        )
    };
    notify_runes_changed(&app, &state);
    Ok(view)
}

/// Where recommended spells put Flash: "d", "f" or "keep".
/// The rune pages the user saved for a champion.
#[tauri::command]
pub(crate) fn get_rune_presets(state: State<'_, AppState>, champion_id: i64) -> Vec<crate::vault::SavedRunes> {
    state.rune_presets(champion_id)
}

#[tauri::command]
pub(crate) fn save_rune_preset(state: State<'_, AppState>, champion_id: i64, name: String, selection: runes::RuneSelection) -> Result<Vec<crate::vault::SavedRunes>, String> {
    state.change_rune_presets(champion_id, |config| config.save_rune_preset(champion_id, &name, selection))
}

#[tauri::command]
pub(crate) fn delete_rune_preset(state: State<'_, AppState>, champion_id: i64, name: String) -> Result<Vec<crate::vault::SavedRunes>, String> {
    state.change_rune_presets(champion_id, |config| {
        config.delete_rune_preset(champion_id, &name);
        Ok(())
    })
}

#[tauri::command]
pub(crate) fn get_flash_key(state: State<'_, AppState>) -> String {
    state.flash_key()
}

#[tauri::command]
pub(crate) fn set_flash_key(app: tauri::AppHandle, state: State<'_, AppState>, key: String) -> Result<String, String> {
    state.set_flash_key(&key)?;
    notify_runes_changed(&app, &state);
    Ok(state.flash_key())
}

#[tauri::command]
pub(crate) fn set_import_items_with_runes(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AppView, String> {
    state.set_import_items_with_runes(enabled)?;
    let view = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        view(
            &config,
            &state.bundled_deceive,
            &state.remote.status(),
            false,
            state.hotkey_active.load(Ordering::SeqCst),
        )
    };
    notify_runes_changed(&app, &state);
    Ok(view)
}

/// Applies one summoner spell to a slot, swapping when it is already in the
/// other slot. Published so both surfaces reload the new spells.
#[tauri::command]
pub(crate) async fn apply_spell(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    slot: String,
    spell_id: i64,
) -> Result<(), String> {
    let slot = if slot.eq_ignore_ascii_case("d") || slot == "1" {
        0
    } else {
        1
    };
    runes::spells::apply_pick(slot, spell_id)
        .await
        .map_err(|e| e.message().to_string())?;
    notify_runes_changed(&app, &state);
    Ok(())
}

#[tauri::command]
pub(crate) fn set_rune_tier(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    tier: String,
) -> Result<AppView, String> {
    state.set_rune_tier(&tier)?;
    let view = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        view(
            &config,
            &state.bundled_deceive,
            &state.remote.status(),
            false,
            state.hotkey_active.load(Ordering::SeqCst),
        )
    };
    notify_runes_changed(&app, &state);
    Ok(view)
}

/// Nudges the desktop flyout and the phone to reload runes with the new setting.
fn notify_runes_changed(app: &tauri::AppHandle, state: &AppState) {
    state.remote.notify_runes_changed();
    let _ = app.emit("runes_changed", serde_json::Value::Null);
}

#[tauri::command]
pub(crate) async fn champ_select_status() -> runes::ChampSelectEvent {
    runes::current_status().await
}

#[tauri::command]
pub(crate) async fn rune_icon(id: i64) -> Result<String, String> {
    let bytes = runes::icon(id).await.map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub(crate) async fn role_icon(role: String) -> Result<String, String> {
    let bytes = runes::roles::icon(&role)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub(crate) async fn spell_icon(id: i64) -> Result<String, String> {
    let bytes = runes::spells::icon(id)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub(crate) async fn item_icon(id: i64) -> Result<String, String> {
    let bytes = runes::items::icon(id)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// A match-history objective icon (dragon, baron, ...) as a data URL; only listed names are served.
#[tauri::command]
pub(crate) async fn objective_icon(name: String) -> Result<String, String> {
    let bytes = runes::objectives::icon(&name)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub(crate) async fn rank_icon(tier: String) -> Result<String, String> {
    let bytes = runes::ranks::icon(&tier)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// League's level frame for a summoner level, as a data URL.
#[tauri::command]
pub(crate) async fn level_border(level: u64) -> Result<String, String> {
    let bytes = runes::ranks::level_border(level).await.map_err(|e| e.message().to_string())?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// The full ranked emblem behind the dashboard's rank card, as a data URL.
#[tauri::command]
pub(crate) async fn rank_emblem(tier: String) -> Result<String, String> {
    let bytes = runes::ranks::emblem(&tier)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
pub(crate) async fn get_pro_builds(
    champion_id: i64,
    position: String,
    page: Option<u32>,
    is_otp: Option<bool>,
) -> runes::ProBuildsView {
    runes::pro_builds_view(
        champion_id,
        &position,
        page.unwrap_or(1),
        is_otp.unwrap_or(false),
    )
    .await
}

/// The PNG bytes for a pro team's logo, addressed by the team's display name.
#[tauri::command]
pub(crate) async fn team_icon(team: String) -> Result<String, String> {
    let bytes = runes::teams::icon(&team)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// The 6-item build people build with one preset's keystone, from lolalytics.
/// `None` means there is no build to show for that card.
#[tauri::command]
pub(crate) async fn get_keystone_build(
    champion_id: i64,
    position: String,
    tier: String,
    keystone: i64,
    most_played: Option<runes::RuneSelection>,
) -> Option<runes::PresetBuildView> {
    runes::preset_build_view(
        champion_id,
        &position,
        &tier,
        keystone,
        most_played.as_ref(),
    )
    .await
}

/// A common build on another keystone than the ones already shown (`exclude`),
/// from lolalytics. `None` when no other keystone is played often enough.
#[tauri::command]
pub(crate) async fn get_other_build(
    champion_id: i64,
    position: String,
    tier: String,
    exclude: Vec<i64>,
) -> Option<runes::view::HighestWinView> {
    runes::other_build_view(champion_id, &position, &tier, &exclude).await
}

/// The lane matchup build against the enemy laner, from lolalytics. `None`
/// means there is nothing to show; a low sample comes back flagged `fallback`.
#[tauri::command]
pub(crate) async fn get_matchup(
    champion_id: i64,
    enemy_champion_id: i64,
    position: String,
    tier: String,
) -> Option<runes::MatchupView> {
    runes::matchup_view(champion_id, enemy_champion_id, &position, &tier).await
}

/// A champion's matchup table (who beats it, who it beats), from lolalytics.
#[tauri::command]
pub(crate) async fn get_champion_counters(
    champion_id: i64,
    position: Option<String>,
    tier: String,
) -> runes::ChampionCountersView {
    runes::champion_counters_view(champion_id, position.as_deref(), &tier).await
}

/// A champion's tier, rates, lane rank and most common build, from lolalytics.
#[tauri::command]
pub(crate) async fn get_champion_overview(
    champion_id: i64,
    position: Option<String>,
    tier: String,
) -> runes::ChampionOverviewView {
    runes::champion_overview_view(champion_id, position.as_deref(), &tier).await
}

/// The latest League patch notes from Riot's site, parsed and cached on disk.
#[tauri::command]
pub(crate) async fn get_latest_patch_notes(
    force: Option<bool>,
    version: Option<String>,
) -> Result<runes::patch_notes::PatchNotes, String> {
    runes::patch_notes::latest_patch_notes(force.unwrap_or(false), version).await
}

/// The newest patch with published notes, e.g. "26.21".
#[tauri::command]
pub(crate) async fn newest_patch_notes() -> Result<String, String> {
    runes::patch_notes::newest_patch().await
}

/// Key art for one of the two selectable patches, as a data URL.
#[tauri::command]
pub(crate) async fn patch_banner(version: String) -> Result<String, String> {
    runes::patch_notes::banner(&version).await
}

/// The current game patch, for the sidebar badge.
#[tauri::command]
pub(crate) async fn get_current_patch() -> Option<String> {
    runes::patch::patches().await.map(|patches| patches.current)
}

/// The strongest champions for a role and rank bracket, from lolalytics.
#[tauri::command]
pub(crate) async fn get_tier_list(position: String, tier: String) -> runes::TierListView {
    runes::tier_list_view(&position, &tier).await
}

/// The best bot lane ADC + Support pairs for a rank bracket, from u.gg.
#[tauri::command]
pub(crate) async fn get_duo_list(tier: String) -> runes::DuoListView {
    runes::duo_list_view(&tier).await
}

/// Every champion, for the Champion tab search box.
#[tauri::command]
pub(crate) async fn get_champion_list() -> Vec<runes::ChampionOption> {
    runes::champion_list().await
}

#[tauri::command]
pub(crate) async fn champion_icon(id: i64) -> Result<String, String> {
    let bytes = runes::champion_icon::icon(id)
        .await
        .map_err(|e| e.message().to_string())?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Imports a whole build as starter, build and alternatives blocks, like auto-apply does.
#[tauri::command]
pub(crate) async fn import_full_item_build(
    champion_id: i64,
    champion_name: String,
    build: runes::view::KeystoneBuildView,
) -> Result<String, String> {
    runes::item_sets::import_keystone_build(champion_id, &champion_name, &build)
        .await
        .map_err(|error| error.message().to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri exposes these as named command arguments.
pub(crate) async fn apply_rune_page(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    selection: runes::RuneSelection,
    preset_index: Option<usize>,
    spells: Option<Vec<i64>>,
    position: Option<String>,
    enemy_champion_id: Option<i64>,
    highest_win: Option<bool>,
) -> Result<runes::AppliedView, String> {
    let owned = state.rune_page_id();
    let keystone = selection.keystone;
    let spells = spells.as_deref().and_then(runes::spells::pair_from_ids);
    let applied = runes::apply_selection(
        selection,
        preset_index,
        owned,
        spells,
        state.apply_spells_with_runes(),
    )
    .await
    .map_err(|e| e.message().to_string())?;
    // Presets carry a recommended item build; exact pro pages import items
    // through their own button.
    if (preset_index.is_some() || enemy_champion_id.is_some() || highest_win == Some(true))
        && state.import_items_with_runes()
    {
        runes::spawn_preset_items_import(
            position,
            state.rune_tier(),
            keystone,
            enemy_champion_id,
            highest_win.unwrap_or(false),
        );
    }
    if let Some(id) = applied.page_id {
        let handle = app.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Some(state) = handle.try_state::<AppState>() {
                let _ = state.set_rune_page_id(id);
            }
        })
        .await;
    }
    state.remote.clone().publish_runes(applied.clone());
    let _ = app.emit("runes_changed", applied.clone());
    Ok(applied)
}
