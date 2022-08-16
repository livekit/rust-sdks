//
//  main.m
//  ios-project
//
//  Created by Théo Monnom on 15/08/2022.
//

#import <UIKit/UIKit.h>
#import "AppDelegate.h"
#import <WebRTC/RTCPeerConnectionFactory.h>

void test_rust(void);


int main(int argc, char * argv[]) {
    NSString * appDelegateClassName;
    @autoreleasepool {
        // Setup code that might create autoreleased objects goes here.
        appDelegateClassName = NSStringFromClass([AppDelegate class]);
    }
    

    test_rust();
    
    printf("This is a test");
    return UIApplicationMain(argc, argv, nil, appDelegateClassName);
}
