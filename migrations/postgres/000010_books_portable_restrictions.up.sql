-- Portability migration for kubuno-db 0.6.0 (PostgreSQL side).
--
-- Two PostgreSQL-only shapes are retired so the module runs identically on
-- MySQL/MariaDB and SQLite, where the same logic now lives in Rust:
--
--   1. `user_restrictions.library_ids` moves from `uuid[]` to a JSONB array of
--      UUID strings. Arrays exist on PostgreSQL alone; the application reads and
--      writes this column whole as a JSON array (kubuno_db `JsonVec<Uuid>` /
--      `DbValue`), and the "is this library allowed" test is decided in Rust
--      from the decoded set rather than with `= ANY(...)`.
--
--   2. The SQL predicate helpers `lib_allowed`, `age_ok`, `content_ok` and
--      `age_limited` are dropped. They encoded the visibility and age-gate rules
--      as `LANGUAGE sql` functions no other engine has; the identical decision
--      is now assembled in `services::access` from the reader's decoded
--      restriction. Nothing calls them any more.

-- 1. uuid[] -> jsonb (NULL stays NULL = "every library allowed").
ALTER TABLE books.user_restrictions
    ALTER COLUMN library_ids TYPE jsonb USING to_jsonb(library_ids);

-- 2. Retire the predicate functions (superseded by the Rust access layer).
DROP FUNCTION IF EXISTS books.content_ok(uuid, int, boolean);
DROP FUNCTION IF EXISTS books.age_limited(uuid);
DROP FUNCTION IF EXISTS books.age_ok(uuid, int);
DROP FUNCTION IF EXISTS books.lib_allowed(uuid, uuid);
