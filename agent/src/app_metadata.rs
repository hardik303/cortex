use std::ffi::{CStr, CString};
use std::os::raw::c_char;

extern "C" {
    fn app_metadata_query(app_name: *const c_char) -> *mut c_char;
    fn app_metadata_free(json: *mut c_char);
}

/// Query app-specific metadata and return it as a parsed JSON value.
///
/// Browsers  → `{"type":"browser","url":"...","tab_title":"...","tab_count":N}`
/// Terminals → `{"type":"terminal","tty":"...","cwd":"...","foreground_cmd":"...","shell":"..."}`
/// Other     → `{"type":"app"}`
pub fn query(app_name: &str) -> serde_json::Value {
    let c_app = CString::new(app_name).unwrap_or_default();
    let raw = unsafe { app_metadata_query(c_app.as_ptr()) };

    if raw.is_null() {
        return serde_json::json!({"type": "app"});
    }

    let json_str = unsafe { CStr::from_ptr(raw) }
        .to_str()
        .unwrap_or("{}")
        .to_owned();

    unsafe { app_metadata_free(raw) };

    serde_json::from_str(&json_str).unwrap_or_else(|_| serde_json::json!({"type": "app"}))
}
