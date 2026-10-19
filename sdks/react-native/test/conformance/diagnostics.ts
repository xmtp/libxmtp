import {
  Backend,
  Client,
  type ClientOptions,
  type Signer,
  type LogRecord,
  generateInboxId,
  initLogging,
  localSignerFromPrivateKey,
  setLogSink,
  XmtpError,
} from "@xmtp/react-native-sdk";

import { loggingContract } from "../../../browser/test/platform/logging-contract";
import { assert, equal } from "./assertions";
import { Mailbox } from "./async";

/** Reuses the sibling contract through the installed RN package's real JSI transport. */
export async function diagnosticsJourney(): Promise<string[]> {
  await initLogging({ level: "error" });
  try {
    await loggingContract({
      async setLogSink(sink) {
        await setLogSink(
          sink
            ? {
                async log(record) {
                  const fields = new Map(record.fields);
                  const error = fields.get("error") ?? record.message;
                  if (error.includes("invalid local signer private key"))
                    fields.set("sequence", "0");
                  else {
                    assert(
                      error.includes("invalid public identity"),
                      "overflow workload must emit only its two known diagnostic records",
                    );
                    fields.set("sequence", "1");
                  }
                  await sink.log({
                    fields,
                    droppedRecords: record.droppedRecords,
                  });
                },
              }
            : undefined,
        );
      },
      async emit(count) {
        for (let index = 0; index < count; index++) {
          let failure: unknown;
          try {
            if (index % 2 === 0)
              await localSignerFromPrivateKey(new Uint8Array(31));
            else generateInboxId({ kind: "ethereum", identifier: "invalid" });
          } catch (error) {
            failure = error;
          }
          assert(
            failure instanceof
              (index % 2 === 0
                ? XmtpError.InvalidInput
                : XmtpError.InvalidArgument),
            "diagnostic workload must reach native input validation",
          );
          equal(
            failure.details.code,
            index % 2 === 0 ? "InvalidInput" : "InvalidArgument",
            "diagnostic input error code",
          );
        }
      },
    });
    return [
      "native log transport obeys shared exact overflow, order, failure, replacement and reentrant sink contract",
    ];
  } finally {
    await setLogSink();
  }
}

/** Secrets enter the real native APIs, and their error records leave the native sink. */
export async function redactionJourney(
  signer: Signer,
  options: ClientOptions,
): Promise<string[]> {
  await initLogging({ level: "error" });
  const records = new Mailbox<LogRecord>();
  const captured: LogRecord[] = [];
  await setLogSink({
    async log(record) {
      captured.push(record);
      records.push(record);
    },
  });
  const credential = "RN_PRIVATE_AUTH_TOKEN_174d";
  const signing = "RN_PRIVATE_SIGNING_KEY_174d".padEnd(31, "!");
  const encryption = "RN_PRIVATE_DATABASE_KEY_174d".padEnd(31, "!");
  const bytes = (text: string) =>
    Uint8Array.from(text, (character) => character.charCodeAt(0));
  const url =
    typeof options.backend === "object" && "url" in options.backend
      ? options.backend.url
      : "";
  try {
    for (const [operation, expected] of [
      [
        () =>
          Backend.connect({
            url,
            credentials: {
              value: `Bearer ${credential}\n`,
              expiresAtSeconds: 9_223_372_036_854_775_807n,
            },
          }),
        "credential header value is invalid",
      ],
      [
        () => localSignerFromPrivateKey(bytes(signing)),
        "invalid local signer private key",
      ],
      [
        () =>
          Client.create(signer, {
            ...options,
            storage: { location: "inMemory", encryptionKey: bytes(encryption) },
          }),
        "storage encryption key must be 32 bytes",
      ],
    ] as const) {
      let failure: unknown;
      try {
        await operation();
      } catch (error) {
        failure = error;
      }
      assert(
        failure instanceof XmtpError.InvalidInput,
        "redaction workload must reach the specific native input validation",
      );
      equal(
        failure.details.message,
        expected,
        "redaction must not pass at an unrelated guard",
      );
      // This record is an admission barrier: no empty sink capture can pass.
      for (;;) {
        const record = await records.next(`native diagnostic for ${expected}`);
        if ((record.fields.get("error") ?? record.message).includes(expected))
          break;
      }
    }
    const text = captured
      .map(
        (record) =>
          `${record.message} ${JSON.stringify(Array.from(record.fields))}`,
      )
      .join("\n");
    for (const secret of [credential, signing, encryption]) {
      assert(
        !text.includes(secret),
        "native app log sink must redact secret text",
      );
      const secretBytes = Array.from(bytes(secret));
      assert(
        !text.includes(
          secretBytes
            .map((value) => value.toString(16).padStart(2, "0"))
            .join(""),
        ),
        "native app log sink must redact secret hex",
      );
      assert(
        !text.includes(secretBytes.join(", ")) &&
          !text.includes(secretBytes.join(",")),
        "native app log sink must redact secret byte arrays",
      );
    }
    return [
      "native diagnostic records redact auth, signing and database key sentinels after each actual validation failure",
    ];
  } finally {
    await setLogSink();
  }
}
