@import Foundation;
@import AppKit;

#include "app_metadata.h"
#include <libproc.h>
#include <stdlib.h>
#include <string.h>
#include <sys/sysctl.h>

// ---------------------------------------------------------------------------
// Generic helpers
// ---------------------------------------------------------------------------

static NSString *run_applescript(NSString *src) {
    NSAppleScript *script = [[NSAppleScript alloc] initWithSource:src];
    NSDictionary *err = nil;
    NSAppleEventDescriptor *desc = [script executeAndReturnError:&err];
    return err ? nil : desc.stringValue;
}

static NSString *run_command(NSString *path, NSArray<NSString *> *args) {
    NSPipe *out = [NSPipe pipe];
    NSTask *task = [NSTask new];
    task.executableURL = [NSURL fileURLWithPath:path];
    task.arguments = args;
    task.standardOutput = out;
    task.standardError = [NSPipe pipe];
    NSError *err = nil;
    if (![task launchAndReturnError:&err] || err) return nil;
    [task waitUntilExit];
    NSData *data = [[out fileHandleForReading] readDataToEndOfFile];
    return [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
}

// ---------------------------------------------------------------------------
// Terminal helpers
// ---------------------------------------------------------------------------

// Returns all (pid, stat, comm) rows for processes on a TTY short name.
// ttyShort: e.g. "ttys003" (without /dev/ prefix)
static NSArray<NSDictionary *> *ps_for_tty(NSString *ttyShort) {
    if (ttyShort.length == 0) return @[];
    NSString *out = run_command(@"/bin/ps", @[@"-t", ttyShort, @"-o", @"pid=,stat=,comm="]);
    if (!out) return @[];

    NSMutableArray *rows = [NSMutableArray array];
    for (NSString *raw in [out componentsSeparatedByString:@"\n"]) {
        // Split on whitespace; columns: [pid, stat, comm]
        NSMutableArray *cols = [NSMutableArray array];
        for (NSString *tok in [raw componentsSeparatedByCharactersInSet:NSCharacterSet.whitespaceCharacterSet]) {
            if (tok.length > 0) [cols addObject:tok];
        }
        if (cols.count < 3) continue;
        [rows addObject:@{@"pid": @([cols[0] intValue]), @"stat": cols[1], @"comm": cols[2]}];
    }
    return rows;
}

// Working directory of a process via libproc.
static NSString *cwd_for_pid(pid_t pid) {
    struct proc_vnodepathinfo vpi;
    int ret = proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, 0, &vpi, sizeof(vpi));
    if (ret <= 0) return @"";
    return [NSString stringWithUTF8String:vpi.pvi_cdir.vip_path] ?: @"";
}

// Full command line of a process via KERN_PROCARGS2.
static NSString *cmdline_for_pid(pid_t pid) {
    int mib[3] = {CTL_KERN, KERN_PROCARGS2, (int)pid};
    size_t size = 0;
    if (sysctl(mib, 3, NULL, &size, NULL, 0) != 0 || size == 0) return @"";

    char *buf = malloc(size);
    if (!buf) return @"";
    if (sysctl(mib, 3, buf, &size, NULL, 0) != 0) { free(buf); return @""; }

    // KERN_PROCARGS2: int argc | exec_path\0 | \0... | argv[0]\0 | argv[1]\0 | ...
    int argc;
    memcpy(&argc, buf, sizeof(int));

    char *ptr = buf + sizeof(int);
    char *end = buf + size;

    // Skip exec_path
    while (ptr < end && *ptr) ptr++;
    while (ptr < end && !*ptr) ptr++;

    // Collect up to argc args
    NSMutableArray<NSString *> *parts = [NSMutableArray array];
    for (int i = 0; i < argc && ptr < end; i++) {
        NSString *arg = [NSString stringWithUTF8String:ptr];
        if (arg) [parts addObject:arg];
        while (ptr < end && *ptr) ptr++;
        ptr++;
    }
    free(buf);
    return [parts componentsJoinedByString:@" "];
}

static NSSet<NSString *> *shell_names(void) {
    return [NSSet setWithArray:@[@"zsh", @"bash", @"fish", @"sh", @"csh",
                                 @"tcsh", @"dash", @"ksh", @"elvish", @"nu"]];
}

// Login shells show up as "-zsh", "-bash", etc. Strip the leading dash.
static NSString *bare_comm(NSString *comm) {
    NSString *base = comm.lastPathComponent;
    return [base hasPrefix:@"-"] ? [base substringFromIndex:1] : base;
}

// Given a TTY short name, find foreground process and return metadata.
static NSDictionary *fg_info_for_tty(NSString *ttyShort) {
    NSArray *rows = ps_for_tty(ttyShort);
    NSSet *shells = shell_names();

    // Rows with '+' in stat are in the foreground process group.
    NSMutableArray *fg = [NSMutableArray array];
    for (NSDictionary *row in rows) {
        if ([row[@"stat"] containsString:@"+"]) [fg addObject:row];
    }
    if (fg.count == 0) return @{};

    // Prefer a non-shell command (means something is actively running).
    NSDictionary *chosen = fg.firstObject;
    for (NSDictionary *row in fg) {
        NSString *comm = bare_comm(row[@"comm"]);
        if (![shells containsObject:comm]) { chosen = row; break; }
    }

    pid_t pid = [chosen[@"pid"] intValue];
    NSString *comm    = bare_comm(chosen[@"comm"]);
    NSString *cwd     = cwd_for_pid(pid);
    NSString *cmdline = cmdline_for_pid(pid);
    NSString *shell   = @"";

    // Detect shell: if the foreground cmd is a shell, it IS the shell;
    // otherwise walk ALL tty rows (shell steps into background while user cmd runs).
    if ([shells containsObject:comm]) {
        shell = comm;
    } else {
        for (NSDictionary *row in rows) {
            NSString *c = bare_comm(row[@"comm"]);
            if ([shells containsObject:c]) { shell = c; break; }
        }
    }

    return @{
        @"tty":             ttyShort,
        @"cwd":             cwd ?: @"",
        @"foreground_cmd":  cmdline.length > 0 ? cmdline : comm,
        @"shell":           shell,
    };
}

// ---------------------------------------------------------------------------
// Browser queries
// ---------------------------------------------------------------------------

static NSDictionary *query_chromium(NSString *app) {
    NSString *url   = run_applescript([NSString stringWithFormat:
        @"tell application \"%@\" to get URL of active tab of front window", app]) ?: @"";
    NSString *title = run_applescript([NSString stringWithFormat:
        @"tell application \"%@\" to get title of active tab of front window", app]) ?: @"";
    NSString *countStr = run_applescript([NSString stringWithFormat:
        @"tell application \"%@\" to get count of tabs of front window", app]);
    return @{@"url": url, @"tab_title": title,
             @"tab_count": countStr ? @([countStr intValue]) : @(-1)};
}

static NSDictionary *query_safari(void) {
    NSString *url   = run_applescript(@"tell application \"Safari\" to get URL of current tab of front window") ?: @"";
    NSString *title = run_applescript(@"tell application \"Safari\" to get name of current tab of front window") ?: @"";
    NSString *countStr = run_applescript(@"tell application \"Safari\" to get count of tabs of front window");
    return @{@"url": url, @"tab_title": title,
             @"tab_count": countStr ? @([countStr intValue]) : @(-1)};
}

// ---------------------------------------------------------------------------
// Terminal queries
// ---------------------------------------------------------------------------

static NSDictionary *query_terminal_app(void) {
    NSString *ttyPath = run_applescript(@"tell application \"Terminal\" to get tty of front window");
    if (!ttyPath) return @{};
    NSString *ttyShort = ttyPath.lastPathComponent; // "/dev/ttys003" → "ttys003"
    return fg_info_for_tty(ttyShort);
}

static NSDictionary *query_iterm2(void) {
    NSString *ttyPath = run_applescript(
        @"tell application \"iTerm2\" to get tty of current session of current tab of current window");
    NSString *cwd     = run_applescript(
        @"tell application \"iTerm2\" to get current working directory of current session of current tab of current window");

    NSMutableDictionary *info = [NSMutableDictionary dictionary];
    if (cwd) info[@"cwd"] = cwd;

    if (ttyPath) {
        NSString *ttyShort = ttyPath.lastPathComponent;
        info[@"tty"] = ttyShort;
        NSDictionary *fg = fg_info_for_tty(ttyShort);
        [info addEntriesFromDictionary:fg];
        if (cwd) info[@"cwd"] = cwd; // prefer iTerm2's own cwd (more accurate)
    }
    return info;
}

// ---------------------------------------------------------------------------
// Public C API
// ---------------------------------------------------------------------------

char *app_metadata_query(const char *app_name) {
    @autoreleasepool {
        NSString *app = [NSString stringWithUTF8String:app_name ?: ""];
        NSMutableDictionary *meta = [NSMutableDictionary dictionary];

        // ── Browsers ──────────────────────────────────────────────────────
        if ([@[@"Arc", @"Google Chrome", @"Brave Browser",
               @"Microsoft Edge", @"Chromium"] containsObject:app]) {
            meta[@"type"] = @"browser";
            [meta addEntriesFromDictionary:query_chromium(app)];

        } else if ([@[@"Safari", @"Safari Technology Preview"] containsObject:app]) {
            meta[@"type"] = @"browser";
            [meta addEntriesFromDictionary:query_safari()];

        } else if ([@[@"Firefox", @"Firefox Developer Edition",
                      @"Firefox Nightly"] containsObject:app]) {
            meta[@"type"] = @"browser";
            NSString *title = run_applescript(@"tell application \"Firefox\" to get name of front window");
            if (title) meta[@"tab_title"] = title;

        // ── Terminals ─────────────────────────────────────────────────────
        } else if ([app isEqualToString:@"Terminal"]) {
            meta[@"type"] = @"terminal";
            [meta addEntriesFromDictionary:query_terminal_app()];

        } else if ([app isEqualToString:@"iTerm2"]) {
            meta[@"type"] = @"terminal";
            [meta addEntriesFromDictionary:query_iterm2()];

        } else if ([@[@"Warp", @"Alacritty", @"Kitty", @"Hyper",
                      @"Ghostty"] containsObject:app]) {
            // Modern terminals with limited AppleScript — tty via process listing
            meta[@"type"] = @"terminal";

        } else {
            meta[@"type"] = @"app";
        }

        NSData *jsonData = [NSJSONSerialization dataWithJSONObject:meta options:0 error:nil];
        NSString *jsonStr = [[NSString alloc] initWithData:jsonData encoding:NSUTF8StringEncoding] ?: @"{}";
        const char *utf8 = jsonStr.UTF8String ?: "{}";
        char *result = malloc(strlen(utf8) + 1);
        strcpy(result, utf8);
        return result;
    }
}

void app_metadata_free(char *json) {
    free(json);
}
