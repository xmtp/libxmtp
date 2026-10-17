import { beforeAll, describe, expect, it } from "vitest";

import {
  createRegisteredIdentity,
  parseJsonOutput,
  runWithIdentity,
} from "./helpers.js";

// The code after # must have exactly 4 digits.
const MALFORMED = "nick#23";

describe("commands that take handles", () => {
  let creator: Awaited<ReturnType<typeof createRegisteredIdentity>>;
  let groupId: string;

  beforeAll(async () => {
    creator = await createRegisteredIdentity();
    const member = await createRegisteredIdentity();
    const result = await runWithIdentity(creator, [
      "conversations",
      "create-group",
      member.address,
      "--json",
    ]);
    expect(result.exitCode).toBe(0);
    groupId = parseJsonOutput<{ id: string }>(result.stdout).id;
  });

  it.each([
    ["create-dm", () => ["conversations", "create-dm", MALFORMED]],
    ["create-group", () => ["conversations", "create-group", MALFORMED]],
    ["get-dm", () => ["conversations", "get-dm", MALFORMED]],
    ["add-members", () => ["conversation", "add-members", groupId, MALFORMED]],
    [
      "remove-members",
      () => ["conversation", "remove-members", groupId, MALFORMED],
    ],
  ])("%s names a malformed handle", async (_name, args) => {
    const result = await runWithIdentity(creator, [
      ...args(),
      "--identifier-kind",
      "handle",
    ]);

    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toContain(`${MALFORMED} is not a valid handle.`);
    expect(result.stderr).not.toContain("USAGE");
  });
});
