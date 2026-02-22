@import Foundation;
@import Vision;
@import CoreImage;
@import CoreGraphics;
@import AppKit;

#include "ocr_wrapper.h"
#include <stdlib.h>
#include <string.h>

// ---------------------------------------------------------------------------
// Warm request — initialized once, kept alive so the language model stays loaded
// ---------------------------------------------------------------------------

static VNRecognizeTextRequest *g_request = nil;
static NSLock *g_lock = nil;

static void ensure_request(void) {
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        g_lock = [[NSLock alloc] init];
        g_request = [[VNRecognizeTextRequest alloc] init];
        g_request.recognitionLevel = VNRequestTextRecognitionLevelAccurate;
        g_request.usesLanguageCorrection = YES;
    });
}

// ---------------------------------------------------------------------------
// Reading-order sort helpers
// ---------------------------------------------------------------------------

static NSString *sort_observations_reading_order(NSArray<VNRecognizedTextObservation *> *observations) {
    if (observations.count == 0) return @"";

    // Vision uses bottom-left origin; sort descending by midY → top-to-bottom visually.
    NSArray *sorted = [observations sortedArrayUsingComparator:^NSComparisonResult(
        VNRecognizedTextObservation *a, VNRecognizedTextObservation *b) {
        CGFloat aMid = CGRectGetMidY(a.boundingBox);
        CGFloat bMid = CGRectGetMidY(b.boundingBox);
        if (aMid > bMid) return NSOrderedAscending;
        if (aMid < bMid) return NSOrderedDescending;
        return NSOrderedSame;
    }];

    // Group into lines using 50% vertical overlap ratio.
    NSMutableArray<NSMutableArray<VNRecognizedTextObservation *> *> *lines = [NSMutableArray array];

    for (VNRecognizedTextObservation *obs in sorted) {
        CGRect box = obs.boundingBox;
        CGFloat obsTop    = CGRectGetMaxY(box);
        CGFloat obsBottom = CGRectGetMinY(box);
        CGFloat obsHeight = CGRectGetHeight(box);

        BOOL placed = NO;
        for (NSMutableArray<VNRecognizedTextObservation *> *line in lines) {
            CGRect rep = line.firstObject.boundingBox;
            CGFloat lineTop    = CGRectGetMaxY(rep);
            CGFloat lineBottom = CGRectGetMinY(rep);
            CGFloat lineHeight = CGRectGetHeight(rep);

            CGFloat overlapTop    = MIN(obsTop, lineTop);
            CGFloat overlapBottom = MAX(obsBottom, lineBottom);
            CGFloat overlap = overlapTop - overlapBottom;

            CGFloat minHeight = MIN(obsHeight, lineHeight);
            if (minHeight > 0 && (overlap / minHeight) >= 0.5) {
                [line addObject:obs];
                placed = YES;
                break;
            }
        }
        if (!placed) {
            [lines addObject:[NSMutableArray arrayWithObject:obs]];
        }
    }

    // Within each line, sort left-to-right.
    NSMutableArray<NSString *> *lineStrings = [NSMutableArray array];
    for (NSMutableArray<VNRecognizedTextObservation *> *line in lines) {
        NSArray *lineSorted = [line sortedArrayUsingComparator:^NSComparisonResult(
            VNRecognizedTextObservation *a, VNRecognizedTextObservation *b) {
            CGFloat aX = CGRectGetMinX(a.boundingBox);
            CGFloat bX = CGRectGetMinX(b.boundingBox);
            if (aX < bX) return NSOrderedAscending;
            if (aX > bX) return NSOrderedDescending;
            return NSOrderedSame;
        }];

        NSMutableArray<NSString *> *words = [NSMutableArray array];
        for (VNRecognizedTextObservation *obs in lineSorted) {
            VNRecognizedText *top = [obs topCandidates:1].firstObject;
            if (top && top.string.length > 0) {
                [words addObject:top.string];
            }
        }
        if (words.count > 0) {
            [lineStrings addObject:[words componentsJoinedByString:@" "]];
        }
    }

    return [lineStrings componentsJoinedByString:@"\n"];
}

// ---------------------------------------------------------------------------
// Public C API
// ---------------------------------------------------------------------------

void ocr_prewarm(void) {
    @autoreleasepool {
        ensure_request();

        // Create a 1×1 white image and run a request through it to force the
        // language correction model to load before the first real capture.
        uint8_t white[4] = {0xFF, 0xFF, 0xFF, 0xFF}; // BGRA
        CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
        CGDataProviderRef dp = CGDataProviderCreateWithData(NULL, white, 4, NULL);
        CGImageRef img = CGImageCreate(1, 1, 8, 32, 4, cs,
            kCGBitmapByteOrder32Little | kCGImageAlphaPremultipliedFirst,
            dp, NULL, false, kCGRenderingIntentDefault);
        CGDataProviderRelease(dp);
        CGColorSpaceRelease(cs);

        if (img) {
            [g_lock lock];
            VNImageRequestHandler *h = [[VNImageRequestHandler alloc]
                initWithCGImage:img options:@{}];
            NSError *err = nil;
            [h performRequests:@[g_request] error:&err];
            [g_lock unlock];
            CGImageRelease(img);
        }
    }
}

char *ocr_recognize(const uint8_t *pixels, uint32_t width, uint32_t height, uint32_t bpr) {
    @autoreleasepool {
        ensure_request();

        // Build CGImage from raw BGRA pixels.
        CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();
        CGDataProviderRef provider = CGDataProviderCreateWithData(
            NULL, pixels, (size_t)bpr * height, NULL);

        CGImageRef cgImage = CGImageCreate(
            width, height,
            8, 32, bpr,
            colorSpace,
            kCGBitmapByteOrder32Little | kCGImageAlphaPremultipliedFirst,
            provider, NULL, false, kCGRenderingIntentDefault);

        CGDataProviderRelease(provider);
        CGColorSpaceRelease(colorSpace);

        if (!cgImage) {
            const char *err = "ocr_error: failed to create CGImage";
            char *result = malloc(strlen(err) + 1);
            strcpy(result, err);
            return result;
        }

        // Lock so the shared request is used by one thread at a time.
        [g_lock lock];
        VNImageRequestHandler *handler = [[VNImageRequestHandler alloc]
            initWithCGImage:cgImage options:@{}];
        NSError *visionError = nil;
        [handler performRequests:@[g_request] error:&visionError];
        // Snapshot results before releasing the lock.
        NSArray<VNRecognizedTextObservation *> *observations =
            visionError ? nil : [g_request.results copy];
        [g_lock unlock];

        CGImageRelease(cgImage);

        NSString *text = observations ? sort_observations_reading_order(observations) : @"";

        const char *utf8 = text.UTF8String ?: "";
        char *result = malloc(strlen(utf8) + 1);
        strcpy(result, utf8);
        return result;
    }
}

void ocr_free_result(char *result) {
    free(result);
}
