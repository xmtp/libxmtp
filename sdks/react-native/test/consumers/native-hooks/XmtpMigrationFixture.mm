#import <React/RCTBridgeModule.h>

// Reports the raw OS sandbox root, separate from SDK path policy.
@interface XmtpMigrationFixture : NSObject <RCTBridgeModule>
@end

@implementation XmtpMigrationFixture
RCT_EXPORT_MODULE();
+ (BOOL)requiresMainQueueSetup { return NO; }
RCT_EXPORT_METHOD(osStorage:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject) {
  NSString *base = NSSearchPathForDirectoriesInDomains(NSApplicationSupportDirectory, NSUserDomainMask, YES).firstObject;
  NSString *bundle = NSBundle.mainBundle.bundleIdentifier;
  if (!base || !bundle) { reject(@"FIXTURE_ROOT", @"OS did not provide application roots", nil); return; }
  resolve(@{@"applicationSupportDirectory": base, @"bundleId": bundle});
}
@end
