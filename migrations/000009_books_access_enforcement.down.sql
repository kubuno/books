DROP INDEX IF EXISTS books.idx_books_restrictions_age;
DROP FUNCTION IF EXISTS books.content_ok(uuid, int, boolean);
DROP FUNCTION IF EXISTS books.age_limited(uuid);
