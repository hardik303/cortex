#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Returns the app name and window title for the topmost visible window on the
/// monitor whose screen-coordinate rect is (x, y, width, height).
/// CGWindowList is already in z-order (front to back), so the first intersecting
/// window is the one visually on top — regardless of which app has keyboard focus.
/// Both *out_app and *out_title are heap-allocated; free each with window_info_free().
void window_info_for_monitor(int32_t x, int32_t y, int32_t width, int32_t height,
                              char **out_app, char **out_title);

/// Free a string returned by window_info_for_monitor.
void window_info_free(char *s);

#ifdef __cplusplus
}
#endif
