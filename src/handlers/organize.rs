// Organisation: collections (groups of series), read lists (ordered books), saved searches
// (virtual libraries) and facets (tag browser). All owner-scoped; public ones are visible to all.
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::BooksError,
    middleware::auth::AuthUser,
    models::content::{Book, Series},
    services::access,
    state::AppState,
};

// ── Collections ──────────────────────────────────────────────────────────────────
#[derive(Debug, Deserialize)]
pub struct CollectionDto {
    pub name:        String,
    pub description: Option<String>,
    pub is_public:   Option<bool>,
}

pub async fn list_collections(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    let rows = sqlx::query_as::<_, (Uuid, String, Option<String>, bool, i64)>(
        "SELECT c.id, c.name, c.description, c.is_public, \
                (SELECT count(*) FROM books.collection_series cs WHERE cs.collection_id = c.id) \
         FROM books.collections c WHERE c.is_public OR c.owner_id = $1 ORDER BY c.name",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    let items: Vec<_> = rows
        .iter()
        .map(|(id, name, desc, pubc, n)| json!({ "id": id, "name": name, "description": desc, "is_public": pubc, "series_count": n }))
        .collect();
    Ok(Json(json!({ "collections": items })))
}

pub async fn create_collection(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(dto): Json<CollectionDto>,
) -> Result<(StatusCode, Json<Value>), BooksError> {
    let id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO books.collections (owner_id, name, description, is_public) VALUES ($1,$2,$3,$4) RETURNING id",
    )
    .bind(user.id)
    .bind(&dto.name)
    .bind(&dto.description)
    .bind(dto.is_public.unwrap_or(true))
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn get_collection(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let coll = sqlx::query_as::<_, (Uuid, String, Option<String>, bool)>(
        "SELECT id, name, description, is_public FROM books.collections WHERE id = $1 AND (is_public OR owner_id = $2)",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| BooksError::NotFound("Collection".into()))?;

    // Membership is not a permission: a collection may be public while some of
    // the series it lists sit in a library this reader was never given. The
    // join on `libraries` is what was missing.
    let readable = access::readable_series("$2", state.instance().block_unrated_sql());
    let series = sqlx::query_as::<_, Series>(&format!(
        "SELECT s.* FROM books.collection_series cs \
         JOIN books.series s ON s.id = cs.series_id \
         JOIN books.libraries l ON l.id = s.library_id \
         WHERE cs.collection_id = $1 AND {readable} ORDER BY cs.position, s.name"
    ))
    .bind(id)
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({ "collection": { "id": coll.0, "name": coll.1, "description": coll.2, "is_public": coll.3 }, "series": series })))
}

async fn owns_collection(state: &AppState, user_id: Uuid, id: Uuid) -> Result<(), BooksError> {
    let ok = sqlx::query_scalar::<_, Uuid>("SELECT id FROM books.collections WHERE id = $1 AND owner_id = $2")
        .bind(id).bind(user_id).fetch_optional(&state.db).await?;
    ok.map(|_| ()).ok_or(BooksError::Forbidden)
}

pub async fn update_collection(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>, Json(dto): Json<CollectionDto>,
) -> Result<Json<Value>, BooksError> {
    owns_collection(&state, user.id, id).await?;
    sqlx::query("UPDATE books.collections SET name=$2, description=$3, is_public=COALESCE($4,is_public), updated_at=now() WHERE id=$1")
        .bind(id).bind(&dto.name).bind(&dto.description).bind(dto.is_public).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_collection(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>,
) -> Result<StatusCode, BooksError> {
    owns_collection(&state, user.id, id).await?;
    sqlx::query("DELETE FROM books.collections WHERE id = $1").bind(id).execute(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct SeriesRefDto { pub series_id: Uuid }

pub async fn add_series_to_collection(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>, Json(dto): Json<SeriesRefDto>,
) -> Result<Json<Value>, BooksError> {
    owns_collection(&state, user.id, id).await?;
    sqlx::query(
        "INSERT INTO books.collection_series (collection_id, series_id, position) \
         VALUES ($1, $2, COALESCE((SELECT max(position)+1 FROM books.collection_series WHERE collection_id=$1), 0)) \
         ON CONFLICT DO NOTHING",
    ).bind(id).bind(dto.series_id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_series_from_collection(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path((id, sid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, BooksError> {
    owns_collection(&state, user.id, id).await?;
    sqlx::query("DELETE FROM books.collection_series WHERE collection_id=$1 AND series_id=$2").bind(id).bind(sid).execute(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Read lists ───────────────────────────────────────────────────────────────────
#[derive(Debug, Deserialize)]
pub struct ReadListDto { pub name: String, pub description: Option<String>, pub is_public: Option<bool> }

pub async fn list_read_lists(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    let rows = sqlx::query_as::<_, (Uuid, String, Option<String>, bool, i64)>(
        "SELECT r.id, r.name, r.description, r.is_public, \
                (SELECT count(*) FROM books.read_list_books rb WHERE rb.read_list_id=r.id) \
         FROM books.read_lists r WHERE r.is_public OR r.owner_id=$1 ORDER BY r.name",
    ).bind(user.id).fetch_all(&state.db).await?;
    let items: Vec<_> = rows.iter().map(|(id,name,desc,pubc,n)| json!({"id":id,"name":name,"description":desc,"is_public":pubc,"book_count":n})).collect();
    Ok(Json(json!({ "read_lists": items })))
}

pub async fn create_read_list(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Json(dto): Json<ReadListDto>,
) -> Result<(StatusCode, Json<Value>), BooksError> {
    let id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO books.read_lists (owner_id, name, description, is_public) VALUES ($1,$2,$3,$4) RETURNING id",
    ).bind(user.id).bind(&dto.name).bind(&dto.description).bind(dto.is_public.unwrap_or(true)).fetch_one(&state.db).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn get_read_list(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let rl = sqlx::query_as::<_, (Uuid, String, Option<String>, bool)>(
        "SELECT id, name, description, is_public FROM books.read_lists WHERE id=$1 AND (is_public OR owner_id=$2)",
    ).bind(id).bind(user.id).fetch_optional(&state.db).await?.ok_or_else(|| BooksError::NotFound("Liste de lecture".into()))?;
    let readable = access::readable_book("$2", state.instance().block_unrated_sql());
    let books = sqlx::query_as::<_, Book>(&format!(
        "SELECT b.* FROM books.read_list_books rb \
         JOIN books.books b ON b.id = rb.book_id \
         JOIN books.libraries l ON l.id = b.library_id \
         WHERE rb.read_list_id = $1 AND {readable} ORDER BY rb.position"
    )).bind(id).bind(user.id).fetch_all(&state.db).await?;
    Ok(Json(json!({ "read_list": {"id":rl.0,"name":rl.1,"description":rl.2,"is_public":rl.3}, "books": books })))
}

async fn owns_read_list(state: &AppState, user_id: Uuid, id: Uuid) -> Result<(), BooksError> {
    let ok = sqlx::query_scalar::<_, Uuid>("SELECT id FROM books.read_lists WHERE id=$1 AND owner_id=$2").bind(id).bind(user_id).fetch_optional(&state.db).await?;
    ok.map(|_| ()).ok_or(BooksError::Forbidden)
}

pub async fn delete_read_list(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>,
) -> Result<StatusCode, BooksError> {
    owns_read_list(&state, user.id, id).await?;
    sqlx::query("DELETE FROM books.read_lists WHERE id=$1").bind(id).execute(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct BookRefDto { pub book_id: Uuid }

pub async fn add_book_to_read_list(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>, Json(dto): Json<BookRefDto>,
) -> Result<Json<Value>, BooksError> {
    owns_read_list(&state, user.id, id).await?;
    sqlx::query(
        "INSERT INTO books.read_list_books (read_list_id, book_id, position) \
         VALUES ($1,$2, COALESCE((SELECT max(position)+1 FROM books.read_list_books WHERE read_list_id=$1),0)) ON CONFLICT DO NOTHING",
    ).bind(id).bind(dto.book_id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_book_from_read_list(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path((id, bid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, BooksError> {
    owns_read_list(&state, user.id, id).await?;
    sqlx::query("DELETE FROM books.read_list_books WHERE read_list_id=$1 AND book_id=$2").bind(id).bind(bid).execute(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Saved searches (virtual libraries) ───────────────────────────────────────────
#[derive(Debug, Deserialize)]
pub struct SavedSearchDto { pub name: String, pub filters: Value }

pub async fn list_saved_searches(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    let rows = sqlx::query_as::<_, (Uuid, String, Value)>("SELECT id, name, filters FROM books.saved_searches WHERE owner_id=$1 ORDER BY name").bind(user.id).fetch_all(&state.db).await?;
    let items: Vec<_> = rows.iter().map(|(id,name,f)| json!({"id":id,"name":name,"filters":f})).collect();
    Ok(Json(json!({ "saved_searches": items })))
}

pub async fn create_saved_search(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Json(dto): Json<SavedSearchDto>,
) -> Result<(StatusCode, Json<Value>), BooksError> {
    let id = sqlx::query_scalar::<_, Uuid>("INSERT INTO books.saved_searches (owner_id, name, filters) VALUES ($1,$2,$3) RETURNING id")
        .bind(user.id).bind(&dto.name).bind(&dto.filters).fetch_one(&state.db).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn delete_saved_search(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>,
) -> Result<StatusCode, BooksError> {
    sqlx::query("DELETE FROM books.saved_searches WHERE id=$1 AND owner_id=$2").bind(id).bind(user.id).execute(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Facets (tag browser) ─────────────────────────────────────────────────────────
#[derive(Debug, sqlx::FromRow, serde::Serialize)]
struct Facet { value: String, count: i64 }

/// A facet that could not be computed comes back empty rather than failing the
/// whole browser — but never silently: a swallowed error here reads, from the
/// interface, exactly like "there is nothing tagged".
fn facet_failed(e: sqlx::Error) -> Vec<Facet> {
    tracing::error!(error = %e, "Calcul d'une facette books échoué");
    Vec::new()
}

pub async fn facets(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    let readable = access::readable_book("$1", state.instance().block_unrated_sql());
    let tags = sqlx::query_as::<_, Facet>(&format!(
        "SELECT t AS value, count(*) AS count FROM books.books b \
         JOIN books.libraries l ON l.id = b.library_id, LATERAL jsonb_array_elements_text(b.tags) t \
         WHERE {readable} GROUP BY t ORDER BY count DESC, value LIMIT 300"
    )).bind(user.id).fetch_all(&state.db).await.unwrap_or_else(facet_failed);
    let authors = sqlx::query_as::<_, Facet>(&format!(
        "SELECT a->>'name' AS value, count(*) AS count FROM books.books b \
         JOIN books.libraries l ON l.id = b.library_id, LATERAL jsonb_array_elements(b.authors) a \
         WHERE {readable} AND a->>'name' IS NOT NULL \
         GROUP BY a->>'name' ORDER BY count DESC, value LIMIT 300"
    )).bind(user.id).fetch_all(&state.db).await.unwrap_or_else(facet_failed);
    let publishers = sqlx::query_as::<_, Facet>(&format!(
        "SELECT b.publisher AS value, count(*) AS count FROM books.books b \
         JOIN books.libraries l ON l.id = b.library_id \
         WHERE {readable} AND b.publisher IS NOT NULL \
         GROUP BY b.publisher ORDER BY count DESC, value LIMIT 200"
    )).bind(user.id).fetch_all(&state.db).await.unwrap_or_else(facet_failed);
    let languages = sqlx::query_as::<_, Facet>(&format!(
        "SELECT b.language AS value, count(*) AS count FROM books.books b \
         JOIN books.libraries l ON l.id = b.library_id \
         WHERE {readable} AND b.language IS NOT NULL \
         GROUP BY b.language ORDER BY count DESC, value LIMIT 100"
    )).bind(user.id).fetch_all(&state.db).await.unwrap_or_else(facet_failed);
    Ok(Json(json!({ "tags": tags, "authors": authors, "publishers": publishers, "languages": languages })))
}
