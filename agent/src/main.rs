mod app_metadata;
mod capture;
mod config;
mod db;
mod extract;
mod ocr;
mod window_info;

use anyhow::{Context, Result};
use chrono::Utc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging (RUST_LOG controls level, e.g. RUST_LOG=info)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Load configuration
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let cfg = config::Config::load(&config_path)
        .with_context(|| format!("Failed to load config from '{config_path}'"))?;

    info!("Connecting to database...");
    let pool = db::connect(&cfg.database.url).await?;
    let pool = Arc::new(pool);
    info!("Database connected.");

    // Pre-warm Vision: loads language correction model once so first real
    // capture returns in milliseconds instead of ~30 seconds.
    info!("Pre-warming Vision OCR model...");
    tokio::task::spawn_blocking(ocr::prewarm).await?;
    info!("Vision OCR model ready.");

    // Semaphore: max 2 concurrent Ollama NER calls (local inference, no rate limit concern).
    // Cap at 2 so we don't queue up too many frames while inference is running.
    let llm_sem = Arc::new(Semaphore::new(2));

    // ── Background: assign frames into sessions every 5 minutes ──────────────
    {
        let pool = Arc::clone(&pool);
        let gap = chrono::Duration::minutes(cfg.kg.session_gap_mins as i64);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(5 * 60));
            ticker.tick().await; // skip immediate first tick
            loop {
                ticker.tick().await;
                match db::assign_sessions(&pool, gap).await {
                    Ok(n) if n > 0 => info!(frames_assigned = n, "Sessions updated"),
                    Ok(_) => {}
                    Err(e) => warn!("assign_sessions failed: {e:#}"),
                }
            }
        });
    }

    // ── Background: expire raw OCR text every hour ────────────────────────────
    {
        let pool = Arc::clone(&pool);
        let ttl = chrono::Duration::days(cfg.kg.ocr_ttl_days as i64);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(60 * 60));
            ticker.tick().await; // skip immediate first tick
            loop {
                ticker.tick().await;
                match db::expire_ocr_text(&pool, ttl).await {
                    Ok(n) if n > 0 => info!(rows_expired = n, "OCR text expired"),
                    Ok(_) => {}
                    Err(e) => warn!("expire_ocr_text failed: {e:#}"),
                }
            }
        });
    }

    let interval = Duration::from_secs_f64(cfg.capture.interval_secs);
    info!(
        interval_secs = cfg.capture.interval_secs,
        "Starting capture loop"
    );

    loop {
        let cycle_start = Instant::now();

        // Capture all monitors
        let frames = match capture::capture_all_monitors() {
            Ok(f) => f,
            Err(e) => {
                error!("Screen capture failed: {e:#}");
                tokio::time::sleep(interval).await;
                continue;
            }
        };

        let captured_at = Utc::now();
        let mut all_empty = true;

        for frame in frames {
            let pixels = frame.pixels.clone();
            let width = frame.width;
            let height = frame.height;
            let bpr = frame.bytes_per_row;
            let monitor_id = frame.monitor_id;
            let mon_x = frame.monitor_x;
            let mon_y = frame.monitor_y;

            // Per-monitor window info: finds the topmost visible window on this
            // monitor by CGWindowList intersection — not just the focused app.
            let win = window_info::for_monitor(mon_x, mon_y, width, height);
            let app = win.app_name;
            let title = win.window_title;

            // App-specific metadata: browser URL/tabs or terminal cwd/cmd
            let metadata = app_metadata::query(&app);

            // OCR is CPU-intensive and synchronous — run on a blocking thread
            let ocr_text = match tokio::task::spawn_blocking(move || {
                ocr::recognize(&pixels, width, height, bpr)
            })
            .await
            {
                Ok(Ok(text)) => text,
                Ok(Err(e)) => {
                    error!(monitor_id, "OCR failed: {e:#}");
                    continue;
                }
                Err(e) => {
                    error!(monitor_id, "OCR task panicked: {e}");
                    continue;
                }
            };

            if !ocr_text.trim().is_empty() {
                all_empty = false;
            }

            let frame_id = match db::insert_frame(
                &pool,
                captured_at,
                &app,
                &title,
                &ocr_text,
                monitor_id as i64,
                &metadata,
            )
            .await
            {
                Ok(Some(id)) => {
                    info!(
                        monitor_id,
                        app = %app,
                        meta = %metadata,
                        ocr_len = ocr_text.len(),
                        "Frame inserted"
                    );
                    id
                }
                Ok(None) => {
                    info!(monitor_id, "Frame deduplicated (no change)");
                    continue;
                }
                Err(e) => {
                    error!(monitor_id, "DB insert failed: {e:#}");
                    continue;
                }
            };

            // ── Rule-based metadata extraction (synchronous, always runs) ──────
            let meta_edges = extract::from_metadata(&app, &metadata);
            if !meta_edges.is_empty() {
                let pool2 = Arc::clone(&pool);
                tokio::spawn(async move {
                    if let Err(e) = db::upsert_kg(&pool2, frame_id, &meta_edges).await {
                        warn!("upsert_kg (metadata) failed: {e:#}");
                    }
                });
            }

            // ── LLM NER via Ollama (async, sampled, non-fatal) ────────────────
            let ollama_endpoint = cfg.kg.ollama_endpoint.clone();
            let ollama_model = cfg.kg.ollama_model.clone();
            let sample_rate = cfg.kg.llm_sample_rate;
            if !ollama_endpoint.is_empty()
                && !ocr_text.trim().is_empty()
                && rand::random::<f64>() < sample_rate
            {
                let pool2 = Arc::clone(&pool);
                let sem = Arc::clone(&llm_sem);
                let app2 = app.clone();
                let title2 = title.clone();
                let ocr2 = ocr_text.clone();
                tokio::spawn(async move {
                    let _permit = sem.acquire().await;
                    let edges = extract::from_ocr_llm(
                        &app2, &title2, &ocr2, &ollama_endpoint, &ollama_model,
                    ).await;
                    if !edges.is_empty() {
                        if let Err(e) = db::upsert_kg(&pool2, frame_id, &edges).await {
                            warn!("upsert_kg (LLM) failed: {e:#}");
                        }
                    }
                });
            }
        }

        if all_empty {
            warn!(
                "All monitors returned empty OCR text. Check that Screen Recording \
                 permission is granted in System Settings → Privacy & Security → Screen Recording."
            );
        }

        // Sleep for the remainder of the interval
        let elapsed = cycle_start.elapsed();
        if elapsed < interval {
            tokio::time::sleep(interval - elapsed).await;
        }
    }
}
