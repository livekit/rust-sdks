#import "sdk/objc/base/RTCVideoCapturer.h"
#import "sdk/objc/components/capturer/RTCCameraVideoCapturer.h"

void LKPrintDevices() {
    NSArray<AVCaptureDevice*>* devices = [RTCCameraVideoCapturer captureDevices];
    [devices enumerateObjectsUsingBlock:^(AVCaptureDevice* device, NSUInteger i,
                                          BOOL* stop) {
        NSLog(@"%@", device.localizedName);
        NSLog(@"%@", device.uniqueID);
        NSLog(@"%@", device.modelID);
    }];
}