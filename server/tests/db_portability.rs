//! Runs the books module's own migrations and the write/read paths its handlers
//! use against a real server of **each** engine, from a single compiled binary —
//! the proof that the engine is a run-time choice, not a build-time one.
//!
//! The scanner reads the `drive`/`core` schemas over the shared database and
//! cannot run in isolation; this binary exercises the parts that are the
//! module's own: the content chain (library → series → book → format), reading
//! progress (an upsert merged in Rust), the per-user restriction (a `uuid[]`
//! turned into a JSON array) and the access predicates assembled in
//! `services::access` (library visibility + the age gate).
//!
//! * SQLite always runs (a temp file, no server).
//! * PostgreSQL runs when `KUBUNO_PG_TEST_URL` points at a throwaway database.
//! * MySQL/MariaDB runs when `KUBUNO_MYSQL_TEST_URL` does.
//!
//! ```sh
//! KUBUNO_PG_TEST_URL=postgres://u:p@127.0.0.1:5432/kubuno_test \
//! KUBUNO_MYSQL_TEST_URL=mysql://u:p@127.0.0.1:3306/books \
//!   cargo test --test db_portability
//! ```

use kubuno_books::services::access::Restriction;
use kubuno_books::SCHEMA;
use kubuno_db::dialect::Assign;
use kubuno_db::{new_id, params, DbPool, DbQueryBuilder};
use uuid::Uuid;

fn base_settings(engine: &str) -> kubuno_db::DbSettings {
    kubuno_db::DbSettings {
        engine: engine.to_string(),
        url: None,
        host: None,
        port: None,
        user: None,
        password: None,
        database: None,
        path: None,
        max_connections: 4,
        min_connections: 0,
        connect_timeout: std::time::Duration::from_secs(10),
        run_migrations: true,
        schema_prefix: None,
    }
}

/// Migrations run one at a time: PostgreSQL and MySQL suites may share a server.
static EXCLUSIVE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn migrated_pool(settings: kubuno_db::DbSettings) -> (DbPool, impl Sized) {
    let guard = EXCLUSIVE.lock().await;
    let pool = kubuno_db::connect(&settings, SCHEMA).await.expect("connect");
    kubuno_db::migrations!(
        "./migrations/postgres",
        "./migrations/mysql",
        "./migrations/sqlite",
    )
    .run(&pool, SCHEMA)
    .await
    .expect("migrations");
    (pool, guard)
}

// ── Direct writes mirroring the handlers ─────────────────────────────────────

async fn insert_library(pool: &DbPool, owner: Uuid, name: &str) -> Uuid {
    let id = new_id();
    pool.execute(
        "INSERT INTO books.libraries (id, owner_id, name, lib_type, path, is_shared, settings) \
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
        params![id, owner, name, "comics", "/tmp/lib", true, serde_json::json!({})],
    )
    .await
    .expect("insert library");
    id
}

async fn insert_series(pool: &DbPool, library: Uuid, owner: Uuid, folder: Uuid, name: &str) -> Uuid {
    let id = new_id();
    pool.execute(
        "INSERT INTO books.series (id, library_id, owner_id, name, folder_id, genres, tags, metadata) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        params![id, library, owner, name, folder, serde_json::json!([]), serde_json::json!([]), serde_json::json!({})],
    )
    .await
    .expect("insert series");
    id
}

#[allow(clippy::too_many_arguments)]
async fn insert_book(
    pool: &DbPool,
    library: Uuid,
    series: Option<Uuid>,
    owner: Uuid,
    folder: Uuid,
    title: &str,
    book_key: &str,
    age_rating: Option<i32>,
    authors: serde_json::Value,
    tags: serde_json::Value,
) -> Uuid {
    let id = new_id();
    pool.execute(
        "INSERT INTO books.books \
           (id, library_id, series_id, owner_id, folder_id, title, sort_title, book_key, \
            age_rating, identifiers, authors, tags, metadata) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
        params![id, library, series, owner, folder, title, title, book_key,
                age_rating, serde_json::json!({}), authors, tags, serde_json::json!({})],
    )
    .await
    .expect("insert book");
    id
}

async fn insert_format(pool: &DbPool, book: Uuid, owner: Uuid, format: &str) -> Uuid {
    let id = new_id();
    pool.execute(
        "INSERT INTO books.book_formats \
           (id, book_id, owner_id, format, file_id, file_name, storage_path, size_bytes, format_metadata) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        params![id, book, owner, format, new_id(), format!("f.{format}"), "[Drive]/f", 10i64, serde_json::json!({})],
    )
    .await
    .expect("insert format");
    id
}

/// The listing predicate the handlers build, as a boolean over one book id.
async fn book_is_readable(
    pool: &DbPool,
    restriction: &Restriction,
    reader: Uuid,
    book: Uuid,
    block_unrated: bool,
) -> bool {
    let mut qb = DbQueryBuilder::new(
        pool.backend(),
        "SELECT b.id FROM books.books b JOIN books.libraries l ON l.id = b.library_id WHERE b.id = ",
    );
    qb.push_bind(book).push(" AND ");
    restriction.push_readable_book(&mut qb, reader, block_unrated);
    qb.fetch_optional_scalar::<Uuid>(pool).await.expect("readable").is_some()
}

// ── The suite, run once per engine ───────────────────────────────────────────

async fn full_suite(pool: &DbPool) {
    let owner = new_id();
    let reader = new_id(); // a different account that only sees shared libraries

    // ── Content chain ──
    let library = insert_library(pool, owner, "Comics").await;
    let folder = new_id();
    let series = insert_series(pool, library, owner, folder, "A Series").await;
    let book = insert_book(
        pool, library, Some(series), owner, folder, "Volume 1", "vol1",
        None, serde_json::json!([{"name": "Jane Doe", "role": "author"}]), serde_json::json!(["scifi"]),
    ).await;
    let cbz = insert_format(pool, book, owner, "cbz").await;
    let _pdf = insert_format(pool, book, owner, "pdf").await;

    // Round-trip a full Book row (JSON columns, nullable date/rating decode).
    let title: (String,) = pool
        .fetch_one_as::<(String,)>("SELECT title FROM books.books WHERE id = $1", params![book])
        .await
        .expect("reselect book");
    assert_eq!(title.0, "Volume 1");

    // cover_format_id can be set and read back as a nullable UUID.
    pool.execute(
        "UPDATE books.books SET cover_format_id = $1 WHERE id = $2",
        params![cbz, book],
    )
    .await
    .expect("set cover");
    let cover = pool
        .fetch_one_as::<(Option<Uuid>,)>("SELECT cover_format_id FROM books.books WHERE id = $1", params![book])
        .await
        .expect("read cover");
    assert_eq!(cover.0, Some(cbz));

    // ── Formats attached in Rust (the portable replacement for ARRAY(...)) ──
    let mut fmts = pool
        .fetch_all_as::<(Uuid, String)>(
            "SELECT book_id, format FROM books.book_formats WHERE book_id = $1 ORDER BY format",
            params![book],
        )
        .await
        .expect("formats");
    fmts.sort_by(|a, b| a.1.cmp(&b.1));
    let codes: Vec<&str> = fmts.iter().map(|(_, f)| f.as_str()).collect();
    assert_eq!(codes, vec!["cbz", "pdf"]);

    // ── Reading progress: the merged upsert keeps a field left out ──
    let upsert = pool.backend().upsert(
        "books.read_progress",
        &["user_id", "book_id"],
        &[Assign::Incoming("page"), Assign::Incoming("location"), Assign::Incoming("completed"), Assign::Incoming("updated_at")],
    );
    pool.execute(
        &format!(
            "INSERT INTO books.read_progress (user_id, book_id, page, location, completed, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6){upsert}"
        ),
        params![owner, book, 5i32, Some("cfi"), false, chrono::Utc::now()],
    )
    .await
    .expect("progress insert");
    // Second write bumps the page but keeps the location.
    pool.execute(
        &format!(
            "INSERT INTO books.read_progress (user_id, book_id, page, location, completed, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6){upsert}"
        ),
        params![owner, book, 9i32, Some("cfi"), false, chrono::Utc::now()],
    )
    .await
    .expect("progress upsert");
    let prog = pool
        .fetch_one_as::<(i32, Option<String>, bool)>(
            "SELECT page, location, completed FROM books.read_progress WHERE user_id = $1 AND book_id = $2",
            params![owner, book],
        )
        .await
        .expect("progress read");
    assert_eq!(prog, (9, Some("cfi".to_string()), false));

    // ── Per-user restriction: uuid[] → JSON array round-trip ──
    // The reader is limited to a set that does NOT include our library.
    let other_lib = new_id();
    let restr_upsert = pool.backend().upsert(
        "books.user_restrictions",
        &["user_id"],
        &[Assign::Incoming("library_ids"), Assign::Incoming("age_max"), Assign::Incoming("updated_at")],
    );
    pool.execute(
        &format!(
            "INSERT INTO books.user_restrictions (user_id, library_ids, age_max, updated_at) \
             VALUES ($1,$2,$3,$4){restr_upsert}"
        ),
        params![reader, vec![other_lib], None::<i32>, chrono::Utc::now()],
    )
    .await
    .expect("restriction insert");

    let loaded = Restriction::load(pool, reader).await.expect("load restriction");
    assert_eq!(loaded.library_ids.as_deref(), Some(&[other_lib][..]), "uuid[] JSON round-trip");
    assert_eq!(loaded.age_max, None);

    // The shared library is visible in principle, but the restriction excludes it.
    assert!(
        !book_is_readable(pool, &loaded, reader, book, false).await,
        "a library outside the allowed set must be hidden"
    );

    // An unrestricted owner sees the book.
    let none = Restriction::default();
    assert!(book_is_readable(pool, &none, owner, book, false).await);

    // ── Age gate ──
    let adult = insert_book(
        pool, library, None, owner, new_id(), "Adult", "adult",
        Some(18), serde_json::json!([]), serde_json::json!([]),
    ).await;
    let _ = insert_format(pool, adult, owner, "cbz").await;
    let teen = Restriction { library_ids: None, age_max: Some(12) };
    assert!(
        !book_is_readable(pool, &teen, owner, adult, false).await,
        "an 18+ book must fail a 12 ceiling"
    );
    assert!(
        book_is_readable(pool, &teen, owner, book, false).await,
        "an unrated book passes a ceiling when unrated is not blocked"
    );
    assert!(
        !book_is_readable(pool, &teen, owner, book, true).await,
        "an unrated book is withheld when the instance blocks unrated"
    );

    // ── Collections: membership + DO NOTHING idempotency ──
    let coll = new_id();
    pool.execute(
        "INSERT INTO books.collections (id, owner_id, name, is_public) VALUES ($1,$2,$3,$4)",
        params![coll, owner, "My collection", true],
    )
    .await
    .expect("insert collection");
    let ignore = pool.backend().insert_ignore_prefix();
    let on_conflict = pool.backend().on_conflict_do_nothing(&["collection_id", "series_id"]);
    for _ in 0..2 {
        pool.execute(
            &format!(
                "INSERT {ignore}INTO books.collection_series (collection_id, series_id, position) \
                 VALUES ($1,$2,$3){on_conflict}"
            ),
            params![coll, series, 0i32],
        )
        .await
        .expect("collection membership");
    }
    let members = pool
        .fetch_scalar::<i64>(
            &format!(
                "SELECT {} FROM books.collection_series WHERE collection_id = $1",
                pool.backend().count_bigint("*")
            ),
            params![coll],
        )
        .await
        .expect("count members");
    assert_eq!(members, 1, "DO NOTHING must keep the membership at one");

    // ── Cascade delete: removing the library empties the chain ──
    pool.execute("DELETE FROM books.libraries WHERE id = $1", params![library])
        .await
        .expect("delete library");
    let remaining = pool
        .fetch_scalar::<i64>(
            &format!(
                "SELECT {} FROM books.books WHERE library_id = $1",
                pool.backend().count_bigint("*")
            ),
            params![library],
        )
        .await
        .expect("count remaining");
    assert_eq!(remaining, 0, "ON DELETE CASCADE must remove the books");
}

#[tokio::test]
async fn sqlite_from_the_one_binary() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut s = base_settings("sqlite");
    s.path = Some(dir.path().to_string_lossy().into_owned());
    let (pool, _keep) = migrated_pool(s).await;
    full_suite(&pool).await;
}

#[tokio::test]
async fn postgres_from_the_one_binary() {
    let Ok(url) = std::env::var("KUBUNO_PG_TEST_URL") else {
        eprintln!("skipping: KUBUNO_PG_TEST_URL not set");
        return;
    };
    let mut s = base_settings("postgres");
    s.url = Some(url);
    let (pool, _keep) = migrated_pool(s).await;
    full_suite(&pool).await;
}

#[tokio::test]
async fn mysql_from_the_one_binary() {
    let Ok(url) = std::env::var("KUBUNO_MYSQL_TEST_URL") else {
        eprintln!("skipping: KUBUNO_MYSQL_TEST_URL not set");
        return;
    };
    let mut s = base_settings("mysql");
    s.url = Some(url);
    let (pool, _keep) = migrated_pool(s).await;
    full_suite(&pool).await;
}
