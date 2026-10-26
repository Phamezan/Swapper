//! The pro builds list: probuildstats/u.gg matches as cards.

use super::*;

/// `"16_19"` -> `"16.19"`, the patch label players recognise.
pub(super) fn patch_label(version: &str) -> String {
    let version = version.trim();
    if version.is_empty() {
        "–".to_string()
    } else {
        version.replace('_', ".")
    }
}

/// A short "how long ago" string from millisecond timestamps.
pub(super) fn played_ago(now_ms: i64, at_ms: i64) -> String {
    let seconds = ((now_ms - at_ms).max(0)) / 1000;
    if seconds < 60 {
        "just now".to_string()
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86_400)
    }
}

pub(super) fn now_ms() -> i64 {
    super::super::provider::now_ms()
}

/// Fewer finished items than this, boots aside, is a remake or early surrender, not a build to copy.
const MIN_CORE_ITEMS: usize = 2;

pub(super) fn pro_build_view(
    matched: &probuilds::ProMatch,
    now: i64,
    catalog: &items::Catalog,
) -> Option<ProBuildView> {
    let selection = matched.selection()?;
    let built = matched.built_items(catalog);
    if built.iter().filter(|id| !catalog.is_boots(**id)).count() < MIN_CORE_ITEMS {
        return None;
    }
    let names = &*catalog.names();
    let team = if matched.pro_info.current_team.trim().is_empty() {
        matched.current_team.trim()
    } else {
        matched.pro_info.current_team.trim()
    };
    Some(ProBuildView {
        match_id: matched.match_id,
        pro_name: matched.display_name().to_string(),
        team: team.to_string(),
        league: matched.pro_league.clone(),
        region: probuilds::region_for_league(&matched.pro_league).to_string(),
        role: matched.calculated_role.clone(),
        role_label: probuilds::role_label(&matched.calculated_role).to_string(),
        win: matched.win,
        played_ago: played_ago(now, matched.match_timestamp),
        patch: patch_label(&matched.version),
        keystone: selection.keystone,
        primary_page_id: selection.primary_page_id,
        secondary_page_id: selection.secondary_page_id,
        primary_runes: selection.primary_runes,
        secondary_runes: selection.secondary_runes,
        shards: selection.shards,
        kills: matched.total_kills,
        deaths: matched.total_deaths,
        assists: matched.total_assists,
        spells: matched
            .summoner_spells
            .iter()
            .copied()
            .filter(|id| *id > 0)
            .take(2)
            .collect(),
        final_items: item_views(&matched.final_items(), names),
        completed_items: item_views(&built, names),
        item_path: matched
            .item_path_groups()
            .into_iter()
            .map(|(minute, items)| ItemPathGroupView {
                minute,
                items: items
                    .into_iter()
                    .map(|(id, count)| ItemStackView {
                        id,
                        name: item_name(names, id),
                        count,
                    })
                    .collect(),
            })
            .collect(),
        skill_order: matched
            .skill_order()
            .into_iter()
            .map(|letter| letter.to_string())
            .collect(),
        otp: probuilds::is_otp(team),
    })
}

pub(super) fn unavailable_pro_builds(champion_id: i64, position: &str, role: &str, page: u32, message: &str) -> ProBuildsView {
    ProBuildsView {
        champion_id,
        position: position.to_string(),
        role: role.to_string(),
        page,
        has_more: false,
        matches: Vec::new(),
        unavailable: true,
        message: Some(message.to_string()),
        stale: false,
        updated_at: None,
    }
}

/// Builds the Pro builds tab for a champion and role. The frontend calls this
/// once when the tab opens and again for each explicit "load more". `is_otp`
/// selects the OTP list instead of the pro list.
pub async fn pro_builds_view(champion_id: i64, position: &str, page: u32, is_otp: bool) -> ProBuildsView {
    let role = probuilds::role_arg(position).unwrap_or("all");
    let page = page.max(1);
    if champion_id <= 0 {
        return ProBuildsView {
            champion_id,
            position: position.to_string(),
            role: role.to_string(),
            page,
            has_more: false,
            matches: Vec::new(),
            unavailable: false,
            message: Some("Pick a champion to load pro builds.".to_string()),
            stale: false,
            updated_at: None,
        };
    }
    match pro_page(champion_id, position, page, is_otp).await {
        Ok(ProPage { matches, has_more, stale, fetched_at }) => {
            let now = now_ms();
            // Names come from the client's item catalog; empty when unavailable,
            // in which case each icon falls back to "Item <id>".
            let catalog = if matches.is_empty() { None } else { items::catalog().await.ok() };
            let catalog = catalog.unwrap_or_default();
            let views: Vec<ProBuildView> = matches
                .iter()
                .filter_map(|entry| pro_build_view(entry, now, &catalog))
                .collect();
            ProBuildsView {
                champion_id,
                position: position.to_string(),
                role: role.to_string(),
                page,
                has_more,
                matches: views,
                unavailable: false,
                message: None,
                stale,
                updated_at: fetched_at,
            }
        }
        Err(error) => unavailable_pro_builds(
            champion_id,
            position,
            role,
            page,
            error.message(),
        ),
    }
}

/// u.gg API pages read for one "Pros" page. The unfiltered list is mostly
/// one-trick games for some champions (Graves jungle: 2-5 pros per 20 rows),
/// so one API page alone can leave the tab nearly empty.
pub(super) const PRO_API_PAGES_PER_PAGE: u32 = 3;

pub(super) struct ProPage {
    matches: Vec<probuilds::ProMatch>,
    has_more: bool,
    stale: bool,
    fetched_at: Option<i64>,
}

pub(super) fn is_otp_entry(entry: &probuilds::ProMatch) -> bool {
    probuilds::is_otp(&entry.current_team) || probuilds::is_otp(&entry.pro_info.current_team)
}

/// One page of the Pros or OTPs list. u.gg's `isOtp: false` list is every
/// recent game, pros and one-tricks mixed, so "Pros" filters the one-tricks out
/// of it. `isOtp: true` is the one-trick list; when it is empty (u.gg tracks no
/// one-trick for that champion and role) the first page falls back to the
/// one-trick games found in the mixed list.
pub(super) async fn pro_page(champion_id: i64, position: &str, page: u32, is_otp: bool) -> Result<ProPage, super::super::RuneError> {
    if is_otp {
        let sourced = data::pro_builds(champion_id, position, page, true).await?;
        if !sourced.value.is_empty() || page > 1 {
            return Ok(ProPage {
                has_more: sourced.value.len() >= probuilds::PAGE_SIZE,
                matches: sourced.value,
                stale: sourced.stale,
                fetched_at: sourced.fetched_at,
            });
        }
        let mut mixed = mixed_pages(champion_id, position, 1).await?;
        mixed.matches.retain(is_otp_entry);
        mixed.has_more = false;
        return Ok(mixed);
    }
    let mut mixed = mixed_pages(champion_id, position, page).await?;
    mixed.matches.retain(|entry| !is_otp_entry(entry));
    Ok(mixed)
}

/// The mixed (pros and one-tricks) games behind one tab page: up to
/// [`PRO_API_PAGES_PER_PAGE`] API pages, stopping at the end of the list. Only
/// a failure on the first API page is an error; a later one ends the page early.
pub(super) async fn mixed_pages(champion_id: i64, position: &str, page: u32) -> Result<ProPage, super::super::RuneError> {
    let first = (page - 1) * PRO_API_PAGES_PER_PAGE + 1;
    let mut out = ProPage { matches: Vec::new(), has_more: true, stale: false, fetched_at: None };
    // The API pages are independent requests: fetch them together, then read them in order.
    let pages = futures_util::future::join_all(
        (first..first + PRO_API_PAGES_PER_PAGE).map(|api_page| data::pro_builds(champion_id, position, api_page, false)),
    )
    .await;
    for (api_page, result) in (first..).zip(pages) {
        let sourced = match result {
            Ok(sourced) => sourced,
            Err(error) if api_page == first => return Err(error),
            Err(_) => break,
        };
        let full = sourced.value.len() >= probuilds::PAGE_SIZE;
        out.stale |= sourced.stale;
        out.fetched_at = out.fetched_at.or(sourced.fetched_at);
        out.matches.extend(sourced.value);
        if !full {
            out.has_more = false;
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod live_pro_page_check {
    /// Network: Graves jungle's mixed list is mostly one-tricks and Gnar top has
    /// no u.gg one-trick list, so both exercise the filter and the fallback.
    #[tokio::test]
    #[ignore = "network: hits u.gg"]
    async fn pros_exclude_one_tricks_and_otps_fall_back_to_the_mixed_list() {
        for (id, pos) in [(104, "jungle"), (150, "top")] {
            let pros = super::pro_page(id, pos, 1, false).await.unwrap();
            assert!(!pros.matches.is_empty(), "{id} {pos}: no pros");
            assert!(!pros.matches.iter().any(super::is_otp_entry), "{id} {pos}: one-trick in Pros");
            let otps = super::pro_page(id, pos, 1, true).await.unwrap();
            assert!(otps.matches.iter().all(super::is_otp_entry), "{id} {pos}: pro in OTPs");
        }
    }
}
