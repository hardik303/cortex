use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

#[repr(C)]
struct RawBrowserInfo {
    url: *mut c_char,
    tab_title: *mut c_char,
    tab_count: c_int,
}

extern "C" {
    fn browser_info_query(app_name: *const c_char) -> RawBrowserInfo;
    fn browser_info_free(info: RawBrowserInfo);
}

pub struct BrowserInfo {
    pub url: String,
    pub tab_title: String,
    pub tab_count: i32,
}

pub fn query(app_name: &str) -> BrowserInfo {
    let c_app = CString::new(app_name).unwrap_or_default();
    let raw = unsafe { browser_info_query(c_app.as_ptr()) };

    let url = if raw.url.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(raw.url) }
            .to_str()
            .unwrap_or("")
            .to_owned()
    };

    let tab_title = if raw.tab_title.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(raw.tab_title) }
            .to_str()
            .unwrap_or("")
            .to_owned()
    };

    let tab_count = raw.tab_count;
    unsafe { browser_info_free(raw) };

    BrowserInfo { url, tab_title, tab_count }
}
