import assert from "node:assert/strict";
import { test } from "node:test";

import { parseLaunchEnvironment } from "./launchEnvironment.ts";

test("reads every launch variable", () => {
  assert.deepEqual(
    parseLaunchEnvironment({
      XMTP_BACKEND_URL: "http://127.0.0.1:5150",
      XMTP_ALLOW_PRIVATE_ATTACHMENTS: "1",
      XMTP_UI_TEST_ATTACHMENT: "proof.txt:cHJvb2Y=",
    }),
    {
      backendUrl: "http://127.0.0.1:5150",
      allowPrivateAttachments: true,
      uiTestAttachment: "proof.txt:cHJvb2Y=",
    },
  );
});

test("keeps an empty backend URL, like the iOS app", () => {
  assert.equal(parseLaunchEnvironment({ XMTP_BACKEND_URL: "" }).backendUrl, "");
  assert.equal(parseLaunchEnvironment({}).backendUrl, undefined);
});

test("allows private attachments only for exactly 1", () => {
  for (const value of ["", "0", "true", "yes", " 1", "1 ", "01", 1, true]) {
    assert.equal(
      parseLaunchEnvironment({ XMTP_ALLOW_PRIVATE_ATTACHMENTS: value })
        .allowPrivateAttachments,
      false,
      `value ${JSON.stringify(value)}`,
    );
  }
});

test("ignores values that are not strings", () => {
  assert.deepEqual(
    parseLaunchEnvironment({
      XMTP_BACKEND_URL: 5150,
      XMTP_UI_TEST_ATTACHMENT: null,
    }),
    {
      backendUrl: undefined,
      allowPrivateAttachments: false,
      uiTestAttachment: undefined,
    },
  );
});
