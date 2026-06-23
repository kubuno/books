// Online metadata providers (free, no API key): OpenLibrary + Google Books. Used to enrich a
// book's metadata ("download metadata"). Pure read-only outbound lookups.
use serde::Serialize;
use serde_json::Value;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub source:      String,
    pub title:       String,
    pub authors:     Vec<String>,
    pub publisher:   Option<String>,
    pub date:        Option<String>,
    pub isbn:        Option<String>,
    pub description: Option<String>,
    pub language:    Option<String>,
    pub tags:        Vec<String>,
    pub cover_url:   Option<String>,
}

const UA: &str = "KubunoBooks/0.1 (+https://github.com/kubuno/books)";

pub async fn search(state: &AppState, query: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    if let Ok(v) = openlibrary(state, query).await {
        out.extend(v);
    }
    if let Ok(v) = google_books(state, query).await {
        out.extend(v);
    }
    out.truncate(20);
    out
}

async fn openlibrary(state: &AppState, query: &str) -> Result<Vec<Candidate>, reqwest::Error> {
    let url = "https://openlibrary.org/search.json";
    let resp: Value = state
        .http
        .get(url)
        .query(&[("q", query), ("limit", "8")])
        .header("User-Agent", UA)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await?
        .json()
        .await?;
    let docs = resp.get("docs").and_then(|d| d.as_array()).cloned().unwrap_or_default();
    Ok(docs
        .iter()
        .map(|d| {
            let str_arr = |k: &str| {
                d.get(k)
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<_>>())
                    .unwrap_or_default()
            };
            let cover_url = d
                .get("cover_i")
                .and_then(|v| v.as_i64())
                .map(|id| format!("https://covers.openlibrary.org/b/id/{id}-L.jpg"));
            Candidate {
                source: "openlibrary".into(),
                title: d.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                authors: str_arr("author_name"),
                publisher: str_arr("publisher").into_iter().next(),
                date: d.get("first_publish_year").and_then(|v| v.as_i64()).map(|y| y.to_string()),
                isbn: str_arr("isbn").into_iter().next(),
                description: None,
                language: str_arr("language").into_iter().next(),
                tags: str_arr("subject").into_iter().take(8).collect(),
                cover_url,
            }
        })
        .filter(|c| !c.title.is_empty())
        .collect())
}

async fn google_books(state: &AppState, query: &str) -> Result<Vec<Candidate>, reqwest::Error> {
    let url = "https://www.googleapis.com/books/v1/volumes";
    let resp: Value = state
        .http
        .get(url)
        .query(&[("q", query), ("maxResults", "8")])
        .header("User-Agent", UA)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await?
        .json()
        .await?;
    let items = resp.get("items").and_then(|d| d.as_array()).cloned().unwrap_or_default();
    Ok(items
        .iter()
        .filter_map(|it| it.get("volumeInfo"))
        .map(|vi| {
            let arr = |k: &str| {
                vi.get(k)
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect::<Vec<_>>())
                    .unwrap_or_default()
            };
            let isbn = vi
                .get("industryIdentifiers")
                .and_then(|v| v.as_array())
                .and_then(|a| {
                    a.iter()
                        .find(|i| i.get("type").and_then(|t| t.as_str()) == Some("ISBN_13"))
                        .or_else(|| a.first())
                })
                .and_then(|i| i.get("identifier").and_then(|x| x.as_str()).map(String::from));
            let cover_url = vi
                .get("imageLinks")
                .and_then(|l| l.get("thumbnail").or_else(|| l.get("smallThumbnail")))
                .and_then(|v| v.as_str())
                .map(|s| s.replace("http://", "https://"));
            Candidate {
                source: "google".into(),
                title: vi.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                authors: arr("authors"),
                publisher: vi.get("publisher").and_then(|v| v.as_str()).map(String::from),
                date: vi.get("publishedDate").and_then(|v| v.as_str()).map(String::from),
                isbn,
                description: vi.get("description").and_then(|v| v.as_str()).map(String::from),
                language: vi.get("language").and_then(|v| v.as_str()).map(String::from),
                tags: arr("categories"),
                cover_url,
            }
        })
        .filter(|c| !c.title.is_empty())
        .collect())
}

// ── Series-level metadata (presentation page) ─────────────────────────────────────
// A series rarely has its own ISBN; the best free sources for a *presentation* (synopsis,
// genres, artwork) are Wikipedia (rich summaries, no key) and Google Books (by series name).

#[derive(Debug, Clone, Serialize)]
pub struct SeriesCandidate {
    pub source:      String,
    pub title:       String,
    pub description: Option<String>,
    pub publisher:   Option<String>,
    pub authors:     Vec<String>,
    pub genres:      Vec<String>,
    pub cover_url:   Option<String>,
}

/// Search the web for series information. `name` = series title; `hint` = an optional
/// member-book title / author used to disambiguate. Tries Wikipedia (fr then en) +
/// Google Books. Returns ranked candidates (richest description first).
pub async fn search_series(state: &AppState, name: &str, hint: Option<&str>) -> Vec<SeriesCandidate> {
    let mut out = Vec::new();
    for lang in ["fr", "en"] {
        if let Ok(Some(c)) = wikipedia_series(state, lang, name).await {
            out.push(c);
        }
    }
    // Google Books: search the series name (+ hint) — descriptions are per-volume but
    // often summarise the series; useful as a fallback.
    let gq = match hint {
        Some(h) if !h.is_empty() => format!("{name} {h}"),
        _ => name.to_string(),
    };
    if let Ok(v) = google_books(state, &gq).await {
        for c in v.into_iter().filter(|c| c.description.is_some()).take(3) {
            out.push(SeriesCandidate {
                source: c.source, title: c.title, description: c.description,
                publisher: c.publisher, authors: c.authors, genres: c.tags, cover_url: c.cover_url,
            });
        }
    }
    // Keep insertion order (Wikipedia fr → en → Google Books): a French-first preference
    // for the auto-enrichment, which takes the first candidate.
    out.truncate(10);
    out
}

/// One Wikipedia summary for a query (best matching article in `lang`).
async fn wikipedia_series(state: &AppState, lang: &str, name: &str) -> Result<Option<SeriesCandidate>, reqwest::Error> {
    // One call: full-text search → top page, with intro extract + thumbnail.
    let url = format!("https://{lang}.wikipedia.org/w/api.php");
    let resp: Value = state.http.get(&url)
        .query(&[
            ("action", "query"), ("format", "json"),
            ("prop", "extracts|pageimages"),
            ("exintro", "1"), ("explaintext", "1"),
            ("piprop", "thumbnail"), ("pithumbsize", "480"),
            ("generator", "search"), ("gsrsearch", name), ("gsrlimit", "1"),
            ("redirects", "1"),
        ])
        .header("User-Agent", UA)
        .timeout(std::time::Duration::from_secs(8))
        .send().await?
        .json().await?;
    let pages = resp.pointer("/query/pages").and_then(|p| p.as_object());
    let Some(page) = pages.and_then(|m| m.values().next()) else { return Ok(None) };
    let extract = page.get("extract").and_then(|v| v.as_str()).map(|s| s.trim().to_string())
        .filter(|s| s.len() > 40);
    let Some(description) = extract else { return Ok(None) };
    let title = page.get("title").and_then(|v| v.as_str()).unwrap_or(name).to_string();
    let cover_url = page.pointer("/thumbnail/source").and_then(|v| v.as_str()).map(String::from);
    Ok(Some(SeriesCandidate {
        source: format!("wikipedia:{lang}"),
        title, description: Some(description),
        publisher: None, authors: Vec::new(), genres: Vec::new(), cover_url,
    }))
}

/// Download an image URL and store it as a series' custom cover (best-effort).
pub async fn download_series_cover(state: &AppState, series_id: uuid::Uuid, url: &str) -> bool {
    let Ok(resp) = state.http.get(url).header("User-Agent", UA).timeout(std::time::Duration::from_secs(10)).send().await else {
        return false;
    };
    let Ok(bytes) = resp.bytes().await else { return false };
    let raw = bytes.to_vec();
    let jpg = match tokio::task::spawn_blocking(move || crate::services::decode::thumbnail(&raw, 480)).await {
        Ok(Ok(j)) => j,
        _ => return false,
    };
    state.storage.put(&format!("cache/cover/custom_series_{series_id}.jpg"), bytes::Bytes::from(jpg)).await.is_ok()
}

/// Auto-fill a series' presentation fields from the web (best-effort, fills empties only).
/// Returns true if anything was applied. Used after a scan and by the manual refresh.
pub async fn enrich_series(state: &AppState, series_id: uuid::Uuid) -> bool {
    let Some((name, description, genres)) = sqlx::query_as::<_, (String, Option<String>, Value)>(
        "SELECT name, description, genres FROM books.series WHERE id = $1",
    ).bind(series_id).fetch_optional(&state.db).await.ok().flatten() else { return false };

    let has_desc = description.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
    let has_genres = genres.as_array().map(|a| !a.is_empty()).unwrap_or(false);
    if has_desc && has_genres {
        return false; // already enriched
    }

    // A sample member-book title helps disambiguate the series.
    let hint = sqlx::query_scalar::<_, String>(
        "SELECT title FROM books.books WHERE series_id = $1 ORDER BY series_index NULLS LAST, title LIMIT 1",
    ).bind(series_id).fetch_optional(&state.db).await.ok().flatten();

    let Some(best) = search_series(state, &name, hint.as_deref()).await.into_iter().next() else {
        return false;
    };

    let new_desc = if has_desc { description } else { best.description };
    let new_genres = if has_genres || best.genres.is_empty() { genres } else { serde_json::json!(best.genres) };
    let _ = sqlx::query(
        "UPDATE books.series SET description = COALESCE($2, description), genres = $3, \
         publisher = COALESCE(publisher, $4), updated_at = now() WHERE id = $1",
    ).bind(series_id).bind(&new_desc).bind(&new_genres).bind(&best.publisher)
     .execute(&state.db).await;

    // Fetch a series artwork only when the books didn't already provide a cover.
    if let Some(url) = best.cover_url {
        let has_cover = sqlx::query_scalar::<_, Option<uuid::Uuid>>(
            "SELECT cover_format_id FROM books.series WHERE id = $1",
        ).bind(series_id).fetch_optional(&state.db).await.ok().flatten().flatten().is_some();
        let has_custom = state.storage.get(&format!("cache/cover/custom_series_{series_id}.jpg")).await.is_ok();
        if !has_cover && !has_custom {
            download_series_cover(state, series_id, &url).await;
        }
    }
    true
}

/// Enrich every series of a library that still lacks a description (background, best-effort).
pub async fn enrich_library_series(state: &AppState, library_id: uuid::Uuid) {
    let ids = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM books.series WHERE library_id = $1 AND (description IS NULL OR description = '')",
    ).bind(library_id).fetch_all(&state.db).await.unwrap_or_default();
    for id in ids {
        let _ = enrich_series(state, id).await;
    }
}

/// Download an image URL and store it as a book's custom cover (best-effort).
pub async fn download_cover(state: &AppState, book_id: uuid::Uuid, url: &str) -> bool {
    let Ok(resp) = state.http.get(url).header("User-Agent", UA).timeout(std::time::Duration::from_secs(10)).send().await else {
        return false;
    };
    let Ok(bytes) = resp.bytes().await else { return false };
    let raw = bytes.to_vec();
    let jpg = match tokio::task::spawn_blocking(move || crate::services::decode::thumbnail(&raw, 480)).await {
        Ok(Ok(j)) => j,
        _ => return false,
    };
    state
        .storage
        .put(&format!("cache/cover/custom_{book_id}.jpg"), bytes::Bytes::from(jpg))
        .await
        .is_ok()
}
