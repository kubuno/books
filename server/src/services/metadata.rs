// Apply embedded metadata (ComicInfo.xml / EPUB OPF) onto a book, filling EMPTY fields only so
// user edits are never clobbered. EPUB titles do replace the filename-derived title (much better).
use kubuno_db::params;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{errors::BooksError, services::decode, state::AppState};

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { format!("{n}") }
}

/// Whether a JSON value is an empty array (`[]`) or absent — the state that lets
/// embedded metadata fill `authors` / `tags`.
fn is_empty_json_array(v: &Value) -> bool {
    v.as_array().map(|a| a.is_empty()).unwrap_or(true)
}

pub async fn apply_embedded(state: &AppState, book_id: Uuid) -> Result<bool, BooksError> {
    let row = state
        .db
        .fetch_optional_as::<(String, String, Option<String>, Uuid, Value, Value, Value)>(
            "SELECT bf.format, bf.storage_path, bf.local_cache_path, bf.owner_id, l.settings, \
                    b.authors, b.tags \
             FROM books.book_formats bf \
             JOIN books.books b ON b.cover_format_id = bf.id \
             JOIN books.libraries l ON l.id = b.library_id WHERE b.id = $1",
            params![book_id],
        )
        .await?;
    let Some((format, storage_path, local_cache_path, owner_id, settings, cur_authors, cur_tags)) = row
    else {
        return Ok(false);
    };

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

    // The age rating is what makes the parental control mean anything: until it
    // is filled, every book is unrated and an age ceiling filters nothing. It is
    // written only when the instance asks for it, and — like every other field
    // here — only into an EMPTY column, so an administrator's correction is
    // never overwritten by a rescan.
    let age_rating = state.instance().import_age_rating.then_some(m.age_rating).flatten();

    // Fill authors/tags only when the stored value is empty; decided in Rust so
    // the statement stays a plain assignment (no per-engine JSON comparison).
    let new_authors: Value = match authors {
        Some(a) if is_empty_json_array(&cur_authors) => a,
        _ => cur_authors,
    };
    let new_tags: Value = match tags {
        Some(t) if is_empty_json_array(&cur_tags) => t,
        _ => cur_tags,
    };

    state
        .db
        .execute(
            "UPDATE books.books SET \
               description       = COALESCE(description, $1), \
               publisher         = COALESCE(publisher, $2), \
               published_date    = COALESCE(published_date, $3), \
               language          = COALESCE(language, $4), \
               isbn              = COALESCE(isbn, $5), \
               authors           = $6, \
               tags              = $7, \
               series_index      = COALESCE(series_index, $8), \
               reading_direction = COALESCE(reading_direction, $9), \
               title             = COALESCE($10, title), \
               age_rating        = COALESCE(age_rating, $11), \
               updated_at        = $12 \
             WHERE id = $13",
            params![
                description,
                publisher,
                pub_date,
                language,
                isbn,
                new_authors,
                new_tags,
                series_index,
                rdir,
                title_override,
                age_rating,
                chrono::Utc::now(),
                book_id,
            ],
        )
        .await?;
    Ok(true)
}

/// Import embedded metadata for all books in a library that still lack it (best-effort).
///
/// NOTE on the age rating: the selection below is deliberately narrow — it only
/// picks books that were never enriched at all. So turning `import_age_rating`
/// ON does not retro-rate a library that was already scanned: it applies to
/// books indexed from then on. Widening it to "every unrated book" would mean
/// re-opening and re-decoding every unrated file on EVERY scan, forever, since
/// most files carry no rating at all and would stay selected. Existing books are
/// rated from the metadata editor instead.
pub async fn import_library(state: &AppState, library_id: Uuid) {
    // "Never enriched at all": no description and an empty authors array. The
    // empty-array test is done in Rust (JSON `=` has no portable spelling).
    let rows = state
        .db
        .fetch_all_as::<(Uuid, Value)>(
            "SELECT id, authors FROM books.books WHERE library_id = $1 AND description IS NULL",
            params![library_id],
        )
        .await
        .unwrap_or_default();
    for (id, authors) in rows {
        if is_empty_json_array(&authors) {
            let _ = apply_embedded(state, id).await;
        }
    }
}
