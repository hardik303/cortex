use anyhow::Result;
use std::ffi::CStr;
use std::os::raw::{c_char, c_uint};

extern "C" {
    fn ocr_prewarm();
    fn ocr_recognize(pixels: *const u8, width: c_uint, height: c_uint, bpr: c_uint) -> *mut c_char;
    fn ocr_free_result(result: *mut c_char);
}

/// RAII wrapper around the heap-allocated C string returned by ocr_recognize.
struct OcrResult(*mut c_char);

impl Drop for OcrResult {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { ocr_free_result(self.0) };
        }
    }
}

/// Pre-warm the Vision model — call once at startup from a blocking thread.
pub fn prewarm() {
    unsafe { ocr_prewarm() };
}

/// Run Apple Vision OCR on raw BGRA pixels.
/// This is synchronous and CPU-intensive — callers should use spawn_blocking.
pub fn recognize(pixels: &[u8], width: u32, height: u32, bytes_per_row: u32) -> Result<String> {
    let raw = unsafe {
        ocr_recognize(
            pixels.as_ptr(),
            width as c_uint,
            height as c_uint,
            bytes_per_row as c_uint,
        )
    };

    let result = OcrResult(raw);
    if result.0.is_null() {
        return Ok(String::new());
    }

    let text = unsafe { CStr::from_ptr(result.0) }
        .to_str()
        .unwrap_or("")
        .to_owned();

    Ok(text)
}
