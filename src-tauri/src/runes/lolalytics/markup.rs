//! The fallback build parser for lolalytics' visible markup.

use super::*;

/// Fallback parser for the visible markup, used when the Qwik state is absent
/// or unrecognised. It can recover the starting items and common item path, but
/// the markup does not provide reliable per-candidate sample details.
pub(super) fn parse_anchors(html: &str) -> Option<KeystoneBuild> {
    let start_at = html.find("Starting Items");
    let core_at = html.find("Core Build")?;
    let item4_at = core_at + html[core_at..].find("Item 4")?;
    let item5_at = item4_at + html[item4_at..].find("Item 5")?;
    let item6_at = item5_at + html[item5_at..].find("Item 6")?;
    let mut items: Vec<i64> = Vec::new();
    for id in item_ids(&html[core_at..item4_at]) {
        if !items.contains(&id) {
            items.push(id);
            if items.len() == 3 {
                break;
            }
        }
    }
    if items.is_empty() {
        return None;
    }
    let boundary = (item6_at + 6000).min(html.len());
    for (from, to) in [
        (item4_at, item5_at),
        (item5_at, item6_at),
        (item6_at, boundary),
    ] {
        if let Some(id) = item_ids(&html[from..to])
            .into_iter()
            .find(|id| !items.contains(id))
        {
            items.push(id);
        }
    }
    let start_fragment = start_at
        .filter(|start| *start < core_at)
        .map(|start| &html[start..core_at]);
    let games = start_fragment.map(games_between).unwrap_or(0);
    let starters = start_fragment
        .map(item_ids)
        .unwrap_or_default()
        .into_iter()
        .map(|id| ItemStack { id, count: 1 })
        .collect();
    Some(KeystoneBuild {
        starters,
        items,
        options: Vec::new(),
        games,
        core_len: 0,
        core_games: 0,
        core_win_pct: None,
        skill_priority: None,
        skill_order: None,
        slots: Vec::new(),
        patch: None,
    })
}

/// The item ids in a markup fragment, in order, collapsing the adjacent pair
/// each icon produces (`srcset` plus `src`).
pub(super) fn item_ids(fragment: &str) -> Vec<i64> {
    let mut ids: Vec<i64> = Vec::new();
    let mut rest = fragment;
    while let Some(at) = rest.find("item64/") {
        rest = &rest[at + "item64/".len()..];
        let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
        if digits.is_empty() {
            continue;
        }
        if let Ok(id) = digits.parse::<i64>() {
            if ids.last() != Some(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

/// The game count from the starting-items row, e.g. `70,627 Games`.
pub(super) fn games_between(fragment: &str) -> u64 {
    let text = strip_tags(fragment);
    let Some(at) = text.find("Games") else {
        return 0;
    };
    let before = &text[..at];
    let digits: String = before
        .chars()
        .rev()
        .take_while(|ch| ch.is_ascii_digit() || *ch == ',')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.replace(',', "").parse().unwrap_or(0)
}

/// Drops HTML tags and comments so text patterns can be matched directly.
pub(super) fn strip_tags(fragment: &str) -> String {
    let mut out = String::with_capacity(fragment.len());
    let mut rest = fragment;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        match rest[at..].find('>') {
            Some(end) => rest = &rest[at + end + 1..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}
