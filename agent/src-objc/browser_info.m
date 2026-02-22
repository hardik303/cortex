@import Foundation;
@import AppKit;

#include "browser_info.h"
#include <stdlib.h>
#include <string.h>

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

static char *dup_nsstring(NSString *s) {
    const char *utf8 = s.length > 0 ? s.UTF8String : "";
    if (!utf8) utf8 = "";
    char *result = malloc(strlen(utf8) + 1);
    strcpy(result, utf8);
    return result;
}

static NSString *run_applescript(NSString *src) {
    NSAppleScript *script = [[NSAppleScript alloc] initWithSource:src];
    NSDictionary *errInfo = nil;
    NSAppleEventDescriptor *desc = [script executeAndReturnError:&errInfo];
    if (errInfo || !desc) return nil;
    return desc.stringValue;
}

static NSString *run_applescript_int(NSString *src, int32_t *out) {
    NSAppleScript *script = [[NSAppleScript alloc] initWithSource:src];
    NSDictionary *errInfo = nil;
    NSAppleEventDescriptor *desc = [script executeAndReturnError:&errInfo];
    if (errInfo || !desc) { *out = -1; return nil; }
    *out = (int32_t)desc.int32Value;
    return nil;
}

// ---------------------------------------------------------------------------
// Per-browser AppleScript templates
// ---------------------------------------------------------------------------

// Chromium-based: Arc, Google Chrome, Brave Browser, Microsoft Edge, Chromium
static BrowserInfo query_chromium(NSString *app) {
    NSString *urlScript = [NSString stringWithFormat:
        @"tell application \"%@\" to get URL of active tab of front window", app];
    NSString *titleScript = [NSString stringWithFormat:
        @"tell application \"%@\" to get title of active tab of front window", app];
    NSString *countScript = [NSString stringWithFormat:
        @"tell application \"%@\" to get count of tabs of front window", app];

    NSString *url   = run_applescript(urlScript)   ?: @"";
    NSString *title = run_applescript(titleScript) ?: @"";
    int32_t count = -1;
    run_applescript_int(countScript, &count);

    return (BrowserInfo){ dup_nsstring(url), dup_nsstring(title), count };
}

// Safari
static BrowserInfo query_safari(void) {
    NSString *url   = run_applescript(@"tell application \"Safari\" to get URL of current tab of front window")   ?: @"";
    NSString *title = run_applescript(@"tell application \"Safari\" to get name of current tab of front window") ?: @"";
    int32_t count = -1;
    run_applescript_int(@"tell application \"Safari\" to get count of tabs of front window", &count);
    return (BrowserInfo){ dup_nsstring(url), dup_nsstring(title), count };
}

// Firefox (limited AppleScript support — title only via window name)
static BrowserInfo query_firefox(void) {
    NSString *title = run_applescript(@"tell application \"Firefox\" to get name of front window") ?: @"";
    return (BrowserInfo){ dup_nsstring(@""), dup_nsstring(title), -1 };
}

// ---------------------------------------------------------------------------
// Public C API
// ---------------------------------------------------------------------------

BrowserInfo browser_info_query(const char *app_name) {
    @autoreleasepool {
        NSString *app = [NSString stringWithUTF8String:app_name ?: ""];

        if ([app isEqualToString:@"Arc"] ||
            [app isEqualToString:@"Google Chrome"] ||
            [app isEqualToString:@"Brave Browser"] ||
            [app isEqualToString:@"Microsoft Edge"] ||
            [app isEqualToString:@"Chromium"]) {
            return query_chromium(app);
        }

        if ([app isEqualToString:@"Safari"] ||
            [app isEqualToString:@"Safari Technology Preview"]) {
            return query_safari();
        }

        if ([app isEqualToString:@"Firefox"] ||
            [app isEqualToString:@"Firefox Developer Edition"] ||
            [app isEqualToString:@"Firefox Nightly"]) {
            return query_firefox();
        }

        // Not a recognised browser
        return (BrowserInfo){ dup_nsstring(@""), dup_nsstring(@""), -1 };
    }
}

void browser_info_free(BrowserInfo info) {
    free(info.url);
    free(info.tab_title);
}
