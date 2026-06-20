CREATE SCHEMA IF NOT EXISTS books;

-- Shared trigger function: bump updated_at on every UPDATE.
CREATE OR REPLACE FUNCTION books.set_updated_at()
RETURNS TRIGGER AS $$
BEGIN NEW.updated_at = NOW(); RETURN NEW; END;
$$ LANGUAGE plpgsql;

CREATE TABLE IF NOT EXISTS books.libraries (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id        UUID,
    name            VARCHAR(255) NOT NULL,
    lib_type        VARCHAR(20)  NOT NULL
                        CHECK (lib_type IN ('books','comics','ebooks')),
    path            TEXT NOT NULL,
    icon            VARCHAR(50)  NOT NULL DEFAULT '📚',
    color           VARCHAR(7)   NOT NULL DEFAULT '#1a73e8',
    is_shared       BOOLEAN      NOT NULL DEFAULT TRUE,
    item_count      INTEGER      NOT NULL DEFAULT 0,
    last_scan_at    TIMESTAMPTZ,
    scan_status     VARCHAR(10)  NOT NULL DEFAULT 'idle'
                        CHECK (scan_status IN ('idle','scanning','error')),
    scan_error      TEXT,
    source_type     TEXT         NOT NULL DEFAULT 'filesystem',
    files_folder_id UUID,
    files_owner_id  UUID,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_books_libs_type ON books.libraries(lib_type);

DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger WHERE tgname = 'books_libraries_updated_at'
    ) THEN
        CREATE TRIGGER books_libraries_updated_at
            BEFORE UPDATE ON books.libraries
            FOR EACH ROW EXECUTE FUNCTION books.set_updated_at();
    END IF;
END $$;

-- Simple key/value settings store for the books module admin panel.
CREATE TABLE IF NOT EXISTS books.settings (
    key        TEXT        PRIMARY KEY,
    value      TEXT        NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
