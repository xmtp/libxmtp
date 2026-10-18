#import "XmtpReactNative.h"
#include "../cpp/XmtpNative.h"
#include "../cpp/XmtpRuntime.h"
#import <UIKit/UIKit.h>
#include <mutex>

namespace {
struct ModuleState {
  std::mutex mutex;
  bool invalidated = false;
  std::shared_ptr<xmtp::reactnative::Runtime> owner;
};

bool observeLifecycle() {
  __block bool seeded = false;
  void (^observe)(void) = ^{
    static dispatch_once_t once;
    dispatch_once(&once, ^{
      NSNotificationCenter *center = NSNotificationCenter.defaultCenter;
      // The notification center retains these process observers. They are
      // independent of React runtime generations and never schedule JS work.
      [center addObserverForName:UIApplicationDidEnterBackgroundNotification
                          object:nil
                           queue:NSOperationQueue.mainQueue
                      usingBlock:^(NSNotification *) {
                        xmtp::reactnative::setForeground(false);
                      }];
      [center addObserverForName:UIApplicationWillEnterForegroundNotification
                          object:nil
                           queue:NSOperationQueue.mainQueue
                      usingBlock:^(NSNotification *) {
                        xmtp::reactnative::setForeground(true);
                      }];
    });
    seeded = xmtp::reactnative::seedLifecycle(
        UIApplication.sharedApplication.applicationState !=
        UIApplicationStateBackground);
  };
  if (NSThread.isMainThread)
    observe();
  else
    dispatch_sync(dispatch_get_main_queue(), observe);
  return seeded;
}

std::string storageDirectory() {
  NSURL *base = [NSFileManager.defaultManager
                    URLsForDirectory:NSApplicationSupportDirectory
                           inDomains:NSUserDomainMask]
                    .firstObject;
  NSString *bundle = NSBundle.mainBundle.bundleIdentifier;
  if (base == nil || bundle.length == 0)
    return {};
  NSURL *directory = [[base URLByAppendingPathComponent:bundle isDirectory:YES]
      URLByAppendingPathComponent:@"xmtp"
                      isDirectory:YES];
  return directory.path.UTF8String;
}

class XmtpTurboModule final : public facebook::react::ObjCTurboModule {
public:
  XmtpTurboModule(const InitParams &params, std::shared_ptr<ModuleState> state)
      : ObjCTurboModule(params), state_(std::move(state)) {
    methodMap_["install"] = MethodMetadata{0, install};
    methodMap_["defaultStorageDirectory"] =
        MethodMetadata{0, defaultStorageDirectory};
  }

private:
  static facebook::jsi::Value install(facebook::jsi::Runtime &runtime,
                                      facebook::react::TurboModule &module,
                                      const facebook::jsi::Value *, size_t) {
    auto &self = static_cast<XmtpTurboModule &>(module);
    {
      std::lock_guard lock(self.state_->mutex);
      if (self.state_->invalidated)
        return false;
      if (self.state_->owner)
        return true;
    }
    // Never hold the module mutex while hopping to the main thread: native
    // invalidation can run there while this synchronous JS call is pending.
    if (!observeLifecycle())
      return false;
    std::lock_guard lock(self.state_->mutex);
    if (self.state_->invalidated)
      return false;
    if (self.state_->owner)
      return true;
    self.state_->owner =
        xmtp::reactnative::Runtime::install(runtime, self.jsInvoker_);
    return true;
  }

  static facebook::jsi::Value
  defaultStorageDirectory(facebook::jsi::Runtime &runtime,
                          facebook::react::TurboModule &,
                          const facebook::jsi::Value *, size_t) {
    return facebook::jsi::String::createFromUtf8(runtime, storageDirectory());
  }
  std::shared_ptr<ModuleState> state_;
};
} // namespace

@implementation XmtpReactNative {
  std::shared_ptr<ModuleState> _state;
}
RCT_EXPORT_MODULE(XmtpReactNative)

+ (BOOL)requiresMainQueueSetup {
  return NO;
}

// RN invalidates this module inline on its JS thread, before destroying the
// runtime in the same task. Posting another JS task here would be too late.
- (dispatch_queue_t)methodQueue {
  return RCTJSThread;
}

- (instancetype)init {
  self = [super init];
  if (self)
    _state = std::make_shared<ModuleState>();
  return self;
}

- (std::shared_ptr<facebook::react::TurboModule>)getTurboModule:
    (const facebook::react::ObjCTurboModule::InitParams &)params {
  return std::make_shared<XmtpTurboModule>(params, _state);
}

- (void)invalidate {
  std::shared_ptr<xmtp::reactnative::Runtime> owner;
  {
    std::lock_guard lock(_state->mutex);
    _state->invalidated = true;
    owner = std::move(_state->owner);
  }
  if (owner)
    owner->invalidate();
}

- (void)dealloc {
  [self invalidate];
}
@end
