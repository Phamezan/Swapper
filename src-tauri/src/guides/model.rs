use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

pub(super) const MAX_CHAMPION_BYTES: usize = 2 * 1024 * 1024;
pub(super) const MAX_INDEX_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum Lane {
    #[serde(rename = "top")] Top,
    #[serde(rename = "jungle")] Jungle,
    #[serde(rename = "mid")] Mid,
    #[serde(rename = "adc")] Adc,
    #[serde(rename = "support")] Support,
}

pub(crate) fn lane(value: &str) -> Option<Lane> {
    serde_json::from_value(serde_json::Value::String(value.into())).ok()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SectionId {
    #[serde(rename = "overview")] Overview,
    #[serde(rename = "early-lane")] EarlyLane,
    #[serde(rename = "mid-lane")] MidLane,
    #[serde(rename = "win-condition")] WinCondition,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) enum Scope {
    #[serde(rename = "champion")] Champion,
    #[serde(rename = "enemy")] Enemy,
    #[serde(rename = "pair")] Pair,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) enum AbilityKey { P, Q, W, E, R }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AbilityRef { champion: i64, key: AbilityKey }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum Outdated { Flag(bool), Reason(String) }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Tip {
    #[serde(skip_serializing_if = "Option::is_none")] heading: Option<String>,
    pub text: String,
    scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")] source_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub patch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] ability_keys: Option<Vec<AbilityRef>>,
    #[serde(skip_serializing_if = "Option::is_none")] item_ids: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub outdated: Option<Outdated>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Alternative { title: String, source_id: String, tips: Vec<Tip> }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Timeline { at: String, champion: String, enemy: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Section {
    tips: Vec<Tip>,
    #[serde(skip_serializing_if = "Option::is_none")] alternatives: Option<Vec<Alternative>>,
    #[serde(skip_serializing_if = "Option::is_none")] timeline: Option<Vec<Timeline>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) enum SourceKind {
    #[serde(rename = "mobafire")] Mobafire,
    #[serde(rename = "sheet")] Sheet,
    #[serde(rename = "youtube")] Youtube,
    #[serde(rename = "web")] Web,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Source { id: String, author: String, rank_claim: String, url: String, kind: SourceKind }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) enum Difficulty { Easy, Medium, Hard, #[serde(rename = "Skill matchup")] SkillMatchup }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Guide {
    pub champion: i64,
    // No enemy denotes a general guide; parsing enforces the two content shapes.
    #[serde(skip_serializing_if = "Option::is_none")] pub enemy: Option<i64>,
    pub lane: Lane,
    pub patch: String,
    reviewed: bool,
    #[serde(skip_serializing_if = "Option::is_none")] difficulty: Option<Difficulty>,
    #[serde(skip_serializing_if = "Option::is_none")] favored_when: Option<String>,
    key_tips: [Tip; 3],
    #[serde(skip_serializing_if = "Option::is_none")] enemy_tips: Option<BTreeMap<SectionId, Vec<Tip>>>,
    sections: BTreeMap<SectionId, Section>,
    itemization: Vec<Tip>,
    sources: Vec<Source>,
}

impl Guide {
    pub(super) fn tips_mut(&mut self) -> impl Iterator<Item = &mut Tip> {
        self.key_tips.iter_mut().chain(self.itemization.iter_mut())
            .chain(self.enemy_tips.iter_mut().flat_map(|tips| tips.values_mut().flatten()))
            .chain(self.sections.values_mut().flat_map(|s| s.tips.iter_mut()
                .chain(s.alternatives.iter_mut().flatten().flat_map(|a| a.tips.iter_mut()))))
    }

    fn valid(&self, general: bool) -> bool {
        let enemy = self.enemy.unwrap_or(self.champion);
        let prose = |s: &str| valid_text(s, self.champion, enemy);
        let sources: HashSet<_> = self.sources.iter().map(|s| s.id.as_str()).collect();
        let tip = |t: &Tip| prose(&t.text) && !t.text.trim().is_empty()
            && matches!(t.scope, Scope::Champion) == general
            && t.heading.as_deref().is_none_or(prose)
            && t.patch.as_deref().is_none_or(|p| patch_number(p).is_some())
            && t.source_ids.iter().flatten().all(|id| sources.contains(id.as_str()))
            && t.ability_keys.iter().flatten().all(|a| a.champion == self.champion || a.champion == enemy)
            && t.item_ids.iter().flatten().all(|id| *id > 0 && *id <= i32::MAX as i64)
            && match &t.outdated { Some(Outdated::Flag(false)) => false, Some(Outdated::Reason(s)) => prose(s), _ => true };
        self.champion > 0 && enemy > 0 && self.champion <= i32::MAX as i64 && enemy <= i32::MAX as i64
            && if general { self.enemy.is_none() && self.difficulty.is_none() && self.favored_when.is_none() && self.enemy_tips.is_none() }
                else { self.enemy.is_some() && self.difficulty.is_some() && self.favored_when.as_deref().is_some_and(prose) && self.enemy_tips.is_some() }
            && patch_number(&self.patch).is_some()
            && self.sources.len() == sources.len()
            && self.sources.iter().all(|s| !s.id.is_empty() && s.id.len() <= 128 && prose(&s.author) && prose(&s.rank_claim)
                && s.url.len() <= 2048 && crate::commands::guide::validate_guide_source_url(&s.url).is_ok())
            && self.sections.len() == 4
            && self.key_tips.iter().all(tip) && self.itemization.iter().all(tip)
            && self.enemy_tips.iter().flat_map(|tips| tips.values().flatten()).all(tip)
            && self.sections.values().all(|s| s.tips.iter().all(tip)
                && s.alternatives.iter().flatten().all(|a| prose(&a.title) && sources.contains(a.source_id.as_str()) && a.tips.iter().all(tip))
                && s.timeline.iter().flatten().all(|t| prose(&t.at) && prose(&t.champion) && prose(&t.enemy)))
    }
}

pub(super) fn patch_number(patch: &str) -> Option<(u32, u32)> {
    let (major, minor) = patch.split_once('.')?;
    if major.is_empty() || minor.is_empty() || patch.len() > 12 || !major.bytes().chain(minor.bytes()).all(|b| b.is_ascii_digit()) { return None; }
    Some((major.parse().ok()?, minor.parse().ok()?))
}

fn valid_text(text: &str, champion: i64, enemy: i64) -> bool {
    static UNSAFE: OnceLock<regex::Regex> = OnceLock::new();
    if text.len() > 16_384 || text.chars().any(|c| c.is_control() && c != '\n' && c != '\t')
        || UNSAFE.get_or_init(|| regex::Regex::new(r"(?i)<\s*[/!a-z][^>]*>|\b\d+(?:[.,]\d+)?[\s–-]*seconds?\b").unwrap()).is_match(text) { return false; }
    let mut rest = text;
    while let Some(at) = rest.find(['{', '}']) {
        rest = &rest[at..];
        if !rest.starts_with('{') { return false; }
        let Some(end) = rest.find('}') else { return false };
        let parts: Vec<_> = rest[1..end].split(':').collect();
        if parts.len() != 3 || parts[0] != "cd"
            || parts[1].starts_with('0') || !parts[1].bytes().all(|b| b.is_ascii_digit())
            || !matches!(parts[2], "Q" | "W" | "E" | "R")
            || !parts[1].parse::<i64>().is_ok_and(|id| id == champion || id == enemy) { return false; }
        rest = &rest[end+1..];
    }
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IndexEntry {
    pub sha256: String, pub guides: usize,
    #[serde(skip_serializing_if = "Option::is_none")] pub general: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct GuideIndex { schema: u32, generated_at: String, pub champions: BTreeMap<String, IndexEntry> }

pub(super) fn parse_index(bytes: &[u8]) -> Option<GuideIndex> {
    if bytes.len() > MAX_INDEX_BYTES { return None; }
    let index: GuideIndex = serde_json::from_slice(bytes).ok()?;
    (index.schema == 1 && !index.generated_at.is_empty() && index.generated_at.len() <= 64
        && index.champions.len() <= 2048 && index.champions.iter().all(|(id, e)| {
            id.parse::<i32>().is_ok_and(|n| n > 0 && n.to_string() == *id)
                && (e.guides > 0 || e.general.unwrap_or(0) > 0) && e.guides <= 2048 && e.general.unwrap_or(0) <= 5
                && e.sha256.len() == 64 && e.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        })).then_some(index)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChampionFile {
    schema: u32, champion: i64, generated_at: String, guides: Vec<serde_json::Value>,
    #[serde(default)] general: Vec<serde_json::Value>,
}

pub(super) fn parse_champion(bytes: &[u8], champion: i64) -> Option<Vec<Guide>> {
    if bytes.len() > MAX_CHAMPION_BYTES { return None; }
    let file: ChampionFile = serde_json::from_slice(bytes).ok()?;
    if file.schema != 1 || file.champion != champion || file.generated_at.is_empty() || file.generated_at.len() > 64 || file.guides.len() > 2048 || file.general.len() > 2048 { return None; }
    let mut guides: Vec<Guide> = file.guides.into_iter().filter_map(|v| serde_json::from_value::<Guide>(v).ok())
        .filter(|g| g.champion == champion && g.valid(false)).collect();
    guides.extend(file.general.into_iter()
        .filter(|v| ["enemy", "difficulty", "favoredWhen", "enemyTips"].iter().all(|key| v.get(key).is_none()))
        .filter_map(|v| serde_json::from_value::<Guide>(v).ok())
        .filter(|g| g.champion == champion && g.valid(true)));
    // Duplicate pair/lane keys are ambiguous; do not pick whichever arrived first.
    let keys: Vec<_> = guides.iter().map(|g| (g.enemy, g.lane.clone())).collect();
    let mut i = 0;
    guides.retain(|_| { let key = &keys[i]; i += 1; keys.iter().filter(|k| *k == key).count() == 1 });
    for guide in &mut guides { for tip in guide.tips_mut() { tip.outdated = None; } }
    Some(guides)
}

pub(super) fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

pub(super) fn verified_champion(bytes: &[u8], sha256: &str, champion: i64) -> Option<Vec<Guide>> {
    (bytes.len() <= MAX_CHAMPION_BYTES && digest(bytes).eq_ignore_ascii_case(sha256))
        .then(|| parse_champion(bytes, champion)).flatten()
}
