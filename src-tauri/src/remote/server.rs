mod actions;
mod assets;
mod lan;
mod runes;
mod socket;

use super::RemoteCore;
use actions::{
    accept, champion_icon, champions, game, lock_champion, prepick_champion, select_champion,
};
use assets::asset;
use axum::extract::State;
use axum::http::header;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use lan::{authorize_lan, consume_handoff, handoff_ping, mint_handoff, pair_lan_device};
use runes::{
    apply_runes, apply_spells, champion_counters, champion_list, champion_overview, duo_list,
    import_item_build, item_icon, keystone_build, matchup, pro_builds, rank_icon, role_icon,
    rune_icon, runes, set_auto_apply, set_import_items_with_runes, set_rune_tier,
    set_spells_with_runes, spell_icon, team_icon, tier_list,
};
use socket::socket;
use std::sync::Arc;

pub fn router(core: Arc<RemoteCore>) -> Router {
    routes().with_state(core)
}

fn routes() -> Router<Arc<RemoteCore>> {
    Router::new()
        .route("/api/status", get(status))
        .route("/api/game", get(game))
        .route("/api/champions", get(champions))
        .route("/api/champion/icon/{id}", get(champion_icon))
        .route("/api/ready-check/accept", axum::routing::post(accept))
        .route("/api/champion/select", axum::routing::post(select_champion))
        .route(
            "/api/champion/prepick",
            axum::routing::post(prepick_champion),
        )
        .route("/api/champion/lock", axum::routing::post(lock_champion))
        .route("/api/runes", get(runes))
        .route("/api/runes/pro-builds", get(pro_builds))
        .route("/api/runes/keystone-build", get(keystone_build))
        .route("/api/runes/matchup", get(matchup))
        .route("/api/runes/counters", get(champion_counters))
        .route("/api/runes/overview", get(champion_overview))
        .route("/api/runes/tierlist", get(tier_list))
        .route("/api/runes/duos", get(duo_list))
        .route("/api/runes/champions", get(champion_list))
        .route("/api/items/import", axum::routing::post(import_item_build))
        .route("/api/runes/apply", axum::routing::post(apply_runes))
        .route("/api/runes/auto-apply", axum::routing::post(set_auto_apply))
        .route("/api/runes/tier", axum::routing::post(set_rune_tier))
        .route(
            "/api/runes/spells-setting",
            axum::routing::post(set_spells_with_runes),
        )
        .route(
            "/api/runes/items-setting",
            axum::routing::post(set_import_items_with_runes),
        )
        .route("/api/runes/spells", axum::routing::post(apply_spells))
        .route("/api/rune/icon/{id}", get(rune_icon))
        .route("/api/role/icon/{role}", get(role_icon))
        .route("/api/spell/icon/{id}", get(spell_icon))
        .route("/api/item/icon/{id}", get(item_icon))
        .route("/api/rank/icon/{tier}", get(rank_icon))
        .route("/api/team/icon/{team}", get(team_icon))
        .route("/ws", get(socket))
        .fallback(asset)
}

pub fn lan_router(core: Arc<RemoteCore>) -> Router {
    routes()
        .route("/pair", get(pair_lan_device))
        // Authenticated: mints a one-time handoff token for the calling device.
        .route("/handoff/token", axum::routing::post(mint_handoff))
        // Unauthenticated: consumes the token on the `.local` origin only.
        .route("/handoff", get(consume_handoff))
        .route("/handoff/ping", get(handoff_ping))
        .layer(middleware::from_fn_with_state(core.clone(), authorize_lan))
        .with_state(core)
}

async fn status(State(core): State<Arc<RemoteCore>>) -> Response {
    let body = serde_json::to_vec(&core.status()).unwrap_or_else(|_| b"{}".to_vec());
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}
