use serde_json::json;
use warp::reply::json;
use warp::{Rejection, Reply};

// Health handler, responds with { ok: true }
pub async fn health() -> Result<impl Reply, Rejection> {
    Ok(json(&json!({"ok": true})))
}
