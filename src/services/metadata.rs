// Apply embedded metadata (ComicInfo.xml / EPUB OPF) onto a book, filling EMPTY fields only so
// user edits are never clobbered. EPUB titles do replace the filename-derived title (much better).
use serde_json::json;
use uuid::Uuid;

use crate::{errors::BooksError, services::decode, state::AppState};

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n}") }
}

pub async fn apply_embedded(state: &AppState, book_id: Uuid) -> Result<bool, BooksError> {
    let row = sqlx::query_as::<_, (String, String, Option<String>, Uuid, serde_json::Value)>(
        "SELECT bf.format, bf.storage_path, bf.local_cache_path, bf.owner_id, l.settings \
         FROM books.book_formats bf \
         JOIN books.books b ON b.cover_format_id = bf.id \
         JOIN books.libraries l ON l.id = b.library_id WHERE b.id = $1",
    )
    .bind(book_id)
    .fetch_optional(&state.db)
    .await?;
    let Some((format, storage_path, local_cache_path, owner_id, settings)) = row else { return Ok(false) };

    // Respect the library's granular import flags.
    let getb = |ptr: &str, def: bool| settings.pointer(ptr).and_then(|v| v.as_bool()).unwrap_or(def);
    let is_comic = matches!(format.as_str(), "cbz" | "cbr" | "cb7");
    let import_on = if is_comic { getb("/metadata/import_comicinfo", true) }
                    else if format == "epub" { getb("/metadata/import_epub", true) }
                    else { true };
    if !import_on {
        return Ok(false);
    }
    let (allow_book, allow_series) = if is_comic {
        (getb("/metadata/comicinfo_book", true), getb("/metadata/comicinfo_series", true))
    } else if format == "epub" {
        (getb("/metadata/epub_book", true), getb("/metadata/epub_series", true))
    } else {
        (true, true)
    };
    let volume_in_title = getb("/metadata/comicinfo_volume_in_title", true);

    let path = decode::resolve_storage(&state.settings, &storage_path, local_cache_path.as_deref(), owner_id)?;
    let fmt2 = format.clone();
    let meta = tokio::task::spawn_blocking(move || decode::embedded_meta(&fmt2, &path))
        .await
        .map_err(|e| BooksError::Internal(anyhow::anyhow!(e)))?;
    let Some(m) = meta else { return Ok(false) };

    // Book-level fields (suppressed when book import is disabled).
    let description = allow_book.then(|| m.description.clone()).flatten();
    let isbn = allow_book.then(|| m.isbn.clone()).flatten();
    let pub_date = allow_book.then_some(m.date).flatten();
    let authors = allow_book.then(|| {
        json!(m.authors.iter().map(|(n, r)| json!({ "name": n, "role": r })).collect::<Vec<_>>())
    });
    let title_override = if allow_book {
        if format == "epub" {
            m.title.clone()
        } else if is_comic && volume_in_title {
            match (&m.series, m.number) {
                (Some(s), Some(n)) => Some(format!("{s} #{}", fmt_num(n))),
                _ => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    // Series-level fields (suppressed when series import is disabled).
    let publisher = allow_series.then(|| m.publisher.clone()).flatten();
    let language = allow_series.then(|| m.language.clone()).flatten();
    let tags = allow_series.then(|| json!(m.tags));
    let series_index = allow_series.then_some(m.number).flatten();
    let rdir = (allow_series && m.rtl).then(|| "rtl".to_string());

    sqlx::query(
        "UPDATE books.books SET \
           description       = COALESCE(description, $2), \
           publisher         = COALESCE(publisher, $3), \
           published_date    = COALESCE(published_date, $4), \
           language          = COALESCE(language, $5), \
           isbn              = COALESCE(isbn, $6), \
           authors           = CASE WHEN authors = '[]'::jsonb AND $7::jsonb IS NOT NULL THEN $7 ELSE authors END, \
           tags              = CASE WHEN tags = '[]'::jsonb AND $8::jsonb IS NOT NULL THEN $8 ELSE tags END, \
           series_index      = COALESCE(series_index, $9), \
           reading_direction = COALESCE(reading_direction, $10), \
           title             = CASE WHEN $11::text IS NOT NULL THEN $11 ELSE title END, \
           updated_at = now() \
         WHERE id = $1",
    )
    .bind(book_id)
    .bind(description)
    .bind(publisher)
    .bind(pub_date)
    .bind(language)
    .bind(isbn)
    .bind(&authors)
    .bind(&tags)
    .bind(series_index)
    .bind(rdir)
    .bind(title_override)
    .execute(&state.db)
    .await?;
    Ok(true)
}

/// Import embedded metadata for all books in a library that still lack it (best-effort).
pub async fn import_library(state: &AppState, library_id: Uuid) {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM books.books WHERE library_id = $1 AND description IS NULL AND authors = '[]'::jsonb",
    )
    .bind(library_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    for id in ids {
        let _ = apply_embedded(state, id).await;
    }
}
