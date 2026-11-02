use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

// VODs grow champion files: popular champions carry 1,000+ videos.
pub(super) const MAX_CHAMPION_BYTES: usize = 8 * 1024 * 1024;
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
    #[serde(skip_serializing_if = "Option::is_none")] pub vods: Option<usize>,
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
                && (e.guides > 0 || e.general.unwrap_or(0) > 0 || e.vods.unwrap_or(0) > 0)
                && e.guides <= 2048 && e.general.unwrap_or(0) <= 5 && e.vods.unwrap_or(0) <= 2048
                && e.sha256.len() == 64 && e.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        })).then_some(index)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChampionFile {
    schema: u32, champion: i64, generated_at: String, guides: Vec<serde_json::Value>,
    #[serde(default)] general: Vec<serde_json::Value>,
    #[serde(default)] vods: Vec<serde_json::Value>,
}

pub(super) fn parse_champion(bytes: &[u8], champion: i64) -> Option<Vec<Guide>> {
    if bytes.len() > MAX_CHAMPION_BYTES { return None; }
    let file: ChampionFile = serde_json::from_slice(bytes).ok()?;
    if !file.valid(champion) { return None; }
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

impl ChampionFile {
    fn valid(&self, champion: i64) -> bool {
        self.schema == 1 && self.champion == champion && !self.generated_at.is_empty() && self.generated_at.len() <= 64
            && self.guides.len() <= 2048 && self.general.len() <= 2048 && self.vods.len() <= 2048
    }
}

// VOD types ignore unknown fields: content published later must not make installed apps drop the video.
// Ignored fields are also never forwarded, so content cannot set app-only fields like `fixture`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MatchupVod {
    video_id: String, title: String,
    // Replay channels often name neither the player nor the result; both are optional.
    #[serde(default, skip_serializing_if = "Option::is_none")] player: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] team: Option<String>,
    rank_claim: String, patch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] result: Option<String>,
    duration_sec: u32,
    /// Upload day as YYYY-MM-DD, when the harvester knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")] uploaded: Option<String>,
    #[serde(default)] moments: Vec<VodMoment>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VodMoment {
    at_sec: u32, label: String,
    #[serde(skip_serializing_if = "Option::is_none")] section: Option<SectionId>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "known_kind")] kind: Option<MomentKind>,
}

// Mirrors MOMENT_KINDS in the guides repo's scripts/vods.mjs.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum MomentKind { Kill, Death, Teamfight, Dragon, Baron, Herald, Grubs, Tower, Trade, Roam, Other }

// A kind added by newer content drops only the icon, never the moment or its video.
fn known_kind<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<MomentKind>, D::Error> {
    Ok(serde_json::from_value(serde_json::Value::deserialize(deserializer)?).ok())
}

#[derive(Deserialize)]
struct VodMatchup { enemy: i64, lane: Lane, videos: Vec<serde_json::Value> }

fn vod_text(text: &str) -> bool {
    static MARKUP: OnceLock<regex::Regex> = OnceLock::new();
    !text.trim().is_empty() && valid_text(text, 0, 0) && !text.contains(['{', '}'])
        && !MARKUP.get_or_init(|| regex::Regex::new(r"<|>|\*\*|__|~~|`|\[[^\]]*\]\([^)]*\)|(?:^|\W)[*_][^*_\n]+[*_](?:\W|$)").unwrap()).is_match(text)
}

/// A YYYY-MM-DD day; only used for display and ordering, so month lengths are not checked.
fn upload_day(day: &str) -> bool {
    let parts: Vec<_> = day.split('-').collect();
    let number = |i: usize, len: usize, range: std::ops::RangeInclusive<u32>| parts.get(i)
        .is_some_and(|p| p.len() == len && p.bytes().all(|b| b.is_ascii_digit()) && p.parse().is_ok_and(|n| range.contains(&n)));
    parts.len() == 3 && number(0, 4, 2000..=2999) && number(1, 2, 1..=12) && number(2, 2, 1..=31)
}

impl MatchupVod {
    fn valid(&self) -> bool {
        self.video_id.len() == 11 && self.video_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            && [&self.title, &self.rank_claim].iter().all(|s| vod_text(s)) && self.team.as_deref().is_none_or(vod_text)
            && self.player.as_deref().is_none_or(vod_text)
            && self.patch.len() <= 32 && (2..=3).contains(&self.patch.split('.').count())
            && self.patch.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            && self.result.as_deref().is_none_or(|r| matches!(r, "W" | "L")) && self.duration_sec > 0
            && self.uploaded.as_deref().is_none_or(upload_day)
            && self.moments.iter().all(|m| m.at_sec < self.duration_sec && vod_text(&m.label))
    }
}

/// Valid VOD matchups; duplicate enemy/lane keys are ambiguous, so every copy is dropped.
fn vod_matchups(bytes: &[u8], champion: i64) -> Vec<VodMatchup> {
    if bytes.len() > MAX_CHAMPION_BYTES { return vec![]; }
    let Ok(file) = serde_json::from_slice::<ChampionFile>(bytes) else { return vec![] };
    if !file.valid(champion) { return vec![]; }
    let mut matchups: Vec<VodMatchup> = file.vods.into_iter().filter_map(|v| serde_json::from_value::<VodMatchup>(v).ok())
        .filter(|v| v.enemy > 0 && v.enemy <= i32::MAX as i64).collect();
    let keys: Vec<_> = matchups.iter().map(|m| (m.enemy, m.lane.clone())).collect();
    matchups.retain(|m| keys.iter().filter(|k| k.0 == m.enemy && k.1 == m.lane).count() == 1);
    matchups
}

fn valid_videos(matchup: VodMatchup) -> Vec<MatchupVod> {
    let mut videos: Vec<MatchupVod> = matchup.videos.into_iter().filter_map(|v| serde_json::from_value::<MatchupVod>(v).ok()).filter(MatchupVod::valid).collect();
    let ids: Vec<_> = videos.iter().map(|v| v.video_id.clone()).collect();
    videos.retain(|v| ids.iter().filter(|id| **id == v.video_id).count() == 1);
    videos
}

pub(super) fn parse_vods(bytes: &[u8], champion: i64, enemy: i64, position: Lane) -> Vec<MatchupVod> {
    vod_matchups(bytes, champion).into_iter().find(|m| m.enemy == enemy && m.lane == position).map(valid_videos).unwrap_or_default()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChampionVod { pub enemy: i64, pub lane: Lane, #[serde(flatten)] pub vod: MatchupVod }

/// Every VOD of a champion across enemies and lanes, validated exactly like `parse_vods`.
pub(super) fn parse_champion_vods(bytes: &[u8], champion: i64) -> Vec<ChampionVod> {
    let mut videos: Vec<MatchupVod> = vec![];
    let mut keys = vec![];
    for matchup in vod_matchups(bytes, champion) {
        let (enemy, lane) = (matchup.enemy, matchup.lane.clone());
        for vod in valid_videos(matchup) { keys.push((enemy, lane.clone())); videos.push(vod); }
    }
    // One game has one opponent: a video filed under two matchups is ambiguous, like a duplicate within one.
    let ids: Vec<_> = videos.iter().map(|v| v.video_id.clone()).collect();
    videos.into_iter().zip(keys).filter(|(v, _)| ids.iter().filter(|id| **id == v.video_id).count() == 1)
        .map(|(vod, (enemy, lane))| ChampionVod { enemy, lane, vod }).collect()
}

pub(super) fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

pub(super) fn verified_champion(bytes: &[u8], sha256: &str, champion: i64) -> Option<Vec<Guide>> {
    (bytes.len() <= MAX_CHAMPION_BYTES && digest(bytes).eq_ignore_ascii_case(sha256))
        .then(|| parse_champion(bytes, champion)).flatten()
}
