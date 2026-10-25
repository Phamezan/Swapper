use super::{Player, LIMIT};
use serde_json::{json, Value};
use std::{collections::{BTreeMap, VecDeque}, sync::{Mutex, OnceLock}, time::{Duration, Instant}};

const MAX_BODY: usize = 128 * 1024;
// shortcut: OP.GG rotates server-action IDs on deployments; update this after a failed live contract test.
const AUTOCOMPLETE_ACTION: &str = "402c9587dc35c9a189a48efae20bebb24826369a95";
static CACHE: Mutex<Option<Cache<Vec<Player>>>> = Mutex::new(None);
static BACKOFF: Mutex<Option<Instant>> = Mutex::new(None);
static HTTP: OnceLock<Option<reqwest::Client>> = OnceLock::new();

pub(super) struct Cache<T> {
    entries: BTreeMap<String, (Instant, T)>,
    requests: VecDeque<Instant>,
}

impl<T: Clone> Default for Cache<T> {
    fn default() -> Self { Self { entries: BTreeMap::new(), requests: VecDeque::new() } }
}

impl<T: Clone> Cache<T> {
    pub(super) fn get(&self, key: &str, ttl: Duration) -> Option<T> {
        self.entries.get(key).filter(|(at, _)| at.elapsed() < ttl).map(|(_, value)| value.clone())
    }
    pub(super) fn put(&mut self, key: String, value: T) {
        if self.entries.len() >= 64 { self.entries.clear(); }
        self.entries.insert(key, (Instant::now(), value));
    }
    pub(super) fn reserve(&mut self, per_minute: usize) -> bool {
        self.requests.retain(|at| at.elapsed() < Duration::from_secs(60));
        if self.requests.len() >= per_minute || self.requests.back().is_some_and(|at| at.elapsed() < Duration::from_millis(250)) {
            return false;
        }
        self.requests.push_back(Instant::now());
        true
    }
}

pub(super) fn region_slug(region: &str) -> Option<&'static str> {
    Some(match crate::runes::ugg_record::region_id(region)? {
        "euw1" => "euw", "eun1" => "eune", "na1" => "na", "br1" => "br",
        "la1" => "lan", "la2" => "las", "oc1" => "oce", "tr1" => "tr",
        "jp1" => "jp", "ph2" => "ph", "sg2" => "sg", "th2" => "th",
        "tw2" => "tw", "vn2" => "vn", "me1" => "me", "kr" => "kr", "ru" => "ru",
        _ => return None,
    })
}

pub(super) fn identity(name: &str, tag: &str) -> Option<String> {
    let id = format!("{name}#{tag}");
    let (clean_name, clean_tag) = super::riot_id(&id).ok()?;
    if name != clean_name || tag != clean_tag || name.contains('#') || tag.chars().any(char::is_whitespace) {
        return None;
    }
    Some(id)
}

pub(super) fn parse_autocomplete(body: &str, query: &str) -> Option<Vec<Player>> {
    if body.len() > MAX_BODY { return None; }
    // Read the action's result record only; never render OP.GG's HTML, links or image URLs.
    let result = body.lines().find_map(|line| line.strip_prefix("1:"))?;
    let value: Value = serde_json::from_str(result).ok()?;
    let rows = value["summoners"].as_array()?;
    if rows.len() > 100 { return None; }
    let mut players = Vec::new();
    for row in rows {
        let Some(id) = row["gameName"].as_str().zip(row["tagline"].as_str())
            .and_then(|(name, tag)| identity(name, tag)) else { continue };
        if !super::prefix(&id, query) || players.iter().any(|p: &Player| p.riot_id.to_lowercase() == id.to_lowercase()) { continue; }
        players.push(Player { riot_id: id, icon: None, level: None, ally: None, last_played: None, live: false });
    }
    super::rank_players(&mut players, query);
    players.truncate(LIMIT);
    Some(players)
}

pub(super) async fn post(url: &str, body: &Value, action: Option<&str>) -> Option<String> {
    if BACKOFF.lock().ok()?.is_some_and(|until| until > Instant::now()) { return None; }
    let client = HTTP.get_or_init(|| reqwest::Client::builder()
        .timeout(Duration::from_secs(4)).connect_timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none()).user_agent("Mozilla/5.0 Swapper/1.0")
        .build().ok()).as_ref()?;
    let mut request = client.post(url);
    if let Some(action) = action {
        request = request.header("Next-Action", action).header("Accept", "text/x-component")
            .header("Origin", "https://op.gg").header("Content-Type", "text/plain;charset=UTF-8")
            .body(serde_json::to_vec(body).ok()?);
    } else { request = request.json(body); }
    let mut response = request.send().await.ok()?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        *BACKOFF.lock().ok()? = Some(Instant::now() + Duration::from_secs(60));
    }
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > MAX_BODY as u64) { return None; }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > MAX_BODY { return None; }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).ok()
}

pub(super) async fn autocomplete(region: &str, query: &str) -> Vec<Player> {
    let Some(region) = region_slug(region) else { return vec![] };
    if query.chars().count() < 2 { return vec![]; }
    let key = format!("{region}:{}", query.to_lowercase());
    {
        let Ok(mut guard) = CACHE.lock() else { return vec![] };
        let cache = guard.get_or_insert_with(Cache::default);
        if let Some(players) = cache.get(&key, Duration::from_secs(60)) { return players; }
        if !cache.reserve(30) { return vec![]; }
    }
    let body = json!([{"region": region, "value": query, "locale": "en_US"}]);
    let players = post("https://op.gg/", &body, Some(AUTOCOMPLETE_ACTION)).await
        .and_then(|body| parse_autocomplete(&body, query)).unwrap_or_default();
    if let Ok(mut guard) = CACHE.lock() { guard.get_or_insert_with(Cache::default).put(key, players.clone()); }
    players
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Public OP.GG request; verifies the current action contract without player logs."]
    async fn live_opgg_autocomplete_contract() {
        let body = json!([{"region":"euw","value":"Example","locale":"en_US"}]);
        let response = post("https://op.gg/", &body, Some(AUTOCOMPLETE_ACTION)).await.expect("OP.GG request unavailable.");
        assert!(parse_autocomplete(&response, "Example").is_some(), "OP.GG autocomplete contract changed.");
    }
    #[test]
    fn cache_is_bounded_and_limits_requests_without_extending_hits() {
        let mut cache = Cache::<usize>::default();
        for n in 0..100 { cache.put(n.to_string(), n); }
        assert!(cache.entries.len() <= 64);
        assert_eq!(cache.get("99", Duration::from_secs(60)), Some(99));
        assert_eq!(cache.get("99", Duration::ZERO), None);
        assert!(cache.reserve(30));
        assert!(!cache.reserve(30));
        cache.requests = (0..30).map(|_| Instant::now() - Duration::from_secs(1)).collect();
        assert!(!cache.reserve(30));
    }
    #[test]
    fn opgg_flight_response_only_keeps_valid_matching_identities() {
        let players = parse_autocomplete(include_str!("fixtures/opgg-autocomplete.txt"), "example").unwrap();
        assert_eq!(players.len(), 2);
        assert_eq!(players[0].riot_id, "Example#TAG");
        assert!(players.iter().all(|p| !p.live && p.icon.is_none()));
        assert!(parse_autocomplete("1:{\"summoners\":null}", "example").is_none());
        assert!(parse_autocomplete("1:E{\"digest\":\"error\"}", "example").is_none());
        assert!(parse_autocomplete(&" ".repeat(128 * 1024 + 1), "example").is_none());
    }
    #[test]
    fn unknown_regions_never_fall_back_to_another_server() {
        assert_eq!(region_slug("EUW1"), Some("euw"));
        assert_eq!(region_slug("EUN1"), Some("eune"));
        assert_eq!(region_slug("unknown"), None);
    }
}
