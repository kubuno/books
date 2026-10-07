use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use kubuno_db::{new_id, params, DbQueryBuilder};
use kubuno_storage::path::user_folder_dir;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::BooksError,
    middleware::auth::AuthUser,
    models::library::{BookLibrary, CreateLibraryDto, UpdateLibraryDto},
    services::access::Restriction,
    state::AppState,
};

/// Valid library types for the books module.
const LIB_TYPES: &[&str] = &["books", "comics", "ebooks"];

/// The column list every library row is returned with.
macro_rules! library_columns {
    () => {
        r#"id, owner_id, name, lib_type, path, icon, color,
    is_shared, item_count, last_scan_at, scan_status, scan_error,
    source_type, files_folder_id, files_owner_id,
    remote_mount_id, remote_mount_path, remote_owner_id,
    settings, created_at, updated_at"#
    };
}

const RESELECT_LIBRARY_SQL: &str =
    concat!("SELECT ", library_columns!(), " FROM books.libraries WHERE id = $1");

pub async fn list_libraries(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        concat!("SELECT ", library_columns!(), " FROM books.libraries l WHERE "),
    );
    restriction.push_visible_library(&mut qb, user.id);
    qb.push_order_by("name");
    let rows = qb.fetch_all_as::<BookLibrary>(&state.db).await?;
    Ok(Json(json!({ "libraries": rows })))
}

pub async fn create_library(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(dto): Json<CreateLibraryDto>,
) -> Result<(StatusCode, Json<Value>), BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    if !LIB_TYPES.contains(&dto.lib_type.as_str()) {
        return Err(BooksError::Validation("lib_type invalide".into()));
    }

    let source_type = dto.source_type.as_deref().unwrap_or("filesystem");
    let id = new_id();
    let icon = dto.icon.as_deref().unwrap_or("📚").to_string();
    let color = dto.color.as_deref().unwrap_or("#1a73e8").to_string();
    let is_shared = dto.is_shared.unwrap_or(true);
    let settings = dto.settings.clone().unwrap_or_else(|| serde_json::json!({}));

    if source_type == "remote_mount" {
        let mount_id = dto.remote_mount_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| BooksError::Validation("remote_mount_id requis pour source remote_mount".into()))?
            .to_string();
        let mount_path = dto.remote_mount_path.as_deref().unwrap_or("").to_string();

        state
            .db
            .execute(
                "INSERT INTO books.libraries \
                 (id, owner_id, name, lib_type, path, icon, color, is_shared, \
                  source_type, remote_mount_id, remote_mount_path, remote_owner_id, settings) \
                 VALUES ($1,$2,$3,$4,'',$5,$6,$7,'remote_mount',$8,$9,$10,$11)",
                params![id, user.id, dto.name, dto.lib_type, icon, color, is_shared, mount_id, mount_path, user.id, settings],
            )
            .await?;
    } else if source_type == "files_folder" {
        let folder_id = dto.files_folder_id
            .ok_or_else(|| BooksError::Validation("files_folder_id requis pour source files_folder".into()))?;
        // The folder's owner can be supplied or resolved from the drive folder itself.
        // NOTE: cross-schema read into the drive schema (PostgreSQL/MySQL only).
        let owner_id = match dto.files_owner_id {
            Some(o) => o,
            None => state
                .db
                .fetch_optional_scalar::<Uuid>(
                    "SELECT owner_id FROM drive.folders WHERE id = $1 AND is_trashed = FALSE",
                    params![folder_id],
                )
                .await?
                .ok_or_else(|| BooksError::NotFound(format!("Dossier drive {folder_id}")))?,
        };
        let base = state.settings.storage.files_storage_base.as_deref()
            .ok_or_else(|| BooksError::Validation("storage.files_storage_base non configuré sur ce serveur".into()))?;

        let folder_path: String = state
            .db
            .fetch_optional_scalar::<String>(
                "SELECT path FROM drive.folders WHERE id = $1 AND owner_id = $2",
                params![folder_id, owner_id],
            )
            .await?
            .ok_or_else(|| BooksError::NotFound(format!("Dossier drive {folder_id}")))?;

        let rel = user_folder_dir(owner_id, &folder_path);
        let resolved = format!("{}/{}", base.trim_end_matches('/'), rel.to_string_lossy());

        state
            .db
            .execute(
                "INSERT INTO books.libraries \
                 (id, owner_id, name, lib_type, path, icon, color, is_shared, \
                  source_type, files_folder_id, files_owner_id, settings) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'files_folder',$9,$10,$11)",
                params![id, user.id, dto.name, dto.lib_type, resolved, icon, color, is_shared, folder_id, owner_id, settings],
            )
            .await?;
    } else {
        let path = dto.path.clone()
            .filter(|p| !p.is_empty())
            .ok_or_else(|| BooksError::Validation("path requis pour source filesystem".into()))?;

        state
            .db
            .execute(
                "INSERT INTO books.libraries (id, owner_id, name, lib_type, path, icon, color, is_shared, settings) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                params![id, user.id, dto.name, dto.lib_type, path, icon, color, is_shared, settings],
            )
            .await?;
    }

    let row = state
        .db
        .fetch_one_as::<BookLibrary>(RESELECT_LIBRARY_SQL, params![id])
        .await?;
    Ok((StatusCode::CREATED, Json(json!(row))))
}

pub async fn update_library(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateLibraryDto>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    let mut qb = DbQueryBuilder::new(state.db.backend(), "UPDATE books.libraries SET ");
    qb.push("name = COALESCE(").push_bind(dto.name).push(", name)");
    qb.push(", path = COALESCE(").push_bind(dto.path).push(", path)");
    qb.push(", icon = COALESCE(").push_bind(dto.icon).push(", icon)");
    qb.push(", color = COALESCE(").push_bind(dto.color).push(", color)");
    qb.push(", is_shared = COALESCE(").push_bind(dto.is_shared).push(", is_shared)");
    qb.push(", settings = COALESCE(").push_bind(dto.settings).push(", settings)");
    qb.push(", updated_at = ").push_bind(Utc::now());
    qb.push(" WHERE id = ").push_bind(id);
    qb.execute(&state.db).await?;

    let row = state
        .db
        .fetch_optional_as::<BookLibrary>(RESELECT_LIBRARY_SQL, params![id])
        .await?
        .ok_or_else(|| BooksError::NotFound(format!("Bibliothèque {id}")))?;
    Ok(Json(json!(row)))
}

/// Folder picker source from the drive module (admins only).
#[derive(Debug, sqlx::FromRow)]
struct FilesFolderRow {
    id:                 Uuid,
    owner_id:           Uuid,
    path:               String,
    name:               String,
    owner_email:        String,
    owner_display_name: Option<String>,
}

pub async fn list_files_folders(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    // NOTE: cross-schema read into the drive + core schemas (PostgreSQL/MySQL only).
    let rows = state
        .db
        .fetch_all_as::<FilesFolderRow>(
            "SELECT f.id, f.owner_id, f.path, f.name, \
                    u.email        AS owner_email, \
                    u.display_name AS owner_display_name \
             FROM drive.folders f \
             JOIN core.users u ON u.id = f.owner_id \
             ORDER BY u.email, f.path",
            params![],
        )
        .await?;

    let folders: Vec<_> = rows.iter().map(|r| json!({
        "id":                 r.id,
        "owner_id":           r.owner_id,
        "path":               r.path,
        "name":               r.name,
        "owner_email":        r.owner_email,
        "owner_display_name": r.owner_display_name,
    })).collect();

    Ok(Json(json!({ "folders": folders })))
}

pub async fn delete_library(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }

    // Capture this library's cached remote files before the cascade removes the rows.
    let cached = state
        .db
        .fetch_all_as::<(String,)>(
            "SELECT bf.local_cache_path FROM books.book_formats bf \
             JOIN books.books b ON b.id = bf.book_id \
             WHERE b.library_id = $1 AND bf.local_cache_path IS NOT NULL",
            params![id],
        )
        .await?;

    let affected = state
        .db
        .execute("DELETE FROM books.libraries WHERE id = $1", params![id])
        .await?;

    if affected == 0 {
        return Err(BooksError::NotFound(format!("Bibliothèque {id}")));
    }

    // Remove now-orphaned cache files (a file may be shared by another library on
    // the same mount — only delete it when no remaining format references it).
    for (path,) in cached {
        let still_used = state
            .db
            .fetch_optional_scalar::<i64>(
                &format!(
                    "SELECT {} FROM books.book_formats WHERE local_cache_path = $1",
                    state.db.backend().count_bigint("*")
                ),
                params![&path],
            )
            .await
            .ok()
            .flatten()
            .unwrap_or(1);
        if still_used == 0 {
            let _ = tokio::fs::remove_file(&path).await;
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Start an asynchronous scan of the library's Drive folder (admins only).
pub async fn start_scan(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }

    let exists = state
        .db
        .fetch_optional_scalar::<Uuid>("SELECT id FROM books.libraries WHERE id = $1", params![id])
        .await?;
    if exists.is_none() {
        return Err(BooksError::NotFound(format!("Bibliothèque {id}")));
    }

    state
        .db
        .execute(
            "UPDATE books.libraries SET scan_status = 'scanning', scan_error = NULL WHERE id = $1",
            params![id],
        )
        .await?;

    let st = state.clone();
    tokio::spawn(async move { crate::services::scan::scan_library(st, id).await });

    Ok(Json(json!({ "message": "Scan démarré", "library_id": id })))
}

/// Progress of a scan. Restricted like every other library route.
pub async fn scan_status(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let restriction = Restriction::load(&state.db, user.id).await?;
    let mut qb = DbQueryBuilder::new(
        state.db.backend(),
        "SELECT scan_status FROM books.libraries l WHERE id = ",
    );
    qb.push_bind(id).push(" AND ");
    restriction.push_visible_library(&mut qb, user.id);
    let status = qb.fetch_optional_scalar::<String>(&state.db).await?;

    Ok(Json(json!({ "status": status.unwrap_or_else(|| "idle".into()) })))
}
