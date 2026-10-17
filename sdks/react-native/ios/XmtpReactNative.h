#import <Foundation/Foundation.h>
#import <React/RCTBridgeModule.h>
#import <React/RCTInvalidating.h>
#import <ReactCommon/RCTTurboModule.h>

@interface XmtpReactNative
    : NSObject <RCTBridgeModule, RCTTurboModule, RCTInvalidating>
@end
