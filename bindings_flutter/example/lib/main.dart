import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'dart:async';

import 'package:xmtp_bindings_flutter/xmtp_bindings_flutter.dart';

void main() {
  runApp(const MyApp());
}

class MyApp extends StatefulWidget {
  const MyApp({super.key});

  @override
  State<MyApp> createState() => _MyAppState();
}

class _MyAppState extends State<MyApp> {
  late Future<String> identifierResult;

  @override
  void initState() {
    super.initState();
    // Randomly generated key with a known-reference identifier.
    // This ensures that we're generating consistently across SDKs.
    const key = [
      69, 239, 223, 17, 3, 219, 126, 21, 172, 74, 55, 18, 123, 240, 246, 149, 158, 74, 183,
      229, 236, 98, 133, 184, 95, 44, 130, 35, 138, 113, 36, 211,
    ];
    identifierResult = libxmtp.generatePrivatePreferencesTopicIdentifier(
      privateKeyBytes: Uint8List.fromList(key),
    );
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        appBar: AppBar(
          title: const Text('libxmtp bindings'),
        ),
        body: SingleChildScrollView(
          child: Container(
            padding: const EdgeInsets.all(10),
            child: Column(
              children: [
                FutureBuilder<String>(
                  future: identifierResult,
                  builder: (BuildContext context, AsyncSnapshot<String> identifier) {
                    final value =
                        (identifier.hasData) ? identifier.data : 'loading';
                    return Text(
                      'identifier([...]) = $value',
                      style: const TextStyle(fontSize: 25),
                      textAlign: TextAlign.center,
                    );
                  },
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
