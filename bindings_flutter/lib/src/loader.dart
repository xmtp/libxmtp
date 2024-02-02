// This initializes `libxmtp` as a shared instance.
import 'dart:io' as io;

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import './generated/frb_generated.dart';

const _lib = 'xmtp_bindings_flutter';
final RustLib libxmtp = RustLib.instance;

Future<void> libxmtpInit() => RustLib.init(
    externalLibrary: ExternalLibrary.open(
        io.Platform.isIOS || io.Platform.isMacOS
            ? 'lib$_lib.dylib'
            : 'lib$_lib.so'));

void libxmtpDispose() => RustLib.dispose();
