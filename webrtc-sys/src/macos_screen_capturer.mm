/*
 * Copyright 2025 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "livekit/macos_screen_capturer.h"

#import <CoreMedia/CoreMedia.h>
#import <CoreVideo/CoreVideo.h>
#import <Foundation/Foundation.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#include <dispatch/dispatch.h>

@interface LKMacosScreenCaptureOutput : NSObject <SCStreamOutput> {
  livekit_ffi::MacosScreenCapturer* _owner;
}
- (instancetype)initWithOwner:(livekit_ffi::MacosScreenCapturer*)owner;
@end

@implementation LKMacosScreenCaptureOutput
- (instancetype)initWithOwner:(livekit_ffi::MacosScreenCapturer*)owner {
  self = [super init];
  if (self) {
    _owner = owner;
  }
  return self;
}

- (void)stream:(SCStream*)stream
    didOutputSampleBuffer:(CMSampleBufferRef)sampleBuffer
                   ofType:(SCStreamOutputType)type {
  (void)stream;
  if (type != SCStreamOutputTypeScreen || !_owner) {
    return;
  }

  CVPixelBufferRef pixel_buffer = CMSampleBufferGetImageBuffer(sampleBuffer);
  if (!pixel_buffer) {
    _owner->on_error(false);
    return;
  }

  CVPixelBufferRetain(pixel_buffer);
  _owner->on_frame(pixel_buffer);
}
@end

namespace livekit_ffi {
namespace {

SCShareableContent* copy_shareable_content() {
  if (@available(macOS 12.3, *)) {
    dispatch_semaphore_t sema = dispatch_semaphore_create(0);
    __block SCShareableContent* content = nil;

    [SCShareableContent
        getShareableContentExcludingDesktopWindows:NO
                                onScreenWindowsOnly:YES
                                 completionHandler:^(SCShareableContent* shareable_content,
                                                     NSError* error) {
                                   (void)error;
                                   if (shareable_content) {
                                     content = [shareable_content retain];
                                   }
                                   dispatch_semaphore_signal(sema);
                                 }];

    dispatch_semaphore_wait(sema, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));
#if !OS_OBJECT_USE_OBJC
    dispatch_release(sema);
#endif
    return content;
  }

  return nil;
}

SCDisplay* copy_display_with_id(uint32_t display_id) {
  SCShareableContent* content = copy_shareable_content();
  if (!content) {
    return nil;
  }

  SCDisplay* selected = nil;
  for (SCDisplay* display in content.displays) {
    if (display.displayID == display_id) {
      selected = [display retain];
      break;
    }
  }

  [content release];
  return selected;
}

}  // namespace

MacosScreenFrame::MacosScreenFrame(void* pixel_buffer)
    : pixel_buffer_(pixel_buffer) {}

MacosScreenFrame::~MacosScreenFrame() {
  if (pixel_buffer_) {
    CVPixelBufferRelease((CVPixelBufferRef)pixel_buffer_);
  }
}

int32_t MacosScreenFrame::width() const {
  return pixel_buffer_ ? (int32_t)CVPixelBufferGetWidth((CVPixelBufferRef)pixel_buffer_) : 0;
}

int32_t MacosScreenFrame::height() const {
  return pixel_buffer_ ? (int32_t)CVPixelBufferGetHeight((CVPixelBufferRef)pixel_buffer_) : 0;
}

uintptr_t MacosScreenFrame::pixel_buffer() const {
  if (!pixel_buffer_) {
    return 0;
  }

  CVPixelBufferRetain((CVPixelBufferRef)pixel_buffer_);
  return reinterpret_cast<uintptr_t>(pixel_buffer_);
}

MacosScreenCapturer::MacosScreenCapturer()
    : stream_(nullptr), output_(nullptr), queue_(nullptr), callback_(std::nullopt) {}

MacosScreenCapturer::~MacosScreenCapturer() {
  stop();
}

rust::Vec<MacosScreen> MacosScreenCapturer::get_screen_list() const {
  rust::Vec<MacosScreen> screens;
  if (@available(macOS 12.3, *)) {
    SCShareableContent* content = copy_shareable_content();
    if (!content) {
      return screens;
    }

    for (SCDisplay* display in content.displays) {
      screens.push_back(MacosScreen{
          display.displayID,
          rust::String("Display " + std::to_string(display.displayID)),
          (int32_t)display.width,
          (int32_t)display.height,
      });
    }

    [content release];
  }

  return screens;
}

bool MacosScreenCapturer::start(
    uint32_t display_id,
    uint32_t fps,
    rust::Box<MacosScreenCapturerCallbackWrapper> callback) {
  if (@available(macOS 12.3, *)) {
    stop();

    SCDisplay* display = copy_display_with_id(display_id);
    if (!display) {
      return false;
    }

    SCContentFilter* filter =
        [[SCContentFilter alloc] initWithDisplay:display excludingWindows:@[]];
    SCStreamConfiguration* config = [[SCStreamConfiguration alloc] init];
    config.width = display.width;
    config.height = display.height;
    config.showsCursor = NO;
    config.queueDepth = 3;
    config.pixelFormat = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange;
    config.minimumFrameInterval = CMTimeMake(1, fps == 0 ? 60 : fps);

    LKMacosScreenCaptureOutput* output =
        [[LKMacosScreenCaptureOutput alloc] initWithOwner:this];
    dispatch_queue_t queue =
        dispatch_queue_create("io.livekit.rust-sdks.local-video.sck", DISPATCH_QUEUE_SERIAL);
    SCStream* stream = [[SCStream alloc] initWithFilter:filter
                                          configuration:config
                                               delegate:nil];

    NSError* add_error = nil;
    BOOL added = [stream addStreamOutput:output
                                    type:SCStreamOutputTypeScreen
                      sampleHandlerQueue:queue
                                   error:&add_error];
    if (!added) {
      (void)add_error;
      [stream release];
      [output release];
      [config release];
      [filter release];
      [display release];
#if !OS_OBJECT_USE_OBJC
      dispatch_release(queue);
#endif
      return false;
    }

    dispatch_semaphore_t sema = dispatch_semaphore_create(0);
    __block BOOL started = NO;
    [stream startCaptureWithCompletionHandler:^(NSError* error) {
      started = error == nil;
      dispatch_semaphore_signal(sema);
    }];
    dispatch_semaphore_wait(sema, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC));
#if !OS_OBJECT_USE_OBJC
    dispatch_release(sema);
#endif

    [config release];
    [filter release];
    [display release];

    if (!started) {
      [stream release];
      [output release];
#if !OS_OBJECT_USE_OBJC
      dispatch_release(queue);
#endif
      return false;
    }

    callback_ = std::move(callback);
    stream_ = stream;
    output_ = output;
    queue_ = queue;
    return true;
  }

  return false;
}

void MacosScreenCapturer::stop() {
  if (@available(macOS 12.3, *)) {
    SCStream* stream = (SCStream*)stream_;
    if (stream) {
      [stream stopCaptureWithCompletionHandler:^(NSError* error) {
        (void)error;
      }];
      [stream release];
      stream_ = nullptr;
    }
  }

  if (output_) {
    [(LKMacosScreenCaptureOutput*)output_ release];
    output_ = nullptr;
  }

#if !OS_OBJECT_USE_OBJC
  if (queue_) {
    dispatch_release((dispatch_queue_t)queue_);
    queue_ = nullptr;
  }
#else
  queue_ = nullptr;
#endif

  callback_ = std::nullopt;
}

void MacosScreenCapturer::on_frame(void* pixel_buffer) {
  if (!callback_) {
    CVPixelBufferRelease((CVPixelBufferRef)pixel_buffer);
    return;
  }

  (*callback_)->on_capture_result(
      MacosScreenCaptureResult::Success,
      std::make_unique<MacosScreenFrame>(pixel_buffer));
}

void MacosScreenCapturer::on_error(bool permanent) {
  if (!callback_) {
    return;
  }

  (*callback_)->on_capture_result(
      permanent ? MacosScreenCaptureResult::ErrorPermanent
                : MacosScreenCaptureResult::ErrorTemporary,
      std::unique_ptr<MacosScreenFrame>());
}

std::unique_ptr<MacosScreenCapturer> new_macos_screen_capturer() {
  return std::make_unique<MacosScreenCapturer>();
}

}  // namespace livekit_ffi
