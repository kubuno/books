use axum::{
    extract::{Extension, Path, Query, State},
    Json,
};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::BooksError,
    middleware::auth::AuthUser,
    models::content::{Book, BookFormat, BookListItem, Series},
    state::AppState,
};

// Access rule reused everywhere: a library is visible when shared or owned by the user, and
// allowed by the user's per-account library restriction (P7).
const VISIBLE: &str = "(l.is_shared OR l.owner_id = $1) AND books.lib_allowed($1, l.id)";

#[derive(Debug, Deserialize)]
pub struct SeriesQuery {
    pub library_id: Option<Uuid>,
}

pub async fn list_series(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<SeriesQuery>,
) -> Result<Json<Value>, BooksError> {
    let rows = sqlx::query_as::<_, Series>(&format!(
        "SELECT s.* FROM books.series s JOIN books.libraries l ON l.id = s.library_id \
         WHERE {VISIBLE} AND ($2::uuid IS NULL OR s.library_id = $2) \
         ORDER BY s.sort_name NULLS LAST, s.name"
    ))
    .bind(user.id)
    .bind(q.library_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "series": rows })))
}

pub async fn get_series(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let row = sqlx::query_as::<_, Series>(&format!(
        "SELECT s.* FROM books.series s JOIN books.libraries l ON l.id = s.library_id \
         WHERE {VISIBLE} AND s.id = $2"
    ))
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| BooksError::NotFound(format!("Série {id}")))?;
    let lib = sqlx::query_as::<_, (Uuid, String)>("SELECT id, name FROM books.libraries WHERE id = $1")
        .bind(row.library_id)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(json!({ "series": row, "library": { "id": lib.0, "name": lib.1 } })))
}

pub async fn series_books(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let rows = sqlx::query_as::<_, BookListItem>(&format!(
        "SELECT b.id, b.library_id, b.series_id, b.title, b.sort_title, b.series_index, \
                b.page_count, b.cover_format_id, b.added_at, \
                COALESCE(ARRAY(SELECT f.format FROM books.book_formats f WHERE f.book_id = b.id ORDER BY f.format), '{{}}') AS formats \
         FROM books.books b JOIN books.libraries l ON l.id = b.library_id \
         WHERE {VISIBLE} AND books.age_ok($1, b.age_rating) AND b.series_id = $2 \
         ORDER BY b.series_index NULLS LAST, b.sort_title NULLS LAST, b.title"
    ))
    .bind(user.id)
    .bind(id)
    .fetch_all(&state.db)
    .await?;
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

const BOOK_LIST_COLS: &str = "b.id, b.library_id, b.series_id, b.title, b.sort_title, b.series_index, \
    b.page_count, b.cover_format_id, b.added_at, \
    COALESCE(ARRAY(SELECT f.format FROM books.book_formats f WHERE f.book_id = b.id ORDER BY f.format), '{}') AS formats";

pub async fn list_books(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<BooksQuery>,
) -> Result<Json<Value>, BooksError> {
    let limit = q.limit.unwrap_or(60).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    let sort = match q.sort.as_deref() {
        Some("title") => "b.sort_title NULLS LAST, b.title",
        Some("series") => "b.series_index NULLS LAST, b.sort_title NULLS LAST, b.title",
        Some("updated") => "b.updated_at DESC",
        _ => "b.added_at DESC",
    };
    let rows = sqlx::query_as::<_, BookListItem>(&format!(
        "SELECT {BOOK_LIST_COLS} \
         FROM books.books b JOIN books.libraries l ON l.id = b.library_id \
         WHERE {VISIBLE} \
           AND books.age_ok($1, b.age_rating) \
           AND ($2::uuid IS NULL OR b.library_id = $2) \
           AND ($3::uuid IS NULL OR b.series_id = $3) \
           AND ($4::text IS NULL OR b.title ILIKE '%' || $4 || '%') \
           AND ($5::text IS NULL OR b.tags @> jsonb_build_array($5::text)) \
           AND ($6::text IS NULL OR b.authors @> jsonb_build_array(jsonb_build_object('name', $6::text))) \
           AND ($7::text IS NULL OR b.publisher = $7) \
           AND ($8::text IS NULL OR b.language = $8) \
           AND ($9::text IS NULL OR EXISTS (SELECT 1 FROM books.book_formats f WHERE f.book_id = b.id AND f.format = $9)) \
         ORDER BY {sort} \
         LIMIT $10 OFFSET $11"
    ))
    .bind(user.id)
    .bind(q.library_id)
    .bind(q.series_id)
    .bind(q.search)
    .bind(q.tag)
    .bind(q.author)
    .bind(q.publisher)
    .bind(q.language)
    .bind(q.format)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "books": rows })))
}

pub async fn recent_books(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<BooksQuery>,
) -> Result<Json<Value>, BooksError> {
    let limit = q.limit.unwrap_or(24).clamp(1, 100);
    let rows = sqlx::query_as::<_, BookListItem>(&format!(
        "SELECT b.id, b.library_id, b.series_id, b.title, b.sort_title, b.series_index, \
                b.page_count, b.cover_format_id, b.added_at, \
                COALESCE(ARRAY(SELECT f.format FROM books.book_formats f WHERE f.book_id = b.id ORDER BY f.format), '{{}}') AS formats \
         FROM books.books b JOIN books.libraries l ON l.id = b.library_id \
         WHERE {VISIBLE} AND books.age_ok($1, b.age_rating) ORDER BY b.added_at DESC LIMIT $2"
    ))
    .bind(user.id)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "books": rows })))
}

pub async fn get_book(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let book = sqlx::query_as::<_, Book>(&format!(
        "SELECT b.* FROM books.books b JOIN books.libraries l ON l.id = b.library_id \
         WHERE {VISIBLE} AND books.age_ok($1, b.age_rating) AND b.id = $2"
    ))
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| BooksError::NotFound(format!("Livre {id}")))?;

    let formats = sqlx::query_as::<_, BookFormat>(
        "SELECT * FROM books.book_formats WHERE book_id = $1 ORDER BY format",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    // Breadcrumb context: library + (optional) series names.
    let lib = sqlx::query_as::<_, (Uuid, String)>("SELECT id, name FROM books.libraries WHERE id = $1")
        .bind(book.library_id)
        .fetch_one(&state.db)
        .await?;
    let series = match book.series_id {
        Some(sid) => sqlx::query_as::<_, (Uuid, String)>("SELECT id, name FROM books.series WHERE id = $1")
            .bind(sid)
            .fetch_optional(&state.db)
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

const BOOK_UPDATE_SET: &str = "\
    title             = COALESCE($2, title), \
    sort_title        = COALESCE($3, sort_title), \
    series_index      = COALESCE($4, series_index), \
    description       = COALESCE($5, description), \
    publisher         = COALESCE($6, publisher), \
    published_date    = COALESCE($7, published_date), \
    isbn              = COALESCE($8, isbn), \
    language          = COALESCE($9, language), \
    rating            = COALESCE($10, rating), \
    age_rating        = COALESCE($11, age_rating), \
    reading_direction = COALESCE($12, reading_direction), \
    authors           = COALESCE($13, authors), \
    tags              = COALESCE($14, tags), \
    identifiers       = COALESCE($15, identifiers), \
    updated_at = now()";

fn bind_book_update<'q>(
    q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    dto: &'q UpdateBookDto,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    q.bind(&dto.title)
        .bind(&dto.sort_title)
        .bind(dto.series_index)
        .bind(&dto.description)
        .bind(&dto.publisher)
        .bind(dto.published_date)
        .bind(&dto.isbn)
        .bind(&dto.language)
        .bind(dto.rating)
        .bind(dto.age_rating)
        .bind(&dto.reading_direction)
        .bind(&dto.authors)
        .bind(&dto.tags)
        .bind(&dto.identifiers)
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
    let sql = format!("UPDATE books.books SET {BOOK_UPDATE_SET} WHERE id = $1 RETURNING id");
    let updated = bind_book_update(sqlx::query(&sql).bind(id), &dto)
        .fetch_optional(&state.db)
        .await?;
    if updated.is_none() {
        return Err(BooksError::NotFound(format!("Livre {id}")));
    }
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
    let sql = format!("UPDATE books.books SET {BOOK_UPDATE_SET} WHERE id = ANY($1)");
    let n = bind_book_update(sqlx::query(&sql).bind(&dto.ids), &dto.fields)
        .execute(&state.db)
        .await?
        .rows_affected();
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
    let updated = sqlx::query(
        "UPDATE books.series SET \
           name              = COALESCE($2, name), \
           sort_name         = COALESCE($3, sort_name), \
           description       = COALESCE($4, description), \
           publisher         = COALESCE($5, publisher), \
           language          = COALESCE($6, language), \
           age_rating        = COALESCE($7, age_rating), \
           reading_direction = COALESCE($8, reading_direction), \
           total_book_count  = COALESCE($9, total_book_count), \
           genres            = COALESCE($10, genres), \
           tags              = COALESCE($11, tags), \
           updated_at = now() \
         WHERE id = $1 RETURNING id",
    )
    .bind(id)
    .bind(&dto.name)
    .bind(&dto.sort_name)
    .bind(&dto.description)
    .bind(&dto.publisher)
    .bind(&dto.language)
    .bind(dto.age_rating)
    .bind(&dto.reading_direction)
    .bind(dto.total_book_count)
    .bind(&dto.genres)
    .bind(&dto.tags)
    .fetch_optional(&state.db)
    .await?;
    if updated.is_none() {
        return Err(BooksError::NotFound(format!("Série {id}")));
    }
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

    let n = sqlx::query(
        "UPDATE books.books SET \
           title          = COALESCE($2, title), \
           publisher      = COALESCE($3, publisher), \
           published_date = COALESCE($4, published_date), \
           isbn           = COALESCE($5, isbn), \
           description    = COALESCE($6, description), \
           language       = COALESCE($7, language), \
           authors        = COALESCE($8, authors), \
           tags           = COALESCE($9, tags), \
           updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(&dto.title)
    .bind(&dto.publisher)
    .bind(date)
    .bind(&dto.isbn)
    .bind(&dto.description)
    .bind(&dto.language)
    .bind(&authors)
    .bind(&tags)
    .execute(&state.db)
    .await?
    .rows_affected();
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
    formats:        Vec<String>,
    added_at:       chrono::DateTime<chrono::Utc>,
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
    let rows = sqlx::query_as::<_, ExportRow>(
        "SELECT b.title, s.name AS series_name, b.series_index, b.authors, b.publisher, \
                b.published_date, b.isbn, b.language, b.tags, b.rating, b.page_count, \
                COALESCE(ARRAY(SELECT f.format FROM books.book_formats f WHERE f.book_id = b.id ORDER BY f.format), '{}') AS formats, \
                b.added_at \
         FROM books.books b JOIN books.libraries l ON l.id = b.library_id \
         LEFT JOIN books.series s ON s.id = b.series_id \
         WHERE (l.is_shared OR l.owner_id = $1) AND ($2::uuid IS NULL OR b.library_id = $2) \
         ORDER BY s.name NULLS LAST, b.series_index NULLS LAST, b.title",
    )
    .bind(user.id)
    .bind(q.library_id)
    .fetch_all(&state.db)
    .await?;

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
    let rows = sqlx::query_as::<_, (String, Value)>(
        "SELECT bf.content_hash AS hash, \
                jsonb_agg(DISTINCT jsonb_build_object('id', b.id, 'title', b.title)) AS books \
         FROM books.book_formats bf \
         JOIN books.books b ON b.id = bf.book_id \
         JOIN books.libraries l ON l.id = b.library_id \
         WHERE bf.content_hash IS NOT NULL AND (l.is_shared OR l.owner_id = $1) \
         GROUP BY bf.content_hash HAVING count(DISTINCT b.id) > 1",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    let groups: Vec<_> = rows.iter().map(|(hash, books)| json!({ "hash": hash, "books": books })).collect();
    Ok(Json(json!({ "duplicates": groups })))
}
