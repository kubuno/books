//! The two predicates every query returning content to a reader must carry.
//!
//! ## Why this is a module and not a constant
//!
//! Restrictions used to be expressed as one `&'static str` living in
//! `handlers::content`, spelled `$1` because that happened to be where the
//! reader's id was bound there. Every other handler binds it somewhere else —
//! `media::visible_format` binds it at `$2` — so those handlers simply went
//! without, and the restriction stopped at the listings. The catalogue export,
//! the OPDS feed, the covers, the page images and the download route all
//! answered in full for a reader who was supposed to see one shelf.
//!
//! Making the parameter an ARGUMENT is what lets the same rule travel to a
//! query that binds its reader anywhere, which is the only reason the holes
//! could be closed at all.
//!
//! ## On splicing rather than binding
//!
//! Both helpers return SQL text built from a caller-chosen placeholder name and
//! a column name. Neither ever receives a value that came from a request: the
//! placeholders are literals written at the call site, the column names are
//! literals too, and `block_unrated` is an instance setting reduced to one of
//! two keywords by [`crate::config::instance::InstanceConfig::block_unrated_sql`].
//! The module assembles all of its SQL this way; the values themselves stay
//! bound.

/// A library is visible when it is shared or owned by the reader, AND the
/// reader's per-account library restriction allows it.
///
/// `user` is the placeholder the reader's id is bound to in the caller's query
/// (`"$1"`, `"$2"`…). The library table must be aliased `l`.
pub fn visible_library(user: &str) -> String {
    format!("(l.is_shared OR l.owner_id = {user}) AND books.lib_allowed({user}, l.id)")
}

/// A row clears the reader's age ceiling.
///
/// `col` is the qualified age column (`"b.age_rating"`, `"s.age_rating"`).
/// `block_unrated` is `"TRUE"` or `"FALSE"` — see the module docs above.
pub fn content_allowed(user: &str, col: &str, block_unrated: &str) -> String {
    format!("books.content_ok({user}, {col}, {block_unrated})")
}

/// Both rules at once, for the common case of listing books.
pub fn readable_book(user: &str, block_unrated: &str) -> String {
    format!(
        "{} AND {}",
        visible_library(user),
        content_allowed(user, "b.age_rating", block_unrated),
    )
}

/// The age rating a series is judged on.
///
/// A series almost never carries one of its own: the scanner reads
/// `<AgeRating>` out of each FILE, so the rating lands on the books. Judging a
/// series on its own empty column would mean that turning "block unrated
/// content" on empties the series view while the book view still shows the
/// rated books — the same library, answering two different things.
///
/// So an unrated series inherits the STRICTEST rating among its books. That is
/// the conservative direction: a collection containing one adult volume is
/// treated as adult, never the reverse. A series whose books are all unrated
/// stays unrated, and the instance setting decides what that means.
/// `alias` is how the series table is named in the caller's query — `s` in the
/// listings, `t` in the generic cover lookup, which is exactly why this takes an
/// argument rather than hard-coding one.
pub fn series_effective_rating(alias: &str) -> String {
    format!(
        "COALESCE({alias}.age_rating, \
         (SELECT max(sb.age_rating) FROM books.books sb WHERE sb.series_id = {alias}.id))"
    )
}

/// Both rules at once, for listing series.
pub fn readable_series(user: &str, block_unrated: &str) -> String {
    format!(
        "{} AND {}",
        visible_library(user),
        content_allowed(user, &series_effective_rating("s"), block_unrated),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The placeholder must appear in BOTH halves of the visibility rule.
    /// Getting one of the two wrong is invisible in testing — the query still
    /// runs, it just stops restricting.
    ///
    /// Asserted on the STRUCTURE, not on a count: an earlier version of this
    /// test expected the placeholder three times, which was simply one more
    /// than the rule has ever contained. A count says nothing about WHERE the
    /// placeholder landed, and it turns any future clause into a false alarm —
    /// the failure mode worth catching is a half that lost its reader, so that
    /// is what is checked.
    #[test]
    fn the_reader_placeholder_reaches_every_clause() {
        let sql = visible_library("$2");
        let halves: Vec<&str> = sql.split(" AND ").collect();
        assert_eq!(halves.len(), 2, "la règle de visibilité doit garder ses deux moitiés : {sql}");
        for half in halves {
            assert!(half.contains("$2"), "moitié sans le lecteur — elle ne restreint plus : {half}");
        }
        assert!(!sql.contains("$1"), "le placeholder du caller ne doit pas être codé en dur : {sql}");
    }

    #[test]
    fn a_readable_book_carries_the_library_rule_and_the_age_rule() {
        let sql = readable_book("$1", "TRUE");
        assert!(sql.contains("books.lib_allowed($1"));
        assert!(sql.contains("books.content_ok($1, b.age_rating, TRUE)"));
    }

    #[test]
    fn a_book_is_rated_on_its_own_column() {
        assert!(readable_book("$1", "FALSE").contains("b.age_rating"));
    }

    /// A series inherits the strictest rating of its books when it carries
    /// none — `max`, never `min`: one adult volume makes the series adult.
    #[test]
    fn an_unrated_series_inherits_the_strictest_rating_of_its_books() {
        let sql = readable_series("$1", "TRUE");
        assert!(sql.contains("COALESCE(s.age_rating"));
        assert!(sql.contains("max(sb.age_rating)"));
        assert!(!sql.contains("min(sb.age_rating)"));
    }
}
