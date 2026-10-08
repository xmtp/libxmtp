import { prepareMigrationArchive, XmtpError } from "@xmtp/agent-sdk";
import {
  prepareMigrationArchive as nodePrepare,
  XmtpError as NodeError,
} from "@xmtp/node-sdk";
import { expect, it } from "vitest";

// verifies: MIG-001, MIG-003
it("re-exports migration and typed errors from the normal Node package", () => {
  expect(prepareMigrationArchive).toBe(nodePrepare);
  expect(typeof prepareMigrationArchive).toBe("function");
  expect(typeof XmtpError.MigrationRecordRead).toBe("function");
  expect(typeof XmtpError.MigrationOutput).toBe("function");
  expect(XmtpError.MigrationRecordRead).toBe(NodeError.MigrationRecordRead);
  expect(XmtpError.MigrationOutput).toBe(NodeError.MigrationOutput);
});
