#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Metadata extracted from the active browser tab.
/// All strings are heap-allocated UTF-8; free each with browser_info_free().
typedef struct {
    char *url;          ///< Active tab URL (empty string if not a browser / unsupported)
    char *tab_title;    ///< Active tab page title
    int32_t tab_count;  ///< Number of open tabs in the front window (-1 if unknown)
} BrowserInfo;

/// Query the active tab of the frontmost browser window using AppleScript.
/// Supports: Arc, Google Chrome, Safari, Brave Browser, Microsoft Edge, Firefox.
/// Returns a BrowserInfo struct; call browser_info_free() on it when done.
BrowserInfo browser_info_query(const char *app_name);

/// Free all strings inside a BrowserInfo returned by browser_info_query.
void browser_info_free(BrowserInfo info);

#ifdef __cplusplus
}
#endif
