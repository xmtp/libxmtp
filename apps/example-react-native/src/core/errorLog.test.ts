import assert from "node:assert/strict";
import { test } from "node:test";

import { errorLogText, LOGGED_ID_LENGTH } from "./errorLog.ts";

const groupId = "0123456789abcdef".repeat(4);
const inboxId = "fedcba98".repeat(8);
const messageId = "A1B2C3D4".repeat(8);
const address = `0x${"5e".repeat(20)}`;
const groupBytes = Array.from({ length: 32 }, (_, index) => index * 8 + 1);

/** The shape of the SDK's `XmtpError`: a fixed code and free message text. */
class SdkError extends Error {
  readonly details: { readonly code: string; readonly message: string };

  constructor(code: string, message: string) {
    super(message);
    this.name = `XmtpError.${code}`;
    this.details = { code, message };
  }
}

test("logged error text never holds a full ID", () => {
  const cases: {
    error: unknown;
    shows: readonly string[];
    hides: readonly string[];
  }[] = [
    // An SDK error logs only its code: its message has no fixed ID form.
    {
      error: new SdkError("Storage", `group with id ${groupId} not found`),
      shows: ["Storage"],
      hides: [groupId, groupId.slice(0, LOGGED_ID_LENGTH), "not found"],
    },
    // Other errors keep their text and a short prefix of each ID.
    {
      error: new TypeError(`no conversation ${groupId}`),
      shows: [
        "TypeError",
        "no conversation",
        groupId.slice(0, LOGGED_ID_LENGTH),
      ],
      hides: [groupId],
    },
    {
      error: new Error(`inbox ${inboxId} and message ${messageId}`),
      shows: [
        inboxId.slice(0, LOGGED_ID_LENGTH),
        messageId.slice(0, LOGGED_ID_LENGTH),
      ],
      hides: [inboxId, messageId],
    },
    {
      error: `address ${address}`,
      shows: ["address", address.slice(0, LOGGED_ID_LENGTH)],
      hides: [address, address.slice(2)],
    },
    {
      error: new Error(`panicked at GroupId([${groupBytes.join(", ")}])`),
      shows: ["panicked at GroupId"],
      hides: [groupBytes.join(", ")],
    },
  ];
  for (const { error, shows, hides } of cases) {
    const text = errorLogText(error);
    for (const part of shows)
      assert.ok(text.includes(part), `${text} shows ${part}`);
    for (const part of hides)
      assert.ok(!text.includes(part), `${text} hides ${part}`);
  }
});
