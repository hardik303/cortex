use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use sqlx::types::Json;
use tracing::debug;

use crate::extract::ExtractedEdge;

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
///
/// Returns `Ok(Some(frame_id))` on insert, `Ok(None)` on dedup skip.
pub async fn insert_frame(
    pool: &PgPool,
    captured_at: DateTime<Utc>,
    app_name: &str,
    window_title: &str,
    ocr_text: &str,
    monitor_id: i64,
    metadata: &serde_json::Value,
) -> Result<Option<i64>> {
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
            return Ok(None);
        }
    }

    let row: (i64,) = sqlx::query_as(
        "INSERT INTO frames
            (captured_at, app_name, window_title, ocr_text, monitor_id, ocr_text_hash, metadata)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(captured_at)
    .bind(app_name)
    .bind(window_title)
    .bind(ocr_text)
    .bind(monitor_id)
    .bind(&hash)
    .bind(Json(metadata))
    .fetch_one(pool)
    .await?;

    Ok(Some(row.0))
}

/// Upsert KG nodes and insert edges for a single frame.
///
/// Each `ExtractedEdge` resolves to:
///   - Optional src node (UPSERT)
///   - dst node (UPSERT)
///   - kg_edges row (INSERT)
///
/// All done in a single transaction.
pub async fn upsert_kg(pool: &PgPool, frame_id: i64, edges: &[ExtractedEdge]) -> Result<()> {
    if edges.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;

    for edge in edges {
        // Resolve src node (if present)
        let src_node_id: Option<i64> = if let (Some(kind), Some(val)) = (&edge.src_kind, &edge.src_value) {
            let row: (i64,) = sqlx::query_as(
                "INSERT INTO kg_nodes (node_type, value)
                 VALUES ($1, $2)
                 ON CONFLICT (node_type, value) DO UPDATE SET node_type = EXCLUDED.node_type
                 RETURNING id",
            )
            .bind(kind)
            .bind(val)
            .fetch_one(&mut *tx)
            .await?;
            Some(row.0)
        } else {
            None
        };

        // Resolve dst node (always present)
        let dst_row: (i64,) = sqlx::query_as(
            "INSERT INTO kg_nodes (node_type, value)
             VALUES ($1, $2)
             ON CONFLICT (node_type, value) DO UPDATE SET node_type = EXCLUDED.node_type
             RETURNING id",
        )
        .bind(&edge.dst_kind)
        .bind(&edge.dst_value)
        .fetch_one(&mut *tx)
        .await?;
        let dst_node_id = dst_row.0;

        // Insert edge
        sqlx::query(
            "INSERT INTO kg_edges (frame_id, src_node_id, relation, dst_node_id)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(frame_id)
        .bind(src_node_id)
        .bind(&edge.relation)
        .bind(dst_node_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Expire raw OCR text older than `ttl`.
/// Sets `ocr_text = NULL` for affected rows; `ocr_text_hash` is preserved for dedup.
/// Returns the number of rows updated.
pub async fn expire_ocr_text(pool: &PgPool, ttl: Duration) -> Result<u64> {
    let cutoff = Utc::now() - ttl;
    let result = sqlx::query(
        "UPDATE frames
         SET ocr_text = NULL
         WHERE captured_at < $1
           AND ocr_text IS NOT NULL",
    )
    .bind(cutoff)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Group unassigned frames into sessions by gap analysis.
///
/// A "session" is a contiguous run of frames where consecutive frames are
/// within `gap` of each other.  The most-recent open group is left unassigned
/// until it ages past `gap` (avoids cutting an active session in two).
///
/// Safe under concurrent calls: all queries filter `WHERE session_id IS NULL`.
///
/// Returns the number of frames assigned.
pub async fn assign_sessions(pool: &PgPool, gap: Duration) -> Result<u64> {
    // Fetch all unassigned frames sorted by time
    let rows: Vec<(i64, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, captured_at FROM frames
         WHERE session_id IS NULL
         ORDER BY captured_at ASC",
    )
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(0);
    }

    let gap_std = gap.to_std().unwrap_or(std::time::Duration::from_secs(30 * 60));
    let now = Utc::now();

    // Group into runs
    let mut groups: Vec<Vec<(i64, DateTime<Utc>)>> = Vec::new();
    let mut current: Vec<(i64, DateTime<Utc>)> = Vec::new();

    for row in rows {
        if current.is_empty() {
            current.push(row);
        } else {
            let last_time = current.last().unwrap().1;
            let diff = (row.1 - last_time).to_std().unwrap_or(std::time::Duration::ZERO);
            if diff > gap_std {
                groups.push(std::mem::take(&mut current));
            }
            current.push(row);
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }

    let mut total_assigned: u64 = 0;

    for group in groups {
        let started_at = group.first().unwrap().1;
        let ended_at = group.last().unwrap().1;

        // Leave the trailing open group unassigned if it hasn't aged past gap
        let age = (now - ended_at).to_std().unwrap_or(std::time::Duration::ZERO);
        if age < gap_std {
            continue;
        }

        let frame_count = group.len() as i32;
        let frame_ids: Vec<i64> = group.iter().map(|(id, _)| *id).collect();

        let mut tx = pool.begin().await?;

        // Create session
        let session_row: (i64,) = sqlx::query_as(
            "INSERT INTO kg_sessions (started_at, ended_at, frame_count)
             VALUES ($1, $2, $3)
             RETURNING id",
        )
        .bind(started_at)
        .bind(ended_at)
        .bind(frame_count)
        .fetch_one(&mut *tx)
        .await?;
        let session_id = session_row.0;

        // Assign frames
        let rows_affected = sqlx::query(
            "UPDATE frames SET session_id = $1
             WHERE id = ANY($2) AND session_id IS NULL",
        )
        .bind(session_id)
        .bind(&frame_ids)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        tx.commit().await?;
        total_assigned += rows_affected;
    }

    Ok(total_assigned)
}
