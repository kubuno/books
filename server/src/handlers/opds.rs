// OPDS 1.2 catalog feeds (Atom XML) so external reading apps can browse and download.
use axum::{
    extract::{Extension, Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use kubuno_db::DbQueryBuilder;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    errors::BooksError,
    middleware::auth::AuthUser,
    models::content::{BookListItem, Series},
    services::{access::Restriction, formats},
    state::AppState,
};

const OPDS_CT: &str = "application/atom+xml; charset=utf-8";
const NAV: &str = "application/atom+xml;profile=opds-catalog;kind=navigation";
const ACQ: &str = "application/atom+xml;profile=opds-catalog;kind=acquisition";
const BASE: &str = "/api/v1/books/opds";

const OPDS_BOOK_COLS: &str = "b.id, b.library_id, b.series_id, b.title, b.sort_title, \
    b.series_index, b.page_count, b.cover_format_id, b.added_at";

/// Instance policy gate, applied to every feed of this file.
fn opds_open(state: &AppState) -> Result<(), BooksError> {
    if state.instance().opds_enabled {
        Ok(())
    } else {
        Err(BooksError::Forbidden)
    }
}

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

/// One catalogue entry. The acquisition link is omitted entirely when downloads
/// are off.
fn book_entry(b: &BookListItem, allow_downloads: bool) -> String {
    let id = b.id;
    let download_type = match b.formats.first().map(|s| s.as_str()) {
        Some("pdf") => "application/pdf",
        Some("epub") => "application/epub+zip",
        _ => "application/zip",
    };
    let acquisition = if allow_downloads {
        format!(
            "<link rel=\"http://opds-spec.org/acquisition\" href=\"/api/v1/books/books/{id}/download\" type=\"{download_type}\"/>"
        )
    } else {
        String::new()
    };
    format!(
        "  <entry><id>urn:kubuno:book:{id}</id><title>{}</title>\
         <link rel=\"http://opds-spec.org/image\" href=\"/api/v1/books/books/{id}/cover\" type=\"image/jpeg\"/>\
         <link rel=\"http://opds-spec.org/image/thumbnail\" href=\"/api/v1/books/books/{id}/cover\" type=\"image/jpeg\"/>\
         {acquisition}</entry>\n",
        esc(&b.title)
    )
}

pub async fn root(State(state): State<AppState>, Extension(_u): Extension<AuthUser>) -> Result<Response, BooksError> {
    opds_open(&state)?;
    let mut body = String::new();
    body.push_str(&nav_entry("Séries", "Toutes les séries", &format!("{BASE}/series")));
    body.push_str(&nav_entry("Récents", "Livres ajoutés récemment", &format!("{BASE}/recent")));
    Ok(feed(BASE, "Kubuno Books", &body))
}

pub async fn series_nav(State(state): State<AppState>, Extension(user): Extension<AuthUser>) -> Result<Response, BooksError> {
    opds_open(&state)?;
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT s.* FROM books.series s JOIN books.libraries l ON l.id = s.library_id WHERE ",
    );
    restriction.push_readable_series(&mut qb, user.id, block);
    qb.push_order_by("(s.sort_name IS NULL), s.sort_name, s.name");
    let rows = qb.fetch_all_as::<Series>(&state.db).await?;
    let mut body = String::new();
    for s in &rows {
        body.push_str(&nav_entry(&s.name, &format!("{} livre(s)", s.book_count), &format!("{BASE}/series/{}", s.id)));
    }
    Ok(feed(&format!("{BASE}/series"), "Séries", &body))
}

/// The shared body of both acquisition feeds. `series`/`limit` are mutually
/// exclusive: a series feed orders by index, the recent feed by date with a cap.
async fn books_query(
    state: &AppState,
    user_id: Uuid,
    series: Option<Uuid>,
    limit: Option<i64>,
) -> Result<Vec<BookListItem>, BooksError> {
    let block = state.instance().block_unrated;
    let restriction = Restriction::load(&state.db, user_id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        format!("SELECT {OPDS_BOOK_COLS} FROM books.books b JOIN books.libraries l ON l.id = b.library_id WHERE "),
    );
    restriction.push_readable_book(&mut qb, user_id, block);
    if let Some(sid) = series {
        qb.push(" AND b.series_id = ").push_bind(sid);
        qb.push_order_by("(b.series_index IS NULL), b.series_index, b.title");
    } else {
        qb.push_order_by("b.added_at DESC");
        if let Some(l) = limit {
            qb.push_limit_offset(l, 0);
        }
    }
    let mut rows = qb.fetch_all_as::<BookListItem>(&state.db).await?;
    formats::attach(&state.db, &mut rows).await?;
    Ok(rows)
}

pub async fn series_acq(State(state): State<AppState>, Extension(user): Extension<AuthUser>, Path(id): Path<Uuid>) -> Result<Response, BooksError> {
    opds_open(&state)?;
    let allow_downloads = state.instance().allow_downloads;
    let books = books_query(&state, user.id, Some(id), None).await?;
    let mut body = String::new();
    for b in &books {
        body.push_str(&book_entry(b, allow_downloads));
    }
    Ok(feed_acq(&format!("{BASE}/series/{id}"), "Série", &body))
}

#[derive(Debug, Deserialize)]
pub struct RecentQuery {
    pub limit: Option<i64>,
}

pub async fn recent_acq(State(state): State<AppState>, Extension(user): Extension<AuthUser>, Query(q): Query<RecentQuery>) -> Result<Response, BooksError> {
    opds_open(&state)?;
    let allow_downloads = state.instance().allow_downloads;
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let books = books_query(&state, user.id, None, Some(limit)).await?;
    let mut body = String::new();
    for b in &books {
        body.push_str(&book_entry(b, allow_downloads));
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
