//! Attaching format codes to book-list rows in Rust.
//!
//! The listings return a `formats: Vec<String>` per book. On PostgreSQL that was
//! a correlated `ARRAY(SELECT ...)` subquery; array aggregation has no shared
//! spelling across the three engines (`array_agg` / `JSON_ARRAYAGG` /
//! `json_group_array`), so instead the rows are fetched without it and their
//! formats are filled here from a single grouped query.

use std::collections::HashMap;

use kubuno_db::{DbPool, DbQueryBuilder};
use uuid::Uuid;

/// A list row that carries a book id and an initially-empty `formats` field.
pub trait WithFormats {
    fn book_id(&self) -> Uuid;
    fn set_formats(&mut self, formats: Vec<String>);
}

/// Fills each row's `formats` with its book's format codes, ordered, in one
/// query. A no-op for an empty slice.
pub async fn attach<T: WithFormats>(db: &DbPool, rows: &mut [T]) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }
    let ids: Vec<Uuid> = rows.iter().map(WithFormats::book_id).collect();
    let mut qb = DbQueryBuilder::new(
        db.backend(),
        "SELECT book_id, format FROM books.book_formats WHERE book_id",
    );
    qb.push_in(ids);
    qb.push_order_by("book_id, format");
    let pairs = qb.fetch_all_as::<(Uuid, String)>(db).await?;

    let mut by_book: HashMap<Uuid, Vec<String>> = HashMap::new();
    for (book_id, format) in pairs {
        by_book.entry(book_id).or_default().push(format);
    }
    for row in rows.iter_mut() {
        if let Some(formats) = by_book.remove(&row.book_id()) {
            row.set_formats(formats);
        }
    }
    Ok(())
}
