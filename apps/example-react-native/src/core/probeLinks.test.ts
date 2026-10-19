import assert from "node:assert/strict";
import { test } from "node:test";

import {
  chatProbeLinkConfirmation,
  probeLinkConfirmation,
} from "./probeLinks.ts";
import type { SessionState } from "./sessionController.ts";

type Kind = SessionState<unknown>["kind"];

test("a probe link asks before it can end the session or change the account", () => {
  const notSignedOut: readonly Kind[] = [
    "starting",
    "storageFailed",
    "opening",
    "active",
    "restoreFailed",
    "resetIncomplete",
  ];
  const cases: readonly (readonly [string, Kind, string | undefined])[] = [
    ["signOut", "active", "signOut"],
    ["delete", "active", "delete"],
    ["allowRequests", "active", "allowRequests"],
    ["claim", "active", "claim"],
    ["createGroup", "active", "createGroup"],
    // A signed-out app has no session to end: Join and Retry run at once.
    ["connect", "signedOut", undefined],
    ["retry", "signedOut", undefined],
    ...notSignedOut.flatMap((kind) => [
      ["connect", kind, "connect"] as const,
      ["retry", kind, "retry"] as const,
    ]),
    ["start", "active", undefined],
    ["restore", "active", undefined],
    ["probe", "active", undefined],
    ["handles", "active", undefined],
    ["continueReset", "resetIncomplete", undefined],
    ["contend", "active", undefined],
  ];
  for (const [action, kind, confirmation] of cases) {
    assert.equal(
      probeLinkConfirmation(action, kind),
      confirmation,
      `${action} (${kind})`,
    );
  }
});

test("a chat probe link asks before it changes data", () => {
  const cases: readonly (readonly [string, string | undefined])[] = [
    // Seen by other people.
    ["retry", "retry"],
    ["react", "react"],
    ["delete", "delete"],
    // Replace the chat's saved draft.
    ["reply", "reply"],
    ["edit", "edit"],
    // Send faults for this app run only.
    ["failNextSend", undefined],
    ["failPublish", undefined],
    ["allowPublish", undefined],
  ];
  for (const [action, confirmation] of cases) {
    assert.equal(chatProbeLinkConfirmation(action), confirmation, action);
  }
});
