-- Makes the per-user restrictions of migration 000005 enforceable everywhere.
--
-- 000005 shipped `user_restrictions` (allowed libraries + age ceiling) and two
-- helpers, `lib_allowed()` and `age_ok()`. `age_ok()` answers TRUE for any row
-- whose `age_rating` is NULL — and since nothing ever populated `age_rating`,
-- an age ceiling filtered exactly nothing. A parental control that lets
-- everything through is worse than none: it states a protection that does not
-- exist.
--
-- Two things were missing, both added here.
--
-- 1. `age_limited(uid)` — whether the reader is a RESTRICTED account at all.
--    Without it, "block unrated content" would empty the library for everybody,
--    because unrated is the normal state of a freshly scanned collection. The
--    rule has to apply only to accounts that were given a ceiling.
--
-- 2. `content_ok(uid, rating, block_unrated)` — the decision itself, with the
--    unrated case made explicit and configurable. `block_unrated` is passed by
--    the caller (it is an instance setting, never user input) rather than read
--    from a table, so the function stays free of hidden state and the module
--    keeps one single source for its settings.
--
-- `age_ok()` is left untouched: it belongs to an APPLIED migration, other
-- queries still call it, and replacing its meaning under them is exactly the
-- kind of silent change this file exists to avoid.

-- TRUE when the account carries an age ceiling. Absent row, or a row with a
-- NULL ceiling, means "no parental control for this reader".
CREATE OR REPLACE FUNCTION books.age_limited(uid uuid) RETURNS boolean AS $$
    SELECT COALESCE(
        (SELECT age_max IS NOT NULL FROM books.user_restrictions WHERE user_id = uid),
        false)
$$ LANGUAGE sql STABLE;

-- Whether a piece of content clears the reader's ceiling.
--   ar IS NULL (unrated) → allowed, unless the instance blocks unrated content
--                          AND the reader is a restricted account.
--   otherwise            → the reader's ceiling must reach the required age.
CREATE OR REPLACE FUNCTION books.content_ok(uid uuid, ar int, block_unrated boolean)
RETURNS boolean AS $$
    SELECT CASE
        WHEN ar IS NULL THEN NOT (block_unrated AND books.age_limited(uid))
        ELSE COALESCE(
            (SELECT age_max FROM books.user_restrictions WHERE user_id = uid),
            2147483647) >= ar
    END
$$ LANGUAGE sql STABLE;

-- The restriction lookup now runs once per row of every listing; without this
-- the planner re-scans the table for each candidate book.
CREATE INDEX IF NOT EXISTS idx_books_restrictions_age
    ON books.user_restrictions (user_id)
    WHERE age_max IS NOT NULL;
