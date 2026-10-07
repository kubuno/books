// Organisation: collections (groups of series), read lists (ordered books), saved searches
// (virtual libraries) and facets (tag browser). All owner-scoped; public ones are visible to all.
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use kubuno_db::dialect::SqlType;
use kubuno_db::{new_id, params, DbQueryBuilder};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::BooksError,
    middleware::auth::AuthUser,
    models::content::{Book, Series},
    services::access::Restriction,
    state::AppState,
};

/// Next `position` for a membership table, computed in Rust. A `MAX(position)+1`
/// subquery over the same table inside an INSERT is rejected by MySQL, and the
/// `int4` result needs a portable width (see the dialect cast).
async fn next_position(
    state: &AppState,
    table: &'static str,
    key_col: &'static str,
    key: Uuid,
) -> Result<i32, BooksError> {
    let n = state
        .db
        .fetch_scalar::<i64>(
            &format!(
                "SELECT {} FROM books.{table} WHERE {key_col} = $1",
                state.db.backend().cast("COALESCE(MAX(position) + 1, 0)", SqlType::BigInt)
            ),
            params![key],
        )
        .await?;
    Ok(n as i32)
}

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
    let cnt = state.db.backend().count_bigint("*");
    let rows = state
        .db
        .fetch_all_as::<(Uuid, String, Option<String>, bool, i64)>(
            &format!(
                "SELECT c.id, c.name, c.description, c.is_public, \
                        (SELECT {cnt} FROM books.collection_series cs WHERE cs.collection_id = c.id) \
                 FROM books.collections c WHERE c.is_public OR c.owner_id = $1 ORDER BY c.name"
            ),
            params![user.id],
        )
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
    let id = new_id();
    state
        .db
        .execute(
            "INSERT INTO books.collections (id, owner_id, name, description, is_public) VALUES ($1,$2,$3,$4,$5)",
            params![id, user.id, dto.name, dto.description, dto.is_public.unwrap_or(true)],
        )
        .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn get_collection(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let coll = state
        .db
        .fetch_optional_as::<(Uuid, String, Option<String>, bool)>(
            "SELECT id, name, description, is_public FROM books.collections WHERE id = $1 AND (is_public OR owner_id = $2)",
            params![id, user.id],
        )
        .await?
        .ok_or_else(|| BooksError::NotFound("Collection".into()))?;

    // Membership is not a permission: a collection may be public while some of
    // the series it lists sit in a library this reader was never given.
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT s.* FROM books.collection_series cs \
         JOIN books.series s ON s.id = cs.series_id \
         JOIN books.libraries l ON l.id = s.library_id \
         WHERE cs.collection_id = ",
    );
    qb.push_bind(id).push(" AND ");
    restriction.push_readable_series(&mut qb, user.id, block);
    qb.push_order_by("cs.position, s.name");
    let series = qb.fetch_all_as::<Series>(&state.db).await?;
    Ok(Json(json!({ "collection": { "id": coll.0, "name": coll.1, "description": coll.2, "is_public": coll.3 }, "series": series })))
}

async fn owns_collection(state: &AppState, user_id: Uuid, id: Uuid) -> Result<(), BooksError> {
    let ok = state
        .db
        .fetch_optional_scalar::<Uuid>(
            "SELECT id FROM books.collections WHERE id = $1 AND owner_id = $2",
            params![id, user_id],
        )
        .await?;
    ok.map(|_| ()).ok_or(BooksError::Forbidden)
}

pub async fn update_collection(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>, Json(dto): Json<CollectionDto>,
) -> Result<Json<Value>, BooksError> {
    owns_collection(&state, user.id, id).await?;
    state
        .db
        .execute(
            "UPDATE books.collections SET name=$1, description=$2, is_public=COALESCE($3,is_public), updated_at=$4 WHERE id=$5",
            params![dto.name, dto.description, dto.is_public, chrono::Utc::now(), id],
        )
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_collection(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>,
) -> Result<StatusCode, BooksError> {
    owns_collection(&state, user.id, id).await?;
    state.db.execute("DELETE FROM books.collections WHERE id = $1", params![id]).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct SeriesRefDto { pub series_id: Uuid }

pub async fn add_series_to_collection(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>, Json(dto): Json<SeriesRefDto>,
) -> Result<Json<Value>, BooksError> {
    owns_collection(&state, user.id, id).await?;
    let position = next_position(&state, "collection_series", "collection_id", id).await?;
    let ignore = state.db.backend().insert_ignore_prefix();
    let on_conflict = state.db.backend().on_conflict_do_nothing(&["collection_id", "series_id"]);
    state
        .db
        .execute(
            &format!(
                "INSERT {ignore}INTO books.collection_series (collection_id, series_id, position) \
                 VALUES ($1, $2, $3){on_conflict}"
            ),
            params![id, dto.series_id, position],
        )
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_series_from_collection(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path((id, sid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, BooksError> {
    owns_collection(&state, user.id, id).await?;
    state
        .db
        .execute(
            "DELETE FROM books.collection_series WHERE collection_id=$1 AND series_id=$2",
            params![id, sid],
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Read lists ───────────────────────────────────────────────────────────────────
#[derive(Debug, Deserialize)]
pub struct ReadListDto { pub name: String, pub description: Option<String>, pub is_public: Option<bool> }

pub async fn list_read_lists(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    let cnt = state.db.backend().count_bigint("*");
    let rows = state
        .db
        .fetch_all_as::<(Uuid, String, Option<String>, bool, i64)>(
            &format!(
                "SELECT r.id, r.name, r.description, r.is_public, \
                        (SELECT {cnt} FROM books.read_list_books rb WHERE rb.read_list_id=r.id) \
                 FROM books.read_lists r WHERE r.is_public OR r.owner_id=$1 ORDER BY r.name"
            ),
            params![user.id],
        )
        .await?;
    let items: Vec<_> = rows.iter().map(|(id,name,desc,pubc,n)| json!({"id":id,"name":name,"description":desc,"is_public":pubc,"book_count":n})).collect();
    Ok(Json(json!({ "read_lists": items })))
}

pub async fn create_read_list(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Json(dto): Json<ReadListDto>,
) -> Result<(StatusCode, Json<Value>), BooksError> {
    let id = new_id();
    state
        .db
        .execute(
            "INSERT INTO books.read_lists (id, owner_id, name, description, is_public) VALUES ($1,$2,$3,$4,$5)",
            params![id, user.id, dto.name, dto.description, dto.is_public.unwrap_or(true)],
        )
        .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn get_read_list(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let rl = state
        .db
        .fetch_optional_as::<(Uuid, String, Option<String>, bool)>(
            "SELECT id, name, description, is_public FROM books.read_lists WHERE id=$1 AND (is_public OR owner_id=$2)",
            params![id, user.id],
        )
        .await?
        .ok_or_else(|| BooksError::NotFound("Liste de lecture".into()))?;
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT b.* FROM books.read_list_books rb \
         JOIN books.books b ON b.id = rb.book_id \
         JOIN books.libraries l ON l.id = b.library_id \
         WHERE rb.read_list_id = ",
    );
    qb.push_bind(id).push(" AND ");
    restriction.push_readable_book(&mut qb, user.id, block);
    qb.push_order_by("rb.position");
    let books = qb.fetch_all_as::<Book>(&state.db).await?;
    Ok(Json(json!({ "read_list": {"id":rl.0,"name":rl.1,"description":rl.2,"is_public":rl.3}, "books": books })))
}

async fn owns_read_list(state: &AppState, user_id: Uuid, id: Uuid) -> Result<(), BooksError> {
    let ok = state
        .db
        .fetch_optional_scalar::<Uuid>(
            "SELECT id FROM books.read_lists WHERE id=$1 AND owner_id=$2",
            params![id, user_id],
        )
        .await?;
    ok.map(|_| ()).ok_or(BooksError::Forbidden)
}

pub async fn delete_read_list(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>,
) -> Result<StatusCode, BooksError> {
    owns_read_list(&state, user.id, id).await?;
    state.db.execute("DELETE FROM books.read_lists WHERE id=$1", params![id]).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct BookRefDto { pub book_id: Uuid }

pub async fn add_book_to_read_list(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>, Json(dto): Json<BookRefDto>,
) -> Result<Json<Value>, BooksError> {
    owns_read_list(&state, user.id, id).await?;
    let position = next_position(&state, "read_list_books", "read_list_id", id).await?;
    let ignore = state.db.backend().insert_ignore_prefix();
    let on_conflict = state.db.backend().on_conflict_do_nothing(&["read_list_id", "book_id"]);
    state
        .db
        .execute(
            &format!(
                "INSERT {ignore}INTO books.read_list_books (read_list_id, book_id, position) \
                 VALUES ($1,$2,$3){on_conflict}"
            ),
            params![id, dto.book_id, position],
        )
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_book_from_read_list(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path((id, bid)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, BooksError> {
    owns_read_list(&state, user.id, id).await?;
    state
        .db
        .execute(
            "DELETE FROM books.read_list_books WHERE read_list_id=$1 AND book_id=$2",
            params![id, bid],
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Saved searches (virtual libraries) ───────────────────────────────────────────
#[derive(Debug, Deserialize)]
pub struct SavedSearchDto { pub name: String, pub filters: Value }

pub async fn list_saved_searches(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    let rows = state
        .db
        .fetch_all_as::<(Uuid, String, Value)>(
            "SELECT id, name, filters FROM books.saved_searches WHERE owner_id=$1 ORDER BY name",
            params![user.id],
        )
        .await?;
    let items: Vec<_> = rows.iter().map(|(id,name,f)| json!({"id":id,"name":name,"filters":f})).collect();
    Ok(Json(json!({ "saved_searches": items })))
}

pub async fn create_saved_search(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Json(dto): Json<SavedSearchDto>,
) -> Result<(StatusCode, Json<Value>), BooksError> {
    let id = new_id();
    state
        .db
        .execute(
            "INSERT INTO books.saved_searches (id, owner_id, name, filters) VALUES ($1,$2,$3,$4)",
            params![id, user.id, dto.name, dto.filters],
        )
        .await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn delete_saved_search(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>,
) -> Result<StatusCode, BooksError> {
    state
        .db
        .execute(
            "DELETE FROM books.saved_searches WHERE id=$1 AND owner_id=$2",
            params![id, user.id],
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Facets (tag browser) ─────────────────────────────────────────────────────────
#[derive(Debug, serde::Serialize)]
struct Facet { value: String, count: i64 }

/// A facet that could not be computed comes back empty rather than failing the
/// whole browser — but never silently.
fn facet_failed(e: sqlx::Error) -> Vec<Facet> {
    tracing::error!(error = %e, "Calcul d'une facette books échoué");
    Vec::new()
}

/// Sorts a value→count map into the top `limit` facets (count desc, then value).
fn top_facets(counts: std::collections::HashMap<String, i64>, limit: usize) -> Vec<Facet> {
    let mut v: Vec<Facet> = counts.into_iter().map(|(value, count)| Facet { value, count }).collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
    v.truncate(limit);
    v
}

pub async fn facets(
    State(state): State<AppState>, Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;

    // tags + authors: JSON arrays are expanded and counted in Rust, since
    // `jsonb_array_elements` / `LATERAL` have no portable spelling.
    let build = |cols: &'static str| {
        let mut qb = DbQueryBuilder::new(
            state.db.backend(),
            format!("SELECT {cols} FROM books.books b JOIN books.libraries l ON l.id = b.library_id WHERE "),
        );
        restriction.push_readable_book(&mut qb, user.id, block);
        qb
    };

    let tag_rows = build("b.tags").fetch_all_as::<(Value,)>(&state.db).await;
    let tags = match tag_rows {
        Ok(rows) => {
            let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
            for (tags,) in rows {
                if let Some(arr) = tags.as_array() {
                    for t in arr {
                        if let Some(s) = t.as_str() {
                            *counts.entry(s.to_string()).or_default() += 1;
                        }
                    }
                }
            }
            top_facets(counts, 300)
        }
        Err(e) => facet_failed(e),
    };

    let author_rows = build("b.authors").fetch_all_as::<(Value,)>(&state.db).await;
    let authors = match author_rows {
        Ok(rows) => {
            let mut counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
            for (authors,) in rows {
                if let Some(arr) = authors.as_array() {
                    for a in arr {
                        if let Some(name) = a.get("name").and_then(|n| n.as_str()) {
                            *counts.entry(name.to_string()).or_default() += 1;
                        }
                    }
                }
            }
            top_facets(counts, 300)
        }
        Err(e) => facet_failed(e),
    };

    // publishers + languages: scalar columns, grouped in SQL.
    let publishers = scalar_facet(&state, &restriction, user.id, block, "b.publisher", 200).await;
    let languages = scalar_facet(&state, &restriction, user.id, block, "b.language", 100).await;

    Ok(Json(json!({ "tags": tags, "authors": authors, "publishers": publishers, "languages": languages })))
}

/// Grouped count over one scalar book column, restricted to readable books.
async fn scalar_facet(
    state: &AppState,
    restriction: &Restriction,
    user_id: Uuid,
    block: bool,
    col: &'static str,
    limit: i64,
) -> Vec<Facet> {
    let cnt = state.db.backend().count_bigint("*");
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        format!("SELECT {col} AS value, {cnt} AS cnt FROM books.books b JOIN books.libraries l ON l.id = b.library_id WHERE "),
    );
    restriction.push_readable_book(&mut qb, user_id, block);
    qb.push(format!(" AND {col} IS NOT NULL GROUP BY {col}"));
    qb.push_order_by("cnt DESC, value");
    qb.push_limit_offset(limit, 0);
    match qb.fetch_all_as::<(String, i64)>(&state.db).await {
        Ok(rows) => rows.into_iter().map(|(value, count)| Facet { value, count }).collect(),
        Err(e) => facet_failed(e),
    }
}
