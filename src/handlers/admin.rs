use axum::{extract::{Extension, Path, State}, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{errors::BooksError, middleware::auth::AuthUser, state::AppState};

fn require_admin(user: &AuthUser) -> Result<(), BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    Ok(())
}

/// One key/value settings row from `books.settings`.
#[derive(Debug, sqlx::FromRow)]
struct SettingRow {
    key:   String,
    value: String,
}

// ── GET /books/admin/settings ─────────────────────────────────────────────────

pub async fn get_settings(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    require_admin(&user)?;

    let rows = sqlx::query_as::<_, SettingRow>(
        "SELECT key, value FROM books.settings ORDER BY key",
    )
    .fetch_all(&state.db)
    .await?;

    let settings: serde_json::Map<String, Value> = rows
        .into_iter()
        .map(|r| (r.key, Value::String(r.value)))
        .collect();

    Ok(Json(Value::Object(settings)))
}

// ── PATCH /books/admin/settings ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PatchSettingsBody {
    pub metadata_language: Option<String>,
}

pub async fn patch_settings(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<PatchSettingsBody>,
) -> Result<Json<Value>, BooksError> {
    require_admin(&user)?;

    if let Some(lang) = body.metadata_language {
        sqlx::query(
            "INSERT INTO books.settings (key, value) VALUES ('metadata_language', $1) \
             ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()",
        )
        .bind(lang)
        .execute(&state.db)
        .await?;
    }

    Ok(Json(json!({ "ok": true })))
}

// ── Per-user access / age restrictions ───────────────────────────────────────────
pub async fn get_restrictions(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(uid): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    require_admin(&user)?;
    let row = sqlx::query_as::<_, (Option<Vec<Uuid>>, Option<i32>)>(
        "SELECT library_ids, age_max FROM books.user_restrictions WHERE user_id = $1",
    )
    .bind(uid)
    .fetch_optional(&state.db)
    .await?;
    let (library_ids, age_max) = row.unwrap_or((None, None));
    Ok(Json(json!({ "user_id": uid, "library_ids": library_ids, "age_max": age_max })))
}

#[derive(Debug, Deserialize)]
pub struct RestrictionsBody {
    pub library_ids: Option<Vec<Uuid>>,
    pub age_max:     Option<i32>,
}

pub async fn set_restrictions(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(uid): Path<Uuid>,
    Json(body): Json<RestrictionsBody>,
) -> Result<Json<Value>, BooksError> {
    require_admin(&user)?;
    sqlx::query(
        "INSERT INTO books.user_restrictions (user_id, library_ids, age_max) VALUES ($1, $2, $3) \
         ON CONFLICT (user_id) DO UPDATE SET library_ids = $2, age_max = $3, updated_at = now()",
    )
    .bind(uid)
    .bind(&body.library_ids)
    .bind(body.age_max)
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "ok": true })))
}
