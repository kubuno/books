use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BookLibrary {
    pub id:               Uuid,
    pub owner_id:         Option<Uuid>,
    pub name:             String,
    pub lib_type:         String,
    pub path:             String,
    pub icon:             String,
    pub color:            String,
    pub is_shared:        bool,
    pub item_count:       i32,
    pub last_scan_at:     Option<DateTime<Utc>>,
    pub scan_status:      String,
    pub scan_error:       Option<String>,
    pub source_type:      String,
    pub files_folder_id:  Option<Uuid>,
    pub files_owner_id:   Option<Uuid>,
    pub remote_mount_id:  Option<String>,
    pub remote_mount_path: String,
    pub remote_owner_id:  Option<Uuid>,
    /// Scanner / options / metadata-import settings (free-form JSON).
    pub settings:         Value,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateLibraryDto {
    pub name:              String,
    pub lib_type:          String,
    /// Disk path (required for source_type = 'filesystem', ignored otherwise).
    pub path:              Option<String>,
    pub icon:              Option<String>,
    pub color:             Option<String>,
    pub is_shared:         Option<bool>,
    /// 'filesystem' | 'files_folder' | 'remote_mount'.
    pub source_type:       Option<String>,
    /// Required for source_type = 'files_folder'.
    pub files_folder_id:   Option<Uuid>,
    pub files_owner_id:    Option<Uuid>,
    /// Required for source_type = 'remote_mount'.
    pub remote_mount_id:   Option<String>,
    /// Path within the remote mount (empty string = root).
    pub remote_mount_path: Option<String>,
    pub settings:          Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLibraryDto {
    pub name:      Option<String>,
    pub path:      Option<String>,
    pub icon:      Option<String>,
    pub color:     Option<String>,
    pub is_shared: Option<bool>,
    pub settings:  Option<Value>,
}
