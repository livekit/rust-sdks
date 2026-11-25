#import <CoreVideo/CoreVideo.h>
#import <Vision/Vision.h>

#include <stdint.h>
#include <string.h>

// Copies source RGB into the BGRA plane required by Vision.
static int copy_rgb(CVPixelBufferRef image, const uint8_t *rgb, size_t width, size_t height) {
    if (CVPixelBufferLockBaseAddress(image, 0) != kCVReturnSuccess) {
        return 0;
    }
    uint8_t *base = CVPixelBufferGetBaseAddress(image);
    if (base != NULL) {
        const size_t stride = CVPixelBufferGetBytesPerRow(image);
        for (size_t y = 0; y < height; y++) {
            uint8_t *dest = base + y * stride;
            for (size_t x = 0; x < width; x++, rgb += 3, dest += 4) {
                dest[0] = rgb[2];
                dest[1] = rgb[1];
                dest[2] = rgb[0];
                dest[3] = 255;
            }
        }
    }
    CVPixelBufferUnlockBaseAddress(image, 0);
    return base != NULL;
}

// Samples non-background instance labels at fitting-grid pixel centers.
static int copy_mask(CVPixelBufferRef buffer, uint8_t *mask, size_t width, size_t height) {
    if (CVPixelBufferGetPixelFormatType(buffer) != kCVPixelFormatType_OneComponent8) {
        return 0;
    }
    if (CVPixelBufferLockBaseAddress(buffer, kCVPixelBufferLock_ReadOnly) != kCVReturnSuccess) {
        return 0;
    }
    const uint8_t *base = CVPixelBufferGetBaseAddress(buffer);
    if (base != NULL) {
        const size_t source_width = CVPixelBufferGetWidth(buffer);
        const size_t source_height = CVPixelBufferGetHeight(buffer);
        const size_t stride = CVPixelBufferGetBytesPerRow(buffer);
        for (size_t y = 0; y < height; y++) {
            const uint8_t *row = base + ((2 * y + 1) * source_height / (2 * height)) * stride;
            for (size_t x = 0; x < width; x++) {
                mask[y * width + x] = row[(2 * x + 1) * source_width / (2 * width)] != 0;
            }
        }
    }
    CVPixelBufferUnlockBaseAddress(buffer, kCVPixelBufferLock_ReadOnly);
    return base != NULL;
}

// Returns a binary mask without retaining any buffers or handles after the call.
int32_t vector_video_vision_mask(const uint8_t *rgb, size_t width, size_t height,
                                uint8_t *mask, size_t mask_width, size_t mask_height) {
    if (@available(macOS 14.0, *)) {
        @autoreleasepool {
            CVPixelBufferRef image = NULL;
            if (CVPixelBufferCreate(NULL, width, height, kCVPixelFormatType_32BGRA, NULL, &image)
                != kCVReturnSuccess) {
                return 2;
            }
            if (!copy_rgb(image, rgb, width, height)) {
                CVPixelBufferRelease(image);
                return 2;
            }
            VNGenerateForegroundInstanceMaskRequest *request =
                [[VNGenerateForegroundInstanceMaskRequest alloc] init];
            VNImageRequestHandler *handler =
                [[VNImageRequestHandler alloc] initWithCVPixelBuffer:image options:@{}];
            const BOOL success = [handler performRequests:@[request] error:NULL];
            CVPixelBufferRelease(image);
            if (!success) {
                return 3;
            }
            VNInstanceMaskObservation *result = request.results.firstObject;
            if (result == nil) {
                memset(mask, 0, mask_width * mask_height);
                return 0;
            }
            return copy_mask(result.instanceMask, mask, mask_width, mask_height) ? 0 : 4;
        }
    }
    return 1;
}
