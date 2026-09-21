use axum::{extract::{Extension, Path, State}, Json};
use kubuno_db::dialect::Assign;
use kubuno_db::{params, DbQueryBuilder, JsonVec};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{errors::BooksError, middleware::auth::AuthUser, services::settings_key_col, state::AppState};

fn require_admin(user: &AuthUser) -> Result<(), BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    Ok(())
}

// ── GET /books/admin/settings ─────────────────────────────────────────────────

pub async fn get_settings(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    require_admin(&user)?;

    let k = settings_key_col(state.db.backend());
    let rows = state
        .db
        .fetch_all_as::<(String, String)>(
            &format!("SELECT {k}, value FROM books.settings ORDER BY {k}"),
            params![],
        )
        .await?;

    let settings: serde_json::Map<String, Value> = rows
        .into_iter()
        .map(|(key, value)| (key, Value::String(value)))
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
        let k = settings_key_col(state.db.backend());
        let upsert = state.db.backend().upsert(
            "books.settings",
            &["key"],
            &[Assign::Incoming("value"), Assign::Incoming("updated_at")],
        );
        state
            .db
            .execute(
                &format!(
                    "INSERT INTO books.settings ({k}, value, updated_at) VALUES ($1, $2, $3){upsert}"
                ),
                params!["metadata_language", lang, chrono::Utc::now()],
            )
            .await?;
    }

    Ok(Json(json!({ "ok": true })))
}

// ── Per-user access / age restrictions ───────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct RestrictedUserRow {
    id:           Uuid,
    email:        String,
    display_name: Option<String>,
    role:         String,
    library_ids:  Option<JsonVec<Uuid>>,
    age_max:      Option<i32>,
}

/// GET /books/admin/restrictions — every account, with its restriction.
///
/// NOTE: cross-schema join to core.users (PostgreSQL/MySQL only). A module owns
/// its own schema and this route only ever reads accounts.
pub async fn list_restrictions(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    require_admin(&user)?;

    let rows = state
        .db
        .fetch_all_as::<RestrictedUserRow>(
            "SELECT u.id, u.email, u.display_name, u.role, r.library_ids, r.age_max \
               FROM core.users u \
               LEFT JOIN books.user_restrictions r ON r.user_id = u.id \
              WHERE u.is_active = TRUE \
              ORDER BY u.email",
            params![],
        )
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
        "library_ids":  r.library_ids.as_ref().map(|j| &j.0),
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
    let row = state
        .db
        .fetch_optional_as::<(Option<JsonVec<Uuid>>, Option<i32>)>(
            "SELECT library_ids, age_max FROM books.user_restrictions WHERE user_id = $1",
            params![uid],
        )
        .await?;
    let (library_ids, age_max) = row.unwrap_or((None, None));
    let library_ids = library_ids.map(JsonVec::into_inner);
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

    // An age ceiling outside a plausible range is a typo that would silently
    // hide or reveal a whole library. Refused rather than clamped.
    if let Some(age) = body.age_max {
        if !(0..=21).contains(&age) {
            return Err(BooksError::Validation(
                "L'âge maximal doit être compris entre 0 et 21 ans".into(),
            ));
        }
    }
    // An EMPTY list means "no library at all" (distinct from NULL = "every
    // library"). Only a list naming libraries that do not exist is rejected.
    if let Some(ids) = &body.library_ids {
        let mut qb = DbQueryBuilder::new(
            state.db.backend(),
            format!(
                "SELECT {} FROM books.libraries WHERE id",
                state.db.backend().count_bigint("*")
            ),
        );
        qb.push_in(ids.iter().copied());
        let known = qb.fetch_scalar::<i64>(&state.db).await.map_err(|e| {
            tracing::error!(error = %e, "Vérification des bibliothèques d'une restriction échouée");
            BooksError::from(e)
        })?;
        if known != ids.len() as i64 {
            return Err(BooksError::Validation(
                "Une des bibliothèques indiquées n'existe pas".into(),
            ));
        }
    }

    let upsert = state.db.backend().upsert(
        "books.user_restrictions",
        &["user_id"],
        &[Assign::Incoming("library_ids"), Assign::Incoming("age_max"), Assign::Incoming("updated_at")],
    );
    state
        .db
        .execute(
            &format!(
                "INSERT INTO books.user_restrictions (user_id, library_ids, age_max, updated_at) \
                 VALUES ($1, $2, $3, $4){upsert}"
            ),
            params![uid, body.library_ids, body.age_max, chrono::Utc::now()],
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, compte = %uid, "Écriture d'une restriction books échouée");
            BooksError::from(e)
        })?;
    Ok(Json(json!({ "ok": true })))
}
