// This initializes `libxmtp` as a shared instance
// of the ffi generated `XmtpBindingsFlutter` class.

import 'dart:ffi';
import 'dart:io' as io;

import './generated/bridge_generated.dart';
import './generated/bridge_definitions.dart';

const _lib = 'xmtp_bindings_flutter';
final XmtpBindingsFlutter libxmtp = XmtpBindingsFlutterImpl(() {
  if (io.Platform.environment.containsKey('FLUTTER_TEST')) {
    return _loadTestLibrary();
  }
  return _loadLibrary();
}());

DynamicLibrary _loadTestLibrary() {
  var libName = io.Platform.isIOS || io.Platform.isMacOS
      ? 'lib$_lib.dylib'
      : 'lib$_lib.so';
  try {
    return DynamicLibrary.open(libName);
  } catch (e) {
    io.stderr.writeln('--- $_lib Test Library Error ---');
    io.stderr.writeln('Missing $libName');
    io.stderr.writeln('Testing with $_lib requires this dynamic library.');
    // TODO give instructions / include a script to get it
    // TODO ^ gist: build in bindings_flutter then cp the .so/.dylib for your OS
    // $ cd libxmtp/bindings_flutter
    // $ cargo build
    // for running on macos:
    // $ cp target/aarch64-apple-darwin/release/libxmtp_bindings_flutter.dylib /my/project/folder
    rethrow;
  }
}

DynamicLibrary _loadLibrary() {
  if (io.Platform.isIOS || io.Platform.isMacOS) {
    return DynamicLibrary.executable();
  }
  return DynamicLibrary.open('lib$_lib.so');
}
