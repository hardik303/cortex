#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/// Pre-warm the Vision text recognition model.
/// Call once at startup (from a background thread) to pay the model-load cost
/// up front so that subsequent ocr_recognize calls return quickly.
void ocr_prewarm(void);

/// Run Apple Vision OCR on raw BGRA pixel data.
/// @param pixels    Pointer to raw pixel bytes (BGRA format from xcap)
/// @param width     Image width in pixels
/// @param height    Image height in pixels
/// @param bpr       Bytes per row
/// @return          Heap-allocated UTF-8 C string with OCR text (caller must free via ocr_free_result)
char *ocr_recognize(const uint8_t *pixels, uint32_t width, uint32_t height, uint32_t bpr);

/// Free a string previously returned by ocr_recognize.
void ocr_free_result(char *result);

#ifdef __cplusplus
}
#endif
