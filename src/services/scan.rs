// Library scan: a books library points at a Drive folder or a remote mount. We read the drive
// module's schema (drive.folders / drive.files) directly for local libraries, or call the core
// internal API for remote mounts. Files are grouped into series / books and
// upserted into the books schema. Idempotent: re-scanning reconciles additions and removals.
//
// NOTE: the drive.folders / drive.files / core.remote_mounts reads are cross-schema
// (PostgreSQL/MySQL only). A SQLite deployment scans nothing from those.
use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use kubuno_db::dialect::Assign;
use kubuno_db::{new_id, params, DbPool, DbQueryBuilder, DbTx};
use serde::Deserialize;
use uuid::Uuid;

use crate::{config::Settings, errors::BooksError, state::AppState};

const BOOK_EXTS: &[&str] = &["cbz", "cbr", "cb7", "pdf", "epub"];
/// Cover/primary format preference (image-based first).
const FORMAT_PRIORITY: &[&str] = &["cbz", "cb7", "cbr", "pdf", "epub"];

/// Portable "series cover from the first book" ordering (NULLS LAST spelled as a
/// leading `IS NULL` flag, no `NULLS LAST` keyword).
const COVER_ORDER_FIRST: &str =
    "(b.series_index IS NULL), b.series_index, (b.sort_title IS NULL), b.sort_title, b.title";
/// "…from the last book".
const COVER_ORDER_LAST: &str =
    "(b.series_index IS NULL), b.series_index DESC, (b.sort_title IS NULL), b.sort_title DESC, b.title DESC";

/// Reads a format-preference `ORDER BY` as a portable CASE (replacing
/// PostgreSQL's `array_position`).
fn format_priority_case() -> &'static str {
    "CASE f.format WHEN 'cbz' THEN 0 WHEN 'cb7' THEN 1 WHEN 'cbr' THEN 2 \
     WHEN 'pdf' THEN 3 WHEN 'epub' THEN 4 ELSE 99 END"
}

/// Instance-wide default metadata language (`books.settings.metadata_language`).
async fn global_meta_lang(db: &DbPool) -> Option<String> {
    let key = crate::services::settings_key_col(db.backend());
    db.fetch_optional_scalar::<String>(
        &format!("SELECT value FROM books.settings WHERE {key} = $1"),
        params!["metadata_language"],
    )
    .await
    .ok()
    .flatten()
    .filter(|s| !s.is_empty())
}

/// Upsert a row (minting the id in Rust) and read back the id of the row that
/// now exists — the natural-key row, whether it was inserted or already there.
/// The portable stand-in for `INSERT ... ON CONFLICT ... RETURNING id`.
async fn upsert_id(
    tx: &mut DbTx,
    insert_sql: &str,
    insert_params: Vec<kubuno_db::DbValue>,
    reselect_sql: &str,
    reselect_params: Vec<kubuno_db::DbValue>,
) -> Result<Uuid, sqlx::Error> {
    tx.execute(insert_sql, insert_params).await?;
    tx.fetch_optional_scalar::<Uuid>(reselect_sql, reselect_params)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

#[derive(sqlx::FromRow)]
struct LibRow {
    source_type:      String,
    files_folder_id:  Option<Uuid>,
    files_owner_id:   Option<Uuid>,
    remote_mount_id:  Option<String>,
    remote_mount_path: String,
    remote_owner_id:  Option<Uuid>,
    settings:         serde_json::Value,
}

#[derive(sqlx::FromRow)]
struct FolderRow {
    id:   Uuid,
    name: String,
    path: String,
}

#[derive(sqlx::FromRow, Clone)]
struct FileRow {
    id:           Uuid,
    folder_id:    Option<Uuid>,
    name:         String,
    extension:    Option<String>,
    size_bytes:   i64,
    storage_path: String,
    content_hash: Option<String>,
    updated_at:   DateTime<Utc>,
}

fn strip_ext(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// Convert a raw drive storage path `<owner>/files/<rel>` into the canonical
/// owner-agnostic form `[Drive]/<rel>`.
fn to_drive_canonical(storage_path: &str, owner: Uuid) -> String {
    let prefix = format!("{owner}/files/");
    let rel = storage_path.strip_prefix(&prefix).unwrap_or_else(|| storage_path.trim_start_matches('/'));
    format!("{}{}", crate::services::decode::DRIVE_PREFIX, rel)
}

/// Deterministic synthetic folder id for a remote directory (no drive UUID).
fn synthetic_folder_id(library_id: Uuid, path: &str) -> Uuid {
    Uuid::new_v5(&library_id, path.trim_matches('/').as_bytes())
}

/// Entry point used by the scan endpoint (spawned task): logs + records errors.
pub async fn scan_library(state: AppState, library_id: Uuid) {
    if let Err(e) = run_scan(&state, library_id).await {
        tracing::error!(error = %e, %library_id, "Scan de bibliothèque échoué");
        let _ = state
            .db
            .execute(
                "UPDATE books.libraries SET scan_status = 'error', scan_error = $1 WHERE id = $2",
                params![e.to_string(), library_id],
            )
            .await;
    }
}

/// Recompute counts and series covers, and mark the library idle. Shared by both
/// scan paths. `repoint` re-anchors dangling book covers (local scan only).
async fn finalize(
    tx: &mut DbTx,
    library_id: Uuid,
    cover_last: bool,
    repoint: bool,
) -> Result<(), BooksError> {
    // Drop books that lost all their formats, then series that lost all books.
    tx.execute(
        "DELETE FROM books.books WHERE library_id = $1 \
         AND id NOT IN (SELECT book_id FROM books.book_formats)",
        params![library_id],
    )
    .await?;
    tx.execute(
        "DELETE FROM books.series WHERE library_id = $1 \
         AND id NOT IN (SELECT series_id FROM books.books WHERE series_id IS NOT NULL)",
        params![library_id],
    )
    .await?;

    if repoint {
        // Re-anchor covers that are NULL or point at a format that is now gone.
        let case = format_priority_case();
        tx.execute(
            &format!(
                "UPDATE books.books SET cover_format_id = ( \
                   SELECT f.id FROM books.book_formats f WHERE f.book_id = books.books.id \
                   ORDER BY {case} LIMIT 1) \
                 WHERE library_id = $1 \
                   AND (cover_format_id IS NULL \
                        OR NOT EXISTS (SELECT 1 FROM books.book_formats f WHERE f.id = books.books.cover_format_id))"
            ),
            params![library_id],
        )
        .await?;
    }

    tx.execute(
        "UPDATE books.series SET book_count = \
           (SELECT count(*) FROM books.books WHERE series_id = books.series.id) \
         WHERE library_id = $1",
        params![library_id],
    )
    .await?;

    let order = if cover_last { COVER_ORDER_LAST } else { COVER_ORDER_FIRST };
    tx.execute(
        &format!(
            "UPDATE books.series SET cover_format_id = ( \
               SELECT b.cover_format_id FROM books.books b \
               WHERE b.series_id = books.series.id AND b.cover_format_id IS NOT NULL \
               ORDER BY {order} LIMIT 1) \
             WHERE library_id = $1"
        ),
        params![library_id],
    )
    .await?;

    tx.execute(
        "UPDATE books.libraries \
         SET item_count = (SELECT count(*) FROM books.books WHERE library_id = $1), \
             last_scan_at = $2, scan_status = 'idle', scan_error = NULL \
         WHERE id = $3",
        params![library_id, Utc::now(), library_id],
    )
    .await?;
    Ok(())
}

async fn run_scan(state: &AppState, library_id: Uuid) -> Result<(), BooksError> {
    let lib = state
        .db
        .fetch_optional_as::<LibRow>(
            "SELECT source_type, files_folder_id, files_owner_id, \
                    remote_mount_id, remote_mount_path, remote_owner_id, settings \
             FROM books.libraries WHERE id = $1",
            params![library_id],
        )
        .await?
        .ok_or_else(|| BooksError::NotFound(format!("Bibliothèque {library_id}")))?;

    if lib.source_type == "remote_mount" {
        return scan_remote_library(state, library_id, &lib).await;
    }

    if lib.source_type != "files_folder" {
        return Err(BooksError::Validation("Type de bibliothèque non scannable".into()));
    }
    let owner = lib.files_owner_id.ok_or_else(|| BooksError::Validation("files_owner_id manquant".into()))?;
    let root  = lib.files_folder_id.ok_or_else(|| BooksError::Validation("files_folder_id manquant".into()))?;

    // ── Library settings ──
    let s = &lib.settings;
    let getb = |ptr: &str, def: bool| s.pointer(ptr).and_then(|v| v.as_bool()).unwrap_or(def);
    let scan_comics = getb("/scanner/scan_comics", true);
    let scan_pdf = getb("/scanner/scan_pdf", true);
    let scan_epub = getb("/scanner/scan_epub", true);
    let excluded: Vec<String> = s.pointer("/scanner/excluded_dirs").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let cover_last = s.pointer("/options/series_cover").and_then(|v| v.as_str()) == Some("last");
    let import_meta = getb("/metadata/import_comicinfo", true) || getb("/metadata/import_epub", true);
    let oneshots_dir = s.pointer("/scanner/oneshots_dir").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let hash_files = getb("/options/hash_files", true);
    let default_rdir = s.pointer("/options/default_reading_direction").and_then(|v| v.as_str())
        .filter(|x| !x.is_empty()).map(|x| x.to_string());
    let meta_lang = match s.pointer("/metadata/metadata_language").and_then(|v| v.as_str())
        .filter(|x| !x.is_empty())
    {
        Some(l) => Some(l.to_string()),
        None => global_meta_lang(&state.db).await,
    };

    // Root folder path (cross-schema read into the drive schema).
    let root_path = state
        .db
        .fetch_optional_scalar::<String>(
            "SELECT path FROM drive.folders WHERE id = $1 AND owner_id = $2 AND is_trashed = FALSE",
            params![root, owner],
        )
        .await?
        .ok_or_else(|| BooksError::NotFound(format!("Dossier drive {root}")))?;

    // Library subtree = root folder + descendants.
    let like = format!("{root_path}/%");
    let folders = state
        .db
        .fetch_all_as::<FolderRow>(
            "SELECT id, name, path FROM drive.folders \
             WHERE owner_id = $1 AND is_trashed = FALSE AND (id = $2 OR path LIKE $3)",
            params![owner, root, like],
        )
        .await?;

    // Drop excluded directories (and their descendants, matched on any path segment).
    let folders: Vec<FolderRow> = folders
        .into_iter()
        .filter(|f| !f.path.split('/').any(|seg| excluded.iter().any(|e| e.eq_ignore_ascii_case(seg))))
        .collect();

    let folder_ids: Vec<Uuid> = folders.iter().map(|f| f.id).collect();
    let folder_by_id: HashMap<Uuid, &FolderRow> = folders.iter().map(|f| (f.id, f)).collect();

    // Book files inside the subtree — only the enabled file types.
    let mut exts: Vec<String> = Vec::new();
    if scan_comics { exts.extend(["cbz", "cbr", "cb7"].iter().map(|s| s.to_string())); }
    if scan_pdf { exts.push("pdf".into()); }
    if scan_epub { exts.push("epub".into()); }
    if exts.is_empty() { exts = BOOK_EXTS.iter().map(|s| s.to_string()).collect(); }

    // `folder_id = ANY($2) AND lower(extension) = ANY($3)` → portable IN lists.
    let mut fq = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT id, folder_id, name, extension, size_bytes, storage_path, content_hash, updated_at \
         FROM drive.files WHERE owner_id = ",
    );
    fq.push_bind(owner);
    fq.push(" AND is_trashed = FALSE AND folder_id");
    fq.push_in(folder_ids.iter().copied());
    fq.push(" AND lower(extension)");
    fq.push_in(exts.iter().cloned());
    let files = fq.fetch_all_as::<FileRow>(&state.db).await?;

    // Group by (folder_id, lower(basename)) → one book with several format files.
    let mut groups: HashMap<(Uuid, String), Vec<FileRow>> = HashMap::new();
    for f in files {
        let Some(fid) = f.folder_id else { continue };
        let key = strip_ext(&f.name).to_lowercase();
        groups.entry((fid, key)).or_default().push(f);
    }

    let mut tx = state.db.begin().await?;
    let mut series_cache: HashMap<Uuid, Uuid> = HashMap::new();
    let mut seen_files: Vec<Uuid> = Vec::new();

    // Prepared upsert clauses (identical across the loop).
    let series_upsert = state.db.backend().upsert(
        "books.series",
        &["library_id", "folder_id"],
        &[Assign::Incoming("name"), Assign::Incoming("folder_path"), Assign::Incoming("updated_at")],
    );
    let book_upsert = state.db.backend().upsert(
        "books.books",
        &["library_id", "folder_id", "book_key"],
        &[Assign::Incoming("series_id"), Assign::Incoming("title"), Assign::Incoming("file_modified_at"),
          Assign::Incoming("last_scanned_at"), Assign::Incoming("updated_at")],
    );
    let format_upsert = state.db.backend().upsert(
        "books.book_formats",
        &["file_id"],
        &[Assign::Incoming("book_id"), Assign::Incoming("format"), Assign::Incoming("file_name"),
          Assign::Incoming("storage_path"), Assign::Incoming("size_bytes"), Assign::Incoming("content_hash"),
          Assign::Incoming("file_modified_at"), Assign::Incoming("updated_at")],
    );

    for ((folder_id, book_key), group) in &groups {
        let is_oneshot = !oneshots_dir.is_empty()
            && folder_by_id.get(folder_id).map(|f| f.name.eq_ignore_ascii_case(&oneshots_dir)).unwrap_or(false);
        let series_id = if *folder_id == root || is_oneshot {
            None
        } else if let Some(sid) = series_cache.get(folder_id) {
            Some(*sid)
        } else {
            let folder = folder_by_id.get(folder_id);
            let name = folder.map(|f| f.name.clone()).unwrap_or_else(|| "Sans titre".into());
            let path = folder.map(|f| f.path.clone());
            let now = Utc::now();
            let sid = upsert_id(
                &mut tx,
                &format!(
                    "INSERT INTO books.series (id, library_id, owner_id, name, sort_name, folder_id, folder_path, updated_at) \
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8){series_upsert}"
                ),
                params![new_id(), library_id, owner, &name, &name, folder_id, path, now],
                "SELECT id FROM books.series WHERE library_id = $1 AND folder_id = $2",
                params![library_id, folder_id],
            )
            .await?;
            series_cache.insert(*folder_id, sid);
            Some(sid)
        };

        let title = strip_ext(&group[0].name).to_string();
        let file_modified = group.iter().map(|f| f.updated_at).max();
        let now = Utc::now();

        let book_id = upsert_id(
            &mut tx,
            &format!(
                "INSERT INTO books.books \
                   (id, library_id, series_id, owner_id, folder_id, title, sort_title, book_key, \
                    file_modified_at, reading_direction, last_scanned_at, updated_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12){book_upsert}"
            ),
            params![new_id(), library_id, series_id, owner, folder_id, &title, &title, book_key,
                    file_modified, default_rdir.as_deref(), now, now],
            "SELECT id FROM books.books WHERE library_id = $1 AND folder_id = $2 AND book_key = $3",
            params![library_id, folder_id, book_key],
        )
        .await?;

        // One format per distinct extension (first file wins).
        let mut by_format: HashMap<String, &FileRow> = HashMap::new();
        for f in group {
            let ext = f.extension.as_deref().unwrap_or("").to_lowercase();
            by_format.entry(ext).or_insert(f);
        }
        let mut format_ids: HashMap<String, Uuid> = HashMap::new();
        for (ext, f) in &by_format {
            let canonical = to_drive_canonical(&f.storage_path, owner);
            let content_hash = hash_files.then(|| f.content_hash.clone()).flatten();
            let fmt_id = upsert_id(
                &mut tx,
                &format!(
                    "INSERT INTO books.book_formats \
                       (id, book_id, owner_id, format, file_id, file_name, storage_path, size_bytes, \
                        content_hash, file_modified_at, updated_at) \
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11){format_upsert}"
                ),
                params![new_id(), book_id, owner, ext, f.id, &f.name, canonical, f.size_bytes,
                        content_hash, f.updated_at, Utc::now()],
                "SELECT id FROM books.book_formats WHERE file_id = $1",
                params![f.id],
            )
            .await?;
            format_ids.insert(ext.clone(), fmt_id);
            seen_files.push(f.id);
        }

        // Cover/primary format by priority order.
        if let Some(cover_id) = FORMAT_PRIORITY.iter().find_map(|p| format_ids.get(*p)).copied() {
            tx.execute(
                "UPDATE books.books SET cover_format_id = $1 WHERE id = $2 AND cover_format_id IS NULL",
                params![cover_id, book_id],
            )
            .await?;
        }
    }

    // Prune formats whose files left the subtree (empty seen_files removes all).
    let mut pq = DbQueryBuilder::new(
        state.db.backend(),
        "DELETE FROM books.book_formats WHERE book_id IN (SELECT id FROM books.books WHERE library_id = ",
    );
    pq.push_bind(library_id).push(")");
    if !seen_files.is_empty() {
        pq.push(" AND NOT (file_id");
        pq.push_in(seen_files.iter().copied());
        pq.push(")");
    }
    pq.tx_execute(&mut tx).await?;

    finalize(&mut tx, library_id, cover_last, true).await?;
    tx.commit().await?;

    if import_meta {
        crate::services::metadata::import_library(state, library_id).await;
    }
    if let Some(lang) = &meta_lang {
        let _ = state
            .db
            .execute(
                "UPDATE books.books SET language = $1 WHERE library_id = $2 AND language IS NULL",
                params![lang, library_id],
            )
            .await;
    }

    crate::services::providers::enrich_library_series(state, library_id).await;

    tracing::info!(%library_id, books = groups.len(), "Scan terminé");
    Ok(())
}

// ── Remote mount scanning ────────────────────────────────────────────────────

/// One entry returned by the core internal browse API.
#[derive(Deserialize)]
struct RemoteDirEntry {
    name:       String,
    path:       String,
    is_dir:     bool,
    size_bytes: Option<i64>,
}

/// Returns the local directory where remote book files are cached.
fn remote_cache_dir(settings: &Settings, mount_id: &str) -> PathBuf {
    let base = std::path::Path::new(&settings.storage.local_path);
    base.parent()
        .unwrap_or(base)
        .join("remote_cache")
        .join(mount_id)
}

/// Fetches the JSON listing of a remote directory via the core internal API.
async fn browse_remote(
    http:    &reqwest::Client,
    core_url:&str,
    secret:  &str,
    owner_id: Uuid,
    mount_id: &str,
    path:    &str,
) -> Result<Vec<RemoteDirEntry>, BooksError> {
    let enc_path: String = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| urlencoding::encode(s).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let url = if enc_path.is_empty() {
        format!("{}/internal/storage/mounts/{}/{}/browse", core_url.trim_end_matches('/'), owner_id, mount_id)
    } else {
        format!("{}/internal/storage/mounts/{}/{}/browse/{}", core_url.trim_end_matches('/'), owner_id, mount_id, enc_path)
    };
    let resp = http
        .get(&url)
        .header("X-Internal-Secret", secret)
        .send()
        .await
        .map_err(|e| BooksError::Internal(anyhow::anyhow!("browse remote: {e}")))?;
    if !resp.status().is_success() {
        return Err(BooksError::Internal(anyhow::anyhow!(
            "browse remote {url}: HTTP {}", resp.status()
        )));
    }
    let body: serde_json::Value = resp.json().await
        .map_err(|e| BooksError::Internal(anyhow::anyhow!("browse parse: {e}")))?;
    let items = body.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let entries: Vec<RemoteDirEntry> = items
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    Ok(entries)
}

/// Downloads a file from a remote mount and returns its bytes.
async fn download_remote_file(
    http:    &reqwest::Client,
    core_url:&str,
    secret:  &str,
    owner_id: Uuid,
    mount_id: &str,
    path:    &str,
) -> Result<bytes::Bytes, BooksError> {
    let enc_path: String = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| urlencoding::encode(s).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let url = format!(
        "{}/internal/storage/mounts/{}/{}/file/{}",
        core_url.trim_end_matches('/'), owner_id, mount_id, enc_path
    );
    let resp = http
        .get(&url)
        .header("X-Internal-Secret", secret)
        .send()
        .await
        .map_err(|e| BooksError::Internal(anyhow::anyhow!("download remote: {e}")))?;
    if !resp.status().is_success() {
        return Err(BooksError::Internal(anyhow::anyhow!(
            "download remote {url}: HTTP {}", resp.status()
        )));
    }
    resp.bytes().await
        .map_err(|e| BooksError::Internal(anyhow::anyhow!("download bytes: {e}")))
}

/// Iteratively collects all book files under `root_path` from a remote mount.
#[allow(clippy::too_many_arguments)]
async fn collect_remote_files(
    http:     &reqwest::Client,
    core_url: &str,
    secret:   &str,
    owner_id: Uuid,
    mount_id: &str,
    root_path: &str,
    exts:     &[String],
    excluded: &[String],
) -> Vec<(String, String, String, i64)> {
    let mut queue: Vec<(String, u8)> = vec![(root_path.to_string(), 0)];
    let mut out   = Vec::new();

    while let Some((dir_path, depth)) = queue.pop() {
        if depth > 16 { continue; }
        let Ok(entries) = browse_remote(http, core_url, secret, owner_id, mount_id, &dir_path).await else {
            continue;
        };
        for e in entries {
            if excluded.iter().any(|ex| ex.eq_ignore_ascii_case(&e.name)) {
                continue;
            }
            if e.is_dir {
                queue.push((e.path, depth + 1));
            } else {
                let ext = e.name.rsplit('.').next().map(|s| s.to_lowercase()).unwrap_or_default();
                if exts.iter().any(|x| x == &ext) {
                    out.push((e.path, e.name, ext, e.size_bytes.unwrap_or(0)));
                }
            }
        }
    }
    out
}

/// Scan a library whose source is a remote mount.
async fn scan_remote_library(state: &AppState, library_id: Uuid, lib: &LibRow) -> Result<(), BooksError> {
    let mount_id = lib.remote_mount_id.as_deref()
        .ok_or_else(|| BooksError::Validation("remote_mount_id manquant".into()))?;
    let owner_id = lib.remote_owner_id
        .ok_or_else(|| BooksError::Validation("remote_owner_id manquant".into()))?;
    let root_path = lib.remote_mount_path.trim_matches('/').to_string();

    // Human-readable mount name (cross-schema read into core; the id is parsed to
    // a UUID and bound rather than cast in SQL).
    let mount_name: String = match Uuid::parse_str(mount_id) {
        Ok(mount_uuid) => state
            .db
            .fetch_optional_scalar::<String>(
                "SELECT name FROM core.remote_mounts WHERE id = $1",
                params![mount_uuid],
            )
            .await?
            .unwrap_or_else(|| mount_id.to_string()),
        Err(_) => mount_id.to_string(),
    };

    let core_url = &state.settings.core.url;
    let secret   = &state.settings.core.internal_secret;

    let s = &lib.settings;
    let getb = |ptr: &str, def: bool| s.pointer(ptr).and_then(|v| v.as_bool()).unwrap_or(def);
    let scan_comics = getb("/scanner/scan_comics", true);
    let scan_pdf    = getb("/scanner/scan_pdf",    true);
    let scan_epub   = getb("/scanner/scan_epub",   true);
    let excluded: Vec<String> = s.pointer("/scanner/excluded_dirs").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let cover_last = s.pointer("/options/series_cover").and_then(|v| v.as_str()) == Some("last");
    let import_meta = getb("/metadata/import_comicinfo", true) || getb("/metadata/import_epub", true);
    let oneshots_dir = s.pointer("/scanner/oneshots_dir").and_then(|v| v.as_str())
        .unwrap_or("").trim().to_string();
    let default_rdir = s.pointer("/options/default_reading_direction").and_then(|v| v.as_str())
        .filter(|x| !x.is_empty()).map(String::from);
    let meta_lang = match s.pointer("/metadata/metadata_language").and_then(|v| v.as_str())
        .filter(|x| !x.is_empty())
    {
        Some(l) => Some(l.to_string()),
        None => global_meta_lang(&state.db).await,
    };

    let mut exts: Vec<String> = Vec::new();
    if scan_comics { exts.extend(["cbz", "cbr", "cb7"].iter().map(|s| s.to_string())); }
    if scan_pdf    { exts.push("pdf".into()); }
    if scan_epub   { exts.push("epub".into()); }
    if exts.is_empty() { exts = BOOK_EXTS.iter().map(|s| s.to_string()).collect(); }

    let remote_files = collect_remote_files(
        &state.http, core_url, secret, owner_id, mount_id, &root_path, &exts, &excluded,
    ).await;

    let cache_dir = remote_cache_dir(&state.settings, mount_id);
    tokio::fs::create_dir_all(&cache_dir).await
        .map_err(|e| BooksError::Storage(format!("create cache dir: {e}")))?;

    struct CachedFile {
        remote_path: String,
        name:        String,
        ext:         String,
        local_path:  PathBuf,
        size_bytes:  i64,
    }
    let mut cached_files: Vec<CachedFile> = Vec::new();
    for (remote_path, name, ext, size_bytes) in remote_files {
        let hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            remote_path.hash(&mut h);
            format!("{:016x}", h.finish())
        };
        let local_name = format!("{hash}.{ext}");
        let local_path = cache_dir.join(&local_name);

        let needs_download = match tokio::fs::metadata(&local_path).await {
            Ok(m) => m.len() as i64 != size_bytes,
            Err(_) => true,
        };
        if needs_download {
            tracing::debug!(remote_path, "Downloading remote book file");
            match download_remote_file(&state.http, core_url, secret, owner_id, mount_id, &remote_path).await {
                Ok(bytes) => {
                    if let Err(e) = tokio::fs::write(&local_path, &bytes).await {
                        tracing::warn!(%remote_path, error = %e, "Failed to cache remote file");
                        continue;
                    }
                }
                Err(e) => {
                    tracing::warn!(%remote_path, error = %e, "Failed to download remote file");
                    continue;
                }
            }
        }
        cached_files.push(CachedFile { remote_path, name, ext, local_path, size_bytes });
    }

    let mut groups: HashMap<(String, String), Vec<&CachedFile>> = HashMap::new();
    for f in &cached_files {
        let parent = f.remote_path.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default();
        let key = strip_ext(&f.name).to_lowercase();
        groups.entry((parent, key)).or_default().push(f);
    }

    let mut tx = state.db.begin().await?;
    let mut series_cache: HashMap<String, Uuid> = HashMap::new();
    let mut seen_cache_paths: Vec<String> = Vec::new();

    let series_upsert = state.db.backend().upsert(
        "books.series",
        &["library_id", "folder_id"],
        &[Assign::Incoming("name"), Assign::Incoming("folder_path"), Assign::Incoming("updated_at")],
    );
    let book_upsert = state.db.backend().upsert(
        "books.books",
        &["library_id", "folder_id", "book_key"],
        &[Assign::Incoming("series_id"), Assign::Incoming("title"),
          Assign::Incoming("last_scanned_at"), Assign::Incoming("updated_at")],
    );
    let format_upsert = state.db.backend().upsert(
        "books.book_formats",
        &["book_id", "format"],
        &[Assign::Incoming("file_name"), Assign::Incoming("storage_path"), Assign::Incoming("remote_path"),
          Assign::Incoming("local_cache_path"), Assign::Incoming("size_bytes"),
          Assign::Incoming("file_modified_at"), Assign::Incoming("updated_at")],
    );

    for ((parent_path, book_key), group) in &groups {
        let parent_name = parent_path.rsplit('/').next().unwrap_or("").to_string();
        let is_root     = parent_path.trim_matches('/') == root_path.trim_matches('/')
                       || parent_path.is_empty();
        let is_oneshot  = !oneshots_dir.is_empty()
                       && parent_name.eq_ignore_ascii_case(&oneshots_dir);

        let folder_uuid = synthetic_folder_id(library_id, parent_path);

        let series_id = if is_root || is_oneshot {
            None
        } else if let Some(sid) = series_cache.get(parent_path) {
            Some(*sid)
        } else {
            let now = Utc::now();
            let sid = upsert_id(
                &mut tx,
                &format!(
                    "INSERT INTO books.series (id, library_id, owner_id, name, sort_name, folder_id, folder_path, updated_at) \
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8){series_upsert}"
                ),
                params![new_id(), library_id, owner_id, &parent_name, &parent_name, folder_uuid, parent_path, now],
                "SELECT id FROM books.series WHERE library_id = $1 AND folder_id = $2",
                params![library_id, folder_uuid],
            )
            .await?;
            series_cache.insert(parent_path.clone(), sid);
            Some(sid)
        };

        let title = strip_ext(&group[0].name).to_string();
        let now = Utc::now();

        let book_id = upsert_id(
            &mut tx,
            &format!(
                "INSERT INTO books.books \
                   (id, library_id, series_id, owner_id, folder_id, title, sort_title, book_key, \
                    reading_direction, last_scanned_at, updated_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11){book_upsert}"
            ),
            params![new_id(), library_id, series_id, owner_id, folder_uuid, &title, &title, book_key,
                    default_rdir.as_deref(), now, now],
            "SELECT id FROM books.books WHERE library_id = $1 AND folder_id = $2 AND book_key = $3",
            params![library_id, folder_uuid, book_key],
        )
        .await?;

        let mut by_format: HashMap<String, &CachedFile> = HashMap::new();
        for f in group {
            by_format.entry(f.ext.clone()).or_insert(f);
        }
        let mut format_ids: HashMap<String, Uuid> = HashMap::new();
        for (ext, f) in &by_format {
            let local_path_str = f.local_path.to_string_lossy().to_string();
            let canonical = format!("[{}]/{}", mount_name, f.remote_path.trim_start_matches('/'));
            let now = Utc::now();
            let fmt_id = upsert_id(
                &mut tx,
                &format!(
                    "INSERT INTO books.book_formats \
                       (id, book_id, owner_id, format, file_name, storage_path, \
                        remote_path, local_cache_path, size_bytes, file_modified_at, updated_at) \
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11){format_upsert}"
                ),
                params![new_id(), book_id, owner_id, ext, &f.name, canonical,
                        &f.remote_path, &local_path_str, f.size_bytes, now, now],
                "SELECT id FROM books.book_formats WHERE book_id = $1 AND format = $2",
                params![book_id, ext],
            )
            .await?;
            format_ids.insert(ext.clone(), fmt_id);
            seen_cache_paths.push(local_path_str);
        }

        if let Some(cover_id) = FORMAT_PRIORITY.iter().find_map(|p| format_ids.get(*p)).copied() {
            tx.execute(
                "UPDATE books.books SET cover_format_id = $1 WHERE id = $2 AND cover_format_id IS NULL",
                params![cover_id, book_id],
            )
            .await?;
        }
    }

    // Prune formats whose local cache path is no longer present in this scan.
    let mut pq = DbQueryBuilder::new(
        state.db.backend(),
        "DELETE FROM books.book_formats WHERE book_id IN (SELECT id FROM books.books WHERE library_id = ",
    );
    pq.push_bind(library_id).push(") AND local_cache_path IS NOT NULL");
    if !seen_cache_paths.is_empty() {
        pq.push(" AND NOT (local_cache_path");
        pq.push_in(seen_cache_paths.iter().cloned());
        pq.push(")");
    }
    pq.tx_execute(&mut tx).await?;

    finalize(&mut tx, library_id, cover_last, false).await?;
    tx.commit().await?;

    if import_meta {
        crate::services::metadata::import_library(state, library_id).await;
    }
    if let Some(lang) = &meta_lang {
        let _ = state
            .db
            .execute(
                "UPDATE books.books SET language = $1 WHERE library_id = $2 AND language IS NULL",
                params![lang, library_id],
            )
            .await;
    }

    crate::services::providers::enrich_library_series(state, library_id).await;

    tracing::info!(%library_id, books = groups.len(), "Scan distant terminé");
    Ok(())
}
