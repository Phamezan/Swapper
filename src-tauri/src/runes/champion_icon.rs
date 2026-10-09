//! Champion portraits from the League client's game data, for the Champion
//! tab. Cached for the session; concurrent requests for one id share a fetch.

use std::sync::OnceLock;

use super::RuneError;

static FLIGHTS: OnceLock<super::Flights<i64>> = OnceLock::new();

fn gate(id: i64) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    FLIGHTS.get_or_init(super::Flights::new).gate(&id)
}

/// The PNG bytes for a champion id.
pub async fn icon(id: i64) -> Result<Vec<u8>, RuneError> {
    if id <= 0 {
        return Err(RuneError::not_found("Unknown champion."));
    }
    if let Some(bytes) = super::shared().champion_icons.get(&id) {
        return Ok(bytes.clone());
    }
    let gate = gate(id);
    let _guard = gate.lock().await;
    if let Some(bytes) = super::shared().champion_icons.get(&id) {
        return Ok(bytes.clone());
    }
    let lcu = super::lcu().await?;
    let path = format!("/lol-game-data/assets/v1/champion-icons/{id}.png");
    let bytes = super::lcu_get_bytes(&lcu, &path).await?;
    super::shared().champion_icons.insert(id, bytes.clone());
    Ok(bytes)
}
