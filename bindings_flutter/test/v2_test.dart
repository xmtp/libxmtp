import 'dart:math';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:web3dart/web3dart.dart';

import 'package:xmtp_bindings_flutter/xmtp_bindings_flutter.dart';

void main() {
  setUpAll(() async => libxmtpInit());
  tearDownAll(() => libxmtpDispose());
  test(
    'generate user preferences identifier',
    () async {
      // Randomly generated key with a known-reference identifier.
      // This ensures that we're generating consistently across SDKs.
      const key = [
        69, 239, 223, 17, 3, 219, 126, 21, // prevent auto-format
        172, 74, 55, 18, 123, 240, 246, 149,
        158, 74, 183, 229, 236, 98, 133, 184,
        95, 44, 130, 35, 138, 113, 36, 211,
      ];
      var identifier = await generatePrivatePreferencesTopicIdentifier(
        privateKeyBytes: Uint8List.fromList(key),
      );
      expect(identifier, "EBsHSM9lLmELuUVCMJ-tPE0kDcok1io9IwUO6WPC-cM");
    },
  );
  test(
    'encrypt/decrypt user preferences',
    () async {
      var identity = EthPrivateKey.createRandom(Random.secure());
      const original = [1, 2, 3, 4];
      var encrypted = await userPreferencesEncrypt(
        publicKey: identity.publicKey.getEncoded(false),
        privateKey: identity.privateKey,
        message: original,
      );
      expect(encrypted, isNot(original));

      var decrypted = await userPreferencesDecrypt(
        publicKey: identity.publicKey.getEncoded(false),
        privateKey: identity.privateKey,
        encryptedMessage: encrypted,
      );
      expect(decrypted, original);
    },
  );
}
