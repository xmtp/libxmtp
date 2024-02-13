import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:web3dart/web3dart.dart';

import 'package:xmtp_bindings_flutter/xmtp_bindings_flutter.dart';

void main() {
  var dbDir;
  setUpAll(() async {
    dbDir = Directory.systemTemp.createTempSync();
    return libxmtpInit();
  });
  tearDownAll(() {
    libxmtpDispose();
    dbDir.deleteSync(recursive: true);
  });
  test(
    'creating a client with consistent installations',
    () async {
      var wallet = EthPrivateKey.createRandom(Random.secure());
      var dbPath = "${dbDir.path}/${wallet.address.hex}.db";
      var encryptionKey =
          U8Array32(Uint8List.fromList(List.generate(32, (index) => index)));
      var walletSign = (text) async => wallet.signPersonalMessageToUint8List(
          Uint8List.fromList(utf8.encode(text)));
      var host = "http://localhost:5556";

      // When we create a client...
      var createdA = await createClient(
        host: host,
        isSecure: false,
        dbPath: dbPath,
        encryptionKey: encryptionKey,
        accountAddress: wallet.address.hex,
      );
      // ... it should require a signature...
      var clientA = switch (createdA) {
        CreatedClient_Ready() => throw StateError("Should require a signature"),
        CreatedClient_RequiresSignature(field0: var req) =>
          await req.sign(signature: await walletSign(req.textToSign)),
      };

      // ... which should produce a valid installation key.
      var installA = await clientA.installationPublicKey();
      expect(installA, isNotNull);

      // And when we recreate the same client...
      var createdB = await createClient(
        host: host,
        isSecure: false,
        dbPath: dbPath,
        encryptionKey: encryptionKey,
        accountAddress: wallet.address.hex,
      );
      // ... it should be ready without any signature required...
      var clientB = switch (createdB) {
        CreatedClient_Ready(field0: var client) => client,
        CreatedClient_RequiresSignature() =>
          throw StateError("Should not require signature"),
      };
      // ... and it should have the the same installation key.
      var installB = await clientB.installationPublicKey();
      expect(installA, installB);
    },
  );
  test(
    'creating and listing groups',
    () async {
      var walletA = EthPrivateKey.createRandom(Random.secure());
      var walletB = EthPrivateKey.createRandom(Random.secure());
      var walletC = EthPrivateKey.createRandom(Random.secure());
      var clientA = await createClientForWallet(dbDir, walletA);
      var clientB = await createClientForWallet(dbDir, walletB);
      var clientC = await createClientForWallet(dbDir, walletC);

      // At first, none of them have any groups
      for (var client in [clientA, clientB, clientC]) {
        expect((await client.listGroups()).length, 0);
      }

      // But when user A invites them all to a group...
      var group = await clientA.createGroup(
        accountAddresses: [walletB.address.hex, walletC.address.hex],
      );
      await delayToPropagate();

      // ... they all should have the same group.
      for (var client in [clientA, clientB, clientC]) {
        expect((await client.listGroups()).length, 1);
        expect((await client.listGroups()).first.groupId, group.groupId);
      }
    },
  );
  test(
    'creating and listing messages',
    () async {
      var walletA = EthPrivateKey.createRandom(Random.secure());
      var walletB = EthPrivateKey.createRandom(Random.secure());
      var walletC = EthPrivateKey.createRandom(Random.secure());
      var clientA = await createClientForWallet(dbDir, walletA);
      var clientB = await createClientForWallet(dbDir, walletB);
      var clientC = await createClientForWallet(dbDir, walletC);
      var group = await clientA.createGroup(
        accountAddresses: [walletB.address.hex, walletC.address.hex],
      );
      await delayToPropagate();

      // They're all in the group...
      for (var client in [clientA, clientB, clientC]) {
        expect((await client.listGroups()).length, 1);
        expect((await client.listGroups()).first.groupId, group.groupId);
        // ... and at first, none of them see any messages...
        expect((await client.listMessages(groupId: group.groupId)).length, 0);
      }

      // ... but when A sends a message to the group...
      await clientA.sendMessage(
        groupId: group.groupId,
        contentBytes: [0xAA, 0xBB, 0xCC], // some encoded content
      );

      // ... then they should all see the message:
      for (var client in [clientA, clientB, clientC]) {
        var messages = (await client.listMessages(groupId: group.groupId));
        expect(messages.length, 1);
        expect(messages.first.groupId, group.groupId);
        expect(messages.first.senderAccountAddress, walletA.address.hex);
        expect(messages.first.contentBytes, [0xAA, 0xBB, 0xCC]);
        expect(messages.first.sentAtNs, closeTo(nowNs(), 10e9 /* 10s drift */));
      }
    },
  );
  test(
    'adding/removing/listing group members',
    () async {
      var walletA = EthPrivateKey.createRandom(Random.secure());
      var walletB = EthPrivateKey.createRandom(Random.secure());
      var walletC = EthPrivateKey.createRandom(Random.secure());
      var clientA = await createClientForWallet(dbDir, walletA);
      var clientB = await createClientForWallet(dbDir, walletB);
      var clientC = await createClientForWallet(dbDir, walletC);

      // At first, A creates a group with just B...
      var group = await clientA.createGroup(
        accountAddresses: [walletB.address.hex],
      );
      await delayToPropagate();

      // So they should see each other
      for (var client in [clientA, clientB]) {
        expect(
            (await client.listMembers(groupId: group.groupId))
                .map((m) => m.accountAddress)
                .toSet(),
            {
              walletA.address.hex,
              walletB.address.hex,
            });
      }

      // ... and when A adds C to the group...
      await clientA.addMember(
        groupId: group.groupId,
        accountAddress: walletC.address.hex,
      );

      // ... then they should all see each other:
      for (var client in [clientA, clientB]) {
        expect(
            (await client.listMembers(groupId: group.groupId))
                .map((m) => m.accountAddress)
                .toSet(),
            {
              walletA.address.hex,
              walletB.address.hex,
              walletC.address.hex,
            });
      }

      // ... and when A removes B from the group...
      await clientA.removeMember(
        groupId: group.groupId,
        accountAddress: walletB.address.hex,
      );

      // ... then only A and C should see each other:
      for (var client in [clientA, clientC]) {
        expect(
            (await client.listMembers(groupId: group.groupId))
                .map((m) => m.accountAddress)
                .toSet(),
            {
              walletA.address.hex,
              walletC.address.hex,
            });
      }
    },
  );
}

// Helpers

/// A delay to allow messages to propagate before making assertions.
delayToPropagate() => Future.delayed(const Duration(milliseconds: 200));

/// This produces the current time in nanoseconds since the epoch.
nowNs() => DateTime.now().millisecondsSinceEpoch * 1000000;

/// Create a client for a [wallet] in the temporary [dbDir].
Future<Client> createClientForWallet(
  Directory dbDir,
  EthPrivateKey wallet,
) async {
  var dbPath = "${dbDir.path}/${wallet.address.hex}.db";
  var encryptionKey =
      U8Array32(Uint8List.fromList(List.generate(32, (index) => index)));
  var walletSign = (text) async => wallet
      .signPersonalMessageToUint8List(Uint8List.fromList(utf8.encode(text)));
  var host = "http://localhost:5556";

  // When we create a client...
  var createdA = await createClient(
    host: host,
    isSecure: false,
    dbPath: dbPath,
    encryptionKey: encryptionKey,
    accountAddress: wallet.address.hex,
  );
  return switch (createdA) {
    CreatedClient_Ready(field0: var client) => client,
    CreatedClient_RequiresSignature(field0: var req) =>
      await req.sign(signature: await walletSign(req.textToSign)),
  };
}
