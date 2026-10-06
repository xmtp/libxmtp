import { Group, XmtpError, type FieldValue } from "@xmtp/browser-sdk";
import { initPureWasm, metadataFieldRef } from "@xmtp/browser-sdk/pure";
import { beforeAll, expect, test } from "vitest";

import { create } from "./helpers";

beforeAll(() => initPureWasm());

const text = (value: string): FieldValue => ({ kind: "string", value });

// Rust tests cover policies, labels and collections
// (`xmtp_sdk/src/tests/metadata_fields/*`). This test checks that the
// descriptors, values and typed errors cross the worker.
// verifies: META-069
test("well-known catalogue fields read and write through the worker", async () => {
  const alix = await create();
  const bo = await create();
  const group = await alix.conversations.createGroup([bo.inboxId]);
  const groupName = metadataFieldRef("groupName");
  const displayName = metadataFieldRef("userDisplayName");
  expect(groupName.name).toBe("GROUP_NAME");
  expect(displayName).toStrictEqual({
    componentId: 0x800c,
    name: "USER_DISPLAY_NAME",
  });

  const fields = await group.metadataFields();
  const described = (field: typeof groupName) =>
    fields.find(
      (descriptor) => descriptor.field.componentId === field.componentId,
    );
  expect(described(groupName)).toMatchObject({
    field: groupName,
    componentType: { kind: "string" },
    isUserField: false,
  });
  expect(described(displayName)).toMatchObject({
    field: displayName,
    componentType: { kind: "map", keyType: "inboxId", valueType: "string" },
    isUserField: true,
  });

  await group.updateMetadataField(groupName, {
    kind: "replace",
    value: text("Team"),
  });
  await group.updateUserData([{ field: displayName, value: text("Alix") }]);
  await bo.conversations.sync();
  const peer = await bo.conversations.getById(group.id);
  if (!(peer instanceof Group)) throw new Error("Bo did not join the group");
  await peer.sync();
  expect(await peer.metadataValue(groupName)).toStrictEqual({
    kind: "scalar",
    value: text("Team"),
  });
  expect(await peer.userData([displayName], [alix.inboxId])).toStrictEqual(
    new Map([[alix.inboxId, [{ field: displayName, value: text("Alix") }]]]),
  );

  const error = await peer
    .updateMetadataField(
      { componentId: 0xc0ff, name: undefined },
      { kind: "replace", value: text("x") },
    )
    .then(
      () => undefined,
      (reason: unknown) => reason,
    );
  expect(error).toBeInstanceOf(XmtpError.UnknownField);
  expect(error).toMatchObject({
    details: { code: "UnknownField", category: "input", retryable: false },
  });
});
