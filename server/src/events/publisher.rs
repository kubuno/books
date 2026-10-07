use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};
use uuid::Uuid;

// Reserved for future book add/delete event publishing; not wired in the P0 skeleton.
#[allow(dead_code)]
pub struct EventPublisher {
    pub core_url: String,
    pub secret:   String,
    pub http:     Client,
}

#[allow(dead_code)]
impl EventPublisher {
    pub async fn publish(&self, event_type: &str, payload: Value) -> Result<()> {
        let url = format!("{}/internal/events/publish", self.core_url);
        self.http
            .post(&url)
            .header("X-Internal-Secret", &self.secret)
            .json(&json!({
                "event_type":    event_type,
                "source_module": "books",
                "payload":       payload,
            }))
            .send()
            .await?;
        Ok(())
    }

    pub async fn book_added(&self, book_id: Uuid, library_id: Uuid) -> Result<()> {
        self.publish("BookAdded", json!({
            "book_id":    book_id,
            "library_id": library_id,
        })).await
    }

    pub async fn book_deleted(&self, book_id: Uuid) -> Result<()> {
        self.publish("BookDeleted", json!({
            "book_id": book_id,
        })).await
    }
}
