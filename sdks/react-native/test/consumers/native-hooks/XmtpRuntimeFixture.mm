#import <React/RCTBridgeModule.h>
#import <React/RCTReloadCommand.h>
#import <UIKit/UIKit.h>

// Test application module. It is never included in the SDK npm product.
@interface XmtpRuntimeFixture : NSObject <RCTBridgeModule>
@end

@implementation XmtpRuntimeFixture
static NSString *launchURL = @"";
static NSInteger generation = 0;
static NSString *initializationStateWitness = nil;
RCT_EXPORT_MODULE();
+ (BOOL)requiresMainQueueSetup {
  return YES;
}
- (dispatch_queue_t)methodQueue {
  return dispatch_get_main_queue();
}
- (NSDictionary *)constantsToExport {
  if (initializationStateWitness == nil) {
    NSUserDefaults *defaults = NSUserDefaults.standardUserDefaults;
    NSNumber *initializationState =
        [defaults objectForKey:@"XmtpFixtureFirstReactInitializationState"];
    [defaults removeObjectForKey:@"XmtpFixtureFirstReactInitializationState"];
    initializationStateWitness =
        initializationState != nil &&
                initializationState.integerValue == UIApplicationStateBackground
            ? @"background"
            : @"not-background";
  }
  NSArray<NSString *> *arguments = NSProcessInfo.processInfo.arguments;
  NSUInteger launchArgument = [arguments indexOfObject:@"--xmtp-background"];
  if (launchArgument == NSNotFound) {
    launchArgument = [arguments indexOfObject:@"--xmtp-consumer"];
  }
  if (launchURL.length == 0 && launchArgument != NSNotFound &&
      launchArgument + 1 < arguments.count) {
    launchURL = arguments[launchArgument + 1];
  }
  return @{
    @"launchURL" : launchURL,
    @"generation" : @(generation),
    @"initializationState" : initializationStateWitness
  };
}
RCT_EXPORT_METHOD(processState : (RCTPromiseResolveBlock)
                      resolve reject : (RCTPromiseRejectBlock)reject) {
  resolve(UIApplication.sharedApplication.applicationState ==
                  UIApplicationStateBackground
              ? @"background"
              : @"foreground");
}
RCT_EXPORT_METHOD(reload : (NSString *)url expectedGeneration : (double)
                      expectedGeneration resolve : (RCTPromiseResolveBlock)
                          resolve reject : (RCTPromiseRejectBlock)reject) {
  if (generation != (NSInteger)expectedGeneration) {
    reject(@"STALE_GENERATION", @"Fixture reload belongs to an old runtime",
           nil);
    return;
  }
  launchURL = [url copy];
  generation += 1;
  resolve(nil);
  dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 100 * NSEC_PER_MSEC),
                 dispatch_get_main_queue(), ^{
                   // RCTHost's production reload handler invalidates
                   // RCTInstance and creates a replacement Hermes runtime,
                   // including in this release fixture.
                   RCTTriggerReloadCommandListeners(
                       @"XMTP installed conformance");
                 });
}
@end
