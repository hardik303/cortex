use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

extern "C" {
    fn window_info_for_monitor(
        x: c_int,
        y: c_int,
        width: c_int,
        height: c_int,
        out_app: *mut *mut c_char,
        out_title: *mut *mut c_char,
    );
    fn window_info_free(s: *mut c_char);
}

/// RAII wrapper for C strings returned by window_info_* functions.
struct WinStr(*mut c_char);

impl Drop for WinStr {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { window_info_free(self.0) };
        }
    }
}

fn cstr_to_string(raw: *mut c_char) -> String {
    let s = WinStr(raw);
    if s.0.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(s.0) }
        .to_str()
        .unwrap_or("")
        .to_owned()
}

/// App name and window title for the topmost visible window on the given monitor rect.
pub struct MonitorWindowInfo {
    pub app_name: String,
    pub window_title: String,
}

pub fn for_monitor(x: i32, y: i32, width: u32, height: u32) -> MonitorWindowInfo {
    let mut raw_app: *mut c_char = std::ptr::null_mut();
    let mut raw_title: *mut c_char = std::ptr::null_mut();

    unsafe {
        window_info_for_monitor(
            x as c_int,
            y as c_int,
            width as c_int,
            height as c_int,
            &mut raw_app,
            &mut raw_title,
        );
    }

    MonitorWindowInfo {
        app_name: cstr_to_string(raw_app),
        window_title: cstr_to_string(raw_title),
    }
}
