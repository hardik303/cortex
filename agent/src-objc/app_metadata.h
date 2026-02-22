#pragma once

#ifdef __cplusplus
extern "C" {
#endif

/// Returns a heap-allocated JSON string with app-specific metadata.
///
/// Browsers  → {"type":"browser","url":"...","tab_title":"...","tab_count":N}
/// Terminals → {"type":"terminal","tty":"...","cwd":"...","foreground_cmd":"...","shell":"..."}
/// Other     → {"type":"app"}
///
/// Caller must free with app_metadata_free().
char *app_metadata_query(const char *app_name);

/// Free a string returned by app_metadata_query.
void app_metadata_free(char *json);

#ifdef __cplusplus
}
#endif
