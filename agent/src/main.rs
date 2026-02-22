mod app_metadata;
mod capture;
mod config;
mod db;
mod ocr;
mod window_info;

use anyhow::{Context, Result};
use chrono::Utc;
use std::time::{Duration, Instant};
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
    info!("Database connected.");

    // Pre-warm Vision: loads language correction model once so first real
    // capture returns in milliseconds instead of ~30 seconds.
    info!("Pre-warming Vision OCR model...");
    tokio::task::spawn_blocking(ocr::prewarm).await?;
    info!("Vision OCR model ready.");

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

            match db::insert_frame(
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
                Ok(true) => info!(
                    monitor_id,
                    app = %app,
                    meta = %metadata,
                    ocr_len = ocr_text.len(),
                    "Frame inserted"
                ),
                Ok(false) => info!(monitor_id, "Frame deduplicated (no change)"),
                Err(e) => error!(monitor_id, "DB insert failed: {e:#}"),
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
