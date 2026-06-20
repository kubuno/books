// Background scheduler: runs the per-library "scan on startup" and periodic "scan interval"
// settings. A single task scans flagged libraries at boot, then wakes every 5 minutes to scan
// libraries whose interval is due.
use std::time::Duration;

use uuid::Uuid;

use crate::state::AppState;

fn interval_minutes(s: &str) -> Option<i64> {
    Some(match s {
        "hourly" => 60,
        "every_6h" => 360,
        "every_12h" => 720,
        "daily" => 1440,
        "weekly" => 10080,
        _ => return None, // "disabled" or unknown
    })
}

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        startup_scans(&state).await;
        let mut tick = tokio::time::interval(Duration::from_secs(300));
        tick.tick().await; // consume the immediate first tick
        loop {
            tick.tick().await;
            interval_scans(&state).await;
        }
    });
}

async fn startup_scans(state: &AppState) {
    let rows = sqlx::query_as::<_, (Uuid, serde_json::Value)>(
        "SELECT id, settings FROM books.libraries WHERE source_type = 'files_folder'",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    for (id, s) in rows {
        if s.pointer("/scanner/scan_on_startup").and_then(|v| v.as_bool()).unwrap_or(false) {
            trigger(state, id).await;
        }
    }
}

async fn interval_scans(state: &AppState) {
    let rows = sqlx::query_as::<_, (Uuid, serde_json::Value, Option<chrono::DateTime<chrono::Utc>>, String)>(
        "SELECT id, settings, last_scan_at, scan_status FROM books.libraries WHERE source_type = 'files_folder'",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let now = chrono::Utc::now();
    for (id, s, last, status) in rows {
        if status == "scanning" {
            continue;
        }
        let Some(mins) = s.pointer("/scanner/scan_interval").and_then(|v| v.as_str()).and_then(interval_minutes) else {
            continue;
        };
        let due = match last {
            Some(t) => (now - t).num_minutes() >= mins,
            None => true,
        };
        if due {
            trigger(state, id).await;
        }
    }
}

async fn trigger(state: &AppState, id: Uuid) {
    let _ = sqlx::query("UPDATE books.libraries SET scan_status = 'scanning', scan_error = NULL WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await;
    let st = state.clone();
    tokio::spawn(async move { crate::services::scan::scan_library(st, id).await });
}
