// Chromium proofs that need fixture builds, a held worker, or the loopback
// object store. Public browser behavior is tested in `sdks/browser/test`.
import { test } from "vitest";

import {
  checkAttachmentEnd,
  checkEventEnd,
  checkAttachmentWorkerDeath,
  checkAttachmentSourceShape,
} from "./attachment-lifetime.chromium";
import { receivedStandardContentDecodesOnce } from "./message.decode-once.chromium";
import { checkStorageLayout } from "./storage.layout.chromium";
import { checkRealWasmTrap } from "./suite.panic.chromium";

declare const __XMTP_BACKEND_URL__: string;
declare const __SDK_FIXTURE_URL__: string;

test("real WASM trap settles pending bridge calls", checkRealWasmTrap, 30_000);

test(
  "storage layout: labelled, unsafe, and explicit locations reopen offline",
  () => checkStorageLayout(__SDK_FIXTURE_URL__),
  90_000,
);
test(
  "attachments: end waits for an upload; calls fail closed",
  () => checkAttachmentEnd(__SDK_FIXTURE_URL__),
  120_000,
);
test(
  "events: client end settles a held worker reply",
  () => checkEventEnd(__XMTP_BACKEND_URL__),
  60_000,
);
test(
  "attachments: cancelled binding waiter retains storage until PUT completes",
  () => checkAttachmentEnd(__SDK_FIXTURE_URL__, true),
  120_000,
);
test(
  "attachments: package worker death releases storage without replay",
  () => checkAttachmentWorkerDeath(__SDK_FIXTURE_URL__),
  90_000,
);
test(
  "attachments: package failure terminates the worker before reopening storage",
  () => checkAttachmentWorkerDeath(__SDK_FIXTURE_URL__, false),
  90_000,
);
test(
  "attachments: malformed sources have no OPFS side effects",
  () => checkAttachmentSourceShape(__XMTP_BACKEND_URL__),
  60_000,
);
test(
  "received_standard_content_decodes_once",
  () => receivedStandardContentDecodesOnce(__XMTP_BACKEND_URL__),
  90_000,
);
