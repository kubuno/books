use axum::{
    extract::{Extension, Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{errors::BooksError, middleware::auth::AuthUser, state::AppState};

#[derive(Debug, Deserialize)]
pub struct ProgressDto {
    pub page:      Option<i32>,
    pub location:  Option<String>,
    pub completed: Option<bool>,
}

async fn book_visible(state: &AppState, user_id: Uuid, book_id: Uuid) -> Result<(), BooksError> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT b.id FROM books.books b JOIN books.libraries l ON l.id = b.library_id \
         WHERE b.id = $1 AND (l.is_shared OR l.owner_id = $2)",
    )
    .bind(book_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .map(|_| ())
    .ok_or_else(|| BooksError::NotFound(format!("Livre {book_id}")))
}

pub async fn get_progress(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let row = sqlx::query_as::<_, (i32, Option<String>, bool)>(
        "SELECT page, location, completed FROM books.read_progress WHERE user_id = $1 AND book_id = $2",
    )
    .bind(user.id)
    .bind(book_id)
    .fetch_optional(&state.db)
    .await?;
    Ok(match row {
        Some((page, location, completed)) => Json(json!({ "page": page, "location": location, "completed": completed })),
        None => Json(json!({ "page": 0, "location": null, "completed": false })),
    })
}

pub async fn put_progress(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(book_id): Path<Uuid>,
    Json(dto): Json<ProgressDto>,
) -> Result<Json<Value>, BooksError> {
    book_visible(&state, user.id, book_id).await?;
    sqlx::query(
        "INSERT INTO books.read_progress (user_id, book_id, page, location, completed) \
         VALUES ($1, $2, COALESCE($3, 0), $4, COALESCE($5, false)) \
         ON CONFLICT (user_id, book_id) DO UPDATE SET \
           page      = COALESCE($3, books.read_progress.page), \
           location  = COALESCE($4, books.read_progress.location), \
           completed = COALESCE($5, books.read_progress.completed), \
           updated_at = now()",
    )
    .bind(user.id)
    .bind(book_id)
    .bind(dto.page)
    .bind(dto.location)
    .bind(dto.completed)
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn mark_read(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    book_visible(&state, user.id, book_id).await?;
    let last = sqlx::query_scalar::<_, Option<i32>>("SELECT page_count FROM books.books WHERE id = $1")
        .bind(book_id)
        .fetch_one(&state.db)
        .await?
        .map(|c| (c - 1).max(0))
        .unwrap_or(0);
    sqlx::query(
        "INSERT INTO books.read_progress (user_id, book_id, page, completed) VALUES ($1, $2, $3, true) \
         ON CONFLICT (user_id, book_id) DO UPDATE SET page = $3, completed = true, updated_at = now()",
    )
    .bind(user.id)
    .bind(book_id)
    .bind(last)
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn mark_unread(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    sqlx::query("DELETE FROM books.read_progress WHERE user_id = $1 AND book_id = $2")
        .bind(user.id)
        .bind(book_id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
pub struct KeepQuery {
    pub limit: Option<i64>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
struct KeepItem {
    id:                Uuid,
    library_id:        Uuid,
    series_id:         Option<Uuid>,
    title:             String,
    series_index:      Option<f64>,
    page_count:        Option<i32>,
    cover_format_id:   Option<Uuid>,
    formats:           Vec<String>,
    progress_page:     i32,
    progress_updated:  DateTime<Utc>,
}

/// Books in progress (not finished) — "Keep reading".
pub async fn keep_reading(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<KeepQuery>,
) -> Result<Json<Value>, BooksError> {
    let limit = q.limit.unwrap_or(24).clamp(1, 100);
    let rows = sqlx::query_as::<_, KeepItem>(
        "SELECT b.id, b.library_id, b.series_id, b.title, b.series_index, b.page_count, b.cover_format_id, \
                COALESCE(ARRAY(SELECT f.format FROM books.book_formats f WHERE f.book_id = b.id ORDER BY f.format), '{}') AS formats, \
                rp.page AS progress_page, rp.updated_at AS progress_updated \
         FROM books.read_progress rp \
         JOIN books.books b ON b.id = rp.book_id \
         JOIN books.libraries l ON l.id = b.library_id \
         WHERE rp.user_id = $1 AND rp.completed = false AND (rp.page > 0 OR rp.location IS NOT NULL) \
           AND (l.is_shared OR l.owner_id = $1) \
         ORDER BY rp.updated_at DESC LIMIT $2",
    )
    .bind(user.id)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "books": rows })))
}
