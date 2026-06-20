// Library scan: a books library points at a Drive folder or a remote mount. We read the drive
// module's schema (drive.folders / drive.files) directly for local libraries, or call the core
// internal API for remote mounts. Files are grouped into series / books and
// upserted into the books schema. Idempotent: re-scanning reconciles additions and removals.
use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::{config::Settings, errors::BooksError, state::AppState};

const BOOK_EXTS: &[&str] = &["cbz", "cbr", "cb7", "pdf", "epub"];
/// Cover/primary format preference (image-based first).
const FORMAT_PRIORITY: &[&str] = &["cbz", "cb7", "cbr", "pdf", "epub"];

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
/// owner-agnostic form `[Drive]/<rel>`. Falls back to `[Drive]/<as-is>` if the
/// expected prefix is absent.
fn to_drive_canonical(storage_path: &str, owner: Uuid) -> String {
    let prefix = format!("{owner}/files/");
    let rel = storage_path.strip_prefix(&prefix).unwrap_or_else(|| storage_path.trim_start_matches('/'));
    format!("{}{}", crate::services::decode::DRIVE_PREFIX, rel)
}

/// Deterministic synthetic folder id for a remote directory (no drive UUID).
/// Stable per (library, path) so rescans dedup series/books idempotently.
fn synthetic_folder_id(library_id: Uuid, path: &str) -> Uuid {
    Uuid::new_v5(&library_id, path.trim_matches('/').as_bytes())
}

/// Entry point used by the scan endpoint (spawned task): logs + records errors.
pub async fn scan_library(state: AppState, library_id: Uuid) {
    if let Err(e) = run_scan(&state, library_id).await {
        tracing::error!(error = %e, %library_id, "Scan de bibliothèque échoué");
        let _ = sqlx::query(
            "UPDATE books.libraries SET scan_status = 'error', scan_error = $2 WHERE id = $1",
        )
        .bind(library_id)
        .bind(e.to_string())
        .execute(&state.db)
        .await;
    }
}

async fn run_scan(state: &AppState, library_id: Uuid) -> Result<(), BooksError> {
    let lib = sqlx::query_as::<_, LibRow>(
        "SELECT source_type, files_folder_id, files_owner_id, \
                remote_mount_id, remote_mount_path, remote_owner_id, settings \
         FROM books.libraries WHERE id = $1",
    )
    .bind(library_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| BooksError::NotFound(format!("Bibliothèque {library_id}")))?;

    if lib.source_type == "remote_mount" {
        return scan_remote_library(state, library_id, &lib).await;
    }

    if lib.source_type != "files_folder" {
        return Err(BooksError::Validation(
            "Type de bibliothèque non scannable".into(),
        ));
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
    let meta_lang = s.pointer("/metadata/metadata_language").and_then(|v| v.as_str())
        .filter(|x| !x.is_empty()).map(|x| x.to_string());

    // Root folder path (cross-schema read into the drive schema).
    let root_path = sqlx::query_scalar::<_, String>(
        "SELECT path FROM drive.folders WHERE id = $1 AND owner_id = $2 AND is_trashed = FALSE",
    )
    .bind(root)
    .bind(owner)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| BooksError::NotFound(format!("Dossier drive {root}")))?;

    // Library subtree = root folder + descendants.
    let like = format!("{root_path}/%");
    let folders = sqlx::query_as::<_, FolderRow>(
        "SELECT id, name, path FROM drive.folders \
         WHERE owner_id = $1 AND is_trashed = FALSE AND (id = $2 OR path LIKE $3)",
    )
    .bind(owner)
    .bind(root)
    .bind(&like)
    .fetch_all(&state.db)
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
    let files = sqlx::query_as::<_, FileRow>(
        "SELECT id, folder_id, name, extension, size_bytes, storage_path, content_hash, updated_at \
         FROM drive.files \
         WHERE owner_id = $1 AND is_trashed = FALSE AND folder_id = ANY($2) \
           AND lower(extension) = ANY($3)",
    )
    .bind(owner)
    .bind(&folder_ids)
    .bind(&exts)
    .fetch_all(&state.db)
    .await?;

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

    for ((folder_id, book_key), group) in &groups {
        // Series = the containing folder, unless it's the library root or the one-shots folder.
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
            let sid = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO books.series (library_id, owner_id, name, sort_name, folder_id, folder_path) \
                 VALUES ($1,$2,$3,$3,$4,$5) \
                 ON CONFLICT (library_id, folder_id) DO UPDATE \
                   SET name = EXCLUDED.name, folder_path = EXCLUDED.folder_path, updated_at = now() \
                 RETURNING id",
            )
            .bind(library_id)
            .bind(owner)
            .bind(&name)
            .bind(folder_id)
            .bind(&path)
            .fetch_one(&mut *tx)
            .await?;
            series_cache.insert(*folder_id, sid);
            Some(sid)
        };

        let title = strip_ext(&group[0].name).to_string();
        let file_modified = group.iter().map(|f| f.updated_at).max();

        let book_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO books.books \
               (library_id, series_id, owner_id, folder_id, title, sort_title, book_key, file_modified_at, reading_direction, last_scanned_at) \
             VALUES ($1,$2,$3,$4,$5,$5,$6,$7,$8, now()) \
             ON CONFLICT (library_id, folder_id, book_key) DO UPDATE \
               SET series_id = EXCLUDED.series_id, title = EXCLUDED.title, \
                   file_modified_at = EXCLUDED.file_modified_at, last_scanned_at = now(), updated_at = now() \
             RETURNING id",
        )
        .bind(library_id)
        .bind(series_id)
        .bind(owner)
        .bind(folder_id)
        .bind(&title)
        .bind(book_key)
        .bind(file_modified)
        .bind(&default_rdir)
        .fetch_one(&mut *tx)
        .await?;

        // One format per distinct extension (first file wins).
        let mut by_format: HashMap<String, &FileRow> = HashMap::new();
        for f in group {
            let ext = f.extension.as_deref().unwrap_or("").to_lowercase();
            by_format.entry(ext).or_insert(f);
        }
        let mut format_ids: HashMap<String, Uuid> = HashMap::new();
        for (ext, f) in &by_format {
            // Store the canonical "[Drive]/<rel>" form: strip the implicit
            // "<owner>/files/" prefix so the path is owner-agnostic and routable.
            let canonical = to_drive_canonical(&f.storage_path, owner);
            let fmt_id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO books.book_formats \
                   (book_id, owner_id, format, file_id, file_name, storage_path, size_bytes, content_hash, file_modified_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) \
                 ON CONFLICT (file_id) DO UPDATE \
                   SET book_id = EXCLUDED.book_id, format = EXCLUDED.format, file_name = EXCLUDED.file_name, \
                       storage_path = EXCLUDED.storage_path, size_bytes = EXCLUDED.size_bytes, \
                       content_hash = EXCLUDED.content_hash, file_modified_at = EXCLUDED.file_modified_at, updated_at = now() \
                 RETURNING id",
            )
            .bind(book_id)
            .bind(owner)
            .bind(ext)
            .bind(f.id)
            .bind(&f.name)
            .bind(&canonical)
            .bind(f.size_bytes)
            .bind(hash_files.then(|| f.content_hash.clone()).flatten())
            .bind(f.updated_at)
            .fetch_one(&mut *tx)
            .await?;
            format_ids.insert(ext.clone(), fmt_id);
            seen_files.push(f.id);
        }

        // Cover/primary format by priority order.
        if let Some(cover_id) = FORMAT_PRIORITY.iter().find_map(|p| format_ids.get(*p)).copied() {
            sqlx::query("UPDATE books.books SET cover_format_id = $2 WHERE id = $1 AND cover_format_id IS NULL")
                .bind(book_id)
                .bind(cover_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    // Prune entries whose files left the subtree (empty seen_files removes everything).
    sqlx::query(
        "DELETE FROM books.book_formats bf USING books.books b \
         WHERE bf.book_id = b.id AND b.library_id = $1 AND bf.file_id <> ALL($2)",
    )
    .bind(library_id)
    .bind(&seen_files)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM books.books b WHERE b.library_id = $1 \
         AND NOT EXISTS (SELECT 1 FROM books.book_formats f WHERE f.book_id = b.id)",
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM books.series s WHERE s.library_id = $1 \
         AND NOT EXISTS (SELECT 1 FROM books.books b WHERE b.series_id = s.id)",
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await?;

    // Recompute counts + series cover from the first book.
    sqlx::query(
        "UPDATE books.series s SET book_count = (SELECT count(*) FROM books.books b WHERE b.series_id = s.id) \
         WHERE s.library_id = $1",
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await?;

    let cover_order = if cover_last {
        "b.series_index DESC NULLS LAST, b.sort_title DESC NULLS LAST, b.title DESC"
    } else {
        "b.series_index NULLS LAST, b.sort_title NULLS LAST, b.title"
    };
    sqlx::query(&format!(
        "UPDATE books.series s SET cover_format_id = ( \
           SELECT b.cover_format_id FROM books.books b \
           WHERE b.series_id = s.id AND b.cover_format_id IS NOT NULL \
           ORDER BY {cover_order} LIMIT 1) \
         WHERE s.library_id = $1"
    ))
    .bind(library_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE books.libraries \
         SET item_count = (SELECT count(*) FROM books.books WHERE library_id = $1), \
             last_scan_at = now(), scan_status = 'idle', scan_error = NULL \
         WHERE id = $1",
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Best-effort: import embedded metadata (ComicInfo.xml / EPUB OPF) for new books.
    if import_meta {
        crate::services::metadata::import_library(state, library_id).await;
    }
    // Fallback default language for books that still have none.
    if let Some(lang) = &meta_lang {
        let _ = sqlx::query("UPDATE books.books SET language = $2 WHERE library_id = $1 AND language IS NULL")
            .bind(library_id).bind(lang).execute(&state.db).await;
    }

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
    // Sibling of the blob storage directory: .../remote_cache/<mount_id>/
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

/// Iteratively collects all book files under `root_path` from a remote mount
/// (BFS, up to 16 levels deep). Returns (remote_path, name, extension, size_bytes).
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

/// Scan a library whose source is a remote mount. Downloads each book file into
/// a local cache directory, then upserts series/books/formats exactly like a
/// local scan. Idempotent: only re-downloads files whose size changed.
async fn scan_remote_library(state: &AppState, library_id: Uuid, lib: &LibRow) -> Result<(), BooksError> {
    let mount_id = lib.remote_mount_id.as_deref()
        .ok_or_else(|| BooksError::Validation("remote_mount_id manquant".into()))?;
    let owner_id = lib.remote_owner_id
        .ok_or_else(|| BooksError::Validation("remote_owner_id manquant".into()))?;
    let root_path = lib.remote_mount_path.trim_matches('/').to_string();

    // Human-readable mount name for canonical "[<mount_name>]/<path>" storage paths.
    // Cross-schema read into core (books already reads core.users elsewhere).
    let mount_name: String = sqlx::query_scalar::<_, String>(
        "SELECT name FROM core.remote_mounts WHERE id = $1::uuid",
    )
    .bind(mount_id)
    .fetch_optional(&state.db)
    .await?
    .unwrap_or_else(|| mount_id.to_string());

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
    let meta_lang = s.pointer("/metadata/metadata_language").and_then(|v| v.as_str())
        .filter(|x| !x.is_empty()).map(String::from);

    let mut exts: Vec<String> = Vec::new();
    if scan_comics { exts.extend(["cbz", "cbr", "cb7"].iter().map(|s| s.to_string())); }
    if scan_pdf    { exts.push("pdf".into()); }
    if scan_epub   { exts.push("epub".into()); }
    if exts.is_empty() { exts = BOOK_EXTS.iter().map(|s| s.to_string()).collect(); }

    // Recursively list all book files from the remote mount.
    let remote_files = collect_remote_files(
        &state.http, core_url, secret, owner_id, mount_id, &root_path, &exts, &excluded,
    ).await;

    // Prepare local cache directory.
    let cache_dir = remote_cache_dir(&state.settings, mount_id);
    tokio::fs::create_dir_all(&cache_dir).await
        .map_err(|e| BooksError::Storage(format!("create cache dir: {e}")))?;

    // Download each file into the local cache (skip if already cached with same size).
    // Cache key: deterministic filename derived from the remote path.
    struct CachedFile {
        remote_path: String,
        name:        String,
        ext:         String,
        local_path:  PathBuf,
        size_bytes:  i64,
    }
    let mut cached_files: Vec<CachedFile> = Vec::new();
    for (remote_path, name, ext, size_bytes) in remote_files {
        // Derive a safe local filename: sha256-ish (use a hex hash of the path).
        let hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            remote_path.hash(&mut h);
            format!("{:016x}", h.finish())
        };
        let local_name = format!("{hash}.{ext}");
        let local_path = cache_dir.join(&local_name);

        // Re-download only if size changed or file absent.
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

    // Group by (parent_dir_of_remote_path, lower_basename) → one book / several formats.
    // "parent dir" acts as the series folder.
    let mut groups: HashMap<(String, String), Vec<&CachedFile>> = HashMap::new();
    for f in &cached_files {
        let parent = f.remote_path.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default();
        let key = strip_ext(&f.name).to_lowercase();
        groups.entry((parent, key)).or_default().push(f);
    }

    let mut tx = state.db.begin().await?;
    let mut series_cache: HashMap<String, Uuid> = HashMap::new(); // parent_path → series id
    let mut seen_cache_paths: Vec<String> = Vec::new();

    for ((parent_path, book_key), group) in &groups {
        // Series = the parent remote directory, unless it's the root or the one-shots dir.
        let parent_name = parent_path.rsplit('/').next().unwrap_or("").to_string();
        let is_root     = parent_path.trim_matches('/') == root_path.trim_matches('/')
                       || parent_path.is_empty();
        let is_oneshot  = !oneshots_dir.is_empty()
                       && parent_name.eq_ignore_ascii_case(&oneshots_dir);

        // Remote dirs have no drive folder UUID; derive a deterministic synthetic one
        // from (library, parent_path) so series/books dedup idempotently on rescans
        // (mirrors the local scan's real drive folder_id).
        let folder_uuid = synthetic_folder_id(library_id, parent_path);

        let series_id = if is_root || is_oneshot {
            None
        } else if let Some(sid) = series_cache.get(parent_path) {
            Some(*sid)
        } else {
            let sid = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO books.series (library_id, owner_id, name, sort_name, folder_id, folder_path) \
                 VALUES ($1,$2,$3,$3,$4,$5) \
                 ON CONFLICT (library_id, folder_id) DO UPDATE \
                   SET name = EXCLUDED.name, folder_path = EXCLUDED.folder_path, updated_at = now() \
                 RETURNING id",
            )
            .bind(library_id)
            .bind(owner_id)
            .bind(&parent_name)
            .bind(folder_uuid)
            .bind(parent_path)
            .fetch_one(&mut *tx)
            .await?;
            series_cache.insert(parent_path.clone(), sid);
            Some(sid)
        };

        let title = strip_ext(&group[0].name).to_string();

        let book_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO books.books \
               (library_id, series_id, owner_id, folder_id, title, sort_title, book_key, reading_direction, last_scanned_at) \
             VALUES ($1,$2,$3,$4,$5,$5,$6,$7, now()) \
             ON CONFLICT (library_id, folder_id, book_key) DO UPDATE \
               SET series_id = EXCLUDED.series_id, title = EXCLUDED.title, \
                   last_scanned_at = now(), updated_at = now() \
             RETURNING id",
        )
        .bind(library_id)
        .bind(series_id)
        .bind(owner_id)
        .bind(folder_uuid)
        .bind(&title)
        .bind(book_key)
        .bind(&default_rdir)
        .fetch_one(&mut *tx)
        .await?;

        // One format per distinct extension (first file wins).
        let mut by_format: HashMap<String, &CachedFile> = HashMap::new();
        for f in group {
            by_format.entry(f.ext.clone()).or_insert(f);
        }
        let mut format_ids: HashMap<String, Uuid> = HashMap::new();
        for (ext, f) in &by_format {
            let local_path_str = f.local_path.to_string_lossy().to_string();
            // Canonical "[<mount_name>]/<remote_path>" — owner-agnostic, routable.
            let canonical = format!("[{}]/{}", mount_name, f.remote_path.trim_start_matches('/'));
            let fmt_id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO books.book_formats \
                   (book_id, owner_id, format, file_name, storage_path, \
                    remote_path, local_cache_path, size_bytes, file_modified_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8, now()) \
                 ON CONFLICT (book_id, format) DO UPDATE \
                   SET file_name = EXCLUDED.file_name, \
                       storage_path = EXCLUDED.storage_path, \
                       remote_path = EXCLUDED.remote_path, \
                       local_cache_path = EXCLUDED.local_cache_path, \
                       size_bytes = EXCLUDED.size_bytes, \
                       file_modified_at = EXCLUDED.file_modified_at, \
                       updated_at = now() \
                 RETURNING id",
            )
            .bind(book_id)
            .bind(owner_id)
            .bind(ext)
            .bind(&f.name)
            .bind(&canonical)
            .bind(&f.remote_path)
            .bind(&local_path_str)
            .bind(f.size_bytes)
            .fetch_one(&mut *tx)
            .await?;
            format_ids.insert(ext.clone(), fmt_id);
            seen_cache_paths.push(local_path_str);
        }

        // Cover/primary format by priority order.
        if let Some(cover_id) = FORMAT_PRIORITY.iter().find_map(|p| format_ids.get(*p)).copied() {
            sqlx::query("UPDATE books.books SET cover_format_id = $2 WHERE id = $1 AND cover_format_id IS NULL")
                .bind(book_id)
                .bind(cover_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    // Prune formats whose local_cache_path is no longer present in this scan.
    sqlx::query(
        "DELETE FROM books.book_formats bf USING books.books b \
         WHERE bf.book_id = b.id AND b.library_id = $1 \
           AND bf.local_cache_path IS NOT NULL \
           AND bf.local_cache_path <> ALL($2)",
    )
    .bind(library_id)
    .bind(&seen_cache_paths)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM books.books b WHERE b.library_id = $1 \
         AND NOT EXISTS (SELECT 1 FROM books.book_formats f WHERE f.book_id = b.id)",
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "DELETE FROM books.series s WHERE s.library_id = $1 \
         AND NOT EXISTS (SELECT 1 FROM books.books b WHERE b.series_id = s.id)",
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE books.series s \
         SET book_count = (SELECT count(*) FROM books.books b WHERE b.series_id = s.id) \
         WHERE s.library_id = $1",
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await?;

    let cover_order = if cover_last {
        "b.series_index DESC NULLS LAST, b.sort_title DESC NULLS LAST, b.title DESC"
    } else {
        "b.series_index NULLS LAST, b.sort_title NULLS LAST, b.title"
    };
    sqlx::query(&format!(
        "UPDATE books.series s SET cover_format_id = ( \
           SELECT b.cover_format_id FROM books.books b \
           WHERE b.series_id = s.id AND b.cover_format_id IS NOT NULL \
           ORDER BY {cover_order} LIMIT 1) \
         WHERE s.library_id = $1"
    ))
    .bind(library_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE books.libraries \
         SET item_count = (SELECT count(*) FROM books.books WHERE library_id = $1), \
             last_scan_at = now(), scan_status = 'idle', scan_error = NULL \
         WHERE id = $1",
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    if import_meta {
        crate::services::metadata::import_library(state, library_id).await;
    }
    if let Some(lang) = &meta_lang {
        let _ = sqlx::query("UPDATE books.books SET language = $2 WHERE library_id = $1 AND language IS NULL")
            .bind(library_id).bind(lang).execute(&state.db).await;
    }

    tracing::info!(%library_id, books = groups.len(), "Scan distant terminé");
    Ok(())
}
