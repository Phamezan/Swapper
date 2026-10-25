//! CommunityDragon's numeric ability tooltips, separate from the LCU fallback.

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

const BASE: &str = "https://raw.communitydragon.org";
const MAX_BIN: usize = 8 * 1024 * 1024;
const MAX_STRINGS: usize = 40 * 1024 * 1024;
const MAX_SAVED_STRINGS: usize = 4 * 1024 * 1024;
const MAX_TEXT: usize = 8_000;
const TIMEOUT: Duration = Duration::from_secs(8);
type Strings = HashMap<String, String>;
type StringCache = Option<(String, Instant, Option<Arc<Strings>>)>;
static STRINGS: tokio::sync::Mutex<StringCache> = tokio::sync::Mutex::const_new(None);
static FETCHES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

fn safe_alias(alias: &str) -> Option<String> {
    (alias.len() <= 32 && !alias.is_empty() && alias.bytes().all(|c| c.is_ascii_alphanumeric()))
        .then(|| alias.to_ascii_lowercase())
}

fn safe_patch(patch: &str) -> bool {
    let parts: Vec<_> = patch.split('.').collect();
    parts.len() == 2 && parts.iter().all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|c| c.is_ascii_digit()))
}

/// Enrichment is optional: an unavailable CDN leaves every LCU field intact.
pub(super) async fn enrich(sheet: &mut super::Abilities, champion: &Value) -> bool {
    let Some(alias) = champion["alias"].as_str().and_then(safe_alias) else { return false };
    let Some(patches) = crate::runes::patch::patches().await else { return false };
    if !safe_patch(&patches.current) { return false; }
    let dir = crate::vault::data_root().ok().map(|root| root.join("ability-tooltips"));
    let (bin, strings) = tokio::join!(
        champion_bin(&patches.current, &alias, dir.as_deref()),
        strings(&patches.current, dir.as_deref()),
    );
    let (Some(bin), Some(strings)) = (bin, strings) else { return false };
    sheet.passive.tooltip = resolve(&bin, &strings, &alias, "P", 1);
    for spell in &mut sheet.spells {
        spell.tooltip = resolve(&bin, &strings, &alias, &spell.key, super::ranks(sheet.champion_id, &spell.key));
    }
    true
}

fn download_timeout(cap: usize) -> Duration {
    // The ~32 MB string table needs a larger transfer budget than champion bins.
    if cap == MAX_STRINGS { Duration::from_secs(60) } else { TIMEOUT }
}

async fn fetch(path: &str, cap: usize) -> Option<Vec<u8>> {
    // All callers build paths from validated patch/alias components or constants.
    let _permit = FETCHES.acquire().await.ok()?;
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if CLIENT.get().is_none() {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3)).timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Swapper/ability-tooltips").build().ok()?;
        let _ = CLIENT.set(client);
    }
    let client = CLIENT.get()?;
    let mut response = client.get(format!("{BASE}/{path}")).timeout(download_timeout(cap)).send().await.ok()?;
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > cap as u64) { return None; }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len().checked_add(chunk.len())? > cap { return None; }
        bytes.extend_from_slice(&chunk);
    }
    Some(bytes)
}

async fn read(dir: Option<&Path>, name: &str, cap: usize) -> Option<Value> {
    let path = dir?.join(name);
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let file = std::fs::File::open(path).ok()?;
        if file.metadata().ok()?.len() > cap as u64 { return None; }
        let mut bytes = Vec::new();
        file.take(cap as u64 + 1).read_to_end(&mut bytes).ok()?;
        (bytes.len() <= cap).then(|| serde_json::from_slice(&bytes).ok()).flatten()
    }).await.ok().flatten()
}

async fn write(dir: Option<&Path>, name: &str, value: &Value) {
    let Some(dir) = dir.map(Path::to_path_buf) else { return };
    let name = name.to_string();
    let value = value.clone();
    let _ = tokio::task::spawn_blocking(move || {
        if std::fs::create_dir_all(&dir).is_err() { return; }
        let Ok(bytes) = serde_json::to_vec(&value) else { return };
        let pending = dir.join(format!("{name}.{}.tmp", uuid::Uuid::new_v4()));
        if std::fs::write(&pending, bytes).is_err() || std::fs::rename(&pending, dir.join(&name)).is_err() {
            let _ = std::fs::remove_file(pending);
        }
        // Only this cache's flat files; keep total disk use bounded across patches.
        let Ok(entries) = std::fs::read_dir(&dir) else { return };
        let mut files: Vec<_> = entries.filter_map(Result::ok).filter_map(|entry| {
            let meta = entry.metadata().ok().filter(std::fs::Metadata::is_file)?;
            Some((meta.modified().ok()?, meta.len(), entry.path()))
        }).collect();
        files.sort_by_key(|file| file.0);
        let mut total: u64 = files.iter().map(|file| file.1).sum();
        for (_, len, path) in files {
            if total <= 96 * 1024 * 1024 { break; }
            if std::fs::remove_file(path).is_ok() { total = total.saturating_sub(len); }
        }
    }).await;
}

async fn champion_bin(patch: &str, alias: &str, dir: Option<&Path>) -> Option<Value> {
    if !safe_patch(patch) || safe_alias(alias).as_deref() != Some(alias) { return None; }
    let name = format!("{patch}-{alias}.json");
    if let Some(bin) = read(dir, &name, MAX_BIN).await.filter(Value::is_object) { return Some(bin); }
    let bytes = fetch(&format!("{patch}/game/data/characters/{alias}/{alias}.bin.json"), MAX_BIN).await?;
    let bin: Value = serde_json::from_slice(&bytes).ok()?;
    if !bin.is_object() { return None; }
    write(dir, &name, &bin).await;
    Some(bin)
}

async fn strings(patch: &str, dir: Option<&Path>) -> Option<Arc<Strings>> {
    if !safe_patch(patch) { return None; }
    let mut slot = STRINGS.lock().await;
    if let Some((cached_patch, at, found)) = slot.as_ref() {
        if cached_patch == patch && (found.is_some() || at.elapsed() < Duration::from_secs(300)) { return found.clone(); }
    }
    let name = format!("{patch}-strings.json");
    let found = async {
        if let Some(value) = read(dir, &name, MAX_SAVED_STRINGS).await {
            if let Ok(strings) = serde_json::from_value::<Strings>(value) {
                if !strings.is_empty() { return Some(Arc::new(strings)); }
            }
        }
        let bytes = fetch(&format!("{patch}/game/en_us/data/menu/en_us/lol.stringtable.json"), MAX_STRINGS).await?;
        let selected = tokio::task::spawn_blocking(move || -> Option<Strings> {
            let table: Value = serde_json::from_slice(&bytes).ok()?;
            let entries = table["entries"].as_object()?;
            let mut selected: Strings = entries.iter()
                .filter(|(key, _)| key.starts_with("spell_") || key.starts_with("game_character_"))
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect();
            // Referenced strings can sit outside the spell namespace.
            for _ in 0..8 {
                let mut extra = Vec::new();
                for value in selected.values() {
                    map_tokens(value, "{{", "}}", |key| {
                        let key = key.to_ascii_lowercase();
                        if !selected.contains_key(&key) {
                            if let Some(value) = entries.get(&key).and_then(Value::as_str) { extra.push((key, value.to_string())); }
                        }
                        String::new()
                    });
                }
                if extra.is_empty() { break; }
                selected.extend(extra);
            }
            (!selected.is_empty()).then_some(selected)
        }).await.ok()??;
        let value = serde_json::to_value(&selected).ok()?;
        if serde_json::to_vec(&value).ok()?.len() <= MAX_SAVED_STRINGS { write(dir, &name, &value).await; }
        Some(Arc::new(selected))
    }.await;
    *slot = Some((patch.to_string(), Instant::now(), found.clone()));
    found
}

fn hash(name: &str) -> String {
    let hash = name.bytes().fold(2_166_136_261u32, |h, b| (h ^ u32::from(b.to_ascii_lowercase())).wrapping_mul(16_777_619));
    format!("{{{hash:08x}}}")
}

fn same_name(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right) || left.eq_ignore_ascii_case(&hash(right)) || hash(left).eq_ignore_ascii_case(right)
}

fn lookup<'a>(object: &'a Value, key: &str) -> Option<&'a Value> {
    object.as_object()?.iter().find(|(name, _)| same_name(name, key)).map(|(_, value)| value)
}

#[derive(Clone, PartialEq, Eq)]
enum Unit { Flat, Level, Stat(&'static str), Stack }

#[derive(Clone)]
struct Term { values: Vec<f64>, unit: Unit }

#[derive(Clone, Default)]
struct Formula(Vec<Term>);

impl Formula {
    fn term(values: Vec<f64>, unit: Unit) -> Option<Self> {
        (!values.is_empty() && values.iter().all(|v| v.is_finite() && v.abs() < 1e12)).then_some(Self(vec![Term { values, unit }]))
    }

    fn add(&mut self, other: Self) {
        for term in other.0 {
            if let Some(found) = self.0.iter_mut().find(|t| t.unit == term.unit && t.values.len() == term.values.len()) {
                for (left, right) in found.values.iter_mut().zip(term.values) { *left += right; }
            } else { self.0.push(term); }
        }
    }

    fn scale(mut self, factor: f64) -> Self {
        for term in &mut self.0 { for value in &mut term.values { *value *= factor; } }
        self
    }

    fn multiply(self, other: Self) -> Option<Self> {
        let (mut value, factor) = if other.0.len() == 1 && other.0[0].unit == Unit::Flat {
            (self, other.0[0].values.clone())
        } else if self.0.len() == 1 && self.0[0].unit == Unit::Flat {
            (other, self.0[0].values.clone())
        } else { return None };
        for term in &mut value.0 {
            if factor.iter().all(|v| *v == factor[0]) { for v in &mut term.values { *v *= factor[0]; } }
            else if term.unit != Unit::Level && term.values.len() == factor.len() {
                for (v, f) in term.values.iter_mut().zip(&factor) { *v *= f; }
            } else { return None; }
        }
        Some(value)
    }

    fn render(&self, percent: bool, precision: usize) -> String {
        let mut out = String::new();
        for term in &self.0 {
            if term.values.iter().all(|v| v.abs() < 1e-9) { continue; }
            let suffix = if percent { "%" } else { "" };
            let text = match term.unit {
                Unit::Flat => format!("{}{suffix}", per_rank(&term.values, precision)),
                // shortcut: level ranges show endpoints; expand if a level picker is added.
                Unit::Level => format!("{}–{}{suffix} (levels 1–18)", number(term.values[0], precision), number(*term.values.last().unwrap(), precision)),
                Unit::Stat(stat) => {
                    let factor = if percent { 1.0 } else { 100.0 };
                    let values: Vec<_> = term.values.iter().map(|v| v * factor).collect();
                    format!("{}% {}{stat}", per_rank(&values, precision.max(2)), if percent { "per " } else { "" })
                }
                Unit::Stack => format!("{}{suffix} per stack", per_rank(&term.values, precision)),
            };
            if out.is_empty() { out = text; }
            else { out.push_str(&format!(" ({}{text})", if text.starts_with('-') { "" } else { "+" })); }
        }
        if out.is_empty() { format!("0{}", if percent { "%" } else { "" }) } else { out }
    }
}

fn number(value: f64, precision: usize) -> String {
    let value = if value.abs() < 0.000_001 { 0.0 } else { value };
    let text = format!("{value:.precision$}", precision = precision.min(4));
    if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_string() } else { text }
}

fn per_rank(values: &[f64], precision: usize) -> String {
    let values: Vec<_> = values.iter().map(|v| number(*v, precision)).collect();
    if values.iter().all(|v| *v == values[0]) { values[0].clone() } else { values.join("/") }
}

struct Spell<'a> { data: &'a Value, ranks: usize }

impl Spell<'_> {
    fn value(&self, name: &str) -> Option<Formula> {
        let value = self.data["DataValues"].as_array()?.iter().find(|v| v["name"].as_str().is_some_and(|n| same_name(n, name)))?;
        Formula::term(value["values"].as_array()?.iter().take(self.ranks).map(Value::as_f64).collect::<Option<_>>()?, Unit::Flat)
    }

    fn calculation(&self, name: &str, depth: usize) -> Option<Formula> {
        if depth > 12 { return None; }
        let Some(calc) = lookup(&self.data["mSpellCalculations"], name) else { return self.value(name) };
        let mut formula = match calc["__type"].as_str()? {
            "GameCalculation" => {
                let mut sum = Formula::default();
                for part in calc["mFormulaParts"].as_array()?.iter().take(64) {
                    if let Some(value) = self.part(part, depth + 1) { sum.add(value); }
                }
                if sum.0.is_empty() { return self.value(name); }
                sum
            }
            "GameCalculationModified" => self.calculation(calc["mModifiedGameCalculation"].as_str()?, depth + 1)?,
            _ => return self.value(name),
        };
        if let Some(multiplier) = calc.get("mMultiplier") {
            if let Some(factor) = self.part(multiplier, depth + 1) { formula = formula.multiply(factor)?; }
        }
        Some(formula)
    }

    fn stat(part: &Value) -> Option<&'static str> {
        let bonus = part["mStatFormula"].as_u64().unwrap_or(0);
        match (part["mStat"].as_u64().unwrap_or(0), bonus) {
            (0, 0) => Some("AP"), (0, 2) => Some("bonus AP"),
            (1, 0) => Some("Armor"), (1, 2) => Some("bonus Armor"),
            (2, 0) => Some("AD"), (2, 1) => Some("base AD"), (2, 2) => Some("bonus AD"),
            (12, 0) => Some("max Health"), (12, 2) => Some("bonus Health"),
            (15, 0) if part["{a8cb9c14}"].as_bool() == Some(true) => Some("target missing Health"),
            _ => None,
        }
    }

    fn part(&self, part: &Value, depth: usize) -> Option<Formula> {
        if depth > 12 { return None; }
        let flat = |value| Formula::term(vec![value; self.ranks], Unit::Flat);
        match part["__type"].as_str()? {
            "NamedDataValueCalculationPart" => self.value(part["mDataValue"].as_str()?),
            "NumberCalculationPart" => flat(part["mNumber"].as_f64().unwrap_or(0.0)),
            "StatByCoefficientCalculationPart" | "StatByNamedDataValueCalculationPart" | "StatBySubPartCalculationPart" => {
                let mut coefficient = match part["__type"].as_str()? {
                    "StatByCoefficientCalculationPart" => flat(part["mCoefficient"].as_f64()?)?,
                    "StatByNamedDataValueCalculationPart" => self.value(part["mDataValue"].as_str()?)?,
                    _ => self.part(&part["mSubpart"], depth + 1)?,
                };
                if coefficient.0.len() != 1 || coefficient.0[0].unit != Unit::Flat { return None; }
                coefficient.0[0].unit = Unit::Stat(Self::stat(part)?);
                Some(coefficient)
            }
            "BuffCounterByNamedDataValueCalculationPart" | "BuffCounterByCoefficientCalculationPart" => {
                let mut value = if let Some(name) = part["mDataValue"].as_str() { self.value(name)? }
                    else { flat(part["mCoefficient"].as_f64()?)? };
                value.0[0].unit = Unit::Stack;
                Some(value)
            }
            "SumOfSubPartsCalculationPart" => {
                let mut sum = Formula::default();
                for part in part["mSubparts"].as_array()?.iter().take(64) {
                    if let Some(value) = self.part(part, depth + 1) { sum.add(value); }
                }
                (!sum.0.is_empty()).then_some(sum)
            }
            "ProductOfSubPartsCalculationPart" => self.part(&part["mPart1"], depth + 1)?.multiply(self.part(&part["mPart2"], depth + 1)?),
            "ByCharLevelInterpolationCalculationPart" => Formula::term(vec![part["mStartValue"].as_f64().unwrap_or(0.0), part["mEndValue"].as_f64().unwrap_or(0.0)], Unit::Level),
            "ByCharLevelFormulaCalculationPart" => Formula::term(part["mValues"].as_array()?.iter().take(18).map(Value::as_f64).collect::<Option<_>>()?, Unit::Level),
            "ByCharLevelBreakpointsCalculationPart" => {
                let mut value = part["mLevel1Value"].as_f64().unwrap_or(0.0);
                let mut step = part["mInitialBonusPerLevel"].as_f64().unwrap_or(0.0);
                let mut values = vec![value];
                for level in 2..=18 {
                    for point in part["mBreakpoints"].as_array().into_iter().flatten().take(32) {
                        if point["mLevel"].as_u64() == Some(level) {
                            step = point["mBonusPerLevelAtAndAfter"].as_f64().unwrap_or(step);
                            value += point["mAdditionalBonusAtThisLevel"].as_f64().unwrap_or(0.0);
                        }
                    }
                    value += step;
                    values.push(value);
                }
                Formula::term(values, Unit::Level)
            }
            _ => None,
        }
    }
}

fn map_tokens(text: &str, open: &str, close: &str, mut replace: impl FnMut(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        let tail = &rest[start + open.len()..];
        let Some(end) = tail.find(close) else { return out };
        out.push_str(&replace(&tail[..end]));
        if out.len() > MAX_TEXT * 8 { return String::new(); }
        rest = &tail[end + close.len()..];
    }
    out.push_str(rest);
    out
}

fn expand(text: &str, strings: &Strings, depth: usize) -> String {
    if depth >= 8 || text.len() > MAX_TEXT * 8 { return String::new(); }
    map_tokens(text, "{{", "}}", |key| strings.get(&key.to_ascii_lowercase()).map(|text| expand(text, strings, depth + 1)).unwrap_or_default())
}

fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix('<') {
            let Some(end) = tail.find('>') else { break };
            let tag = tail[..end].trim().to_ascii_lowercase();
            if tag.starts_with("br") || matches!(tag.as_str(), "/p" | "/li") { out.push('\n'); }
            rest = &tail[end + 1..];
        } else if let Some(tail) = rest.strip_prefix("%i:") {
            let Some(end) = tail.find('%') else { break };
            rest = &tail[end + 1..];
        } else if rest.starts_with('{') && rest.get(1..9).is_some_and(|s| s.bytes().all(|c| c.is_ascii_hexdigit())) && rest.as_bytes().get(9) == Some(&b'}') {
            rest = &rest[10..];
        } else {
            let c = rest.chars().next().unwrap();
            if c != '@' { out.push(c); }
            rest = &rest[c.len_utf8()..];
        }
    }
    let out = out.replace("&nbsp;", " ").replace("&#39;", "'").replace("&quot;", "\"")
        .replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">");
    let mut lines = Vec::new();
    for line in out.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if !line.is_empty() || lines.last().is_some_and(|last: &String| !last.is_empty()) { lines.push(line); }
    }
    let out = lines.join("\n").trim().to_string();
    out.chars().take(MAX_TEXT).collect()
}

fn resolve(bin: &Value, strings: &Strings, alias: &str, key: &str, ranks: usize) -> Option<String> {
    let root = lookup(bin, &format!("Characters/{alias}/CharacterRecords/Root"))?;
    let path = if key == "P" { root["mCharacterPassiveSpell"].as_str()? }
        else { root["spells"].get(["Q", "W", "E", "R"].iter().position(|k| *k == key)?)?.as_str()? };
    let data = &lookup(bin, path)?["mSpell"];
    let loc = &data["mClientData"]["mTooltipData"]["mLocKeys"];
    let template = strings.get(&loc["keyTooltip"].as_str()?.to_ascii_lowercase())?;
    let template = expand(template, strings, 0);
    let spell = Spell { data, ranks: ranks.clamp(1, 6) };
    let mut rest = template.as_str();
    let mut out = String::new();
    while let Some(start) = rest.find('@') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let Some(end) = tail.find('@') else { rest = ""; break };
        let token = &tail[..end];
        rest = &tail[end + 1..];
        let (name, multiplier) = match token.split_once('*') {
            Some((name, multiplier)) => (name, multiplier.parse::<f64>().ok()),
            None => (token, Some(1.0)),
        };
        let trailing_percent = rest.starts_with('%') && !rest.starts_with("%i:");
        if trailing_percent { rest = &rest[1..]; }
        if let (Some(formula), Some(multiplier)) = (spell.calculation(name, 0), multiplier.filter(|n| n.is_finite() && n.abs() <= 1e6)) {
            let calc = lookup(&data["mSpellCalculations"], name);
            let percent = calc.is_some_and(|c| c["mDisplayAsPercent"] == true);
            let precision = calc.and_then(|c| c["mPrecision"].as_u64()).unwrap_or(2).min(4) as usize;
            // An explicit *100@% already performs the percentage conversion.
            let percent_scale = if percent && !(trailing_percent && multiplier == 100.0) { 100.0 } else { 1.0 };
            out.push_str(&formula.scale(multiplier * percent_scale).render(percent || trailing_percent, precision));
        }
        if out.len() > MAX_TEXT * 8 { return None; }
    }
    out.push_str(rest);
    let out = clean(&out);
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_table_has_a_longer_download_budget_than_champion_bins() {
        assert_eq!(download_timeout(MAX_STRINGS), Duration::from_secs(60));
        assert_eq!(download_timeout(MAX_BIN), Duration::from_secs(8));
    }

    fn tooltip(alias: &str) -> String {
        let fixtures: Value = serde_json::from_str(include_str!("../../tests/fixtures/ability-tooltips.json")).unwrap();
        let fixture = fixtures["champions"].as_array().unwrap().iter().find(|v| v["alias"] == alias).unwrap();
        let strings = serde_json::from_value(fixture["strings"].clone()).unwrap();
        let key = fixture["key"].as_str().unwrap();
        let ranks = if key == "P" { 1 } else { super::super::ranks(fixture["championId"].as_i64().unwrap(), key) };
        let text = resolve(&fixture["bin"], &strings, alias, key, ranks).unwrap_or_default();
        eprintln!("{alias} {key}: {text}");
        text
    }

    #[test]
    fn locke_r_resolves_damage_and_the_hashed_execution_threshold() {
        let text = tooltip("locke");
        assert!(text.contains("75/150/225 (+60% AP) magic damage"), "{text}");
        assert!(text.contains("9/10/11% (+0.5% per stack)"), "{text}");
        assert!(!text.contains('@') && !text.contains("6c540a3a") && !text.contains("300"));
    }

    #[test]
    fn ahri_q_has_five_ranks_and_ap_scaling_on_both_hits() {
        assert_eq!(tooltip("ahri"), "Ahri throws then pulls back her orb, dealing 10/35/60/85/110 (+50% AP) magic damage on the way out and 10/35/60/85/110 (+50% AP) true damage on the way back.");
    }

    #[test]
    fn darius_q_handles_ranked_ad_ratios_and_a_modified_calculation() {
        let text = tooltip("darius");
        assert!(text.contains("10/50/80/110/140 (+90/100/110/120/130% AD) physical damage"), "{text}");
        assert!(text.contains("3.5/17.5/28/38.5/49 (+31.5/35/38.5/42/45.5% AD) damage"), "{text}");
        assert!(text.contains("17% missing Health"));
    }

    #[test]
    fn garen_passive_applies_the_multiplier_and_char_level_breakpoints() {
        assert_eq!(tooltip("garen"), "Garen regenerates 1.5–10.1% (levels 1–18) max Health every 5 seconds if he has not been damaged or hit with enemy Abilities in the last 8 seconds.");
    }

    #[test]
    fn annie_passive_preserves_char_level_scaling_and_paragraphs() {
        assert_eq!(tooltip("annie"), "After using 4 Abilities, Annie's next damaging Ability Stuns for 1.25–1.75 (levels 1–18) seconds.\n\nAnnie begins the game and respawns with Pyromania available.");
    }

    fn sample(data: Value, text: &str, ranks: usize) -> String {
        let bin = serde_json::json!({
            "Characters/Test/CharacterRecords/Root": {"spells": ["Characters/Test/Q"]},
            "Characters/Test/Q": {"mSpell": data},
        });
        let mut bin = bin;
        bin["Characters/Test/Q"]["mSpell"]["mClientData"]["mTooltipData"]["mLocKeys"]["keyTooltip"] = "Spell_Test_Tooltip".into();
        resolve(&bin, &HashMap::from([("spell_test_tooltip".into(), text.into())]), "test", "Q", ranks).unwrap_or_default()
    }

    #[test]
    fn unknown_parts_tokens_icons_and_hashes_never_leak() {
        let data = serde_json::json!({"DataValues": [{"name": "Base", "values": [10,20,30,40,50,60,70]}],
            "mSpellCalculations": {"Damage": {"__type": "GameCalculation", "mFormulaParts": [
                {"__type": "NamedDataValueCalculationPart", "mDataValue": "Base"},
                {"__type": "FutureUnknownCalculationPart"}
            ]}}});
        assert_eq!(sample(data, "<b>Deals @Damage@</b>. @Unknown@ @Unknown*100@% %i:scaleAD% {6c540a3a} {{Unknown}} @SpellModifierDescriptionAppend@", 3), "Deals 10/20/30.");
        assert_eq!(resolve(&Value::Null, &HashMap::new(), "test", "P", 1), None);
    }

    #[test]
    fn named_bonus_ad_and_health_ratios_remain_symbolic() {
        let data = serde_json::json!({"DataValues": [{"name": "Ratio", "values": [0.6,0.6,0.6]}],
            "mSpellCalculations": {"Damage": {"__type": "GameCalculation", "mFormulaParts": [
                {"__type": "NumberCalculationPart", "mNumber": 75},
                {"__type": "StatByNamedDataValueCalculationPart", "mStat": 2, "mStatFormula": 2, "mDataValue": "Ratio"},
                {"__type": "StatByCoefficientCalculationPart", "mStat": 12, "mCoefficient": 0.1}
            ]}}});
        assert_eq!(sample(data.clone(), "Deals @Damage@.", 3), "Deals 75 (+60% bonus AD) (+10% max Health).");
        assert_eq!(sample(data, "Deals @Damage@%i:scaleAD%.", 3), "Deals 75 (+60% bonus AD) (+10% max Health).");
    }

    #[test]
    fn percent_expressions_do_not_duplicate_units_or_float_noise() {
        let data = serde_json::json!({"DataValues": [{"name": "Threshold", "values": [0.09,0.1,0.11,0.12]}],
            "mSpellCalculations": {"Percent": {"__type": "GameCalculation", "mDisplayAsPercent": true, "mPrecision": 1,
                "mFormulaParts": [{"__type": "NamedDataValueCalculationPart", "mDataValue": "Threshold"}]}}});
        assert_eq!(sample(data, "@Threshold*100@% / @Percent@ / @Percent*100@% / @Percent@%", 3), "9/10/11% / 9/10/11% / 9/10/11% / 9/10/11%");
    }

    #[test]
    fn calculation_cycles_are_bounded_and_level_endpoints_are_readable() {
        let data = serde_json::json!({"mSpellCalculations": {
            "Cycle": {"__type": "GameCalculationModified", "mModifiedGameCalculation": "Cycle"},
            "Damage": {"__type": "GameCalculation", "mFormulaParts": [{"__type": "ByCharLevelInterpolationCalculationPart", "mStartValue": 35, "mEndValue": 95}]}
        }});
        assert_eq!(sample(data, "@Cycle@ Heals @Damage@.", 5), "Heals 35–95 (levels 1–18).");
        assert_eq!(expand("{{loop}}", &HashMap::from([("loop".into(), "{{loop}}".into())]), 0), "");
    }

    #[test]
    fn rank_exceptions_and_hashed_data_names_are_respected() {
        let data = serde_json::json!({"DataValues": [{"name": hash("Base"), "values": [1,2,3,4,5,6,7]}]});
        assert_eq!(sample(data.clone(), "@Base@", super::super::ranks(77, "Q")), "1/2/3/4/5/6");
        assert_eq!(sample(data, "@Base@", super::super::ranks(126, "R")), "1/2/3/4");
        assert_eq!(hash("ExecuteTooltipCalc"), "{6c540a3a}");
    }

    #[tokio::test]
    async fn cache_is_bounded_patch_specific_and_tolerates_bad_json() {
        let dir = std::env::temp_dir().join(format!("swapper-ability-test-{}", uuid::Uuid::new_v4()));
        let bin = serde_json::json!({"cached": true});
        write(Some(&dir), "16.21-test.json", &bin).await;
        assert_eq!(read(Some(&dir), "16.21-test.json", MAX_BIN).await, Some(bin));
        assert_eq!(read(Some(&dir), "16.22-test.json", MAX_BIN).await, None);
        assert_eq!(read(Some(&dir), "16.21-test.json", 2).await, None);
        std::fs::write(dir.join("16.21-test.json"), "{").unwrap();
        assert_eq!(read(Some(&dir), "16.21-test.json", MAX_BIN).await, None);
        std::fs::remove_file(dir.join("16.21-test.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[tokio::test]
    async fn unsafe_aliases_and_patches_are_rejected_before_io() {
        for alias in ["../ahri", "ahri?x=1", "ahri\\q", "", "a/b", "https://evil"] {
            assert_eq!(safe_alias(alias), None);
            assert_eq!(champion_bin("16.21", alias, None).await, None);
        }
        for patch in ["latest", "../16.21", "16.21/evil", "16.21?x", "16", "16.21.1"] {
            assert!(!safe_patch(patch));
            assert_eq!(champion_bin(patch, "ahri", None).await, None);
            assert!(strings(patch, None).await.is_none());
        }
        assert_eq!(safe_alias("AurelionSol"), Some("aurelionsol".into()));
    }

    #[tokio::test]
    async fn missing_or_unsafe_aliases_preserve_the_serialized_fallback() {
        let mut sheet = super::super::parse(103, &serde_json::json!({"passive": {"description": "Passive fallback."}, "spells": [{"description": "Spell fallback."}]}));
        let before = sheet.clone();
        for champion in [serde_json::json!({}), serde_json::json!({"alias": "../ahri"})] {
            enrich(&mut sheet, &champion).await;
            assert_eq!(sheet, before);
        }
        let serialized = serde_json::to_value(sheet).unwrap();
        assert_eq!(serialized["spells"][0]["description"], "Spell fallback.");
        assert!(serialized["spells"][0]["tooltip"].is_null());
        assert_eq!(serialized["passive"]["description"], "Passive fallback.");
        assert!(serialized["passive"]["tooltip"].is_null());
    }

    #[tokio::test]
    #[ignore = "fetches live CommunityDragon data; requires the network"]
    async fn live_champion_tooltips() {
        let patch = crate::runes::patch::patches().await.expect("patch versions").current;
        let strings = strings(&patch, None).await.expect("live tooltip strings");
        for (alias, key, ranks) in [("locke", "R", 3), ("ahri", "Q", 5), ("darius", "Q", 5), ("garen", "P", 1), ("annie", "P", 1)] {
            let bin = champion_bin(&patch, alias, None).await.expect("live champion bin");
            let text = resolve(&bin, &strings, alias, key, ranks).expect("resolved live tooltip");
            eprintln!("{patch} {alias} {key}: {text}");
            assert!(!text.contains('@') && !text.contains("{6c540a3a}"));
        }
    }
}
