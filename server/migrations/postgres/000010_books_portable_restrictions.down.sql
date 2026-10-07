-- Revert library_ids to uuid[]. The helper functions are not recreated here:
-- they are dead code the application no longer calls.
ALTER TABLE books.user_restrictions
    ALTER COLUMN library_ids TYPE uuid[]
    USING ARRAY(SELECT jsonb_array_elements_text(library_ids)::uuid);
