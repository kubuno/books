use axum::{
    extract::{Extension, Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use kubuno_db::{dialect::Assign, params, DbQueryBuilder};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::BooksError,
    middleware::auth::AuthUser,
    services::{access::Restriction, formats},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ProgressDto {
    pub page:      Option<i32>,
    pub location:  Option<String>,
    pub completed: Option<bool>,
}

async fn book_visible(state: &AppState, user_id: Uuid, book_id: Uuid) -> Result<(), BooksError> {
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user_id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT b.id FROM books.books b JOIN books.libraries l ON l.id = b.library_id WHERE b.id = ",
    );
    qb.push_bind(book_id).push(" AND ");
    restriction.push_readable_book(&mut qb, user_id, block);
    qb.fetch_optional_scalar::<Uuid>(&state.db)
        .await?
        .map(|_| ())
        .ok_or_else(|| BooksError::NotFound(format!("Livre {book_id}")))
}

pub async fn get_progress(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let row = state
        .db
        .fetch_optional_as::<(i32, Option<String>, bool)>(
            "SELECT page, location, completed FROM books.read_progress WHERE user_id = $1 AND book_id = $2",
            params![user.id, book_id],
        )
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

    // Merge with the existing progress in Rust: a field left out keeps its value
    // (the portable form of the old `COALESCE($n, read_progress.col)` on the
    // conflict branch — an upsert cannot re-read the stored row per column).
    let existing = state
        .db
        .fetch_optional_as::<(i32, Option<String>, bool)>(
            "SELECT page, location, completed FROM books.read_progress WHERE user_id = $1 AND book_id = $2",
            params![user.id, book_id],
        )
        .await?;
    let (cur_page, cur_loc, cur_done) = existing.unwrap_or((0, None, false));
    let page = dto.page.unwrap_or(cur_page);
    let location = dto.location.or(cur_loc);
    let completed = dto.completed.unwrap_or(cur_done);

    let upsert = state.db.backend().upsert(
        "books.read_progress",
        &["user_id", "book_id"],
        &[Assign::Incoming("page"), Assign::Incoming("location"), Assign::Incoming("completed"), Assign::Incoming("updated_at")],
    );
    state
        .db
        .execute(
            &format!(
                "INSERT INTO books.read_progress (user_id, book_id, page, location, completed, updated_at) \
                 VALUES ($1,$2,$3,$4,$5,$6){upsert}"
            ),
            params![user.id, book_id, page, location, completed, Utc::now()],
        )
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn mark_read(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    book_visible(&state, user.id, book_id).await?;
    let last = state
        .db
        .fetch_one_as::<(Option<i32>,)>(
            "SELECT page_count FROM books.books WHERE id = $1",
            params![book_id],
        )
        .await?
        .0
        .map(|c| (c - 1).max(0))
        .unwrap_or(0);
    let upsert = state.db.backend().upsert(
        "books.read_progress",
        &["user_id", "book_id"],
        &[Assign::Incoming("page"), Assign::Incoming("completed"), Assign::Incoming("updated_at")],
    );
    state
        .db
        .execute(
            &format!(
                "INSERT INTO books.read_progress (user_id, book_id, page, completed, updated_at) \
                 VALUES ($1,$2,$3,$4,$5){upsert}"
            ),
            params![user.id, book_id, last, true, Utc::now()],
        )
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn mark_unread(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(book_id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    // The other progress routes check first; this one did not, which let a
    // reader poke at rows for books they were never allowed to see.
    book_visible(&state, user.id, book_id).await?;
    state
        .db
        .execute(
            "DELETE FROM books.read_progress WHERE user_id = $1 AND book_id = $2",
            params![user.id, book_id],
        )
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
    #[sqlx(skip)]
    formats:           Vec<String>,
    progress_page:     i32,
    progress_updated:  DateTime<Utc>,
}

impl formats::WithFormats for KeepItem {
    fn book_id(&self) -> Uuid { self.id }
    fn set_formats(&mut self, formats: Vec<String>) { self.formats = formats; }
}

/// Books in progress (not finished) — "Keep reading".
pub async fn keep_reading(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<KeepQuery>,
) -> Result<Json<Value>, BooksError> {
    let limit = q.limit.unwrap_or(24).clamp(1, 100);
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT b.id, b.library_id, b.series_id, b.title, b.series_index, b.page_count, b.cover_format_id, \
                rp.page AS progress_page, rp.updated_at AS progress_updated \
         FROM books.read_progress rp \
         JOIN books.books b ON b.id = rp.book_id \
         JOIN books.libraries l ON l.id = b.library_id \
         WHERE rp.user_id = ",
    );
    qb.push_bind(user.id);
    qb.push(" AND rp.completed = FALSE AND (rp.page > 0 OR rp.location IS NOT NULL) AND ");
    restriction.push_readable_book(&mut qb, user.id, block);
    qb.push_order_by("rp.updated_at DESC");
    qb.push_limit_offset(limit, 0);
    let mut rows = qb.fetch_all_as::<KeepItem>(&state.db).await?;
    formats::attach(&state.db, &mut rows).await?;
    Ok(Json(json!({ "books": rows })))
}
