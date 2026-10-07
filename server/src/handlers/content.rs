use axum::{
    extract::{Extension, Path, Query, State},
    Json,
};
use chrono::{NaiveDate, Utc};
use kubuno_db::{params, DbQueryBuilder};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::BooksError,
    middleware::auth::AuthUser,
    models::content::{Book, BookFormat, BookListItem, Series},
    services::{access::Restriction, formats, jsonq},
    state::AppState,
};

/// Columns of a book-list row (its `formats` are attached in Rust afterwards).
const BOOK_LIST_COLS: &str = "b.id, b.library_id, b.series_id, b.title, b.sort_title, \
    b.series_index, b.page_count, b.cover_format_id, b.added_at";

#[derive(Debug, Deserialize)]
pub struct SeriesQuery {
    pub library_id: Option<Uuid>,
}

pub async fn list_series(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<SeriesQuery>,
) -> Result<Json<Value>, BooksError> {
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT s.* FROM books.series s JOIN books.libraries l ON l.id = s.library_id WHERE ",
    );
    restriction.push_readable_series(&mut qb, user.id, block);
    if let Some(lib) = q.library_id {
        qb.push(" AND s.library_id = ").push_bind(lib);
    }
    // Portable "NULLS LAST": order the NULL flag first.
    qb.push_order_by("(s.sort_name IS NULL), s.sort_name, s.name");
    let rows = qb.fetch_all_as::<Series>(&state.db).await?;
    Ok(Json(json!({ "series": rows })))
}

pub async fn get_series(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT s.* FROM books.series s JOIN books.libraries l ON l.id = s.library_id WHERE ",
    );
    restriction.push_readable_series(&mut qb, user.id, block);
    qb.push(" AND s.id = ").push_bind(id);
    let row = qb
        .fetch_optional_as::<Series>(&state.db)
        .await?
        .ok_or_else(|| BooksError::NotFound(format!("Série {id}")))?;
    let lib = state
        .db
        .fetch_one_as::<(Uuid, String)>(
            "SELECT id, name FROM books.libraries WHERE id = $1",
            params![row.library_id],
        )
        .await?;
    Ok(Json(json!({ "series": row, "library": { "id": lib.0, "name": lib.1 } })))
}

pub async fn series_books(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        format!("SELECT {BOOK_LIST_COLS} FROM books.books b JOIN books.libraries l ON l.id = b.library_id WHERE "),
    );
    restriction.push_readable_book(&mut qb, user.id, block);
    qb.push(" AND b.series_id = ").push_bind(id);
    qb.push_order_by("(b.series_index IS NULL), b.series_index, (b.sort_title IS NULL), b.sort_title, b.title");
    let mut rows = qb.fetch_all_as::<BookListItem>(&state.db).await?;
    formats::attach(&state.db, &mut rows).await?;
    Ok(Json(json!({ "books": rows })))
}

#[derive(Debug, Deserialize)]
pub struct BooksQuery {
    pub library_id: Option<Uuid>,
    pub series_id:  Option<Uuid>,
    pub search:     Option<String>,
    pub tag:        Option<String>,
    pub author:     Option<String>,
    pub publisher:  Option<String>,
    pub language:   Option<String>,
    pub format:     Option<String>,
    pub sort:       Option<String>,
    pub limit:      Option<i64>,
    pub offset:     Option<i64>,
}

pub async fn list_books(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<BooksQuery>,
) -> Result<Json<Value>, BooksError> {
    let limit = q.limit.unwrap_or(60).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    // Portable "NULLS LAST" orderings; one of four literals.
    let sort = match q.sort.as_deref() {
        Some("title") => "(b.sort_title IS NULL), b.sort_title, b.title",
        Some("series") => "(b.series_index IS NULL), b.series_index, (b.sort_title IS NULL), b.sort_title, b.title",
        Some("updated") => "b.updated_at DESC",
        _ => "b.added_at DESC",
    };
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;

    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        format!("SELECT {BOOK_LIST_COLS} FROM books.books b JOIN books.libraries l ON l.id = b.library_id WHERE "),
    );
    restriction.push_readable_book(&mut qb, user.id, block);
    if let Some(lib) = q.library_id {
        qb.push(" AND b.library_id = ").push_bind(lib);
    }
    if let Some(sid) = q.series_id {
        qb.push(" AND b.series_id = ").push_bind(sid);
    }
    if let Some(search) = q.search.filter(|s| !s.is_empty()) {
        push_title_ilike(&mut qb, &search);
    }
    if let Some(tag) = q.tag.filter(|s| !s.is_empty()) {
        qb.push(" AND ");
        jsonq::push_array_contains(&mut qb, "b.tags", tag);
    }
    if let Some(author) = q.author.filter(|s| !s.is_empty()) {
        qb.push(" AND ");
        jsonq::push_author_name(&mut qb, author);
    }
    if let Some(publisher) = q.publisher.filter(|s| !s.is_empty()) {
        qb.push(" AND b.publisher = ").push_bind(publisher);
    }
    if let Some(language) = q.language.filter(|s| !s.is_empty()) {
        qb.push(" AND b.language = ").push_bind(language);
    }
    if let Some(format) = q.format.filter(|s| !s.is_empty()) {
        qb.push(" AND EXISTS (SELECT 1 FROM books.book_formats f WHERE f.book_id = b.id AND f.format = ")
            .push_bind(format)
            .push(")");
    }
    qb.push_order_by(sort);
    qb.push_limit_offset(limit, offset);

    let mut rows = qb.fetch_all_as::<BookListItem>(&state.db).await?;
    formats::attach(&state.db, &mut rows).await?;
    Ok(Json(json!({ "books": rows })))
}

/// Appends ` AND <title ILIKE '%pattern%'>`, binding the pattern.
fn push_title_ilike(qb: &mut DbQueryBuilder, needle: &str) {
    use kubuno_db::dialect::Backend;
    let pattern = format!("%{needle}%");
    match qb.backend() {
        Backend::Postgres => {
            qb.push(" AND b.title ILIKE ").push_bind(pattern);
        }
        Backend::MySql | Backend::Sqlite => {
            qb.push(" AND LOWER(b.title) LIKE LOWER(").push_bind(pattern).push(")");
        }
    }
}

pub async fn recent_books(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<BooksQuery>,
) -> Result<Json<Value>, BooksError> {
    let limit = q.limit.unwrap_or(24).clamp(1, 100);
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        format!("SELECT {BOOK_LIST_COLS} FROM books.books b JOIN books.libraries l ON l.id = b.library_id WHERE "),
    );
    restriction.push_readable_book(&mut qb, user.id, block);
    qb.push_order_by("b.added_at DESC");
    qb.push_limit_offset(limit, 0);
    let mut rows = qb.fetch_all_as::<BookListItem>(&state.db).await?;
    formats::attach(&state.db, &mut rows).await?;
    Ok(Json(json!({ "books": rows })))
}

pub async fn get_book(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT b.* FROM books.books b JOIN books.libraries l ON l.id = b.library_id WHERE ",
    );
    restriction.push_readable_book(&mut qb, user.id, block);
    qb.push(" AND b.id = ").push_bind(id);
    let book = qb
        .fetch_optional_as::<Book>(&state.db)
        .await?
        .ok_or_else(|| BooksError::NotFound(format!("Livre {id}")))?;

    let formats = state
        .db
        .fetch_all_as::<BookFormat>(
            "SELECT * FROM books.book_formats WHERE book_id = $1 ORDER BY format",
            params![id],
        )
        .await?;

    // Breadcrumb context: library + (optional) series names.
    let lib = state
        .db
        .fetch_one_as::<(Uuid, String)>(
            "SELECT id, name FROM books.libraries WHERE id = $1",
            params![book.library_id],
        )
        .await?;
    let series = match book.series_id {
        Some(sid) => state
            .db
            .fetch_optional_as::<(Uuid, String)>(
                "SELECT id, name FROM books.series WHERE id = $1",
                params![sid],
            )
            .await?
            .map(|(sid, name)| json!({ "id": sid, "name": name })),
        None => None,
    };

    Ok(Json(json!({
        "book": book,
        "formats": formats,
        "library": { "id": lib.0, "name": lib.1 },
        "series": series,
    })))
}

// ── Metadata editing ─────────────────────────────────────────────
#[derive(Debug, Deserialize)]
pub struct UpdateBookDto {
    pub title:             Option<String>,
    pub sort_title:        Option<String>,
    pub series_index:      Option<f64>,
    pub description:       Option<String>,
    pub publisher:         Option<String>,
    pub published_date:    Option<NaiveDate>,
    pub isbn:              Option<String>,
    pub language:          Option<String>,
    pub rating:            Option<f64>,
    pub age_rating:        Option<i32>,
    pub reading_direction: Option<String>,
    pub authors:           Option<Value>,
    pub tags:              Option<Value>,
    pub identifiers:       Option<Value>,
}

/// Pushes the shared `SET` body of the book update (fill-if-provided via
/// COALESCE), binding each value. Does not include `updated_at` or the `WHERE`.
fn push_book_update_set(qb: &mut DbQueryBuilder, dto: &UpdateBookDto) {
    qb.push("title = COALESCE(").push_bind(dto.title.clone()).push(", title)");
    qb.push(", sort_title = COALESCE(").push_bind(dto.sort_title.clone()).push(", sort_title)");
    qb.push(", series_index = COALESCE(").push_bind(dto.series_index).push(", series_index)");
    qb.push(", description = COALESCE(").push_bind(dto.description.clone()).push(", description)");
    qb.push(", publisher = COALESCE(").push_bind(dto.publisher.clone()).push(", publisher)");
    qb.push(", published_date = COALESCE(").push_bind(dto.published_date).push(", published_date)");
    qb.push(", isbn = COALESCE(").push_bind(dto.isbn.clone()).push(", isbn)");
    qb.push(", language = COALESCE(").push_bind(dto.language.clone()).push(", language)");
    qb.push(", rating = COALESCE(").push_bind(dto.rating).push(", rating)");
    qb.push(", age_rating = COALESCE(").push_bind(dto.age_rating).push(", age_rating)");
    qb.push(", reading_direction = COALESCE(").push_bind(dto.reading_direction.clone()).push(", reading_direction)");
    qb.push(", authors = COALESCE(").push_bind(dto.authors.clone()).push(", authors)");
    qb.push(", tags = COALESCE(").push_bind(dto.tags.clone()).push(", tags)");
    qb.push(", identifiers = COALESCE(").push_bind(dto.identifiers.clone()).push(", identifiers)");
}

pub async fn update_book(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateBookDto>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    // Existence is decided by a reselect (portable: MySQL has no RETURNING).
    let exists = state
        .db
        .fetch_optional_scalar::<Uuid>("SELECT id FROM books.books WHERE id = $1", params![id])
        .await?;
    if exists.is_none() {
        return Err(BooksError::NotFound(format!("Livre {id}")));
    }
    let mut qb = DbQueryBuilder::new(state.db.backend(), "UPDATE books.books SET ");
    push_book_update_set(&mut qb, &dto);
    qb.push(", updated_at = ").push_bind(Utc::now());
    qb.push(" WHERE id = ").push_bind(id);
    qb.execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
pub struct BulkUpdateDto {
    pub ids:    Vec<Uuid>,
    #[serde(flatten)]
    pub fields: UpdateBookDto,
}

pub async fn bulk_update_books(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(dto): Json<BulkUpdateDto>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    if dto.ids.is_empty() {
        return Ok(Json(json!({ "updated": 0 })));
    }
    let mut qb = DbQueryBuilder::new(state.db.backend(), "UPDATE books.books SET ");
    push_book_update_set(&mut qb, &dto.fields);
    qb.push(", updated_at = ").push_bind(Utc::now());
    qb.push(" WHERE id").push_in(dto.ids.iter().copied());
    let n = qb.execute(&state.db).await?;
    Ok(Json(json!({ "updated": n })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateSeriesDto {
    pub name:              Option<String>,
    pub sort_name:         Option<String>,
    pub description:       Option<String>,
    pub publisher:         Option<String>,
    pub language:          Option<String>,
    pub age_rating:        Option<i32>,
    pub reading_direction: Option<String>,
    pub total_book_count:  Option<i32>,
    pub genres:            Option<Value>,
    pub tags:              Option<Value>,
}

pub async fn update_series(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateSeriesDto>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    let exists = state
        .db
        .fetch_optional_scalar::<Uuid>("SELECT id FROM books.series WHERE id = $1", params![id])
        .await?;
    if exists.is_none() {
        return Err(BooksError::NotFound(format!("Série {id}")));
    }
    let mut qb = DbQueryBuilder::new(state.db.backend(), "UPDATE books.series SET ");
    qb.push("name = COALESCE(").push_bind(dto.name.clone()).push(", name)");
    qb.push(", sort_name = COALESCE(").push_bind(dto.sort_name.clone()).push(", sort_name)");
    qb.push(", description = COALESCE(").push_bind(dto.description.clone()).push(", description)");
    qb.push(", publisher = COALESCE(").push_bind(dto.publisher.clone()).push(", publisher)");
    qb.push(", language = COALESCE(").push_bind(dto.language.clone()).push(", language)");
    qb.push(", age_rating = COALESCE(").push_bind(dto.age_rating).push(", age_rating)");
    qb.push(", reading_direction = COALESCE(").push_bind(dto.reading_direction.clone()).push(", reading_direction)");
    qb.push(", total_book_count = COALESCE(").push_bind(dto.total_book_count).push(", total_book_count)");
    qb.push(", genres = COALESCE(").push_bind(dto.genres.clone()).push(", genres)");
    qb.push(", tags = COALESCE(").push_bind(dto.tags.clone()).push(", tags)");
    qb.push(", updated_at = ").push_bind(Utc::now());
    qb.push(" WHERE id = ").push_bind(id);
    qb.execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Re-import embedded metadata (ComicInfo.xml / EPUB OPF) for a single book.
pub async fn refresh_book_metadata(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    let applied = crate::services::metadata::apply_embedded(&state, id).await?;
    Ok(Json(json!({ "applied": applied })))
}

/// Re-import embedded metadata for a whole library (background).
pub async fn refresh_library_metadata(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    let st = state.clone();
    tokio::spawn(async move { crate::services::metadata::import_library(&st, id).await });
    Ok(Json(json!({ "started": true })))
}

// ── Online metadata (OpenLibrary / Google Books) ─────────────────────────────────
#[derive(Debug, Deserialize)]
pub struct OnlineSearchQuery {
    pub q: String,
}

pub async fn search_online_metadata(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    Query(q): Query<OnlineSearchQuery>,
) -> Result<Json<Value>, BooksError> {
    let results = crate::services::providers::search(&state, &q.q).await;
    Ok(Json(json!({ "results": results })))
}

// ── Series online presentation metadata (Wikipedia / Google Books) ────────────────

/// Candidate series presentations for `?q=` (Wikipedia + Google Books).
pub async fn search_online_series(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    Query(q): Query<OnlineSearchQuery>,
) -> Result<Json<Value>, BooksError> {
    let results = crate::services::providers::search_series(&state, &q.q, None).await;
    Ok(Json(json!({ "results": results })))
}

#[derive(Debug, Deserialize)]
pub struct ApplySeriesDto {
    pub description:    Option<String>,
    pub publisher:      Option<String>,
    pub genres:         Option<Vec<String>>,
    pub cover_url:      Option<String>,
    pub download_cover: Option<bool>,
}

/// Apply a chosen online presentation onto a series (overwrites the given fields).
pub async fn apply_online_series_metadata(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<ApplySeriesDto>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    let genres = dto.genres.as_ref().map(|g| json!(g));
    let mut qb = DbQueryBuilder::new(state.db.backend(), "UPDATE books.series SET ");
    qb.push("description = COALESCE(").push_bind(dto.description.clone()).push(", description)");
    qb.push(", publisher = COALESCE(").push_bind(dto.publisher.clone()).push(", publisher)");
    qb.push(", genres = COALESCE(").push_bind(genres).push(", genres)");
    qb.push(", updated_at = ").push_bind(Utc::now());
    qb.push(" WHERE id = ").push_bind(id);
    let n = qb.execute(&state.db).await?;
    if n == 0 {
        return Err(BooksError::NotFound(format!("Série {id}")));
    }
    let mut cover = false;
    if dto.download_cover.unwrap_or(false) {
        if let Some(url) = &dto.cover_url {
            cover = crate::services::providers::download_series_cover(&state, id, url).await;
        }
    }
    Ok(Json(json!({ "ok": true, "cover_downloaded": cover })))
}

/// Auto-enrich a single series from the web (fills empty fields). Admin only.
pub async fn refresh_series_metadata(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    let applied = crate::services::providers::enrich_series(&state, id).await;
    Ok(Json(json!({ "applied": applied })))
}

/// Auto-enrich every series of a library from the web (background). Admin only.
pub async fn refresh_library_series_metadata(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    let st = state.clone();
    tokio::spawn(async move { crate::services::providers::enrich_library_series(&st, id).await });
    Ok(Json(json!({ "started": true })))
}

fn parse_date_loose(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if let Ok(d) = NaiveDate::parse_from_str(s.get(..10).unwrap_or(s), "%Y-%m-%d") {
        return Some(d);
    }
    s.get(..4).and_then(|y| y.parse::<i32>().ok()).and_then(|y| NaiveDate::from_ymd_opt(y, 1, 1))
}

#[derive(Debug, Deserialize)]
pub struct ApplyOnlineDto {
    pub title:          Option<String>,
    pub authors:        Option<Vec<String>>,
    pub publisher:      Option<String>,
    pub published_date: Option<String>,
    pub isbn:           Option<String>,
    pub description:    Option<String>,
    pub language:       Option<String>,
    pub tags:           Option<Vec<String>>,
    pub cover_url:      Option<String>,
    pub download_cover: Option<bool>,
}

pub async fn apply_online_metadata(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<ApplyOnlineDto>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    let authors = dto
        .authors
        .as_ref()
        .map(|a| json!(a.iter().map(|n| json!({ "name": n, "role": "author" })).collect::<Vec<_>>()));
    let tags = dto.tags.as_ref().map(|t| json!(t));
    let date = dto.published_date.as_deref().and_then(parse_date_loose);

    let mut qb = DbQueryBuilder::new(state.db.backend(), "UPDATE books.books SET ");
    qb.push("title = COALESCE(").push_bind(dto.title.clone()).push(", title)");
    qb.push(", publisher = COALESCE(").push_bind(dto.publisher.clone()).push(", publisher)");
    qb.push(", published_date = COALESCE(").push_bind(date).push(", published_date)");
    qb.push(", isbn = COALESCE(").push_bind(dto.isbn.clone()).push(", isbn)");
    qb.push(", description = COALESCE(").push_bind(dto.description.clone()).push(", description)");
    qb.push(", language = COALESCE(").push_bind(dto.language.clone()).push(", language)");
    qb.push(", authors = COALESCE(").push_bind(authors).push(", authors)");
    qb.push(", tags = COALESCE(").push_bind(tags).push(", tags)");
    qb.push(", updated_at = ").push_bind(Utc::now());
    qb.push(" WHERE id = ").push_bind(id);
    let n = qb.execute(&state.db).await?;
    if n == 0 {
        return Err(BooksError::NotFound(format!("Livre {id}")));
    }

    let mut cover = false;
    if dto.download_cover.unwrap_or(false) {
        if let Some(url) = &dto.cover_url {
            cover = crate::services::providers::download_cover(&state, id, url).await;
        }
    }
    Ok(Json(json!({ "ok": true, "cover_downloaded": cover })))
}

// ── Catalogue export ─────────────────────────────────────────────
#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub fmt:        Option<String>,
    pub library_id: Option<Uuid>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
struct ExportRow {
    id:             Uuid,
    title:          String,
    series_name:    Option<String>,
    series_index:   Option<f64>,
    authors:        Value,
    publisher:      Option<String>,
    published_date: Option<NaiveDate>,
    isbn:           Option<String>,
    language:       Option<String>,
    tags:           Value,
    rating:         Option<f64>,
    page_count:     Option<i32>,
    #[sqlx(skip)]
    formats:        Vec<String>,
    added_at:       chrono::DateTime<chrono::Utc>,
}

impl formats::WithFormats for ExportRow {
    fn book_id(&self) -> Uuid { self.id }
    fn set_formats(&mut self, formats: Vec<String>) { self.formats = formats; }
}

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn names_of(v: &Value) -> String {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get("name").and_then(|n| n.as_str()).or_else(|| x.as_str()))
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default()
}

fn strings_of(v: &Value) -> String {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("; "))
        .unwrap_or_default()
}

pub async fn export_catalog(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<ExportQuery>,
) -> Result<axum::response::Response, BooksError> {
    use axum::{http::header, response::IntoResponse};
    // An export is a listing like any other: it goes through the same two rules.
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT b.id, b.title, s.name AS series_name, b.series_index, b.authors, b.publisher, \
                b.published_date, b.isbn, b.language, b.tags, b.rating, b.page_count, b.added_at \
         FROM books.books b JOIN books.libraries l ON l.id = b.library_id \
         LEFT JOIN books.series s ON s.id = b.series_id WHERE ",
    );
    restriction.push_readable_book(&mut qb, user.id, block);
    if let Some(lib) = q.library_id {
        qb.push(" AND b.library_id = ").push_bind(lib);
    }
    qb.push_order_by("(s.name IS NULL), s.name, (b.series_index IS NULL), b.series_index, b.title");
    let mut rows = qb.fetch_all_as::<ExportRow>(&state.db).await?;
    formats::attach(&state.db, &mut rows).await?;

    if q.fmt.as_deref() == Some("json") {
        let body = serde_json::to_string(&json!({ "books": rows })).unwrap_or_else(|_| "{}".into());
        return Ok((
            [(header::CONTENT_TYPE, "application/json; charset=utf-8"),
             (header::CONTENT_DISPOSITION, "attachment; filename=\"catalogue.json\"")],
            body,
        )
            .into_response());
    }

    let mut csv = String::from("Title,Series,Number,Authors,Publisher,Published,ISBN,Language,Tags,Rating,Pages,Formats,Added\n");
    for r in &rows {
        let cells = [
            r.title.clone(),
            r.series_name.clone().unwrap_or_default(),
            r.series_index.map(|n| n.to_string()).unwrap_or_default(),
            names_of(&r.authors),
            r.publisher.clone().unwrap_or_default(),
            r.published_date.map(|d| d.to_string()).unwrap_or_default(),
            r.isbn.clone().unwrap_or_default(),
            r.language.clone().unwrap_or_default(),
            strings_of(&r.tags),
            r.rating.map(|n| n.to_string()).unwrap_or_default(),
            r.page_count.map(|n| n.to_string()).unwrap_or_default(),
            r.formats.join("; "),
            r.added_at.format("%Y-%m-%d").to_string(),
        ];
        csv.push_str(&cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
        csv.push('\n');
    }
    Ok((
        [(header::CONTENT_TYPE, "text/csv; charset=utf-8"),
         (header::CONTENT_DISPOSITION, "attachment; filename=\"catalogue.csv\"")],
        csv,
    )
        .into_response())
}

/// Duplicate detection: books whose format files share a content hash.
pub async fn duplicates(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;
    // Rows of (content_hash, book_id, title), grouped in Rust — jsonb_agg has no
    // portable spelling. Only readable books participate.
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT bf.content_hash, b.id, b.title \
         FROM books.book_formats bf \
         JOIN books.books b ON b.id = bf.book_id \
         JOIN books.libraries l ON l.id = b.library_id \
         WHERE bf.content_hash IS NOT NULL AND ",
    );
    restriction.push_readable_book(&mut qb, user.id, block);
    let rows = qb
        .fetch_all_as::<(String, Uuid, String)>(&state.db)
        .await?;

    // Group by hash, keep distinct books, emit only hashes shared by >1 book.
    use std::collections::BTreeMap;
    let mut by_hash: BTreeMap<String, Vec<(Uuid, String)>> = BTreeMap::new();
    for (hash, id, title) in rows {
        let entry = by_hash.entry(hash).or_default();
        if !entry.iter().any(|(i, _)| *i == id) {
            entry.push((id, title));
        }
    }
    let groups: Vec<_> = by_hash
        .into_iter()
        .filter(|(_, books)| books.len() > 1)
        .map(|(hash, books)| {
            let books: Vec<_> = books.into_iter().map(|(id, title)| json!({ "id": id, "title": title })).collect();
            json!({ "hash": hash, "books": books })
        })
        .collect();
    Ok(Json(json!({ "duplicates": groups })))
}
