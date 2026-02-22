@import Foundation;
@import AppKit;
@import CoreGraphics;

#include "window_info.h"
#include <stdlib.h>
#include <string.h>

// ---------------------------------------------------------------------------
// Public C API
// ---------------------------------------------------------------------------

void window_info_for_monitor(int32_t x, int32_t y, int32_t width, int32_t height,
                              char **out_app, char **out_title) {
    @autoreleasepool {
        // Screen-coordinate rect of this monitor (CGWindowBounds uses the same
        // coordinate space as CGDisplayBounds: origin at top-left of primary display).
        CGRect monitorRect = CGRectMake(x, y, width, height);

        // CGWindowListCopyWindowInfo returns all on-screen windows in z-order,
        // front to back — so the first window whose bounds intersect this monitor
        // is the topmost visible window on it.
        CFArrayRef windowList = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID);

        NSString *appName    = @"";
        NSString *windowTitle = @"";

        if (windowList) {
            NSArray *windows = (__bridge_transfer NSArray *)windowList;
            for (NSDictionary *info in windows) {
                // Only consider normal application windows (layer 0).
                NSNumber *layer = info[(id)kCGWindowLayer];
                if (layer.intValue != 0) continue;

                // Parse the window bounds dictionary into a CGRect.
                NSDictionary *boundsDict = info[(id)kCGWindowBounds];
                if (!boundsDict) continue;
                CGRect windowRect = CGRectZero;
                CGRectMakeWithDictionaryRepresentation(
                    (__bridge CFDictionaryRef)boundsDict, &windowRect);

                if (!CGRectIntersectsRect(windowRect, monitorRect)) continue;

                // kCGWindowOwnerName is the app name; no PID lookup needed.
                NSString *owner = info[(id)kCGWindowOwnerName];
                if (owner.length == 0) continue;

                appName     = owner;
                windowTitle = info[(id)kCGWindowName] ?: @"";
                break;
            }
        }

        const char *appUtf8 = appName.UTF8String ?: "";
        *out_app = malloc(strlen(appUtf8) + 1);
        strcpy(*out_app, appUtf8);

        const char *titleUtf8 = windowTitle.UTF8String ?: "";
        *out_title = malloc(strlen(titleUtf8) + 1);
        strcpy(*out_title, titleUtf8);
    }
}

void window_info_free(char *s) {
    free(s);
}
