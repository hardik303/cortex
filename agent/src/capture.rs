use anyhow::Result;

/// A single captured monitor frame.
pub struct CapturedFrame {
    pub monitor_id: u32,
    /// Top-left corner of this monitor in global screen coordinates.
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: u32,
}

/// Capture all monitors and return their raw pixel data.
pub fn capture_all_monitors() -> Result<Vec<CapturedFrame>> {
    let monitors = xcap::Monitor::all()?;
    let mut frames = Vec::with_capacity(monitors.len());

    for (idx, monitor) in monitors.into_iter().enumerate() {
        let monitor_x = monitor.x().unwrap_or(0);
        let monitor_y = monitor.y().unwrap_or(0);
        let image = monitor.capture_image()?;

        let width = image.width();
        let height = image.height();
        let rgba = image.into_raw(); // RGBA

        // xcap returns RGBA; Vision/CoreGraphics needs BGRA — swap R and B.
        let mut bgra = Vec::with_capacity(rgba.len());
        for chunk in rgba.chunks_exact(4) {
            bgra.push(chunk[2]); // B
            bgra.push(chunk[1]); // G
            bgra.push(chunk[0]); // R
            bgra.push(chunk[3]); // A
        }

        let bytes_per_row = width * 4;
        frames.push(CapturedFrame {
            monitor_id: idx as u32,
            monitor_x,
            monitor_y,
            pixels: bgra,
            width,
            height,
            bytes_per_row,
        });
    }

    Ok(frames)
}
