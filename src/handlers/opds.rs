// OPDS 1.2 catalog feeds (Atom XML) so external reading apps can browse and download.
// NOTE: external OPDS apps authenticate with HTTP Basic; that requires the core proxy to accept
// Basic auth and inject the user — these feeds work today behind the standard session/Bearer auth.
use axum::{
    extract::{Extension, Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    errors::BooksError,
    middleware::auth::AuthUser,
    models::content::{BookListItem, Series},
    state::AppState,
};

const OPDS_CT: &str = "application/atom+xml; charset=utf-8";
const NAV: &str = "application/atom+xml;profile=opds-catalog;kind=navigation";
const ACQ: &str = "application/atom+xml;profile=opds-catalog;kind=acquisition";
const BASE: &str = "/api/v1/books/opds";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn feed(self_href: &str, title: &str, body: &str) -> Response {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>{self_href}</id>
  <title>{title}</title>
  <updated>2026-01-01T00:00:00Z</updated>
  <link rel="self" href="{self_href}" type="{NAV}"/>
  <link rel="start" href="{BASE}" type="{NAV}"/>
{body}</feed>"#
    );
    ([(header::CONTENT_TYPE, OPDS_CT)], xml).into_response()
}

fn nav_entry(title: &str, content: &str, href: &str) -> String {
    format!(
        "  <entry><id>{href}</id><title>{}</title><content type=\"text\">{}</content>\
         <link rel=\"subsection\" href=\"{href}\" type=\"{NAV}\"/></entry>\n",
        esc(title), esc(content)
    )
}

fn book_entry(b: &BookListItem) -> String {
    let id = b.id;
    let download_type = match b.formats.first().map(|s| s.as_str()) {
        Some("pdf") => "application/pdf",
        Some("epub") => "application/epub+zip",
        _ => "application/zip",
    };
    format!(
        "  <entry><id>urn:kubuno:book:{id}</id><title>{}</title>\
         <link rel=\"http://opds-spec.org/image\" href=\"/api/v1/books/books/{id}/cover\" type=\"image/jpeg\"/>\
         <link rel=\"http://opds-spec.org/image/thumbnail\" href=\"/api/v1/books/books/{id}/cover\" type=\"image/jpeg\"/>\
         <link rel=\"http://opds-spec.org/acquisition\" href=\"/api/v1/books/books/{id}/download\" type=\"{download_type}\"/>\
         </entry>\n",
        esc(&b.title)
    )
}

pub async fn root(State(_s): State<AppState>, Extension(_u): Extension<AuthUser>) -> Response {
    let mut body = String::new();
    body.push_str(&nav_entry("Séries", "Toutes les séries", &format!("{BASE}/series")));
    body.push_str(&nav_entry("Récents", "Livres ajoutés récemment", &format!("{BASE}/recent")));
    feed(BASE, "Kubuno Books", &body)
}

pub async fn series_nav(State(state): State<AppState>, Extension(user): Extension<AuthUser>) -> Result<Response, BooksError> {
    let rows = sqlx::query_as::<_, Series>(
        "SELECT s.* FROM books.series s JOIN books.libraries l ON l.id = s.library_id \
         WHERE (l.is_shared OR l.owner_id = $1) ORDER BY s.sort_name NULLS LAST, s.name",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    let mut body = String::new();
    for s in &rows {
        body.push_str(&nav_entry(&s.name, &format!("{} livre(s)", s.book_count), &format!("{BASE}/series/{}", s.id)));
    }
    Ok(feed(&format!("{BASE}/series"), "Séries", &body))
}

async fn books_query(state: &AppState, user_id: Uuid, extra: &str, bind_id: Option<Uuid>) -> Result<Vec<BookListItem>, BooksError> {
    let sql = format!(
        "SELECT b.id, b.library_id, b.series_id, b.title, b.sort_title, b.series_index, b.page_count, b.cover_format_id, b.added_at, \
                COALESCE(ARRAY(SELECT f.format FROM books.book_formats f WHERE f.book_id = b.id ORDER BY f.format), '{{}}') AS formats \
         FROM books.books b JOIN books.libraries l ON l.id = b.library_id \
         WHERE (l.is_shared OR l.owner_id = $1) {extra}"
    );
    let mut q = sqlx::query_as::<_, BookListItem>(&sql).bind(user_id);
    if let Some(id) = bind_id {
        q = q.bind(id);
    }
    Ok(q.fetch_all(&state.db).await?)
}

pub async fn series_acq(State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>) -> Result<Response, BooksError> {
    let books = books_query(&state, user.id, "AND b.series_id = $2 ORDER BY b.series_index NULLS LAST, b.title", Some(id)).await?;
    let mut body = String::new();
    for b in &books {
        body.push_str(&book_entry(b));
    }
    Ok(feed_acq(&format!("{BASE}/series/{id}"), "Série", &body))
}

#[derive(Debug, Deserialize)]
pub struct RecentQuery {
    pub limit: Option<i64>,
}

pub async fn recent_acq(State(state): State<AppState>, Extension(user): Extension<AuthUser>, Query(q): Query<RecentQuery>) -> Result<Response, BooksError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let books = books_query(&state, user.id, &format!("ORDER BY b.added_at DESC LIMIT {limit}"), None).await?;
    let mut body = String::new();
    for b in &books {
        body.push_str(&book_entry(b));
    }
    Ok(feed_acq(&format!("{BASE}/recent"), "Ajoutés récemment", &body))
}

fn feed_acq(self_href: &str, title: &str, body: &str) -> Response {
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>{self_href}</id>
  <title>{title}</title>
  <updated>2026-01-01T00:00:00Z</updated>
  <link rel="self" href="{self_href}" type="{ACQ}"/>
  <link rel="start" href="{BASE}" type="{NAV}"/>
{body}</feed>"#
    );
    ([(header::CONTENT_TYPE, OPDS_CT)], xml).into_response()
}
