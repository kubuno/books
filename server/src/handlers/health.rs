use axum::Json;
use serde_json::{json, Value};

/// GET /health — liveness probe (no auth, no DB).
pub async fn health() -> Json<Value> {
    Json(json!({
        "status":  "ok",
        "module":  "books",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
