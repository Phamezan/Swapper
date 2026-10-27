use super::*;
use serde_json::json;

fn game() -> Value {
    let positions = [("TOP","SOLO"),("JUNGLE","NONE"),("MIDDLE","SOLO"),("BOTTOM","DUO_CARRY"),("BOTTOM","DUO_SUPPORT")];
    let participants: Vec<_> = (0..10).map(|i| json!({"participantId":i+1,"teamId":if i<5 {100} else {200},
        "championId":if i==2 {103} else if i==7 {238} else {i+1},"spell1Id":4,"spell2Id":if i%5==1 {11} else {14},
        "timeline":{"lane":positions[i%5].0,"role":positions[i%5].1},
        "stats":{"win":i<5,"totalMinionsKilled":150,"neutralMinionsKilled":0}})).collect();
    json!({"gameId":7,"mapId":11,"queueId":420,"gameDuration":1800,"participants":participants,
        "participantIdentities":[{"participantId":3,"player":{"puuid":"own"}}]})
}

#[test]
fn record_identifies_the_same_lane_on_the_opposing_team() {
    let record = record_from_game(&game(), "own").unwrap();
    assert_eq!((record.champion,record.enemy,record.lane.as_str(),record.win), (103,238,"mid",true));
    let mut value = game();
    value["participants"][7]["timeline"]["lane"] = json!("NONE");
    assert!(record_from_game(&value,"own").is_none(), "never guess a missing lane");
    value["participants"][7]["teamPosition"] = json!("MIDDLE");
    assert!(record_from_game(&value,"own").is_some());
    value["participants"][8]["timeline"]["lane"] = json!("MIDDLE");
    assert!(record_from_game(&value,"own").is_none(), "ambiguous enemy");
    assert!(record_from_game(&game(),"other").is_none());
}

#[test]
fn record_skips_remakes_other_queues_and_incomplete_teams() {
    for queue in [400,420,440] {
        let mut value=game(); value["queueId"]=json!(queue);
        assert!(record_from_game(&value,"own").is_some());
    }
    for queue in [430,450,480,490] {
        let mut value=game(); value["queueId"]=json!(queue);
        assert!(record_from_game(&value,"own").is_none());
    }
    let mut value=game(); value["gameDuration"]=json!(180);
    assert!(record_from_game(&value,"own").is_none());
    value=game(); value["participants"][5]["stats"]["gameEndedInEarlySurrender"]=json!(true);
    assert!(record_from_game(&value,"own").is_none());
    value=game(); value["participants"].as_array_mut().unwrap().pop();
    assert!(record_from_game(&value,"own").is_none());
    value=game(); value["participantIdentities"][0]["participantId"]=Value::Null;
    value["participants"][2]["participantId"]=Value::Null;
    assert!(record_from_game(&value,"own").is_none());
}

#[test]
fn aggregation_keeps_accounts_deduplicates_games_and_filters_lane() {
    let mut store=Store::default();
    let mut record=record_from_game(&game(),"own").unwrap();
    store.accounts.entry("own".into()).or_default().games.insert(7,record.clone());
    store.accounts.get_mut("own").unwrap().games.insert(7,record.clone());
    record.win=false;
    store.accounts.entry("smurf".into()).or_default().games.insert(7,record.clone());
    record.lane="top".into();
    store.accounts.get_mut("own").unwrap().games.insert(8,record);
    let result=aggregate(&store,&["own".into(),"smurf".into()],103,238,"mid").unwrap();
    assert_eq!((result.wins,result.losses),(1,1));
    assert_eq!(aggregate(&store,&["own".into()],103,238,"mid").unwrap().wins,1);
    assert!(aggregate(&store,&["unsaved".into()],103,238,"mid").is_none());
}

#[test]
fn every_lane_is_paired_separately_including_carry_and_support() {
    for (i,lane) in ["top","jungle","mid","adc","support"].iter().enumerate() {
        let mut value = game();
        value["participantIdentities"][0]["participantId"] = json!(i+1);
        let record = record_from_game(&value,"own").unwrap();
        assert_eq!(record.lane,*lane);
        assert_eq!(record.enemy,value["participants"][i+5]["championId"].as_i64().unwrap());
    }
    let mut value = game();
    value["participants"][0]["timeline"]["lane"] = json!("MIDDLE");
    assert!(record_from_game(&value,"own").is_none(),"ambiguous own lane");
}

#[test]
fn storage_merges_pages_and_end_of_game_without_losing_accounts() {
    let path=std::env::temp_dir().join(format!("swapper-record-test-{}.json",uuid::Uuid::new_v4()));
    let record=record_from_game(&game(),"own").unwrap();
    merge_at(&path,"own",&[7],&[(7,record.clone())],Some(Some(7))).unwrap();
    merge_at(&path,"smurf",&[8],&[(8,record.clone())],Some(Some(8))).unwrap();
    merge_at(&path,"own",&[],&[(9,record.clone())],None).unwrap();
    merge_at(&path,"own",&[9],&[(9,record)],Some(Some(9))).unwrap();
    let store=load(&path).unwrap();
    assert_eq!(store.accounts["own"].games.len(),2);
    assert_eq!(store.accounts["smurf"].games.len(),1);
    assert_eq!(store.accounts["own"].head,Some(9));
    std::fs::write(&path,b"broken").unwrap();
    assert!(merge_at(&path,"other",&[],&[],None).is_err());
    assert_eq!(std::fs::read(&path).unwrap(),b"broken","never overwrite an unreadable record store");
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn backfill_pages_then_incremental_only_and_retries_a_failed_page() {
    use std::cell::RefCell;
    let path=std::env::temp_dir().join(format!("swapper-backfill-test-{}.json",uuid::Uuid::new_v4()));
    let calls=RefCell::new(vec![]);
    let rows: Vec<_>=(1..=23).rev().map(|id| {let mut g=game();g["gameId"]=json!(id);g}).collect();
    let run = |new: bool| {
        let rows=&rows; let calls=&calls;
        move |url: String| {
            calls.borrow_mut().push(url.clone());
            let mut all=rows.clone();
            if new { let mut g=game();g["gameId"]=json!(24);all.insert(0,g); }
            let value = if url.contains("current-summoner") { json!({"puuid":"own"}) }
            else {
                let begin=url.split("begIndex=").nth(1).unwrap().split('&').next().unwrap().parse::<usize>().unwrap();
                json!({"games":{"games":all.into_iter().skip(begin).take(PAGE_SIZE).collect::<Vec<_>>()}})
            };
            async move { Ok(value) }
        }
    };
    let check=|v: &Value| if v["puuid"]=="own" {Ok(())} else {Err("switched".into())};
    backfill(&path,"own",run(false),check).await.unwrap();
    let held=load(&path).unwrap();
    assert_eq!(held.accounts["own"].games.len(),23);
    assert!(held.accounts["own"].complete);
    assert_eq!(calls.borrow().iter().filter(|u|u.contains("matches?")).count(),2);
    calls.borrow_mut().clear();
    backfill(&path,"own",run(true),check).await.unwrap();
    assert_eq!(load(&path).unwrap().accounts["own"].games.len(),24);
    assert_eq!(calls.borrow().iter().filter(|u|u.contains("matches?")).count(),1);
    std::fs::remove_file(&path).unwrap();
    let fail_second=|url: String| {
        let request=run(false)(url.clone());
        async move { if url.contains("begIndex=20") {Err("network".into())} else {request.await} }
    };
    assert!(backfill(&path,"own",fail_second,check).await.is_err());
    let held=load(&path).unwrap();
    assert_eq!(held.accounts["own"].games.len(),20);
    assert!(!held.accounts["own"].complete);
    backfill(&path,"own",run(false),check).await.unwrap();
    assert_eq!(load(&path).unwrap().accounts["own"].games.len(),23);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn an_account_switch_discards_the_in_flight_page() {
    let path=std::env::temp_dir().join(format!("swapper-switch-test-{}.json",uuid::Uuid::new_v4()));
    let get=|url: String| async move {
        Ok(if url.contains("current-summoner") {json!({"puuid":"other"})} else {json!({"games":{"games":[game()]}})})
    };
    assert!(backfill(&path,"own",get,|_| Err("switched".into())).await.is_err());
    assert!(!path.exists());
}

#[tokio::test]
async fn repeated_server_pages_finish_backfill_and_future_scans_are_incremental() {
    let path=std::env::temp_dir().join(format!("swapper-clamped-test-{}.json",uuid::Uuid::new_v4()));
    let get=|url: String| async move {
        let rows:Vec<_>=(1..=20).rev().map(|id| {let mut g=game();g["gameId"]=json!(id);g}).collect();
        Ok(if url.contains("current-summoner") {json!({"puuid":"own"})} else {json!({"games":{"games":rows}})})
    };
    let result=backfill(&path,"own",get,|_| Ok(())).await;
    assert!(result.is_ok(),"a clamped server page is the reachable boundary: {result:?}");
    assert!(load(&path).unwrap().accounts["own"].complete);
    backfill(&path,"own",get,|_| Ok(())).await.unwrap();
    assert_eq!(load(&path).unwrap().accounts["own"].games.len(),20);
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
#[ignore = "Read-only League Client history extent probe; requires a logged-in client."]
async fn live_history_extent() {
    let client=crate::runes::lcu().await.expect("League Client unavailable");
    let current=crate::runes::lcu_get::<Value>(&client,"/lol-summoner/v1/current-summoner").await.expect("No signed-in account");
    let puuid=current["puuid"].as_str().unwrap();
    let mut baseline=HashSet::new();
    for (owner,begin,end) in [(puuid,0,19),(puuid,20,39),(puuid,0,99),("current-summoner",20,39)] {
        let url=format!("/lol-match-history/v1/products/lol/{owner}/matches?begIndex={begin}&endIndex={end}");
        let value=crate::runes::lcu_get::<Value>(&client,&url).await.expect("History unavailable");
        let rows=value["games"]["games"].as_array().unwrap();
        let ids:HashSet<_>=rows.iter().filter_map(|g|g["gameId"].as_u64()).collect();
        if baseline.is_empty() {baseline=ids.clone();}
        println!("LCU probe {} range {begin}..{end}: {} rows, {} outside first page; metadata count={} begin={} end={}",
            if owner==puuid {"puuid"} else {"current"},rows.len(),ids.difference(&baseline).count(),
            value["games"]["gameCount"],value["games"]["gameIndexBegin"],value["games"]["gameIndexEnd"]);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let mut eligible=0;let mut identified=0;
    for id in baseline.iter().take(20) {
        let detail=crate::runes::lcu_get::<Value>(&client,&format!("/lol-match-history/v1/games/{id}")).await.expect("Detail unavailable");
        if draft_game(&detail) {eligible+=1;identified+=usize::from(record_from_game(&detail,puuid).is_some());}
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    println!("LCU sample: {identified}/{eligible} draft games have an unambiguous same-position opponent.");
    let mut ids=HashSet::new();
    let mut oldest=u64::MAX;
    for page in 0..100 {
        let begin=page*PAGE_SIZE;
        let url=format!("/lol-match-history/v1/products/lol/{puuid}/matches?begIndex={begin}&endIndex={}",begin+PAGE_SIZE-1);
        let result=crate::runes::lcu_get::<Value>(&client,&url).await;
        let Ok(value)=result else {println!("LCU history: unavailable at offset {begin}; {} distinct games reachable before error.",ids.len()); return};
        let rows=value["games"]["games"].as_array().expect("Unexpected history shape");
        let before=ids.len();
        for row in rows {
            if let Some(id)=row["gameId"].as_u64() {ids.insert(id);}
            if let Some(created)=row["gameCreation"].as_u64() {oldest=oldest.min(created);}
        }
        println!("LCU history offset {begin}: {} rows, {} new, {} distinct total.",rows.len(),ids.len()-before,ids.len());
        if rows.len()<PAGE_SIZE || ids.len()==before {
            let now=crate::runes::provider::now_ms().max(0) as u64;
            println!("LCU history boundary: {} distinct games, oldest about {} days ago.",ids.len(),now.saturating_sub(oldest)/86_400_000);
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    println!("LCU history: reached 2,000 games without finding the boundary.");
}
