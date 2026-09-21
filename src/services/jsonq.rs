//! Portable JSON-array predicates, pushed into a [`DbQueryBuilder`].
//!
//! PostgreSQL expressed "this JSON array column contains …" with `@>` and
//! `jsonb_build_*`. MySQL and SQLite spell the same test with `JSON_CONTAINS` /
//! `json_each`. The value is always bound (never interpolated); each helper
//! pushes a prefix, the bind, and a suffix, so the placeholder lands exactly
//! where the expression needs it.

use kubuno_db::dialect::Backend;
use kubuno_db::DbQueryBuilder;

/// `col` (a JSON array of scalars) contains the string bound here — the portable
/// form of PostgreSQL's `col @> jsonb_build_array($n)`. `col` is a `&'static
/// str` column reference, never request data.
pub fn push_array_contains(qb: &mut DbQueryBuilder, col: &'static str, value: String) {
    match qb.backend() {
        Backend::Postgres => {
            qb.push(col).push(" @> jsonb_build_array(");
            qb.push_bind(value);
            qb.push(")");
        }
        Backend::MySql => {
            qb.push("JSON_CONTAINS(").push(col).push(", JSON_QUOTE(");
            qb.push_bind(value);
            qb.push("))");
        }
        Backend::Sqlite => {
            qb.push("EXISTS (SELECT 1 FROM json_each(").push(col).push(") WHERE value = ");
            qb.push_bind(value);
            qb.push(")");
        }
    }
}

/// `b.authors` (a JSON array of `{name, role}` objects) contains an object whose
/// `name` equals the string bound here.
pub fn push_author_name(qb: &mut DbQueryBuilder, value: String) {
    match qb.backend() {
        Backend::Postgres => {
            qb.push("b.authors @> jsonb_build_array(jsonb_build_object('name', ");
            qb.push_bind(value);
            qb.push("))");
        }
        Backend::MySql => {
            qb.push("JSON_CONTAINS(b.authors, JSON_OBJECT('name', ");
            qb.push_bind(value);
            qb.push("))");
        }
        Backend::Sqlite => {
            qb.push("EXISTS (SELECT 1 FROM json_each(b.authors) WHERE json_extract(value, '$.name') = ");
            qb.push_bind(value);
            qb.push(")");
        }
    }
}
