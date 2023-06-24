#import <AVFoundation/AVFoundation.h>

#include <vector>
#include "livekit/macos/media_devices.h"

namespace livekit {

DeviceFacing to_rust_facing(AVCaptureDevicePosition position) {
  switch (position) {
    case AVCaptureDevicePositionBack:
      return DeviceFacing::Environment;
    case AVCaptureDevicePositionFront:
      return DeviceFacing::User;
    case AVCaptureDevicePositionUnspecified:
      return DeviceFacing::Unknown;
  }
}

std::vector<DeviceInfo> MacMediaDevices::ListDevices() const {
  std::vector<DeviceInfo> devices;

  // video devices
  NSArray* videoDevices = [AVCaptureDevice devicesWithMediaType:AVMediaTypeVideo];
  for (AVCaptureDevice* device in videoDevices) {
    DeviceInfo info{};
    info.facing = to_rust_facing(device.position);
    info.id = [device.uniqueID UTF8String];
    info.name = [device.localizedName UTF8String];
    info.kind = DeviceKind::VideoInput;
    devices.push_back(info);
  }

  // audio devices

  return devices;
}

}  // namespace livekit