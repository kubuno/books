use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Series {
    pub id:                Uuid,
    pub library_id:        Uuid,
    pub owner_id:          Uuid,
    pub name:              String,
    pub sort_name:         Option<String>,
    pub folder_id:         Uuid,
    pub folder_path:       Option<String>,
    pub description:       Option<String>,
    pub publisher:         Option<String>,
    pub genres:            Value,
    pub tags:              Value,
    pub language:          Option<String>,
    pub age_rating:        Option<i32>,
    pub reading_direction: Option<String>,
    pub total_book_count:  Option<i32>,
    pub book_count:        i32,
    pub cover_format_id:   Option<Uuid>,
    pub metadata:          Value,
    pub created_at:        DateTime<Utc>,
    pub updated_at:        DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Book {
    pub id:                Uuid,
    pub library_id:        Uuid,
    pub series_id:         Option<Uuid>,
    pub owner_id:          Uuid,
    pub folder_id:         Option<Uuid>,
    pub title:             String,
    pub sort_title:        Option<String>,
    pub book_key:          String,
    pub series_index:      Option<f64>,
    pub description:       Option<String>,
    pub publisher:         Option<String>,
    pub published_date:    Option<NaiveDate>,
    pub isbn:              Option<String>,
    pub identifiers:       Value,
    pub language:          Option<String>,
    pub page_count:        Option<i32>,
    pub rating:            Option<f64>,
    pub age_rating:        Option<i32>,
    pub reading_direction: Option<String>,
    pub release_date:      Option<NaiveDate>,
    pub authors:           Value,
    pub tags:              Value,
    pub cover_format_id:   Option<Uuid>,
    pub metadata:          Value,
    pub added_at:          DateTime<Utc>,
    pub file_modified_at:  Option<DateTime<Utc>>,
    pub last_scanned_at:   Option<DateTime<Utc>>,
    pub created_at:        DateTime<Utc>,
    pub updated_at:        DateTime<Utc>,
}

/// A book row enriched with its format codes (for list views).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BookListItem {
    pub id:              Uuid,
    pub library_id:      Uuid,
    pub series_id:       Option<Uuid>,
    pub title:           String,
    pub sort_title:      Option<String>,
    pub series_index:    Option<f64>,
    pub page_count:      Option<i32>,
    pub cover_format_id: Option<Uuid>,
    pub added_at:        DateTime<Utc>,
    pub formats:         Vec<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BookFormat {
    pub id:               Uuid,
    pub book_id:          Uuid,
    pub owner_id:         Uuid,
    pub format:           String,
    pub file_id:          Uuid,
    pub file_name:        String,
    pub storage_path:     String,
    pub size_bytes:       i64,
    pub content_hash:     Option<String>,
    pub page_count:       Option<i32>,
    pub format_metadata:  Value,
    pub pages_indexed:    bool,
    pub added_at:         DateTime<Utc>,
    pub file_modified_at: Option<DateTime<Utc>>,
    pub updated_at:       DateTime<Utc>,
}
