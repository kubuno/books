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
