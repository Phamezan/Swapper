use super::*;
use serde_json::{json, Value};

fn vod() -> Value {
    json!({"videoId":"A_b-cD12345","title":"Hold Charm.","player":"Test player","team":"T1",
        "rankClaim":"Challenger","patch":"26.21","result":"W","durationSec":1746,
        "moments":[{"atSec":184,"label":"Trading window","section":"early-lane"}]})
}

fn with_vods(videos: Vec<Value>) -> Vec<u8> {
    let mut file: Value = serde_json::from_slice(&champion_bytes(vec![guide()])).unwrap();
    file["vods"] = json!([{"enemy":238,"lane":"mid","videos":videos}]);
    serde_json::to_vec(&file).unwrap()
}

#[test]
fn vods_are_optional_and_invalid_videos_do_not_discard_valid_siblings_or_guides() {
    let bytes = with_vods(vec![vod()]);
    assert_eq!(parse_champion(&bytes, 103).unwrap().len(), 1);
    assert_eq!(parse_vods(&bytes, 103, 238, Lane::Mid).len(), 1);
    assert!(parse_vods(&bytes, 103, 238, Lane::Top).is_empty());
    assert!(parse_vods(&bytes, 238, 238, Lane::Mid).is_empty());
    assert!(parse_vods(&champion_bytes(vec![guide()]), 103, 238, Lane::Mid).is_empty());
    for (field, value) in [("videoId", json!("bad")), ("title", json!("<b>Bad</b>")),
        ("player", json!("")), ("rankClaim", json!("**Rank**")), ("team", json!("")),
        ("patch", json!("current")), ("result", json!("win")), ("durationSec", json!(0)),
        ("durationSec", json!(1.5)), ("fixture", json!(true)), ("playerImage", json!("https://evil.io/photo")),
        ("moments", json!([{"atSec":1746,"label":"End"}])),
        ("moments", json!([{"atSec":-1,"label":"Bad"}])),
        ("moments", json!([{"atSec":1,"label":"Bad","section":"build"}]))] {
        let mut bad = vod(); bad[field] = value;
        let bytes = with_vods(vec![bad, vod()]);
        assert_eq!(parse_vods(&bytes, 103, 238, Lane::Mid).len(), 1, "{field}");
        assert_eq!(parse_champion(&bytes, 103).unwrap().len(), 1);
    }
    assert!(parse_vods(&with_vods(vec![vod(), vod()]), 103, 238, Lane::Mid).is_empty());
    let mut anonymous = vod(); anonymous.as_object_mut().unwrap().remove("player"); anonymous.as_object_mut().unwrap().remove("result");
    assert_eq!(parse_vods(&with_vods(vec![anonymous]), 103, 238, Lane::Mid).len(), 1);
    let mut only: Value = serde_json::from_slice(&bytes).unwrap(); only["guides"] = json!([]);
    assert_eq!(parse_vods(&serde_json::to_vec(&only).unwrap(), 103, 238, Lane::Mid).len(), 1);
    let mut duplicate: Value = serde_json::from_slice(&bytes).unwrap();
    let entry = duplicate["vods"][0].clone();
    duplicate["vods"].as_array_mut().unwrap().push(entry);
    assert!(parse_vods(&serde_json::to_vec(&duplicate).unwrap(), 103, 238, Lane::Mid).is_empty());
}

#[test]
fn moment_kinds_pass_through_and_unknown_kinds_drop_only_the_kind() {
    let mut video = vod();
    video["moments"] = json!([{"atSec":184,"label":"Dragon","kind":"dragon"}, {"atSec":200,"label":"Ward","kind":"ward"},
        {"atSec":220,"label":"Odd","kind":7}, {"atSec":240,"label":"Plain"}]);
    let vods = parse_vods(&with_vods(vec![video]), 103, 238, Lane::Mid);
    assert_eq!(vods.len(), 1);
    assert_eq!(serde_json::to_value(&vods[0]).unwrap()["moments"], json!([{"atSec":184,"label":"Dragon","kind":"dragon"},
        {"atSec":200,"label":"Ward"}, {"atSec":220,"label":"Odd"}, {"atSec":240,"label":"Plain"}]));
}

#[test]
fn vod_only_index_entries_are_validated() {
    let good = json!({"schema":1,"generatedAt":"2026-11-01T00:00:00Z","champions":{"103":{"sha256":"a".repeat(64),"guides":0,"vods":1}}});
    assert!(parse_index(&serde_json::to_vec(&good).unwrap()).is_some());
    for count in [json!(0), json!(-1), json!(2049), json!(1.5)] {
        let mut bad = good.clone(); bad["champions"]["103"]["vods"] = count;
        assert!(parse_index(&serde_json::to_vec(&bad).unwrap()).is_none());
    }
}

#[tokio::test]
async fn vods_share_the_guide_download_and_survive_an_offline_restart() {
    let root = std::env::temp_dir().join(format!("swapper-vod-test-{}", uuid::Uuid::new_v4()));
    let server = Server::start().await;
    let client = reqwest::Client::new();
    server.publish(with_vods(vec![vod()]));
    let mut cache = GuideCache::open(Some(root.clone()), server.base.clone(), &[]);
    assert_eq!(cache.champion(&client, 103, 100_000).await.len(), 1);
    let bytes = cache.content(&client, 103, 100_001).await;
    assert_eq!(parse_vods(&bytes, 103, 238, Lane::Mid).len(), 1);
    assert_eq!(server.count("/champions/103.json"), 1, "VODs reuse the guide file");
    server.state.files.lock().unwrap().clear();
    let mut reopened = GuideCache::open(Some(root.clone()), server.base.clone(), &[]);
    let bytes = reopened.content(&client, 103, 100_000 + REFRESH_SECONDS).await;
    assert_eq!(parse_vods(&bytes, 103, 238, Lane::Mid).len(), 1);
    assert_eq!(parse_champion(&bytes, 103).unwrap().len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

fn guide() -> Value {
    let tip = json!({"scope":"pair","text":"Trade after {cd:238:W}.","sourceIds":["s"]});
    json!({"champion":103,"enemy":238,"lane":"mid","patch":"26.20","reviewed":true,
        "difficulty":"Skill matchup","favoredWhen":"Hold Charm.","keyTips":[tip,tip,tip],
        "enemyTips":{},"sections":{"overview":{"tips":[]},"early-lane":{"tips":[]},
            "mid-lane":{"tips":[]},"win-condition":{"tips":[]}},"itemization":[],
        "sources":[{"id":"s","author":"Author","rankClaim":"Challenger","kind":"mobafire",
            "url":"https://www.mobafire.com/guide"}]})
}

fn champion_bytes(guides: Vec<Value>) -> Vec<u8> {
    serde_json::to_vec(&json!({"schema":1,"champion":103,"generatedAt":"2026-10-27T20:00:00Z","guides":guides})).unwrap()
}

#[test]
fn untrusted_guides_are_validated_whole_and_invalid_siblings_are_dropped() {
    let good = guide();
    assert_eq!(parse_champion(&champion_bytes(vec![good.clone()]), 103).unwrap().len(), 1);
    for (field, value) in [("fixture", json!(true)), ("schema", json!(1)),
        ("lane", json!("unknown")), ("reviewed", json!("yes")), ("keyTips", json!([]))] {
        let mut bad = good.clone();
        bad[field] = value;
        let valid = parse_champion(&champion_bytes(vec![bad, good.clone()]), 103).unwrap();
        assert_eq!(valid.len(), 1, "{field}");
    }
    for text in ["{cd:999:Q}", "{cd:238:P}", "{cd:0238:W}", "{cd:238:W", "{CD:238:W}",
        "<b>trade</b>", "Wait 20 seconds.", "Wait 1.5 seconds.", "{unknown}", "}", "{", "{}}"] {
        let mut bad = good.clone();
        bad["keyTips"][0]["text"] = json!(text);
        assert!(parse_champion(&champion_bytes(vec![bad]), 103).unwrap().is_empty(), "{text}");
    }
    let mut bad = good.clone();
    bad["sources"][0]["url"] = json!("https://mobafire.com.evil.io/guide");
    assert!(parse_champion(&champion_bytes(vec![bad]), 103).unwrap().is_empty());
    let mut bad = good;
    bad["keyTips"][1]["sourceIds"] = json!(["missing"]);
    assert!(parse_champion(&champion_bytes(vec![bad]), 103).unwrap().is_empty());
    let mut file: Value = serde_json::from_slice(&champion_bytes(vec![guide()])).unwrap();
    file["schema"] = json!(2);
    assert!(parse_champion(&serde_json::to_vec(&file).unwrap(), 103).is_none());
    assert!(parse_champion(&champion_bytes(vec![guide()]), 238).is_none());
    assert!(parse_champion(&vec![b' '; MAX_CHAMPION_BYTES + 1], 103).is_none());
}

#[test]
fn index_and_hash_validation_reject_untrusted_metadata() {
    let bytes = champion_bytes(vec![guide()]);
    let hash = digest(&bytes);
    assert_eq!(hash.len(), 64);
    assert!(verified_champion(&bytes, &hash, 103).is_some());
    assert!(verified_champion(b"changed", &hash, 103).is_none());
    let valid = json!({"schema":1,"generatedAt":"2026-10-27T20:00:00Z","champions":{"103":{"sha256":hash,"guides":1}}});
    assert!(parse_index(&serde_json::to_vec(&valid).unwrap()).is_some());
    for bad in [json!({"schema":2,"generatedAt":"x","champions":{}}),
        json!({"schema":1,"generatedAt":"x","champions":{"../103":{"sha256":"x","guides":1}}}),
        json!({"schema":1,"generatedAt":"x","champions":{"103":{"sha256":"x","guides":1}}})] {
        assert!(parse_index(&serde_json::to_vec(&bad).unwrap()).is_none());
    }
}

#[test]
fn contract_lanes_match_the_app_and_reject_bottom() {
    for position in ["top", "jungle", "mid", "adc", "support"] {
        let mut value = guide();
        value["lane"] = json!(position);
        let parsed = parse_champion(&champion_bytes(vec![value]),103).unwrap().remove(0);
        assert_eq!(serde_json::to_value(&parsed).unwrap()["lane"], position);
        assert_eq!(lane(position), Some(parsed.lane));
    }
    assert!(lane("bottom").is_none());
    let mut value = guide();
    value["lane"] = json!("bottom");
    assert!(parse_champion(&champion_bytes(vec![value]),103).unwrap().is_empty());
}

#[test]
fn outdated_flags_use_each_tips_patch_and_never_flag_live_cooldowns() {
    let mut value = guide();
    value["keyTips"][0]["text"] = json!("Hold Charm.");
    value["keyTips"][1]["text"] = json!("Keep your escape.");
    value["keyTips"][1]["patch"] = json!("26.22");
    let mut guide = parse_champion(&champion_bytes(vec![value]),103).unwrap().remove(0);
    flag_tips(&mut guide, &[("26.21".into(), vec![999])]);
    assert!(serde_json::to_value(&guide).unwrap()["keyTips"][0].get("outdated").is_none());
    flag_tips(&mut guide, &[("26.21".into(), vec![238])]);
    let rendered = serde_json::to_value(&guide).unwrap();
    assert_eq!(rendered["keyTips"][0]["outdated"], true);
    assert!(rendered["keyTips"][1].get("outdated").is_none());
    assert!(rendered["keyTips"][2].get("outdated").is_none());
}

#[derive(Clone, Default)]
struct ServerState {
    files: std::sync::Arc<std::sync::Mutex<BTreeMap<String, Vec<u8>>>>,
    requests: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

struct Server { state: ServerState, base: String, task: tokio::task::JoinHandle<()> }
impl Drop for Server { fn drop(&mut self) { self.task.abort(); } }

impl Server {
    async fn start() -> Self {
        async fn response(axum::extract::State(s): axum::extract::State<ServerState>, uri: axum::http::Uri) -> (axum::http::StatusCode,Vec<u8>) {
            let path = uri.path().to_string();
            s.requests.lock().unwrap().push(path.clone());
            match s.files.lock().unwrap().get(&path) {
                Some(bytes) => (axum::http::StatusCode::OK, bytes.clone()),
                None => (axum::http::StatusCode::NOT_FOUND, vec![]),
            }
        }
        let state = ServerState::default();
        let router = axum::Router::new().fallback(axum::routing::get(response)).with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
        Self { state, base, task }
    }
    fn publish(&self, bytes: Vec<u8>) {
        let index = json!({"schema":1,"generatedAt":"2026-10-27T20:00:00Z","champions":{"103":{"sha256":digest(&bytes),"guides":1}}});
        let mut files = self.state.files.lock().unwrap();
        files.insert("/index.json".into(),serde_json::to_vec(&index).unwrap());
        files.insert("/champions/103.json".into(),bytes);
    }
    fn count(&self, path: &str) -> usize { self.state.requests.lock().unwrap().iter().filter(|p| *p == path).count() }
}

#[tokio::test]
async fn disk_cache_refreshes_six_hourly_downloads_only_changed_hashes_and_survives_failure() {
    let root = std::env::temp_dir().join(format!("swapper-guide-test-{}",uuid::Uuid::new_v4()));
    let server = Server::start().await;
    let client = reqwest::Client::new();
    server.publish(champion_bytes(vec![guide()]));
    let mut cache = GuideCache::open(Some(root.clone()),server.base.clone(),&[]);
    let now = 100_000;
    assert_eq!(cache.champion(&client,103,now).await.len(),1);
    assert_eq!((server.count("/index.json"),server.count("/champions/103.json")),(1,1));
    let mut updated = guide(); updated["favoredWhen"]=json!("Updated advice.");
    server.publish(champion_bytes(vec![updated]));
    assert_eq!(cache.champion(&client,103,now+1).await.len(),1);
    assert_eq!(server.count("/index.json"),1);
    let result = cache.champion(&client,103,now+REFRESH_SECONDS).await;
    assert_eq!(serde_json::to_value(&result[0]).unwrap()["favoredWhen"],"Updated advice.");
    assert_eq!((server.count("/index.json"),server.count("/champions/103.json")),(2,2));
    cache.champion(&client,103,now+2*REFRESH_SECONDS).await;
    assert_eq!((server.count("/index.json"),server.count("/champions/103.json")),(3,2));
    // Changed metadata, damaged body: keep the fully validated old copy.
    server.publish(champion_bytes(vec![guide()]));
    server.state.files.lock().unwrap().insert("/champions/103.json".into(),b"tampered".to_vec());
    let result = cache.champion(&client,103,now+3*REFRESH_SECONDS).await;
    assert_eq!(serde_json::to_value(&result[0]).unwrap()["favoredWhen"],"Updated advice.");
    server.state.files.lock().unwrap().clear();
    assert_eq!(cache.champion(&client,103,now+4*REFRESH_SECONDS).await.len(),1);
    let count = server.count("/index.json");
    let mut reopened = GuideCache::open(Some(root.clone()),server.base.clone(),&[]);
    assert_eq!(reopened.champion(&client,103,now+4*REFRESH_SECONDS+1).await.len(),1);
    assert_eq!(server.count("/index.json"),count,"failed attempts remain throttled across restart");
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn missing_repository_uses_a_verified_bundled_snapshot_or_no_guide() {
    let server = Server::start().await;
    let client = reqwest::Client::new();
    let mut empty = GuideCache::open(None,server.base.clone(),&[]);
    assert!(empty.champion(&client,103,100_000).await.is_empty());
    empty.champion(&client,103,100_001).await;
    assert_eq!(server.count("/index.json"),1,"404 is a six-hour attempt too");
    let bytes: &'static [u8] = Box::leak(champion_bytes(vec![guide()]).into_boxed_slice());
    let index = serde_json::to_vec(&json!({"schema":1,"generatedAt":"2026-10-27T20:00:00Z","champions":{"103":{"sha256":digest(bytes),"guides":1}}})).unwrap();
    let index: &'static [u8] = Box::leak(index.into_boxed_slice());
    let offline: &'static [(&'static str,&'static [u8])] = Box::leak(vec![("index.json",index),("champions/103.json",bytes)].into_boxed_slice());
    let mut cache = GuideCache::open(None,server.base.clone(),offline);
    assert_eq!(cache.champion(&client,103,100_000).await.len(),1);
    assert!(cache.champion(&client,238,100_000).await.is_empty());
    server.publish(vec![b'x';MAX_CHAMPION_BYTES+1]);
    assert_eq!(cache.champion(&client,103,100_000+REFRESH_SECONDS).await.len(),1,"oversized network data falls back to the bundle");
}

fn general() -> Value {
    let mut value = guide();
    for field in ["enemy", "difficulty", "favoredWhen", "enemyTips"] { value.as_object_mut().unwrap().remove(field); }
    for tip in value["keyTips"].as_array_mut().unwrap() {
        tip["scope"] = json!("champion");
        tip["text"] = json!("Hold {cd:103:E}.");
    }
    value
}

fn with_general(guides: Vec<Value>, general: Vec<Value>) -> Vec<u8> {
    let mut file: Value = serde_json::from_slice(&champion_bytes(guides)).unwrap();
    file["general"] = json!(general);
    serde_json::to_vec(&file).unwrap()
}

#[test]
fn general_guides_parse_without_breaking_old_files_or_the_index() {
    assert_eq!(parse_champion(&champion_bytes(vec![guide()]), 103).unwrap().len(), 1);
    let parsed = parse_champion(&with_general(vec![guide()], vec![general()]), 103).unwrap();
    assert_eq!(parsed.len(), 2);
    let rendered = serde_json::to_value(&parsed[1]).unwrap();
    assert_eq!(rendered, general());
    let index = json!({"schema":1,"generatedAt":"x","champions":{"103":{"sha256":"a".repeat(64),"guides":0,"general":1}}});
    assert!(parse_index(&serde_json::to_vec(&index).unwrap()).is_some());
    let mut empty = index;
    empty["champions"]["103"]["general"] = json!(0);
    assert!(parse_index(&serde_json::to_vec(&empty).unwrap()).is_none());
}

#[test]
fn general_guides_validate_participants_scope_and_duplicate_lanes() {
    for (field, value) in [("abilityKeys", json!([{"champion":238,"key":"W"}])),
        ("text", json!("Wait for {cd:238:W}.")), ("scope", json!("pair")),
        ("sourceIds", json!(["missing"]))] {
        let mut bad = general();
        bad["keyTips"][0][field] = value;
        let parsed = parse_champion(&with_general(vec![guide()], vec![bad]), 103).unwrap();
        assert_eq!(parsed.len(), 1, "{field}");
    }
    assert!(parse_champion(&with_general(vec![], vec![general(), general()]), 103).unwrap().is_empty());
}

#[test]
fn selection_prefers_pair_then_general_for_the_lane_then_none() {
    let mut top_pair = guide(); top_pair["lane"] = json!("top");
    let mut top_general = general(); top_general["lane"] = json!("top");
    let bytes = with_general(vec![guide(), top_pair], vec![general(), top_general]);
    let guides = parse_champion(&bytes, 103).unwrap();
    let pair = select_guide(guides.clone(), 238, Lane::Mid).unwrap();
    assert_eq!(pair.enemy, Some(238));
    assert_eq!(pair.lane, Lane::Mid);
    let fallback = select_guide(guides.clone(), 7, Lane::Mid).unwrap();
    assert_eq!(fallback.enemy, None);
    assert_eq!(fallback.lane, Lane::Mid);
    assert!(select_guide(guides.clone(), 238, Lane::Support).is_none());
    assert!(select_guide(guides, 7, Lane::Jungle).is_none());
    let only_general = parse_champion(&with_general(vec![], vec![general()]), 103).unwrap();
    assert!(select_guide(only_general, 238, Lane::Mid).unwrap().enemy.is_none());
    let only_pair = parse_champion(&champion_bytes(vec![guide()]), 103).unwrap();
    assert!(select_guide(only_pair, 7, Lane::Mid).is_none());
}

#[test]
fn general_outdated_flags_only_follow_the_champion_and_respect_live_cooldowns() {
    let mut value = general();
    value["keyTips"][0]["text"] = json!("Hold Charm.");
    value["keyTips"][0]["outdated"] = json!(true);
    value["keyTips"][1]["text"] = json!("Keep your escape.");
    value["keyTips"][1]["patch"] = json!("26.22");
    let mut guide = parse_champion(&with_general(vec![], vec![value]), 103).unwrap().remove(0);
    flag_tips(&mut guide, &[("26.21".into(), vec![238])]);
    assert!(serde_json::to_value(&guide).unwrap()["keyTips"][0].get("outdated").is_none());
    flag_tips(&mut guide, &[("26.21".into(), vec![103])]);
    let rendered = serde_json::to_value(&guide).unwrap();
    assert_eq!(rendered["keyTips"][0]["outdated"], true);
    assert!(rendered["keyTips"][1].get("outdated").is_none());
    assert!(rendered["keyTips"][2].get("outdated").is_none());
}
