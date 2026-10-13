import {
  generateInboxId,
  initLogging,
  localSignerFromPrivateKey,
  setLogSink,
} from "@xmtp/node-sdk";
import { expect, test } from "vitest";

import { loggingContract } from "../../browser/test/platform/logging-contract.js";

// verifies: LOG-002, LOG-003, LOG-004, LOG-005, LOG-007, LOG-011, LOG-012, LOG-013
test("native public log sink obeys queue and generation rules", async () => {
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
                  if (error.includes("invalid local signer private key")) {
                    fields.set("sequence", "0");
                  } else {
                    expect(error).toContain("invalid public identity");
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
          if (index % 2 === 0) {
            await expect(
              localSignerFromPrivateKey(new Uint8Array(31)),
            ).rejects.toThrow();
          } else {
            expect(() =>
              generateInboxId({ kind: "ethereum", identifier: "invalid" }),
            ).toThrow();
          }
        }
      },
    });
  } finally {
    await setLogSink();
  }
}, 120_000);
