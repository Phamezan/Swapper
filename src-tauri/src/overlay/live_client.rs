use serde_json::Value;
use std::time::Duration;

// This client is private, has no caller-controlled URL, no proxy and no redirects.
// The self-signed certificate exception can only reach this loopback origin.
const URL: &str = "https://127.0.0.1:2999/liveclientdata/allgamedata";

pub struct LiveClient(reqwest::Client);

impl LiveClient {
    pub fn new() -> Result<Self, String> {
        reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(800))
            .build()
            .map(Self)
            .map_err(|error| error.to_string())
    }

    pub async fn fetch(&self) -> Result<Value, String> {
        let mut response = self
            .0
            .get(URL)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("Live game response exceeded 2 MB".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    }
}
