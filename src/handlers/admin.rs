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

/// One account and whatever restriction it carries.
#[derive(Debug, sqlx::FromRow)]
struct RestrictedUserRow {
    id:           Uuid,
    email:        String,
    display_name: Option<String>,
    role:         String,
    library_ids:  Option<Vec<Uuid>>,
    age_max:      Option<i32>,
}

/// GET /books/admin/restrictions — every account, with its restriction.
///
/// The console needs the accounts and their restrictions TOGETHER: a page that
/// could only answer "what is set for this id" would force the administrator to
/// already know which accounts are restricted, which is precisely the question
/// being asked. The join reaches into `core.users` — the same cross-schema read
/// `list_files_folders` already relies on; a module cannot invent accounts and
/// this route only ever reads them.
pub async fn list_restrictions(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    require_admin(&user)?;

    let rows = sqlx::query_as::<_, RestrictedUserRow>(
        r#"SELECT u.id, u.email, u.display_name, u.role,
                  r.library_ids, r.age_max
             FROM core.users u
             LEFT JOIN books.user_restrictions r ON r.user_id = u.id
            WHERE u.is_active = TRUE
            ORDER BY u.email"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Lecture des restrictions books échouée");
        BooksError::from(e)
    })?;

    let users: Vec<_> = rows.iter().map(|r| json!({
        "id":           r.id,
        "email":        r.email,
        "display_name": r.display_name,
        "role":         r.role,
        "library_ids":  r.library_ids,
        "age_max":      r.age_max,
    })).collect();

    Ok(Json(json!({ "users": users })))
}

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

    // An age ceiling outside a plausible range is a typo, and a typo that lands
    // in this column silently hides or reveals a whole library. Refused rather
    // than clamped: the administrator must see which value was rejected.
    if let Some(age) = body.age_max {
        if !(0..=21).contains(&age) {
            return Err(BooksError::Validation(
                "L'âge maximal doit être compris entre 0 et 21 ans".into(),
            ));
        }
    }
    // An EMPTY list means "no library at all", which is a legitimate answer and
    // must stay distinct from NULL ("every library"). Only a list naming
    // libraries that do not exist is rejected — it would silently restrict more
    // than the administrator asked.
    if let Some(ids) = &body.library_ids {
        let known: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM books.libraries WHERE id = ANY($1)",
        )
        .bind(ids)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Vérification des bibliothèques d'une restriction échouée");
            BooksError::from(e)
        })?;
        if known != ids.len() as i64 {
            return Err(BooksError::Validation(
                "Une des bibliothèques indiquées n'existe pas".into(),
            ));
        }
    }

    sqlx::query(
        "INSERT INTO books.user_restrictions (user_id, library_ids, age_max) VALUES ($1, $2, $3) \
         ON CONFLICT (user_id) DO UPDATE SET library_ids = $2, age_max = $3, updated_at = now()",
    )
    .bind(uid)
    .bind(&body.library_ids)
    .bind(body.age_max)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, compte = %uid, "Écriture d'une restriction books échouée");
        BooksError::from(e)
    })?;
    Ok(Json(json!({ "ok": true })))
}
