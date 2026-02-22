use anyhow::Result;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use sqlx::types::Json;
use tracing::debug;

/// Create the connection pool.
pub async fn connect(url: &str) -> Result<PgPool> {
    let pool = PgPool::connect(url).await?;
    Ok(pool)
}

/// SHA-256 hex digest of the OCR text — used for deduplication.
pub fn text_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex::encode(hasher.finalize())
}

/// Insert a frame into the database, skipping exact duplicates.
///
/// Dedup logic: if the immediately preceding frame for this monitor has the
/// same OCR hash, skip insertion (screen hasn't changed).
pub async fn insert_frame(
    pool: &PgPool,
    captured_at: DateTime<Utc>,
    app_name: &str,
    window_title: &str,
    ocr_text: &str,
    monitor_id: i64,
    metadata: &serde_json::Value,
) -> Result<bool> {
    let hash = text_hash(ocr_text);

    // Check last hash for this monitor
    let last: Option<(String,)> = sqlx::query_as(
        "SELECT ocr_text_hash FROM frames WHERE monitor_id = $1 ORDER BY captured_at DESC LIMIT 1",
    )
    .bind(monitor_id)
    .fetch_optional(pool)
    .await?;

    if let Some((last_hash,)) = last {
        if last_hash == hash {
            debug!(monitor_id, "Skipping duplicate frame");
            return Ok(false);
        }
    }

    sqlx::query(
        "INSERT INTO frames
            (captured_at, app_name, window_title, ocr_text, monitor_id, ocr_text_hash, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(captured_at)
    .bind(app_name)
    .bind(window_title)
    .bind(ocr_text)
    .bind(monitor_id)
    .bind(&hash)
    .bind(Json(metadata))
    .execute(pool)
    .await?;

    Ok(true)
}
