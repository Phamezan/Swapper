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
        ("durationSec", json!(1.5)), ("uploaded", json!("2026-13-01")), ("uploaded", json!("yesterday")),
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

fn champion_vods(groups: Value) -> Vec<u8> {
    let mut file: Value = serde_json::from_slice(&champion_bytes(vec![guide()])).unwrap();
    file["vods"] = groups;
    serde_json::to_vec(&file).unwrap()
}

fn video(id: &str) -> Value { let mut v = vod(); v["videoId"] = json!(id); v }

#[test]
fn champion_vods_span_every_matchup_with_the_same_validation() {
    let mut bad = video("bad-title-1"); bad["title"] = json!("<b>Bad</b>");
    let bytes = champion_vods(json!([
        {"enemy":238,"lane":"mid","videos":[video("zedMid00001"), bad, video("zedMid00002")]},
        {"enemy":157,"lane":"mid","videos":[video("yasuoMid001")]},
        {"enemy":238,"lane":"top","videos":[video("zedTop00001")]},
        {"enemy":0,"lane":"mid","videos":[video("noEnemy0001")]},
        {"enemy":7,"lane":"bottom","videos":[video("badLane0001")]},
    ]));
    let all = parse_champion_vods(&bytes, 103);
    let keys: Vec<_> = all.iter().map(|v| (v.enemy, v.lane.clone(), serde_json::to_value(v).unwrap()["videoId"].as_str().unwrap().to_string())).collect();
    assert_eq!(keys, vec![(Some(238), Lane::Mid, "zedMid00001".into()), (Some(238), Lane::Mid, "zedMid00002".into()),
        (Some(157), Lane::Mid, "yasuoMid001".into()), (Some(238), Lane::Top, "zedTop00001".into())]);
    let wire = serde_json::to_value(&all[2]).unwrap();
    assert_eq!((wire["enemy"].clone(), wire["lane"].clone(), wire["videoId"].clone(), wire["rankClaim"].clone()),
        (json!(157), json!("mid"), json!("yasuoMid001"), json!("Challenger")), "the matchup key sits beside the flattened video");
    assert_eq!(parse_vods(&bytes, 103, 238, Lane::Mid).len(), 2, "the matchup view still reads the same file");
    assert!(parse_champion_vods(&bytes, 238).is_empty(), "another champion's file is rejected");
    assert!(parse_champion_vods(&champion_bytes(vec![guide()]), 103).is_empty());
    assert!(parse_champion_vods(b"not json", 103).is_empty());
}

#[test]
fn champion_vods_drop_every_copy_of_an_ambiguous_matchup_or_video() {
    let bytes = champion_vods(json!([
        {"enemy":238,"lane":"mid","videos":[video("zedMid00001"), video("twiceInZed1"), video("twiceInZed1")]},
        {"enemy":157,"lane":"mid","videos":[video("yasuoMid001")]},
        {"enemy":157,"lane":"mid","videos":[video("yasuoMid002")]},
        {"enemy":7,"lane":"mid","videos":[video("zedMid00001"), video("leblanc0001")]},
    ]));
    let ids: Vec<_> = parse_champion_vods(&bytes, 103).iter().map(|v| serde_json::to_value(v).unwrap()["videoId"].clone()).collect();
    assert_eq!(ids, vec![json!("leblanc0001")], "duplicate ids, duplicate matchups and a video filed under two enemies are all dropped");
    assert!(parse_vods(&bytes, 103, 157, Lane::Mid).is_empty());
}

#[tokio::test]
async fn champion_files_up_to_eight_megabytes_download_cache_and_parse() {
    assert_eq!(MAX_CHAMPION_BYTES, 8 * 1024 * 1024);
    let mut big = with_vods(vec![vod()]);
    big.resize(6 * 1024 * 1024, b' ');
    assert_eq!(parse_vods(&big, 103, 238, Lane::Mid).len(), 1);
    assert_eq!(parse_champion_vods(&big, 103).len(), 1);
    let mut over = big.clone(); over.resize(MAX_CHAMPION_BYTES + 1, b' ');
    assert!(parse_champion(&over, 103).is_none() && parse_vods(&over, 103, 238, Lane::Mid).is_empty() && parse_champion_vods(&over, 103).is_empty());
    let root = std::env::temp_dir().join(format!("swapper-big-guide-test-{}", uuid::Uuid::new_v4()));
    let server = Server::start().await;
    let client = reqwest::Client::new();
    server.publish(big);
    assert_eq!(GuideCache::open(Some(root.clone()), server.base.clone(), &[]).champion(&client, 103, 100_000).await.len(), 1);
    server.state.files.lock().unwrap().clear();
    let mut reopened = GuideCache::open(Some(root.clone()), server.base.clone(), &[]);
    let bytes = reopened.content(&client, 103, 100_000 + REFRESH_SECONDS).await;
    assert_eq!(parse_champion_vods(&bytes, 103).len(), 1, "the disk cache holds a 6 MB file");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unknown_vod_fields_are_ignored_and_never_forwarded() {
    let mut video = vod();
    video["fixture"] = json!(true);
    video["playerImage"] = json!("https://evil.io/photo");
    video["futureField"] = json!({"any": "shape"});
    video["uploaded"] = json!("2026-08-25");
    video["moments"][0]["futureField"] = json!(1);
    let mut file: Value = serde_json::from_slice(&with_vods(vec![video])).unwrap();
    file["vods"][0]["futureField"] = json!(true);
    let vods = parse_vods(&serde_json::to_vec(&file).unwrap(), 103, 238, Lane::Mid);
    assert_eq!(vods.len(), 1, "newer content keeps working in this app version");
    let wire = serde_json::to_value(&vods[0]).unwrap();
    for field in ["fixture", "playerImage", "futureField"] { assert!(wire.get(field).is_none(), "{field}"); }
    assert!(wire["moments"][0].get("futureField").is_none());
    assert_eq!(wire["uploaded"], json!("2026-08-25"));
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
fn streamer_segments_and_vs_any_vods_validate_and_dedupe_by_segment() {
    let mut stream = vod();
    stream["format"] = json!("stream"); stream["cam"] = json!(true);
    stream["startSec"] = json!(100); stream["endSec"] = json!(900);
    let bytes = with_vods(vec![stream.clone()]);
    let vods = parse_vods(&bytes, 103, 238, Lane::Mid);
    assert_eq!(vods.len(), 1);
    let wire = serde_json::to_value(&vods[0]).unwrap();
    assert_eq!((wire["format"].clone(), wire["cam"].clone(), wire["startSec"].clone(), wire["endSec"].clone()), (json!("stream"), json!(true), json!(100), json!(900)));
    // Invalid format/cam/segment drops only that video.
    for (field, value) in [("format", json!("replay")), ("format", json!("vod")), ("cam", json!(false)), ("startSec", json!(900)), ("endSec", json!(50)), ("endSec", json!(99999))] {
        let mut bad = stream.clone(); bad[field] = value.clone();
        assert_eq!(parse_vods(&with_vods(vec![bad, vod()]), 103, 238, Lane::Mid).len(), 1, "{field}={value}");
    }
    let mut only_start = stream.clone(); only_start.as_object_mut().unwrap().remove("endSec");
    assert_eq!(parse_vods(&with_vods(vec![only_start, vod()]), 103, 238, Lane::Mid).len(), 1, "both-or-neither");
    // Two segments of one stream both survive; the exact same segment drops both copies.
    let mut second = stream.clone(); second["startSec"] = json!(1000); second["endSec"] = json!(1700);
    assert_eq!(parse_vods(&with_vods(vec![stream.clone(), second.clone()]), 103, 238, Lane::Mid).len(), 2);
    assert!(parse_vods(&with_vods(vec![stream.clone(), stream.clone()]), 103, 238, Lane::Mid).is_empty());
    // Same segment under two matchups is still ambiguous across a champion.
    let cross = champion_vods(json!([{"enemy":238,"lane":"mid","videos":[stream.clone()]},{"enemy":157,"lane":"mid","videos":[stream.clone()]}]));
    assert!(parse_champion_vods(&cross, 103).is_empty());
    let cross2 = champion_vods(json!([{"enemy":238,"lane":"mid","videos":[stream.clone()]},{"enemy":157,"lane":"mid","videos":[second]}]));
    assert_eq!(parse_champion_vods(&cross2, 103).len(), 2, "different segments of one stream under different enemies both survive");
    // Enemy-less group: champion/all views keep it (no enemy on the wire), matchup view excludes it.
    let any_bytes = champion_vods(json!([{"lane":"mid","videos":[video("anyEnemy001")]}]));
    let all = parse_champion_vods(&any_bytes, 103);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].enemy, None);
    assert!(serde_json::to_value(&all[0]).unwrap().get("enemy").is_none());
    assert!(parse_vods(&any_bytes, 103, 238, Lane::Mid).is_empty());
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
    active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    peak: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    delay_ms: std::sync::Arc<std::sync::atomic::AtomicU64>,
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
            use std::sync::atomic::Ordering::SeqCst;
            let active = s.active.fetch_add(1, SeqCst) + 1;
            s.peak.fetch_max(active, SeqCst);
            tokio::time::sleep(Duration::from_millis(s.delay_ms.load(SeqCst))).await;
            let result = match s.files.lock().unwrap().get(&path) {
                Some(bytes) => (axum::http::StatusCode::OK, bytes.clone()),
                None => (axum::http::StatusCode::NOT_FOUND, vec![]),
            };
            s.active.fetch_sub(1, SeqCst);
            result
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


#[test]
fn vod_region_and_estimated_patch_are_optional_validated_and_forwarded() {
    for region in ["KR", "NA", "EUW", "EUNE"] {
        for estimated in [true, false] {
            let mut v = vod(); v["region"] = json!(region); v["patchEstimated"] = json!(estimated);
            let bytes = with_vods(vec![v]);
            let wire = serde_json::to_value(&parse_vods(&bytes, 103, 238, Lane::Mid)[0]).unwrap();
            assert_eq!(wire["region"], region);
            assert_eq!(wire["patchEstimated"], estimated);
            let all = serde_json::to_value(&parse_champion_vods(&bytes, 103)[0]).unwrap();
            assert_eq!(all["region"], region);
            assert_eq!(all["patchEstimated"], estimated);
        }
    }
    let wire = serde_json::to_value(&parse_vods(&with_vods(vec![vod()]), 103, 238, Lane::Mid)[0]).unwrap();
    assert!(wire.get("region").is_none() && wire.get("patchEstimated").is_none());
    for (field, value) in [("region", json!("BR")), ("region", json!("kr")), ("region", json!(1)),
        ("patchEstimated", json!("true")), ("patchEstimated", json!(1))] {
        let mut bad = vod(); bad[field] = value;
        let bytes = with_vods(vec![bad, vod()]);
        assert_eq!(parse_vods(&bytes, 103, 238, Lane::Mid).len(), 1, "{field}");
        assert_eq!(parse_champion_vods(&bytes, 103).len(), 1, "{field}");
    }
}


fn library_file(champion: i64, videos: Vec<Value>) -> Vec<u8> {
    let mut file: Value = serde_json::from_slice(&with_vods(videos)).unwrap();
    file["champion"] = json!(champion);
    file["guides"] = json!([]);
    serde_json::to_vec(&file).unwrap()
}

fn publish_library(server: &Server, entries: Vec<(i64, Vec<u8>)>) {
    let mut index = json!({"schema":1,"generatedAt":"2026-11-02","champions":{}});
    let mut files = server.state.files.lock().unwrap();
    for (id, bytes) in entries {
        index["champions"][id.to_string()] = json!({"sha256":digest(&bytes),"guides":0,"vods":1});
        files.insert(format!("/champions/{id}.json"), bytes);
    }
    files.insert("/index.json".into(), serde_json::to_vec(&index).unwrap());
}

#[tokio::test]
async fn all_vods_spans_champions_skips_failed_content_and_reuses_the_content_cache() {
    let server = Server::start().await;
    let mut invalid = video("invalid0001"); invalid["region"] = json!("invalid");
    publish_library(&server, vec![(103, library_file(103, vec![video("ahriVod0001"), invalid])),
        (238, library_file(238, vec![video("zedVod00001")])), (17, library_file(17, vec![vod()])),
        (18, library_file(103, vec![vod()]))]);
    {
        let mut files = server.state.files.lock().unwrap();
        files.remove("/champions/17.json");
        let mut index: Value = serde_json::from_slice(files.get("/index.json").unwrap()).unwrap();
        index["champions"]["222"] = json!({"sha256":"a".repeat(64),"guides":1,"vods":0});
        files.insert("/index.json".into(), serde_json::to_vec(&index).unwrap());
    }
    let root = std::env::temp_dir().join(format!("swapper-all-vod-test-{}", uuid::Uuid::new_v4()));
    let client = reqwest::Client::new();
    let cache = SharedGuideCache::new(GuideCache::open(Some(root.clone()), server.base.clone(), &[]));
    let all = cache.all_vods(&client, 100_000).await;
    let wire = serde_json::to_value(&all).unwrap();
    assert_eq!(all.len(), 2, "a 404 and a wrong-champion file do not discard valid champions");
    assert_eq!((wire[0]["champion"].clone(), wire[0]["enemy"].clone(), wire[0]["lane"].clone(), wire[0]["videoId"].clone()),
        (json!(103), json!(238), json!("mid"), json!("ahriVod0001")));
    assert_eq!(wire[1]["champion"], 238);
    assert_eq!(server.count("/champions/222.json"), 0, "only positive VOD entries are loaded");
    assert_eq!(server.count("/index.json"), 1);
    assert_eq!(parse_champion_vods(&cache.content(&client, 103, 100_001).await, 103).len(), 1);
    assert_eq!(server.count("/champions/103.json"), 1, "single-champion reads reuse the aggregate download");
    assert_eq!(cache.all_vods(&client, 100_001).await.len(), 2);
    assert_eq!(server.count("/champions/17.json"), 1, "failed attempts remain throttled");
    server.state.files.lock().unwrap().clear();
    let reopened = SharedGuideCache::new(GuideCache::open(Some(root.clone()), server.base.clone(), &[]));
    assert_eq!(reopened.all_vods(&client, 100_002).await.len(), 2, "aggregate downloads persist across offline restart");
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn all_vods_fetches_at_most_four_champions_concurrently_in_stable_order() {
    use std::sync::atomic::Ordering::SeqCst;
    let server = Server::start().await;
    server.state.delay_ms.store(25, SeqCst);
    publish_library(&server, (100..106).map(|id| (id, library_file(id, vec![vod()]))).collect());
    let cache = SharedGuideCache::new(GuideCache::open(None, server.base.clone(), &[]));
    let all = cache.all_vods(&reqwest::Client::new(), 100_000).await;
    assert_eq!(all.iter().map(|vod| vod.champion).collect::<Vec<_>>(), (100..106).collect::<Vec<_>>());
    let peak = server.state.peak.load(SeqCst);
    assert!((2..=4).contains(&peak), "expected parallel fetches bounded by four, saw {peak}");
}

#[tokio::test]
async fn all_vods_in_local_mode_uses_only_the_verified_bundled_index_and_content() {
    let bytes: &'static [u8] = Box::leak(library_file(103, vec![vod()]).into_boxed_slice());
    let index: &'static [u8] = Box::leak(serde_json::to_vec(&json!({"schema":1,"generatedAt":"2026-11-02",
        "champions":{"103":{"sha256":digest(bytes),"guides":0,"vods":1},"238":{"sha256":"a".repeat(64),"guides":0,"vods":1}}})).unwrap().into_boxed_slice());
    let offline: &'static [(&'static str, &'static [u8])] = Box::leak(vec![("index.json", index), ("champions/103.json", bytes),
        ("champions/238.json", bytes)].into_boxed_slice());
    let cache = SharedGuideCache::new(GuideCache::open(None, String::new(), offline));
    let all = cache.all_vods(&reqwest::Client::new(), 100_000).await;
    assert_eq!(all.len(), 1, "missing/invalid bundled files are skipped, without accepting an unchecked payload");
    assert_eq!(all[0].champion, 103);
    let data = cache.data.lock().await;
    assert!(data.index.is_none() && data.champions.is_empty(), "local mode bypasses published cache state");
}


#[tokio::test]
async fn all_vods_repairs_corrupt_cached_content_even_when_the_stored_hash_matches() {
    let server = Server::start().await;
    let bytes = library_file(103, vec![vod()]);
    publish_library(&server, vec![(103, bytes.clone())]);
    let root = std::env::temp_dir().join(format!("swapper-all-vod-repair-{}", uuid::Uuid::new_v4()));
    let client = reqwest::Client::new();
    let cache = SharedGuideCache::new(GuideCache::open(Some(root.clone()), server.base.clone(), &[]));
    cache.data.lock().await.champions.insert(103, CachedChampion { sha256: digest(&bytes), content: "corrupt".into() });
    assert_eq!(cache.all_vods(&client, 100_000).await.len(), 1);
    assert!(cache.data.lock().await.cached(103).is_some(), "a valid aggregate download must replace the damaged held body, even with the same hash field");
    assert_eq!(cache.content(&client, 103, 100_001).await, bytes);
    assert_eq!(server.count("/champions/103.json"), 1);
    server.state.files.lock().unwrap().clear();
    let reopened = SharedGuideCache::new(GuideCache::open(Some(root.clone()), server.base.clone(), &[]));
    assert_eq!(reopened.content(&client, 103, 100_002).await, bytes, "the repaired body is persisted");
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn slow_aggregate_does_not_block_another_cached_champion() {
    use std::sync::atomic::Ordering::SeqCst;
    let server = Server::start().await;
    let bytes = library_file(238, vec![vod()]);
    publish_library(&server, vec![(103, library_file(103, vec![vod()])), (238, bytes.clone())]);
    let client = reqwest::Client::new();
    let mut data = GuideCache::open(None, server.base.clone(), &[]);
    data.refresh_index(&client, 100_000).await;
    data.store_champion(238, CachedChampion { sha256: digest(&bytes), content: String::from_utf8(bytes).unwrap() });
    let cache = SharedGuideCache::new(data);
    server.state.delay_ms.store(2000, SeqCst);
    let aggregate = cache.all_vods(&client, 100_001);
    let request = async {
        tokio::time::timeout(Duration::from_secs(2), async {
            while server.count("/champions/103.json") == 0 { tokio::time::sleep(Duration::from_millis(5)).await; }
        }).await.expect("aggregate started downloading");
        let bytes = tokio::time::timeout(Duration::from_millis(250), cache.content(&client, 238, 100_001))
            .await.expect("cached champion must not wait for aggregate downloads");
        assert_eq!(parse_champion_vods(&bytes, 238).len(), 1);
    };
    tokio::join!(aggregate, request);
}

#[tokio::test]
async fn aggregate_and_single_champion_share_one_download() {
    use std::sync::atomic::Ordering::SeqCst;
    let server = Server::start().await;
    publish_library(&server, vec![(103, library_file(103, vec![vod()]))]);
    server.state.delay_ms.store(100, SeqCst);
    let cache = SharedGuideCache::new(GuideCache::open(None, server.base.clone(), &[]));
    let client = reqwest::Client::new();
    let (all, bytes) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(cache.all_vods(&client, 100_000), cache.content(&client, 103, 100_000))
    }).await.expect("concurrent requests complete");
    assert_eq!(all.len(), 1);
    assert_eq!(parse_champion_vods(&bytes, 103).len(), 1);
    assert_eq!(server.count("/index.json"), 1);
    assert_eq!(server.count("/champions/103.json"), 1);
}
