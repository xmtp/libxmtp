// This initializes `libxmtp` as a shared instance.
import 'dart:io' as io;
import 'package:flutter/foundation.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import './generated/frb_generated.dart';

const _lib = 'xmtp_bindings_flutter';
Future<void> libxmtpInit() async {
  if (RustLib.instance.initialized) {
    return;
  }
  var path = io.Platform.environment.containsKey('FLUTTER_TEST')
      ? (io.Platform.isIOS || io.Platform.isMacOS
          ? 'lib$_lib.dylib'
          : 'lib$_lib.so')
      : (io.Platform.isIOS || io.Platform.isMacOS
          ? '$_lib.framework/$_lib'
          : 'lib$_lib.so');
  debugPrint('libxmtp: initializing $path');
  return RustLib.init(externalLibrary: ExternalLibrary.open(path));
}

void libxmtpDispose() => RustLib.dispose();
