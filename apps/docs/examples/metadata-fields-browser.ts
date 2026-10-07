import type { Group } from "@xmtp/browser-sdk";

export async function updateFields(group: Group) {
  // #region update
  const category = await group.metadataField("GROUP_CATEGORY");
  const displayName = await group.metadataField("USER_DISPLAY_NAME");
  const pronouns = await group.metadataField("USER_PRONOUNS");
  if (!category || !displayName || !pronouns) {
    throw new Error("The group has not registered the required fields");
  }

  await group.updateMetadataField(category.field, {
    kind: "replace",
    value: { kind: "string", value: "engineering" },
  });
  await group.updateUserData([
    { field: displayName.field, value: { kind: "string", value: "Alix" } },
    { field: pronouns.field, value: { kind: "string", value: "they/them" } },
  ]);

  await group.sync();
  const profiles = await group.userData(
    [displayName.field, pronouns.field],
    undefined,
  );
  await group.updateUserData([{ field: pronouns.field, value: undefined }]);
  // #endregion update
  return profiles;
}
