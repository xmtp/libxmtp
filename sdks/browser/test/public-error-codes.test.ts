import { XmtpError } from "@xmtp/browser-sdk";
import { expect, test } from "vitest";

import { create } from "./helpers";

test("invalid group member keeps a public input error and group state", async () => {
  const client = await create();
  try {
    const group = await client.conversations.createGroup([]);
    const before = await group.members();
    const error = await group.addMembers(["not_a_real_inbox_id"]).then(
      () => undefined,
      (cause: unknown) => cause,
    );
    expect(error).toBeInstanceOf(XmtpError.InvalidArgument);
    expect((error as XmtpError).details).toMatchObject({
      code: "InvalidArgument",
      category: "input",
      retryable: false,
    });
    expect((error as XmtpError).details.message.length).toBeGreaterThan(0);
    expect(await group.members()).toEqual(before);
  } finally {
    await client.end();
  }
});

test("long group name keeps the limit cause and unchanged name", async () => {
  const client = await create();
  try {
    const group = await client.conversations.createGroup([]);
    const before = (await group.state()).name;
    const error = await group.updateName("a".repeat(1025)).then(
      () => undefined,
      (cause: unknown) => cause,
    );
    expect(error).toBeInstanceOf(XmtpError.InvalidArgument);
    expect((error as XmtpError).details).toEqual({
      code: "InvalidArgument",
      category: "input",
      retryable: false,
      message: "Exceeded max characters for this field. Must be under: 100",
    });
    expect((await group.state()).name).toBe(before);
  } finally {
    await client.end();
  }
});
