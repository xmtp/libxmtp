#import <CommonCrypto/CommonDigest.h>
#import <React/RCTBridgeModule.h>

@interface XmtpMigrationFixture : NSObject <RCTBridgeModule>
@end

@implementation XmtpMigrationFixture
RCT_EXPORT_MODULE();
+ (BOOL)requiresMainQueueSetup { return NO; }
- (NSString *)directory {
  NSString *root = NSSearchPathForDirectoriesInDomains(NSApplicationSupportDirectory, NSUserDomainMask, YES).firstObject;
  return [root stringByAppendingPathComponent:@"xmtp-migration-fixture"];
}
- (NSString *)database:(BOOL)unsupported {
  return [[self directory] stringByAppendingPathComponent:unsupported ? @"unsupported-schema.db3" : @"react-native-5.7.0.db3"];
}
- (NSDictionary *)sourceFiles:(BOOL)unsupported {
  NSMutableDictionary *state = [NSMutableDictionary dictionary];
  for (NSString *suffix in @[@"", @".sqlcipher_salt", @"-wal", @"-shm", @"-journal"]) {
    NSString *path = [[self database:unsupported] stringByAppendingString:suffix];
    if (![[NSFileManager defaultManager] fileExistsAtPath:path]) {
      state[suffix] = [NSNull null];
      continue;
    }
    NSData *data = [NSData dataWithContentsOfFile:path];
    if (!data) [NSException raise:@"FixtureRead" format:@"Could not inspect legacy source"];
    unsigned char digest[CC_SHA256_DIGEST_LENGTH];
    CC_SHA256(data.bytes, (CC_LONG)data.length, digest);
    NSMutableString *hex = [NSMutableString string];
    for (NSUInteger index = 0; index < CC_SHA256_DIGEST_LENGTH; index++) [hex appendFormat:@"%02x", digest[index]];
    state[suffix] = hex;
  }
  return state;
}
RCT_EXPORT_METHOD(prepare:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject) {
  NSError *error = nil;
  NSString *directory = [self directory];
  NSFileManager *files = [NSFileManager defaultManager];
  if ([files fileExistsAtPath:directory]) {
    reject(@"FIXTURE_PREPARE", @"Migration fixture may be prepared only once per clean install", nil);
    return;
  }
  if (![files createDirectoryAtPath:directory withIntermediateDirectories:YES attributes:nil error:&error]) {
    reject(@"FIXTURE_PREPARE", @"Could not create private fixture directory", error);
    return;
  }
  NSString *assets = [[NSBundle mainBundle].resourcePath stringByAppendingPathComponent:@"xmtp-legacy"];
  for (NSString *name in @[@"react-native-5.7.0.db3", @"react-native-5.7.0.db3.sqlcipher_salt", @"unsupported-schema.db3", @"unsupported-schema.db3.sqlcipher_salt"]) {
    if (![files copyItemAtPath:[assets stringByAppendingPathComponent:name] toPath:[directory stringByAppendingPathComponent:name] error:&error]) {
      reject(@"FIXTURE_PREPARE", @"Could not copy immutable legacy fixture", error);
      return;
    }
  }
  NSString *expected = [NSString stringWithContentsOfFile:[assets stringByAppendingPathComponent:@"expected-records.json"] encoding:NSUTF8StringEncoding error:&error];
  NSString *inspection = [NSString stringWithContentsOfFile:[assets stringByAppendingPathComponent:@"source-inspection.json"] encoding:NSUTF8StringEncoding error:&error];
  NSString *unsupported = [NSString stringWithContentsOfFile:[assets stringByAppendingPathComponent:@"unsupported-schema.json"] encoding:NSUTF8StringEncoding error:&error];
  if (!expected || !inspection || !unsupported) {
    reject(@"FIXTURE_PREPARE", @"Could not read independent fixture expectations", error);
    return;
  }
  resolve(@{@"databasePath": [self database:NO], @"outputPath": [directory stringByAppendingPathComponent:@"converted.xmtp"], @"expectedJson": expected, @"inspectionJson": inspection, @"sourceState": [self sourceFiles:NO], @"unsupportedDatabasePath": [self database:YES], @"unsupportedJson": unsupported, @"unsupportedState": [self sourceFiles:YES]});
}
RCT_EXPORT_METHOD(sourceState:(BOOL)unsupported resolve:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject) {
  @try { resolve([self sourceFiles:unsupported]); }
  @catch (NSException *exception) { reject(@"FIXTURE_HASH", @"Could not inspect legacy source files", nil); }
}
RCT_EXPORT_METHOD(osStorage:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject) {
  NSString *base = NSSearchPathForDirectoriesInDomains(NSApplicationSupportDirectory, NSUserDomainMask, YES).firstObject;
  NSString *bundle = NSBundle.mainBundle.bundleIdentifier;
  if (!base || !bundle) { reject(@"FIXTURE_ROOT", @"OS did not provide application roots", nil); return; }
  resolve(@{@"applicationSupportDirectory": base, @"bundleId": bundle});
}
RCT_EXPORT_METHOD(readFile:(NSString *)path resolve:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject) {
  NSData *data = [NSData dataWithContentsOfFile:path];
  if (!path.isAbsolutePath || !data || data.length > 32 * 1024 * 1024) { reject(@"FIXTURE_FILE", @"Could not read bounded absolute native file", nil); return; }
  NSMutableArray *values = [NSMutableArray arrayWithCapacity:data.length];
  const unsigned char *bytes = (const unsigned char *)data.bytes;
  for (NSUInteger index = 0; index < data.length; index++) [values addObject:@(bytes[index])];
  resolve(values);
}

RCT_EXPORT_METHOD(exists:(NSString *)path resolve:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject) {
  resolve(@([NSFileManager.defaultManager fileExistsAtPath:path]));
}

@end
