//! The two predicates every query returning content to a reader must carry —
//! library visibility and the age gate — decided in Rust.
//!
//! ## Why this moved out of SQL
//!
//! On PostgreSQL these rules were `LANGUAGE sql` functions (`lib_allowed`,
//! `content_ok`) reading a `uuid[]` restriction column with `= ANY(...)`.
//! Neither the functions nor arrays exist on MySQL or SQLite. So the reader's
//! restriction — the set of libraries they may see and their age ceiling — is
//! now loaded once with [`Restriction::load`] and folded into each query as
//! bound values, through a [`DbQueryBuilder`]. The decision is identical on the
//! three engines because it is made here, not by the database.
//!
//! ## Why a builder rather than spliced strings
//!
//! The visibility rule binds the reader's id and, when the reader is restricted,
//! the list of allowed library ids. Those are values, so they are *bound*, not
//! interpolated. Callers therefore assemble their listing with a
//! [`DbQueryBuilder`] and let these helpers push the predicate — column names
//! and keywords are `&'static str`, every value goes through `push_bind` /
//! `push_in`.

use kubuno_db::{params, DbPool, DbQueryBuilder, JsonVec};
use uuid::Uuid;

/// The age rating a series is judged on.
///
/// A series almost never carries one of its own: the scanner reads
/// `<AgeRating>` out of each FILE, so the rating lands on the books. An unrated
/// series therefore inherits the STRICTEST rating among its books (`max`, never
/// `min`: one adult volume makes the whole series adult). `s` is how the series
/// table is aliased in the listing queries.
pub const SERIES_RATING_EXPR: &str =
    "COALESCE(s.age_rating, (SELECT max(sb.age_rating) FROM books.books sb WHERE sb.series_id = s.id))";

/// The same expression for the generic cover lookup, where the series table is
/// aliased `t`.
pub const SERIES_RATING_EXPR_T: &str =
    "COALESCE(t.age_rating, (SELECT max(sb.age_rating) FROM books.books sb WHERE sb.series_id = t.id))";

/// A reader's per-account content restriction, loaded once per request.
#[derive(Debug, Clone, Default)]
pub struct Restriction {
    /// `None` = every library allowed; `Some(set)` = only these (an empty set
    /// means no library at all, a legitimate answer distinct from `None`).
    pub library_ids: Option<Vec<Uuid>>,
    /// `None` = no age ceiling; `Some(max)` = content must not exceed it.
    pub age_max: Option<i32>,
}

impl Restriction {
    /// Loads the reader's restriction. An absent row means unrestricted.
    pub async fn load(db: &DbPool, user_id: Uuid) -> Result<Restriction, sqlx::Error> {
        let row = db
            .fetch_optional_as::<(Option<JsonVec<Uuid>>, Option<i32>)>(
                "SELECT library_ids, age_max FROM books.user_restrictions WHERE user_id = $1",
                params![user_id],
            )
            .await?;
        Ok(match row {
            Some((libs, age_max)) => Restriction {
                library_ids: libs.map(JsonVec::into_inner),
                age_max,
            },
            None => Restriction::default(),
        })
    }

    /// Pushes the "library visible to this reader" predicate. The library table
    /// must be aliased `l`. Binds the reader id (and the allowed set, if any).
    pub fn push_visible_library(&self, qb: &mut DbQueryBuilder, user_id: Uuid) {
        qb.push("(l.is_shared OR l.owner_id = ").push_bind(user_id).push(")");
        if let Some(ids) = &self.library_ids {
            // Restricted: the library must be in the allowed set. An empty set
            // renders `IN (NULL)`, which matches nothing — exactly "no library".
            qb.push(" AND l.id").push_in(ids.iter().copied());
        }
    }

    /// Pushes the age-gate predicate on `col` (a `&'static str` column
    /// expression, e.g. `"b.age_rating"` or [`SERIES_RATING_EXPR`]).
    ///
    /// * No ceiling → always allowed.
    /// * Ceiling set → rated content must be within it; unrated content is
    ///   allowed unless the instance blocks unrated for restricted readers.
    pub fn push_age(&self, qb: &mut DbQueryBuilder, col: &str, block_unrated: bool) {
        match self.age_max {
            None => {
                // No ceiling for this reader: nothing to filter.
                qb.push("1 = 1");
            }
            Some(max) => {
                qb.push("(");
                if block_unrated {
                    // Unrated is withheld; rated must be within the ceiling.
                    qb.push(col).push(" IS NOT NULL AND ").push(col).push(" <= ").push_bind(max);
                } else {
                    // Unrated passes; rated must be within the ceiling.
                    qb.push(col).push(" IS NULL OR ").push(col).push(" <= ").push_bind(max);
                }
                qb.push(")");
            }
        }
    }

    /// Both rules at once, for listing books (rated on `b.age_rating`).
    pub fn push_readable_book(&self, qb: &mut DbQueryBuilder, user_id: Uuid, block_unrated: bool) {
        self.push_visible_library(qb, user_id);
        qb.push(" AND ");
        self.push_age(qb, "b.age_rating", block_unrated);
    }

    /// Both rules at once, for listing series (rated on the effective rating).
    pub fn push_readable_series(&self, qb: &mut DbQueryBuilder, user_id: Uuid, block_unrated: bool) {
        self.push_visible_library(qb, user_id);
        qb.push(" AND ");
        self.push_age(qb, SERIES_RATING_EXPR, block_unrated);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_db::dialect::Backend;

    fn qb() -> DbQueryBuilder {
        DbQueryBuilder::new(Backend::Postgres, "")
    }

    #[test]
    fn an_unrestricted_reader_sees_by_ownership_only() {
        let r = Restriction::default();
        let mut b = qb();
        r.push_visible_library(&mut b, Uuid::nil());
        assert!(b.as_sql().contains("l.is_shared OR l.owner_id ="));
        assert!(!b.as_sql().contains(" IN ("));
        assert_eq!(b.bind_count(), 1);
    }

    #[test]
    fn a_restricted_reader_is_limited_to_the_allowed_set() {
        let r = Restriction { library_ids: Some(vec![Uuid::nil(), Uuid::nil()]), age_max: None };
        let mut b = qb();
        r.push_visible_library(&mut b, Uuid::nil());
        assert!(b.as_sql().contains("l.id IN ("));
        // one reader id + two library ids
        assert_eq!(b.bind_count(), 3);
    }

    #[test]
    fn no_ceiling_filters_nothing() {
        let r = Restriction::default();
        let mut b = qb();
        r.push_age(&mut b, "b.age_rating", true);
        assert_eq!(b.as_sql().trim(), "1 = 1");
        assert_eq!(b.bind_count(), 0);
    }

    #[test]
    fn a_ceiling_admits_unrated_unless_the_instance_blocks_it() {
        let r = Restriction { library_ids: None, age_max: Some(12) };
        let mut open = qb();
        r.push_age(&mut open, "b.age_rating", false);
        assert!(open.as_sql().contains("b.age_rating IS NULL OR b.age_rating <="));

        let mut blocked = qb();
        r.push_age(&mut blocked, "b.age_rating", true);
        assert!(blocked.as_sql().contains("b.age_rating IS NOT NULL AND b.age_rating <="));
    }
}
