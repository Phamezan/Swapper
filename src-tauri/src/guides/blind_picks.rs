use super::*;
use std::collections::HashSet;

const MAX_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CuratedPick {
    pub champion: i64,
    pub reason: String,
    sources: Vec<Reference>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference { name: String, url: String }

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct List { patch: String, lanes: BTreeMap<String, Vec<CuratedPick>> }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File { schema: u32, lists: Vec<List> }

#[derive(Clone, Debug)]
pub(crate) struct CuratedList { pub patch: String, pub picks: Vec<CuratedPick> }

fn parse(bytes: &[u8]) -> Option<Vec<List>> {
    if bytes.len() > MAX_BYTES { return None; }
    let file: File = serde_json::from_slice(bytes).ok()?;
    let mut patches = HashSet::new();
    (file.schema == 1 && file.lists.len() <= 100 && file.lists.iter().all(|list| {
        patch_number(&list.patch).is_some() && patches.insert(&list.patch)
            && list.lanes.len() == 5 && list.lanes.iter().all(|(role, picks)| {
                let mut ids = HashSet::new();
                lane(role).is_some() && !picks.is_empty() && picks.len() <= 10 && picks.iter().all(|pick|
                    pick.champion > 0 && pick.champion <= i32::MAX as i64 && ids.insert(pick.champion)
                        && !pick.reason.trim().is_empty() && pick.reason.chars().count() <= 90
                        && !pick.reason.chars().any(char::is_control) && !pick.reason.contains(['<', '>'])
                        && !pick.sources.is_empty() && pick.sources.len() <= 30 && pick.sources.iter().all(|s|
                            !s.name.trim().is_empty() && s.name.len() <= 256 && s.url.len() <= 2048
                                && reqwest::Url::parse(&s.url).is_ok_and(|url| url.scheme() == "https"
                                    && url.host_str().is_some() && url.username().is_empty() && url.password().is_none())))
            })
    })).then_some(file.lists)
}

// Data Dragon uses season 16; the guide reports use Riot's year label 26.
pub(crate) fn report_patch(current: &str) -> String {
    let Some((major, minor)) = patch_number(current) else { return current.into() };
    format!("{}.{minor}", if (15..20).contains(&major) { major + 10 } else { major })
}

fn select(lists: Vec<List>, current: &str, role: &str) -> Option<CuratedList> {
    let current = patch_number(&report_patch(current))?;
    lists.into_iter().filter(|list| patch_number(&list.patch).is_some_and(|p| p <= current))
        .max_by_key(|list| patch_number(&list.patch))
        .and_then(|mut list| list.lanes.remove(role).map(|picks| CuratedList { patch: list.patch, picks }))
}

#[derive(Default)]
struct Cache { attempted: Option<u64>, bytes: Option<Vec<u8>> }

pub(crate) async fn load(current: &str, role: &str) -> Option<CuratedList> {
    static CACHE: OnceLock<tokio::sync::Mutex<Cache>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Default::default).lock().await;
    let (root, base, bundled) = {
        let data = guide_cache().data.lock().await;
        (data.root.clone(), data.base.clone(), data.bundled("blind-picks.json"))
    };
    if cache.bytes.is_none() {
        cache.bytes = root.as_ref().and_then(|p| read_limited(&p.join("blind-picks.json"), MAX_BYTES))
            .filter(|b| parse(b).is_some());
    }
    let now = crate::runes::provider::now_ms().max(0) as u64 / 1000;
    if !base.is_empty() && due(cache.attempted, now) {
        cache.attempted = Some(now);
        if let Some(client) = guide_client() {
            if let Some(bytes) = fetch(&client, &format!("{base}/blind-picks.json"), MAX_BYTES).await.filter(|b| parse(b).is_some()) {
                if let Some(root) = &root { write_atomic(&root.join("blind-picks.json"), &bytes); }
                cache.bytes = Some(bytes);
            }
        }
    }
    // A missing/older published file must not hide a newer bundled list.
    let mut lists = bundled.and_then(parse).unwrap_or_default();
    lists.extend(cache.bytes.as_deref().and_then(parse).unwrap_or_default());
    select(lists, current, role)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file() -> serde_json::Value {
        let picks = json!([{"champion":103,"reason":"Safe farming and escapes.","sources":[{"name":"Coach","url":"https://example.com/guide"}]}]);
        json!({"schema":1,"lists":[{"patch":"26.22","lanes":{
            "top":picks,"jungle":picks,"mid":picks,"adc":picks,"support":picks}}]})
    }

    #[test]
    fn chooses_current_or_newest_older_never_future_and_normalizes_season_patch() {
        let mut data = file();
        for patch in ["26.9", "26.23"] {
            let mut list = data["lists"][0].clone(); list["patch"] = json!(patch);
            data["lists"].as_array_mut().unwrap().push(list);
        }
        let bytes = serde_json::to_vec(&data).unwrap();
        for current in ["26.22", "16.22"] { assert_eq!(select(parse(&bytes).unwrap(), current, "mid").unwrap().patch, "26.22"); }
        assert_eq!(select(parse(&bytes).unwrap(), "26.21", "mid").unwrap().patch, "26.9");
        assert!(select(parse(&bytes).unwrap(), "26.8", "mid").is_none());
        assert!(select(vec![], "26.22", "mid").is_none());
    }

    #[test]
    fn rejects_invalid_content_and_accepts_bundled_curated_list() {
        assert!(parse(include_bytes!("../../resources/guides/blind-picks.json")).is_some());
        for (field, value) in [("champion", json!(0)), ("reason", json!("x".repeat(91))), ("sources", json!([]))] {
            let mut data = file(); data["lists"][0]["lanes"]["mid"][0][field] = value;
            assert!(parse(&serde_json::to_vec(&data).unwrap()).is_none());
        }
    }
}
