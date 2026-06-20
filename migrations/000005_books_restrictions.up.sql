-- Per-user access control: allowed libraries + maximum age rating (content restrictions).
CREATE TABLE books.user_restrictions (
    user_id      uuid PRIMARY KEY,
    library_ids  uuid[],          -- NULL = all libraries allowed
    age_max      int,             -- NULL = no age limit
    updated_at   timestamptz NOT NULL DEFAULT now()
);

-- A library is allowed when the user has no restriction or it is in their allowed set.
CREATE OR REPLACE FUNCTION books.lib_allowed(uid uuid, lib uuid) RETURNS boolean AS $$
    SELECT COALESCE(
        (SELECT library_ids IS NULL OR lib = ANY(library_ids)
         FROM books.user_restrictions WHERE user_id = uid),
        true)
$$ LANGUAGE sql STABLE;

-- A book passes the age gate when it has no rating or is within the user's max.
CREATE OR REPLACE FUNCTION books.age_ok(uid uuid, ar int) RETURNS boolean AS $$
    SELECT ar IS NULL OR COALESCE(
        (SELECT age_max FROM books.user_restrictions WHERE user_id = uid),
        2147483647) >= ar
$$ LANGUAGE sql STABLE;
