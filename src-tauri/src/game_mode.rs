use serde_json::Value;

/// Accept incomplete LCU gameflow or Live Client data unless it identifies another mode.
pub fn supports_rift(session: &Value) -> bool {
    let data = &session["gameData"];
    let queues = [&data["queue"], &session["queue"]];
    // SR custom, draft/blind, solo/flex, Swiftplay/Quickplay and Clash; other queues are out of scope.
    const SR_QUEUES: [i64; 8] = [0, 400, 420, 430, 440, 480, 490, 700];
    for queue in queues {
        if queue["id"].as_i64().is_some_and(|id| id > 0 && !SR_QUEUES.contains(&id)) {
            return false;
        }
        let kind = queue["type"].as_str().unwrap_or("").to_ascii_uppercase();
        if ["BOT", "COOP", "PRACTICE", "TUTORIAL", "ARAM", "TFT", "CHERRY", "URF", "ONEFORALL", "NEXUSBLITZ", "BRAWL"]
            .iter().any(|mode| kind.contains(mode)) {
            return false;
        }
    }
    let maps = [&session["map"]["id"], &data["queue"]["mapId"], &session["queue"]["mapId"], &data["mapNumber"]];
    if maps.iter().any(|map| map.as_i64().is_some_and(|id| id > 0 && id != 11)) {
        return false;
    }
    [&data["queue"]["gameMode"], &session["queue"]["gameMode"], &session["map"]["gameMode"], &data["gameMode"]]
        .iter().all(|mode| {
            let mode = mode.as_str().unwrap_or("").trim().to_ascii_uppercase();
            matches!(mode.as_str(), "" | "CLASSIC" | "SWIFTPLAY")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sr_queues_and_custom_games() {
        for id in [0, 400, 420, 430, 440, 480, 490, 700] {
            assert!(supports_rift(&json!({"gameData":{"queue":{"id":id,"mapId":11,"gameMode":"CLASSIC"}}})), "queue {id}");
            assert!(supports_rift(&json!({"gameData":{"queue":{"id":id}}})), "queue only {id}");
        }
        assert!(supports_rift(&json!({"gameData":{"isCustomGame":true},"map":{"id":11,"gameMode":"CLASSIC"}})));
        assert!(supports_rift(&json!({"gameData":{"queue":{"id":-1,"mapId":11,"gameMode":"CLASSIC"}}})));
        assert!(supports_rift(&json!({"queue":{"id":480,"mapId":11,"gameMode":"SWIFTPLAY"}})));
    }

    #[test]
    fn unsupported_queues_even_on_classic_rift() {
        // ARAM/Clash, Arena, TFT variants, URF/ARURF, OFA, practice, AI, Blitz, Brawl, tutorials.
        for id in [450, 720, 1700, 1710, 1720, 1090, 1100, 1110, 1130, 1160, 1170, 900, 1900, 1020, 2000, 830, 840, 850, 870, 880, 890, 1200, 1300, 2300, 200, 201, 202] {
            assert!(!supports_rift(&json!({"gameData":{"queue":{"id":id,"mapId":11,"gameMode":"CLASSIC"}}})), "queue {id}");
        }
    }

    #[test]
    fn unsupported_modes_and_maps_override_an_sr_queue() {
        for mode in ["ARAM", "CHERRY", "TFT", "URF", "ARURF", "ONEFORALL", "PRACTICETOOL", "NEXUSBLITZ", "BRAWL", "STRAWBERRY", "TUTORIAL"] {
            assert!(!supports_rift(&json!({"gameData":{"queue":{"id":420,"gameMode":mode}}})), "{mode}");
            assert!(!supports_rift(&json!({"gameData":{"gameMode":mode,"mapNumber":11}})), "Live Client {mode}");
        }
        for map in [12, 21, 30, 33, 35] {
            assert!(!supports_rift(&json!({"map":{"id":map},"gameData":{"queue":{"id":420}}})), "map {map}");
            assert!(!supports_rift(&json!({"gameData":{"mapNumber":map}})), "Live Client map {map}");
        }
        assert!(!supports_rift(&json!({"gameData":{"queue":{"type":"BOT"}}})));
        assert!(!supports_rift(&json!({"gameData":{"queue":{"type":"PRACTICETOOL"}}})));
        assert!(!supports_rift(&json!({"queue":{"id":870}})));
        assert!(!supports_rift(&json!({"map":{"id":11,"gameMode":"PRACTICETOOL"},"gameData":{"queue":{"id":-1}}})));
    }

    #[test]
    fn missing_fields_do_not_hide_rift_games() {
        for value in [json!(null), json!({}), json!({"map":{"id":0},"gameData":{"queue":{"id":-1,"gameMode":""}}}), json!({"gameData":{"queue":{"mapId":11}}}), json!({"gameData":{"queue":{"gameMode":"CLASSIC"}}}), json!({"gameData":{"gameMode":"CLASSIC","mapNumber":11}})] {
            assert!(supports_rift(&value), "{value}");
        }
    }
}
