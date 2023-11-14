import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';

import 'package:xmtp_bindings_flutter/xmtp_bindings_flutter.dart';

void main() {
  test(
    skip: "TODO",
    'xmtp_libxmtp should be loaded',
    () async {
      expect(libxmtp, isNotNull);
    },
  );

  test(
    skip: "TODO",
    'generatePrivatePreferencesTopicIdentifier',
    () async {
      // Randomly generated key with a known-reference identifier.
      // This ensures that we're generating consistently across SDKs.
      const key = [
        69, 239, 223, 17, 3, 219, 126, 21, 172, 74, 55, 18, 123, 240, 246, 149, 158, 74, 183,
        229, 236, 98, 133, 184, 95, 44, 130, 35, 138, 113, 36, 211,
      ];
      var identifier = await libxmtp.generatePrivatePreferencesTopicIdentifier(
        privateKeyBytes: Uint8List.fromList(key),
      );
      expect(identifier, "IiwU3YF9vPmMVR9dswQYscoRZyzroOkVBndrmnxJmSk");
    },
  );
}
