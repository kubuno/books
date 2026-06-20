// Serving cover thumbnails and page images. Book files live under the shared drive storage and
// are read directly from disk (random access); generated cover thumbnails are cached in the
// module's own storage. CBZ only for this P2 increment; other formats join via services::decode.
use axum::{
    extract::{Extension, Path, State},
    http::header,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use uuid::Uuid;

use crate::{errors::BooksError, middleware::auth::AuthUser, services::decode, state::AppState};

struct Fmt {
    format:           String,
    storage_path:     String,
    local_cache_path: Option<String>,
    owner_id:         Uuid,
}

/// (page index, width, height) rows produced when indexing page dimensions.
type PageDims = Vec<(i32, Option<i32>, Option<i32>)>;

/// Resolve the canonical `storage_path` of a format to a readable filesystem path.
fn resolve_path(state: &AppState, fmt: &Fmt) -> Result<std::path::PathBuf, BooksError> {
    decode::resolve_storage(
        &state.settings,
        &fmt.storage_path,
        fmt.local_cache_path.as_deref(),
        fmt.owner_id,
    )
}

/// Load a format's file if the requesting user may see its book.
async fn visible_format(state: &AppState, user_id: Uuid, format_id: Uuid) -> Result<Fmt, BooksError> {
    sqlx::query_as::<_, (String, String, Option<String>, Uuid)>(
        "SELECT bf.format, bf.storage_path, bf.local_cache_path, bf.owner_id \
         FROM books.book_formats bf \
         JOIN books.books b ON b.id = bf.book_id \
         JOIN books.libraries l ON l.id = b.library_id \
         WHERE bf.id = $1 AND (l.is_shared OR l.owner_id = $2)",
    )
    .bind(format_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await?
    .map(|(format, storage_path, local_cache_path, owner_id)| Fmt { format, storage_path, local_cache_path, owner_id })
    .ok_or_else(|| BooksError::NotFound("Format introuvable".into()))
}

async fn cover_format_of(
    state: &AppState,
    user_id: Uuid,
    table: &str,
    id: Uuid,
) -> Result<Uuid, BooksError> {
    let sql = format!(
        "SELECT t.cover_format_id FROM books.{table} t \
         JOIN books.libraries l ON l.id = t.library_id \
         WHERE t.id = $1 AND (l.is_shared OR l.owner_id = $2)"
    );
    sqlx::query_scalar::<_, Option<Uuid>>(&sql)
        .bind(id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| BooksError::NotFound("Ressource introuvable".into()))?
        .ok_or_else(|| BooksError::NotFound("Couverture indisponible".into()))
}

/// Render (and cache) a 480px-wide JPEG cover for the given format.
async fn render_cover(state: &AppState, fmt: &Fmt, format_id: Uuid) -> Result<Bytes, BooksError> {
    let cache_key = format!("cache/cover/{format_id}.jpg");
    if let Ok(cached) = state.storage.get(&cache_key).await {
        return Ok(cached);
    }
    // Library cover options: which page is the cover + thumbnail width.
    let settings = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT l.settings FROM books.book_formats bf \
         JOIN books.books b ON b.id = bf.book_id JOIN books.libraries l ON l.id = b.library_id \
         WHERE bf.id = $1",
    )
    .bind(format_id)
    .fetch_optional(&state.db)
    .await?
    .unwrap_or_else(|| serde_json::json!({}));
    let cover_page = settings.pointer("/options/cover_page").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let thumb_w = settings.pointer("/options/thumbnail_width").and_then(|v| v.as_u64()).unwrap_or(480).clamp(120, 1200) as u32;

    let path = resolve_path(state, fmt)?;
    let format = fmt.format.clone();
    let jpg = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, BooksError> {
        // Chosen page of an archive, or the EPUB cover image. (PDF covers are client-rendered.)
        let raw = if decode::is_image_archive(&format) {
            let pages = decode::list_pages(&format, &path)?;
            let entry = pages.get(cover_page).or_else(|| pages.first())
                .ok_or_else(|| BooksError::NotFound("Aucune page".into()))?;
            decode::read_page(&format, &path, entry)?.0
        } else if format == "epub" {
            decode::epub_cover(&path)?.0
        } else {
            return Err(BooksError::NotFound("Couverture indisponible pour ce format".into()));
        };
        decode::thumbnail(&raw, thumb_w)
    })
    .await
    .map_err(|e| BooksError::Internal(anyhow::anyhow!(e)))??;
    let bytes = Bytes::from(jpg);
    let _ = state.storage.put(&cache_key, bytes.clone()).await; // best-effort cache
    Ok(bytes)
}

fn image_response(content_type: &str, bytes: Bytes, immutable: bool) -> Response {
    let cache = if immutable { "private, max-age=86400" } else { "private, max-age=3600" };
    (
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CACHE_CONTROL, cache.to_string()),
        ],
        bytes,
    )
        .into_response()
}

pub async fn book_cover(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Response, BooksError> {
    let fmt_id = cover_format_of(&state, user.id, "books", id).await?;
    // A downloaded/custom cover (P5) wins over the format-derived one.
    if let Ok(custom) = state.storage.get(&format!("cache/cover/custom_{id}.jpg")).await {
        return Ok(image_response("image/jpeg", custom, true));
    }
    let fmt = visible_format(&state, user.id, fmt_id).await?;
    let bytes = render_cover(&state, &fmt, fmt_id).await?;
    Ok(image_response("image/jpeg", bytes, true))
}

pub async fn series_cover(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Response, BooksError> {
    let fmt_id = cover_format_of(&state, user.id, "series", id).await?;
    let fmt = visible_format(&state, user.id, fmt_id).await?;
    let bytes = render_cover(&state, &fmt, fmt_id).await?;
    Ok(image_response("image/jpeg", bytes, true))
}

/// Page count of a book's primary (cover) format; lazily persists it.
pub async fn book_page_count(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<axum::Json<serde_json::Value>, BooksError> {
    let fmt_id = cover_format_of(&state, user.id, "books", id).await?;
    let fmt = visible_format(&state, user.id, fmt_id).await?;
    let path = resolve_path(&state, &fmt)?;

    // Library option: index per-page dimensions (once).
    let already: bool = sqlx::query_scalar("SELECT pages_indexed FROM books.book_formats WHERE id = $1")
        .bind(fmt_id).fetch_optional(&state.db).await?.unwrap_or(false);
    let settings = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT l.settings FROM books.books b JOIN books.libraries l ON l.id = b.library_id WHERE b.id = $1",
    ).bind(id).fetch_optional(&state.db).await?.unwrap_or_else(|| serde_json::json!({}));
    let analyze = settings.pointer("/options/analyze_dimensions").and_then(|v| v.as_bool()).unwrap_or(true);

    let format = fmt.format.clone();
    let want_dims = analyze && !already;
    let (count, dims) = tokio::task::spawn_blocking(move || -> Result<(usize, PageDims), BooksError> {
        if decode::is_image_archive(&format) {
            let pages = decode::list_pages(&format, &path)?;
            let mut dims = Vec::new();
            if want_dims {
                for (i, entry) in pages.iter().enumerate() {
                    if let Ok((bytes, _)) = decode::read_page(&format, &path, entry) {
                        let wh = decode::image_dims(&bytes);
                        dims.push((i as i32, wh.map(|x| x.0 as i32), wh.map(|x| x.1 as i32)));
                    }
                }
            }
            Ok((pages.len(), dims))
        } else if format == "pdf" {
            Ok((decode::pdf_page_count(&path)?, Vec::new()))
        } else {
            Ok((0, Vec::new())) // EPUB is reflowable — paged client-side
        }
    })
    .await
    .map_err(|e| BooksError::Internal(anyhow::anyhow!(e)))??;
    let count = count as i32;

    for (idx, w, h) in &dims {
        let _ = sqlx::query(
            "INSERT INTO books.pages (format_id, idx, width, height) VALUES ($1,$2,$3,$4) \
             ON CONFLICT (format_id, idx) DO UPDATE SET width = $3, height = $4",
        )
        .bind(fmt_id).bind(idx).bind(w).bind(h).execute(&state.db).await;
    }

    sqlx::query("UPDATE books.book_formats SET page_count = $2, pages_indexed = TRUE WHERE id = $1")
        .bind(fmt_id)
        .bind(count)
        .execute(&state.db)
        .await?;
    sqlx::query("UPDATE books.books SET page_count = $2 WHERE id = $1 AND (page_count IS NULL OR page_count = 0)")
        .bind(id)
        .bind(count)
        .execute(&state.db)
        .await?;
    Ok(axum::Json(serde_json::json!({ "page_count": count })))
}

/// Full-resolution image of page `n` (0-based) from a book's primary format.
pub async fn book_page(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path((id, n)): Path<(Uuid, usize)>,
) -> Result<Response, BooksError> {
    let fmt_id = cover_format_of(&state, user.id, "books", id).await?;
    let fmt = visible_format(&state, user.id, fmt_id).await?;
    let path = resolve_path(&state, &fmt)?;
    let format = fmt.format.clone();
    let (bytes, ct) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, String), BooksError> {
        let pages = decode::list_pages(&format, &path)?;
        let entry = pages.get(n).ok_or_else(|| BooksError::NotFound(format!("Page {n}")))?;
        decode::read_page(&format, &path, entry)
    })
    .await
    .map_err(|e| BooksError::Internal(anyhow::anyhow!(e)))??;
    Ok(image_response(&ct, Bytes::from(bytes), true))
}

/// Stream a format's raw file from disk (used by the client-side PDF/EPUB reader and downloads).
pub async fn format_raw(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(format_id): Path<Uuid>,
) -> Result<Response, BooksError> {
    let fmt = visible_format(&state, user.id, format_id).await?;
    let path = resolve_path(&state, &fmt)?;
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| BooksError::Storage(e.to_string()))?;
    let stream = tokio_util::io::ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);
    let ct = match fmt.format.as_str() {
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "cbz" => "application/zip",
        "cb7" => "application/x-7z-compressed",
        "cbr" => "application/vnd.comicbook-rar",
        _ => "application/octet-stream",
    };
    Ok((
        [
            (header::CONTENT_TYPE, ct.to_string()),
            (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
        ],
        body,
    )
        .into_response())
}

/// Download a book's primary format file as an attachment.
pub async fn book_download(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Response, BooksError> {
    let (format, storage_path, local_cache_path, owner_id, file_name) = sqlx::query_as::<_, (String, String, Option<String>, Uuid, String)>(
        "SELECT bf.format, bf.storage_path, bf.local_cache_path, bf.owner_id, bf.file_name \
         FROM books.books b JOIN books.book_formats bf ON bf.id = b.cover_format_id \
         JOIN books.libraries l ON l.id = b.library_id \
         WHERE b.id = $1 AND (l.is_shared OR l.owner_id = $2)",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| BooksError::NotFound(format!("Livre {id}")))?;

    let fmt_tmp = Fmt { format: format.clone(), storage_path, local_cache_path, owner_id };
    let path = resolve_path(&state, &fmt_tmp)?;
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| BooksError::Storage(e.to_string()))?;
    let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file));
    let ct = match format.as_str() {
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "cb7" => "application/x-7z-compressed",
        "cbr" => "application/vnd.comicbook-rar",
        _ => "application/zip",
    };
    let safe = file_name.replace(['"', '\\', '\n', '\r'], "_");
    Ok((
        [
            (header::CONTENT_TYPE, ct.to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{safe}\"")),
        ],
        body,
    )
        .into_response())
}
